//! Supervised process graph; bounded stop, source lifetime checks and audio restoration.
use crate::{
    audio::{self, Mode},
    capture, config, display, storage, transport,
};
use anyhow::{Context, Result};
pub use caster_core::{Request, Status};
use caster_core::{
    SessionState,
    adapters::{AudioBackend, TransportBackend},
};
use serde::Deserialize;
use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    os::{
        fd::AsRawFd,
        unix::{fs::OpenOptionsExt, net::UnixStream, process::CommandExt},
    },
    process::{Child, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

static STOP: AtomicBool = AtomicBool::new(false);
extern "C" fn signal_stop(_: i32) {
    STOP.store(true, Ordering::Relaxed);
}

fn save(status: &Status) -> Result<()> {
    storage::write(
        &storage::directory(storage::Directory::Runtime)?.join("status.json"),
        status,
    )
}
pub fn validate_audio(request: &Request) -> Result<()> {
    use capture::Kind;
    let backend = audio::PulseAudioAdapter;
    anyhow::ensure!(
        !(request.audio == Mode::Application && request.source.kind != Kind::Window),
        "Application audio requires a selected window"
    );
    anyhow::ensure!(
        !(request.audio == Mode::System && request.source.kind == Kind::Window),
        "Window mode never captures system audio"
    );
    anyhow::ensure!(
        backend.supports(&request.source, request.audio),
        "Selected audio mode is not supported for this source"
    );
    Ok(())
}

struct Processes(Vec<Child>);
impl Processes {
    fn stop(&mut self) {
        for child in &mut self.0 {
            if matches!(child.try_wait(), Ok(None)) {
                unsafe {
                    libc::kill(child.id() as i32, libc::SIGTERM);
                }
            }
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        for child in &mut self.0 {
            while matches!(child.try_wait(), Ok(None)) && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(20));
            }
            if matches!(child.try_wait(), Ok(None)) {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
    }
}
impl Drop for Processes {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn worker() -> Result<()> {
    STOP.store(false, Ordering::Relaxed);
    unsafe {
        libc::signal(
            libc::SIGTERM,
            signal_stop as *const () as libc::sighandler_t,
        );
        libc::signal(libc::SIGINT, signal_stop as *const () as libc::sighandler_t);
    }
    let spec: Request =
        storage::read(&storage::directory(storage::Directory::Runtime)?.join("request.json"))?;
    let state = Arc::new(Mutex::new(Status {
        state: SessionState::Connecting,
        detail: "Starting native Cast".into(),
        device: spec.receiver.name.clone(),
        source: spec.source.label.clone(),
        source_id: spec.source.id.clone(),
        device_id: spec.receiver.id.clone(),
        source_kind: Some(spec.source.kind),
        ..Status::default()
    }));
    save(&state.lock().unwrap())?;
    let result = run(&spec, state.clone());
    let restoration = audio::recover().and(display::recover());
    let mut status = state.lock().unwrap();
    if let Err(error) = restoration {
        status.state = SessionState::Error;
        status.detail = format!("Audio restoration needs attention: {error:#}");
        save(&status)?;
        return Err(error);
    }
    match result {
        Ok(()) => {
            status.state = SessionState::Idle;
            status.detail = "Stopped; audio routing restored".into();
            save(&status)?;
            Ok(())
        }
        Err(error) => {
            status.state = SessionState::Error;
            status.detail = format!("{error:#}");
            save(&status)?;
            Err(error)
        }
    }
}
fn run(spec: &Request, state: Arc<Mutex<Status>>) -> Result<()> {
    validate_audio(spec)?;
    audio::recover()?;
    display::recover()?;
    capture::validate(&spec.source)?;
    if spec.source.kind == capture::Kind::Screen {
        display::engage(&spec.source.name)?;
    }
    let bins = config::binaries()?;
    let (profile, quality) =
        caster_core::receiver_profiles::resolve_quality(&spec.receiver, spec.quality, spec.bitrate);
    transport::OpenScreenTransport
        .validate_quality(quality)
        .map_err(|error| anyhow::anyhow!(error.message))?;
    {
        let mut status = state.lock().unwrap();
        status.resolved_quality = format!(
            "{}×{} · {} fps · {} Mbps · {}",
            quality.width,
            quality.height,
            quality.fps,
            quality.bitrate / 1_000_000,
            profile.id
        );
        save(&status)?;
    }
    let mut routing = match spec.audio {
        Mode::Off => None,
        mode => Some(audio::AppAudio::new(
            spec.source.clone(),
            mode == Mode::System,
        )?),
    };
    let monitor = routing.as_ref().map(audio::AppAudio::monitor);
    let log = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(storage::directory(storage::Directory::State)?.join("session.log"))?;
    let mut children = Processes(Vec::new());
    let mut audio_pipe = None;
    if let Some(monitor) = monitor {
        let mut child = audio::command(&monitor)?
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(log.try_clone()?)
            .spawn()
            .context("Starting audio capture")?;
        audio_pipe = child.stdout.take();
        children.0.push(child);
    }
    let audio_fd = audio_pipe.as_ref().map(AsRawFd::as_raw_fd);
    let mut video = capture::command(&spec.source, quality)?
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(log.try_clone()?)
        .spawn()
        .context("Starting video capture")?;
    let video_pipe = video.stdout.take().context("No video pipe")?;
    children.0.push(video);
    let (event_read, event_write) =
        UnixStream::pair().context("Creating transport event channel")?;
    let event_fd = event_write.as_raw_fd();
    let mut command =
        transport::command(&bins.sender, &spec.receiver, quality, audio_fd, event_fd)?;
    // Only the sender inherits these pipes; no global CLOEXEC changes.
    unsafe {
        command.pre_exec(move || {
            for descriptor in [audio_fd, Some(event_fd)].into_iter().flatten() {
                if libc::fcntl(descriptor, libc::F_SETFD, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    let mut sender = command
        .stdin(video_pipe)
        .stdout(log.try_clone()?)
        .stderr(Stdio::piped())
        .spawn()
        .context("Starting Open Screen sender")?;
    let stderr = sender.stderr.take().context("Missing sender diagnostics")?;
    children.0.push(sender);
    drop(audio_pipe);
    drop(event_write);
    let diagnostics = thread::spawn(move || copy_diagnostics(stderr, log));
    let observed = state.clone();
    let events = thread::spawn(move || observe_events(event_read, observed));
    let started = Instant::now();
    let loop_result = (|| -> Result<()> {
        while !STOP.load(Ordering::Relaxed) {
            for child in &mut children.0 {
                if let Some(exit) = child.try_wait()? {
                    anyhow::bail!("Media process stopped ({exit}); see session.log");
                }
            }
            capture::validate(&spec.source)?;
            if let Some(routing) = routing.as_mut() {
                let mut status = state.lock().unwrap();
                status.audio_detail = routing.update()?;
                save(&status)?;
            }
            anyhow::ensure!(
                started.elapsed() < Duration::from_secs(30)
                    || state.lock().unwrap().state == SessionState::Streaming,
                "Receiver did not start streaming within 30 seconds"
            );
            thread::sleep(Duration::from_millis(500));
        }
        Ok(())
    })();
    children.stop();
    let _ = diagnostics.join();
    let event_result = events
        .join()
        .map_err(|_| anyhow::anyhow!("Transport event reader panicked"))?;
    loop_result.and(event_result)
}
fn copy_diagnostics(stderr: std::process::ChildStderr, mut log: File) {
    for line in BufReader::new(stderr).lines().map_while(Result::ok) {
        let _ = writeln!(log, "{line}");
    }
}

#[derive(Debug, Deserialize)]
struct EventEnvelope {
    api: u32,
    #[serde(flatten)]
    event: TransportEvent,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum TransportEvent {
    Connecting,
    Negotiating,
    Negotiated {
        max_width: u32,
        max_height: u32,
        max_fps: f64,
        max_bitrate: u32,
        supports_scaling: bool,
    },
    Health {
        video_latency_ms: f64,
        audio_latency_ms: Option<f64>,
        video_fps: f64,
    },
    Fatal {
        code: String,
    },
}

fn apply_event(status: &mut Status, envelope: EventEnvelope) -> Result<()> {
    anyhow::ensure!(envelope.api == 1, "Unsupported transport event API");
    match envelope.event {
        TransportEvent::Connecting => {
            anyhow::ensure!(
                status.state.can_transition_to(SessionState::Connecting),
                "Invalid transition to connecting"
            );
            status.state = SessionState::Connecting;
            status.detail = "Connecting to receiver".into();
        }
        TransportEvent::Negotiating => {
            anyhow::ensure!(
                status.state.can_transition_to(SessionState::Negotiating),
                "Invalid transition to negotiating"
            );
            status.state = SessionState::Negotiating;
            status.detail = "Negotiating Cast capabilities".into();
        }
        TransportEvent::Negotiated {
            max_width,
            max_height,
            max_fps,
            max_bitrate,
            supports_scaling,
        } => {
            status.detail = format!(
                "Receiver supports up to {max_width}×{max_height} at {max_fps:.0} fps, {} Mbps{}",
                max_bitrate / 1_000_000,
                if supports_scaling {
                    " with scaling"
                } else {
                    ""
                }
            );
        }
        TransportEvent::Health {
            video_latency_ms,
            audio_latency_ms,
            video_fps,
        } => {
            anyhow::ensure!(
                video_latency_ms.is_finite()
                    && video_latency_ms >= 0.0
                    && video_fps.is_finite()
                    && video_fps > 0.0,
                "Invalid transport health event"
            );
            anyhow::ensure!(
                status.state.can_transition_to(SessionState::Streaming),
                "Invalid transition to streaming"
            );
            status.state = SessionState::Streaming;
            status.detail = format!("{} · Native Cast", status.resolved_quality);
            status.scheduled_playout = match audio_latency_ms {
                Some(audio) if audio.is_finite() && audio >= 0.0 => format!(
                    "video {video_latency_ms:.0} ms · audio {audio:.0} ms · {video_fps:.1} fps"
                ),
                _ => format!("video {video_latency_ms:.0} ms · {video_fps:.1} fps"),
            };
        }
        TransportEvent::Fatal { code } => {
            status.state = SessionState::Error;
            status.detail = format!("Cast transport failed ({code})");
        }
    }
    Ok(())
}

fn observe_events(stream: UnixStream, state: Arc<Mutex<Status>>) -> Result<()> {
    for line in BufReader::new(stream).lines() {
        let envelope: EventEnvelope = serde_json::from_str(&line?)?;
        let mut status = state.lock().unwrap();
        apply_event(&mut status, envelope)?;
        save(&status)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use caster_core::{Receiver, Source};
    #[test]
    fn structured_events_drive_typed_states() {
        let mut status = Status {
            state: SessionState::Connecting,
            resolved_quality: "1080p30".into(),
            ..Status::default()
        };
        let negotiating: EventEnvelope =
            serde_json::from_str(r#"{"api":1,"event":"negotiating"}"#).unwrap();
        apply_event(&mut status, negotiating).unwrap();
        assert_eq!(status.state, SessionState::Negotiating);
        let health: EventEnvelope = serde_json::from_str(
            r#"{"api":1,"event":"health","video_latency_ms":410.0,"audio_latency_ms":390.0,"video_fps":29.9}"#,
        )
        .unwrap();
        apply_event(&mut status, health).unwrap();
        assert_eq!(status.state, SessionState::Streaming);
        assert!(status.scheduled_playout.contains("410 ms"));
    }

    #[test]
    fn invalid_health_events_are_rejected() {
        let mut status = Status {
            state: SessionState::Negotiating,
            ..Status::default()
        };
        let event: EventEnvelope = serde_json::from_str(
            r#"{"api":1,"event":"health","video_latency_ms":-1.0,"audio_latency_ms":null,"video_fps":0.0}"#,
        )
        .unwrap();
        assert!(apply_event(&mut status, event).is_err());
        assert_ne!(status.state, SessionState::Streaming);
    }
    fn request(kind: capture::Kind, audio: Mode) -> Request {
        Request {
            receiver: Receiver {
                id: "id".into(),
                name: "TV".into(),
                host: "127.0.0.1".into(),
                port: 8009,
                model: String::new(),
            },
            source: Source {
                id: "id".into(),
                kind,
                name: "source".into(),
                label: "source".into(),
                pid: 1,
                identity: "1:2".into(),
                app: "app".into(),
            },
            audio,
            quality: caster_core::QualityPreset::Auto,
            bitrate: caster_core::BitrateChoice::Auto,
        }
    }
    #[test]
    fn never_substitutes_system_audio_for_window_audio() {
        assert!(validate_audio(&request(capture::Kind::Window, Mode::System)).is_err());
        assert!(validate_audio(&request(capture::Kind::Window, Mode::Application)).is_ok());
        assert!(validate_audio(&request(capture::Kind::Window, Mode::Off)).is_ok());
    }
    #[test]
    fn monitor_requires_explicit_system_audio() {
        assert!(validate_audio(&request(capture::Kind::Screen, Mode::Application)).is_err());
        assert!(validate_audio(&request(capture::Kind::Screen, Mode::System)).is_ok());
    }
}
