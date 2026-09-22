//! Rust Hyprland capture adapter.

pub mod media;
mod protocol;
pub mod window;

use anyhow::{Context, Result};
use caster_core::{Capabilities, Quality, Source, adapters::CaptureBackend};
use std::{
    fs,
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant},
};

pub struct HyprlandCapture;

impl CaptureBackend for HyprlandCapture {
    fn id(&self) -> &'static str {
        "hyprland"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            capture: vec!["window".into(), "screen".into()],
            ..Capabilities::default()
        }
    }

    fn sources(&self) -> caster_core::adapters::AdapterResult<Vec<Source>> {
        enumerate_sources().map_err(|error| {
            caster_core::adapters::AdapterError::new("capture.discovery", format!("{error:#}"))
        })
    }

    fn validate(&self, source: &Source) -> caster_core::adapters::AdapterResult<()> {
        validate_source(source).map_err(|error| {
            caster_core::adapters::AdapterError::new("capture.source_changed", format!("{error:#}"))
        })
    }
}

pub fn json_command(program: &str, args: &[&str]) -> Result<serde_json::Value> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("Cannot run {program}"))?;
    anyhow::ensure!(
        output.status.success(),
        "{program}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

pub fn process_identity(pid: u32) -> Option<String> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let (_, fields) = stat.rsplit_once(')')?;
    Some(format!("{}:{}", pid, fields.split_whitespace().nth(19)?))
}

pub fn enumerate_sources() -> Result<Vec<Source>> {
    use caster_core::SourceKind;

    let monitors = json_command("hyprctl", &["monitors", "-j"])?;
    let windows = json_command("hyprctl", &["clients", "-j"])?;
    let mut result = Vec::new();
    for row in monitors.as_array().context("Malformed monitor list")? {
        let name = row["name"]
            .as_str()
            .context("Monitor has no name")?
            .to_owned();
        result.push(Source {
            id: format!("screen:{name}"),
            kind: SourceKind::Screen,
            label: format!("Screen · {name}"),
            name,
            pid: 0,
            identity: String::new(),
            app: String::new(),
        });
    }
    for row in windows.as_array().context("Malformed window list")? {
        if row["mapped"].as_bool() == Some(false) {
            continue;
        }
        let Some(pid) = row["pid"].as_u64().and_then(|pid| u32::try_from(pid).ok()) else {
            continue;
        };
        let Some(identity) = process_identity(pid) else {
            continue;
        };
        let name = row["address"]
            .as_str()
            .context("Window has no address")?
            .to_owned();
        let app = row["class"].as_str().unwrap_or("Unknown").to_owned();
        let title = row["title"].as_str().unwrap_or("");
        result.push(Source {
            id: format!("window:{name}"),
            kind: SourceKind::Window,
            label: format!("{app} · {title}"),
            name,
            pid,
            identity,
            app,
        });
    }
    Ok(result)
}

pub(crate) fn window_handle(address: &str) -> Result<u32> {
    Ok(u64::from_str_radix(address.trim_start_matches("0x"), 16)? as u32)
}

pub fn validate_source(selected: &Source) -> Result<()> {
    use caster_core::SourceKind;

    let current = enumerate_sources()?;
    anyhow::ensure!(
        current
            .iter()
            .any(|source| source.id == selected.id && source.identity == selected.identity),
        "Selected source closed or changed; choose it again"
    );
    if selected.kind == SourceKind::Window {
        let id = window_handle(&selected.name)?;
        anyhow::ensure!(
            current
                .iter()
                .filter(|source| {
                    source.kind == SourceKind::Window
                        && window_handle(&source.name).ok() == Some(id)
                })
                .count()
                == 1,
            "Ambiguous window handle; capture refused"
        );
    }
    Ok(())
}

/// Capture one Hyprland toplevel and write a paced raw I420/NUT stream.
///
/// This is intentionally a library entry point so alternative supervisors can
/// reuse the backend without duplicating its media pipeline.
pub fn stream_window(address: &str, quality: Quality) -> Result<()> {
    anyhow::ensure!(quality.fps > 0, "Frame rate must be positive");
    let mut capture = window::WindowCapturer::connect(address)?;
    let mut output = media::NutVideoWriter::open(
        Path::new("pipe:1"),
        quality.width,
        quality.height,
        quality.fps,
    )?;
    let epoch = Instant::now();
    let frame_interval = Duration::from_secs_f64(1.0 / f64::from(output.fps()));
    let mut frame_number = 0_u32;
    loop {
        output.write_frame(&capture.next_frame()?)?;
        frame_number = frame_number.wrapping_add(1);
        let deadline = epoch + frame_interval.mul_f64(f64::from(frame_number));
        if let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            thread::sleep(remaining);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_identity_includes_start_time() {
        let pid = std::process::id();
        assert!(
            process_identity(pid)
                .unwrap()
                .starts_with(&format!("{pid}:"))
        );
    }

    #[test]
    fn window_handles_are_data_not_commands() {
        assert_eq!(window_handle("0x1234").unwrap(), 0x1234);
        assert!(window_handle("0x12;command").is_err());
    }
}
