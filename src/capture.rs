//! Compositor-specific capture adapter. Surface capture never falls back to a screen crop.
use crate::config::Quality;
use anyhow::{Context, Result, bail};
pub use caster_core::{Source, SourceKind as Kind};
use std::{fs, path::Path, process::Command};
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
pub fn sources() -> Result<Vec<Source>> {
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
            kind: Kind::Screen,
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
        let Some(pid) = row["pid"].as_u64().and_then(|p| u32::try_from(p).ok()) else {
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
            kind: Kind::Window,
            label: format!("{app} · {title}"),
            name,
            pid,
            identity,
            app,
        });
    }
    Ok(result)
}
fn handle(address: &str) -> Result<u32> {
    Ok(u64::from_str_radix(address.trim_start_matches("0x"), 16)? as u32)
}
pub fn validate(selected: &Source) -> Result<()> {
    let current = sources()?;
    anyhow::ensure!(
        current
            .iter()
            .any(|s| s.id == selected.id && s.identity == selected.identity),
        "Selected source closed or changed; choose it again"
    );
    if selected.kind == Kind::Window {
        let id = handle(&selected.name)?;
        anyhow::ensure!(
            current
                .iter()
                .filter(|s| s.kind == Kind::Window && handle(&s.name).ok() == Some(id))
                .count()
                == 1,
            "Ambiguous window handle; capture refused"
        );
    }
    Ok(())
}
pub fn command(source: &Source, q: Quality, helper: &Path) -> Result<Command> {
    match source.kind {
        Kind::Window => {
            handle(&source.name)?;
            let mut cmd = Command::new(helper);
            cmd.args([
                source.name.clone(),
                q.width.to_string(),
                q.height.to_string(),
            ]);
            Ok(cmd)
        }
        Kind::Screen => {
            if source.name.is_empty() {
                bail!("Empty monitor");
            }
            let canvas = format!(
                "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2,setsar=1",
                q.width, q.height, q.width, q.height
            );
            let mut cmd = Command::new("wf-recorder");
            cmd.args([
                "-y",
                "-D",
                "-r",
                &q.fps.to_string(),
                "-c",
                "rawvideo",
                "-m",
                "nut",
                "-x",
                "yuv420p",
                "-F",
                &canvas,
                "-f",
                "pipe:1",
                "-o",
                &source.name,
            ]);
            Ok(cmd)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_uses_start_time() {
        let pid = std::process::id();
        assert!(
            process_identity(pid)
                .unwrap()
                .starts_with(&format!("{pid}:"))
        );
        assert!(process_identity(u32::MAX).is_none());
    }
    #[test]
    fn window_uses_surface_helper_not_screen_crop() {
        let source = Source {
            id: "window:0x1234".into(),
            kind: Kind::Window,
            name: "0x1234".into(),
            label: "private title".into(),
            pid: 1,
            identity: "1:2".into(),
            app: "app".into(),
        };
        let cmd = command(&source, Quality::default(), Path::new("/window-helper")).unwrap();
        assert_eq!(cmd.get_program(), "/window-helper");
        let args: Vec<_> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(args, ["0x1234", "1920", "1080"]);
    }
    #[test]
    fn monitor_name_is_not_shell_interpolated() {
        let source = Source {
            id: "screen:odd".into(),
            kind: Kind::Screen,
            name: "odd;name".into(),
            label: String::new(),
            pid: 0,
            identity: String::new(),
            app: String::new(),
        };
        let cmd = command(&source, Quality::default(), Path::new("/unused")).unwrap();
        assert_eq!(cmd.get_args().last().unwrap(), "odd;name");
    }
}
