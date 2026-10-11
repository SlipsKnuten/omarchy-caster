//! Temporary 1920x1080 output mode for pixel-perfect screen casts. Restores the
//! previous mode from private runtime state, so it also works after a crash.
use crate::{capture::json_command, storage};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{process::Command, thread, time::Duration};

const CAST_WIDTH: u32 = 1920;
const CAST_HEIGHT: u32 = 1080;
const STATE_FILE: &str = "display.json";
/// Time for the compositor to apply a mode before capture starts.
const SETTLE: Duration = Duration::from_millis(1200);

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
struct Mode {
    output: String,
    width: u32,
    height: u32,
    refresh: f64,
    x: i64,
    y: i64,
    scale: f64,
}

impl Mode {
    /// Lua for Hyprland's `hl.monitor`. The output name is checked first because
    /// it is embedded in the code (a user-created output can carry any name).
    fn lua(&self) -> Result<String> {
        anyhow::ensure!(
            !self.output.is_empty()
                && self
                    .output
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "Unsupported output name"
        );
        Ok(format!(
            "hl.monitor({{ output = \"{}\", mode = \"{}x{}@{:.2}\", position = \"{}x{}\", scale = {} }})",
            self.output, self.width, self.height, self.refresh, self.x, self.y, self.scale
        ))
    }
}

fn parse(monitors: &serde_json::Value, output: &str) -> Option<(Mode, Option<f64>)> {
    let monitor = monitors
        .as_array()?
        .iter()
        .find(|m| m["name"].as_str() == Some(output))?;
    let current = Mode {
        output: output.to_owned(),
        width: monitor["width"].as_u64()? as u32,
        height: monitor["height"].as_u64()? as u32,
        refresh: monitor["refreshRate"].as_f64()?,
        x: monitor["x"].as_i64()?,
        y: monitor["y"].as_i64()?,
        scale: monitor["scale"].as_f64()?,
    };
    let prefix = format!("{CAST_WIDTH}x{CAST_HEIGHT}@");
    // Prefer the 60 Hz mode: that is the rate the cast pipeline captures at.
    let cast_refresh = monitor["availableModes"]
        .as_array()?
        .iter()
        .filter_map(|m| {
            m.as_str()?
                .strip_prefix(&prefix)?
                .strip_suffix("Hz")?
                .parse()
                .ok()
        })
        .min_by(|a: &f64, b: &f64| (a - 60.0).abs().total_cmp(&(b - 60.0).abs()));
    Some((current, cast_refresh))
}

fn monitors() -> Result<serde_json::Value> {
    json_command("hyprctl", &["monitors", "-j"])
}

fn apply(mode: &Mode) -> Result<()> {
    let output = Command::new("hyprctl")
        .args(["eval", &mode.lua()?])
        .output()
        .context("Cannot run hyprctl")?;
    anyhow::ensure!(
        output.status.success() && !String::from_utf8_lossy(&output.stdout).contains("error"),
        "Cannot change display mode: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    thread::sleep(SETTLE);
    Ok(())
}

/// Switches `output` to 1920x1080 for the cast. Returns whether anything changed.
/// The previous mode is stored before the change is made.
pub fn engage(output: &str) -> Result<bool> {
    let Some((current, cast_refresh)) = parse(&monitors()?, output) else {
        return Ok(false);
    };
    let Some(refresh) = cast_refresh else {
        return Ok(false); // Panel has no 1080p mode; the capture filter letterboxes.
    };
    if (current.width, current.height) == (CAST_WIDTH, CAST_HEIGHT) {
        return Ok(false);
    }
    storage::write(
        &storage::directory(storage::Directory::Runtime)?.join(STATE_FILE),
        &current,
    )?;
    let cast = Mode {
        width: CAST_WIDTH,
        height: CAST_HEIGHT,
        refresh,
        ..current
    };
    apply(&cast)?;
    Ok(true)
}

/// Restores a mode that Caster changed. Idempotent, and it only acts when the
/// output is still in the 1920x1080 mode Caster set.
pub fn recover() -> Result<()> {
    let path = storage::directory(storage::Directory::Runtime)?.join(STATE_FILE);
    if !path.exists() {
        return Ok(());
    }
    let previous: Mode = storage::read(&path)?;
    if let Some((current, _)) = parse(&monitors()?, &previous.output)
        && (current.width, current.height) == (CAST_WIDTH, CAST_HEIGHT)
    {
        apply(&previous)?;
    }
    std::fs::remove_file(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn monitors() -> serde_json::Value {
        json!([{
            "name": "eDP-1", "width": 1920, "height": 1200, "refreshRate": 60.003,
            "x": 0, "y": 0, "scale": 1.5,
            "availableModes": ["1920x1200@60.00Hz", "1920x1080@48.00Hz", "1920x1080@60.00Hz"]
        }])
    }

    #[test]
    fn picks_the_60_hz_1080p_mode() {
        let (current, refresh) = parse(&monitors(), "eDP-1").unwrap();
        assert_eq!((current.width, current.height), (1920, 1200));
        assert_eq!(refresh, Some(60.0));
    }

    #[test]
    fn lua_keeps_position_and_scale() {
        let (current, _) = parse(&monitors(), "eDP-1").unwrap();
        assert_eq!(
            current.lua().unwrap(),
            r#"hl.monitor({ output = "eDP-1", mode = "1920x1200@60.00", position = "0x0", scale = 1.5 })"#
        );
    }

    #[test]
    fn output_names_cannot_inject_lua() {
        let (mut current, _) = parse(&monitors(), "eDP-1").unwrap();
        current.output = r#"x" }) os.execute("id") --"#.into();
        assert!(current.lua().is_err());
    }

    #[test]
    fn unknown_output_or_missing_1080p_mode_is_left_alone() {
        assert!(parse(&monitors(), "HDMI-A-1").is_none());
        let mut no_1080 = monitors();
        no_1080[0]["availableModes"] = json!(["1920x1200@60.00Hz"]);
        assert_eq!(parse(&no_1080, "eDP-1").unwrap().1, None);
    }
}
