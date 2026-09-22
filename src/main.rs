mod audio;
mod capture;
mod config;
mod discovery;
mod session;
mod storage;
mod transport;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use std::{
    fs::OpenOptions,
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    process::{Command, ExitCode},
};

const UNIT: &str = "omacast-dev.service";
const API_VERSION: u32 = 1;
#[derive(Parser)]
#[command(version, about = "Experimental native Cast controller")]
struct Cli {
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
        #[arg(long, value_enum, default_value = "off")]
        audio: audio::Mode,
    },
    #[command(hide = true)]
    Worker,
    #[command(hide = true)]
    Recover,
}
fn active(unit: &str) -> bool {
    Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", unit])
        .status()
        .is_ok_and(|s| s.success())
}
fn status() -> Result<session::Status> {
    let path = storage::directory("runtime")?.join("status.json");
    let mut state: session::Status = if path.exists() {
        storage::read(&path)?
    } else {
        session::Status::default()
    };
    if !active(UNIT) && matches!(state.state.as_str(), "connecting" | "streaming") {
        state.state = "error".into();
        state.detail = "Session ended unexpectedly; start again to recover".into();
    }
    Ok(state)
}
fn run(action: Action) -> Result<Value> {
    let root = storage::directory("runtime")?;
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
        Action::Doctor => Ok(
            json!({"binaries":config::binaries()?,"sources":capture::sources()?.len(),"controller":"Rust"}),
        ),
        Action::Recover => {
            audio::recover()?;
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
                let _ = Command::new("systemctl")
                    .args(["--user", "stop", UNIT])
                    .output()?;
                audio::recover()?;
                return Ok(json!(status()?));
            }
            let Action::Start {
                device,
                source,
                audio,
            } = action
            else {
                unreachable!()
            };
            anyhow::ensure!(
                !active(UNIT) && !active("fluxcast-live.service"),
                "Another native cast is active; stop it first"
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
            };
            session::validate_audio(&request)?;
            config::binaries()?;
            audio::recover()?;
            storage::write(&root.join("request.json"), &request)?;
            storage::write(
                &root.join("status.json"),
                &session::Status {
                    state: "connecting".into(),
                    detail: "Starting native Cast".into(),
                    ..session::Status::default()
                },
            )?;
            let exe = std::env::current_exe()?;
            let path = exe.to_str().context("Executable path is not UTF-8")?;
            anyhow::ensure!(
                !path.contains(['\n', '\r', '%', '"', '\\']),
                "Unsupported executable path"
            );
            let cleanup = format!("--property=ExecStopPost=\"{path}\" recover");
            let result = Command::new("systemd-run")
                .args([
                    "--user",
                    "--unit=omacast-dev",
                    "--collect",
                    "--property=TimeoutStopSec=15",
                    "--property=KillMode=mixed",
                    "--property=UMask=0077",
                    &cleanup,
                    path,
                    "worker",
                ])
                .output()?;
            anyhow::ensure!(
                result.status.success(),
                "Cannot start service: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            Ok(json!(status()?))
        }
    }
}
fn main() -> ExitCode {
    match run(Cli::parse().action) {
        Ok(result) => {
            println!("{}", json!({"api":API_VERSION,"ok":true,"result":result}));
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!(
                "{}",
                json!({"api":API_VERSION,"ok":false,"error":format!("{error:#}")})
            );
            ExitCode::FAILURE
        }
    }
}
