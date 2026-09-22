//! Supervised process graph; bounded stop, source lifetime checks and audio restoration.
use crate::{
    audio::{self, Mode},
    capture, config, storage, transport,
};
use anyhow::{Context, Result};
pub use caster_core::{Request, Status};
use caster_core::{
    SessionState,
    adapters::{AudioBackend, TransportBackend},
};
use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    os::{
        fd::AsRawFd,
        unix::{fs::OpenOptionsExt, process::CommandExt},
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
        source_kind: if spec.source.kind == capture::Kind::Window {
            "window"
        } else {
            "screen"
        }
        .into(),
        ..Status::default()
    }));
    save(&state.lock().unwrap())?;
    let result = run(&spec, state.clone());
    let restoration = audio::recover();
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
    capture::validate(&spec.source)?;
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
    let mut routing = if spec.audio == Mode::Application {
        Some(audio::AppAudio::new(spec.source.clone())?)
    } else {
        None
    };
    let monitor = match spec.audio {
        Mode::Application => Some(routing.as_ref().unwrap().monitor()),
        Mode::System => Some(audio::default_monitor()?),
        Mode::Off => None,
    };
    let log = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(storage::directory(storage::Directory::State)?.join("session.log"))?;
    let mut children = Processes(Vec::new());
    let mut audio_pipe = None;
    if let Some(monitor) = monitor {
        let mut child = audio::command(&monitor)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(log.try_clone()?)
            .spawn()
            .context("Starting audio capture")?;
        audio_pipe = child.stdout.take();
        children.0.push(child);
    }
    let fd = audio_pipe.as_ref().map(AsRawFd::as_raw_fd);
    let mut video = capture::command(&spec.source, quality)?
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(log.try_clone()?)
        .spawn()
        .context("Starting video capture")?;
    let video_pipe = video.stdout.take().context("No video pipe")?;
    children.0.push(video);
    let mut command = transport::command(&bins.sender, &spec.receiver, quality, fd)?;
    if let Some(fd) = fd {
        // Only the sender inherits this audio pipe; no global CLOEXEC changes.
        unsafe {
            command.pre_exec(move || {
                if libc::fcntl(fd, libc::F_SETFD, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
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
    let observed = state.clone();
    let reader = thread::spawn(move || observe(stderr, log, observed));
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
    let _ = reader.join();
    loop_result
}
fn observe(stderr: std::process::ChildStderr, mut log: File, state: Arc<Mutex<Status>>) {
    for line in BufReader::new(stderr).lines().map_while(Result::ok) {
        let _ = writeln!(log, "{line}");
        if healthy_video_stats(&line)
            && !STOP.load(Ordering::Relaxed)
            && let Ok(mut status) = state.lock()
        {
            status.state = SessionState::Streaming;
            if status.resolved_quality.is_empty() {
                status.resolved_quality = "1080p30 · 6 Mbps".into();
            }
            status.detail = format!("{} · Native Cast", status.resolved_quality);
            status.scheduled_playout = line
                .split_once("latency")
                .map(|(_, s)| s.trim().to_owned())
                .unwrap_or_default();
            let _ = save(&status);
        }
    }
}

fn healthy_video_stats(line: &str) -> bool {
    if !line.contains("Cast scheduled-playout latency") {
        return false;
    }
    let fps = line
        .split_once("video fps=")
        .and_then(|(_, s)| s.trim().parse::<f64>().ok());
    let latency = line
        .split_once("video=")
        .and_then(|(_, s)| s.split_once("ms"))
        .and_then(|(s, _)| s.parse::<f64>().ok());
    matches!((fps, latency), (Some(f), Some(l)) if f.is_finite() && f > 0.0 && l.is_finite() && l >= 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use caster_core::{Receiver, Source};
    #[test]
    fn invalid_statistics_never_mark_a_stream_healthy() {
        assert!(!healthy_video_stats(
            "Cast scheduled-playout latency: video=-nanms audio=-nanms; video fps=-nan"
        ));
        assert!(!healthy_video_stats(
            "Cast scheduled-playout latency: video=400ms; video fps=0"
        ));
        assert!(healthy_video_stats(
            "Cast scheduled-playout latency: video=400ms audio=-nanms; video fps=29.9"
        ));
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
