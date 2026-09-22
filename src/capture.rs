//! Compositor-specific capture adapter. Surface capture never falls back to a screen crop.
use crate::config::Quality;
use anyhow::{Result, bail};
pub use caster_core::{Source, SourceKind as Kind};
use std::process::Command;

pub fn json_command(program: &str, args: &[&str]) -> Result<serde_json::Value> {
    caster_capture_hyprland::json_command(program, args)
}
pub fn process_identity(pid: u32) -> Option<String> {
    caster_capture_hyprland::process_identity(pid)
}
pub fn sources() -> Result<Vec<Source>> {
    caster_capture_hyprland::enumerate_sources()
}
pub fn validate(selected: &Source) -> Result<()> {
    caster_capture_hyprland::validate_source(selected)
}
pub fn command(source: &Source, q: Quality) -> Result<Command> {
    match source.kind {
        Kind::Window => {
            let mut cmd = Command::new(std::env::current_exe()?);
            cmd.args([
                "capture-window".into(),
                "--address".into(),
                source.name.clone(),
                "--width".into(),
                q.width.to_string(),
                "--height".into(),
                q.height.to_string(),
                "--fps".into(),
                q.fps.to_string(),
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
        let cmd = command(&source, Quality::default()).unwrap();
        assert_eq!(cmd.get_program(), std::env::current_exe().unwrap());
        let args: Vec<_> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(
            args,
            [
                "capture-window",
                "--address",
                "0x1234",
                "--width",
                "1920",
                "--height",
                "1080",
                "--fps",
                "30"
            ]
        );
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
        let cmd = command(&source, Quality::default()).unwrap();
        assert_eq!(cmd.get_args().last().unwrap(), "odd;name");
    }
}
