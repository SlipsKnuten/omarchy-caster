//! Static v1 backend registry. Feature selection happens at compile time;
//! runtime checks only describe whether compiled adapters are usable here.

use crate::{audio, config, discovery, supervisor, transport};
use caster_core::{
    BackendInfo, BackendKind, CapabilityReport,
    adapters::{
        AudioBackend, CaptureBackend, ReceiverDiscovery, SessionSupervisor, TransportBackend,
    },
};
use std::{env, process::Command};

fn command_works(program: &str, arguments: &[&str]) -> bool {
    Command::new(program)
        .args(arguments)
        .output()
        .is_ok_and(|output| output.status.success())
}

pub fn report() -> CapabilityReport {
    let receiver_backend = discovery::GoogleCastDiscovery;
    let capture_backend = caster_capture_hyprland::HyprlandCapture;
    let audio_backend = audio::PulseAudioAdapter;
    let transport_backend = transport::OpenScreenTransport;
    let supervisor_backend = supervisor::SystemdSupervisor;
    let sender = config::binaries();
    let wayland = env::var_os("WAYLAND_DISPLAY").is_some();
    let hyprland = env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some();
    let hyprctl = command_works("hyprctl", &["version", "-j"]);
    let pactl = command_works("pactl", &["info"]);
    let systemd = command_works("systemctl", &["--user", "show-environment"]);
    CapabilityReport {
        backends: vec![
            BackendInfo {
                id: receiver_backend.id().into(),
                kind: BackendKind::ReceiverDiscovery,
                compiled: cfg!(feature = "google-cast"),
                available: cfg!(feature = "google-cast"),
                detail: "Rust mDNS/DNS-SD discovery".into(),
                capabilities: vec!["google-cast".into(), "ipv4".into()],
            },
            BackendInfo {
                id: capture_backend.id().into(),
                kind: BackendKind::Capture,
                compiled: cfg!(feature = "hyprland"),
                available: cfg!(feature = "hyprland") && wayland && hyprland && hyprctl,
                detail: if !wayland || !hyprland {
                    "Hyprland Wayland session not detected".into()
                } else if !hyprctl {
                    "hyprctl cannot reach the compositor".into()
                } else {
                    "Rust window capture; wf-recorder screen compatibility path".into()
                },
                capabilities: capture_backend.capabilities().capture,
            },
            BackendInfo {
                id: audio_backend.id().into(),
                kind: BackendKind::Audio,
                compiled: cfg!(feature = "pulse-audio"),
                available: cfg!(feature = "pulse-audio") && pactl,
                detail: if pactl {
                    "Pulse-compatible PipeWire session reachable".into()
                } else {
                    "pactl cannot reach an audio server".into()
                },
                capabilities: vec!["application".into(), "system".into(), "stereo-48khz".into()],
            },
            BackendInfo {
                id: transport_backend.id().into(),
                kind: BackendKind::Transport,
                compiled: cfg!(feature = "google-cast"),
                available: cfg!(feature = "google-cast") && sender.is_ok(),
                detail: match sender {
                    Ok(binary) => format!(
                        "Open Screen sender from {} ({})",
                        binary.sender_source,
                        binary.sender.display()
                    ),
                    Err(error) => format!("{error:#}"),
                },
                capabilities: vec!["h264".into(), "opus".into(), "1080p30".into()],
            },
            BackendInfo {
                id: supervisor_backend.id().into(),
                kind: BackendKind::Supervisor,
                compiled: cfg!(feature = "systemd"),
                available: cfg!(feature = "systemd") && systemd,
                detail: if systemd {
                    "User service manager reachable".into()
                } else {
                    "systemd user manager unavailable".into()
                },
                capabilities: vec!["transient-session".into(), "bounded-stop".into()],
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_ids_are_unique() {
        let report = report();
        let mut ids: Vec<_> = report.backends.iter().map(|backend| &backend.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), report.backends.len());
    }

    #[test]
    fn registry_has_every_extension_kind() {
        let report = report();
        for kind in [
            BackendKind::ReceiverDiscovery,
            BackendKind::Capture,
            BackendKind::Audio,
            BackendKind::Transport,
            BackendKind::Supervisor,
        ] {
            assert!(report.backends.iter().any(|backend| backend.kind == kind));
        }
    }
}
