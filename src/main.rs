mod audio;
mod backends;
mod capture;
mod config;
mod discovery;
mod display;
mod session;
mod storage;
mod supervisor;
mod transport;

use anyhow::{Context, Result};
use caster_core::{BitrateChoice, QualityPreset, SessionState};
use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use std::{
    fs::OpenOptions,
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    process::ExitCode,
};

#[derive(Parser)]
#[command(version, about = "Native Google Cast mirroring for Omarchy")]
struct Cli {
    /// Emit the stable versioned machine-readable envelope.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    action: Action,
}
#[derive(Subcommand)]
enum Action {
    Devices,
    Sources,
    Status,
    Stop,
    Doctor,
    Start {
        #[arg(long)]
        device: String,
        #[arg(long)]
        source: String,
        #[arg(long, default_value = "off")]
        audio: audio::Mode,
        #[arg(long, default_value = "auto")]
        quality: QualityPreset,
        #[arg(long, default_value = "auto")]
        bitrate: BitrateChoice,
    },
    #[command(hide = true)]
    Worker,
    #[command(hide = true)]
    Recover,
    #[command(hide = true)]
    CaptureWindow {
        #[arg(long)]
        address: String,
        #[arg(long)]
        width: u32,
        #[arg(long)]
        height: u32,
        #[arg(long)]
        fps: u32,
    },
    #[command(hide = true)]
    CaptureAudio {
        #[arg(long)]
        monitor: String,
    },
}

impl Action {
    const fn name(&self) -> &'static str {
        match self {
            Self::Devices => "devices",
            Self::Sources => "sources",
            Self::Status => "status",
            Self::Stop => "stop",
            Self::Doctor => "doctor",
            Self::Start { .. } => "start",
            Self::Worker => "worker",
            Self::Recover => "recover",
            Self::CaptureWindow { .. } => "capture-window",
            Self::CaptureAudio { .. } => "capture-audio",
        }
    }
}
fn status() -> Result<session::Status> {
    let path = storage::directory(storage::Directory::Runtime)?.join("status.json");
    let mut state: session::Status = if path.exists() {
        storage::read(&path)?
    } else {
        session::Status::default()
    };
    if !supervisor::active()
        && matches!(
            state.state,
            SessionState::Connecting | SessionState::Negotiating | SessionState::Streaming
        )
    {
        state.state = SessionState::Error;
        state.detail = "Session ended unexpectedly; start again to recover".into();
    }
    Ok(state)
}
fn run(action: Action) -> Result<Value> {
    let root = storage::directory(storage::Directory::Runtime)?;
    match action {
        Action::Devices => {
            let result = discovery::receivers()?;
            storage::write(&root.join("devices.json"), &result)?;
            Ok(json!(result))
        }
        Action::Sources => {
            let result = capture::sources()?;
            storage::write(&root.join("sources.json"), &result)?;
            Ok(json!(result))
        }
        Action::Status => Ok(json!(status()?)),
        Action::Doctor => Ok(json!({
            "controller": "Rust",
            "capabilities": backends::report(),
            "config": config::load()?,
            "paths": {
                "config": storage::directory(storage::Directory::Config)?,
                "runtime": storage::directory(storage::Directory::Runtime)?,
                "state": storage::directory(storage::Directory::State)?,
            }
        })),
        Action::Recover => {
            audio::recover()?;
            display::recover()?;
            Ok(json!({}))
        }
        Action::Worker => {
            session::worker()?;
            Ok(json!({}))
        }
        action => {
            let lock = OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .mode(0o600)
                .open(root.join("control.lock"))?;
            anyhow::ensure!(
                unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } == 0,
                "Cannot lock session control"
            );
            if matches!(action, Action::Stop) {
                if supervisor::active() {
                    let mut stopping = status()?;
                    stopping.state = SessionState::Stopping;
                    stopping.detail = "Stopping cast and restoring audio".into();
                    storage::write(&root.join("status.json"), &stopping)?;
                }
                supervisor::stop()?;
                audio::recover()?;
                display::recover()?;
                let idle = session::Status {
                    state: SessionState::Idle,
                    detail: "Stopped; audio routing restored".into(),
                    ..session::Status::default()
                };
                storage::write(&root.join("status.json"), &idle)?;
                return Ok(json!(idle));
            }
            let Action::Start {
                device,
                source,
                audio,
                quality,
                bitrate,
            } = action
            else {
                unreachable!()
            };
            anyhow::ensure!(
                !supervisor::active(),
                "Another native cast is active; stop it first"
            );
            anyhow::ensure!(
                !std::process::Command::new("systemctl")
                    .args(["--user", "is-active", "--quiet", "fluxcast-live.service"])
                    .status()
                    .is_ok_and(|status| status.success()),
                "FluxCast is active; stop it first"
            );
            let receivers: Vec<discovery::Receiver> =
                storage::read(&root.join("devices.json")).context("Refresh receivers first")?;
            let sources: Vec<capture::Source> =
                storage::read(&root.join("sources.json")).context("Refresh sources first")?;
            let receiver = receivers
                .into_iter()
                .find(|r| r.id == device)
                .context("Receiver unavailable; refresh devices")?;
            let source = sources
                .into_iter()
                .find(|s| s.id == source)
                .context("Source unavailable; refresh sources")?;
            capture::validate(&source)?;
            let request = session::Request {
                receiver,
                source,
                audio,
                quality,
                bitrate,
            };
            session::validate_audio(&request)?;
            config::binaries()?;
            config::remember(&request)?;
            audio::recover()?;
            display::recover()?;
            storage::write(&root.join("request.json"), &request)?;
            storage::write(
                &root.join("status.json"),
                &session::Status {
                    state: SessionState::Connecting,
                    detail: "Starting native Cast".into(),
                    ..session::Status::default()
                },
            )?;
            supervisor::start(&std::env::current_exe()?)?;
            Ok(json!(status()?))
        }
    }
}
fn human_output(command: &str, result: &Value) -> String {
    match command {
        "devices" => result
            .as_array()
            .map(|rows| {
                rows.iter()
                    .map(|row| {
                        format!(
                            "{}\t{}\t{}:{}",
                            row["name"].as_str().unwrap_or("Unknown receiver"),
                            row["model"].as_str().unwrap_or("Unknown model"),
                            row["host"].as_str().unwrap_or("?"),
                            row["port"].as_u64().unwrap_or(0)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_else(|| "No receivers found".into()),
        "sources" => result
            .as_array()
            .map(|rows| {
                rows.iter()
                    .map(|row| {
                        format!(
                            "{}\t{}",
                            row["kind"].as_str().unwrap_or("source"),
                            row["label"].as_str().unwrap_or("Unknown source")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_else(|| "No sources found".into()),
        "status" | "start" | "stop" => format!(
            "{} — {}",
            result["state"].as_str().unwrap_or("unknown"),
            result["detail"].as_str().unwrap_or("")
        ),
        "doctor" => serde_json::to_string_pretty(result).unwrap_or_else(|_| result.to_string()),
        _ => "Done".into(),
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let json_output = cli.json;
    let action = cli.action;
    if let Action::CaptureWindow {
        address,
        width,
        height,
        fps,
    } = action
    {
        let result = caster_capture_hyprland::stream_window(
            &address,
            caster_core::Quality {
                width,
                height,
                fps,
                bitrate: 0,
            },
        );
        return match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("{error:#}");
                ExitCode::FAILURE
            }
        };
    }
    if let Action::CaptureAudio { monitor } = action {
        return match caster_audio_pulse::stream_monitor(&monitor) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("{error:#}");
                ExitCode::FAILURE
            }
        };
    }
    let command = action.name();
    match run(action) {
        Ok(result) => {
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string(&caster_core::api::Response::success(result)).unwrap()
                );
            } else {
                println!("{}", human_output(command, &result));
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string(&caster_core::api::Response::<Value>::error(
                        "command_failed",
                        format!("{error:#}")
                    ))
                    .unwrap()
                );
            } else {
                eprintln!("Caster: {error:#}");
            }
            ExitCode::FAILURE
        }
    }
}
