//! Stable process boundary to the tested C++ Open Screen implementation.
use crate::{config::Quality, discovery::Receiver};
use anyhow::Result;
use caster_core::adapters::TransportBackend;
use std::{net::IpAddr, path::Path, process::Command};

pub struct OpenScreenTransport;

impl TransportBackend for OpenScreenTransport {
    fn id(&self) -> &'static str {
        "openscreen-cast"
    }

    fn supports(&self, receiver: &Receiver) -> bool {
        receiver.port > 0 && receiver.host.parse::<IpAddr>().is_ok()
    }

    fn validate_quality(&self, quality: Quality) -> caster_core::adapters::AdapterResult<()> {
        if quality.width <= 1920
            && quality.height <= 1080
            && (1..=60).contains(&quality.fps)
            && (1_000_000..=8_000_000).contains(&quality.bitrate)
        {
            Ok(())
        } else {
            Err(caster_core::adapters::AdapterError::new(
                "transport.quality",
                "Google Cast v1 supports up to 1920x1080 at 30 fps and 8 Mbps",
            ))
        }
    }
}

pub fn command(
    binary: &Path,
    receiver: &Receiver,
    q: Quality,
    audio_fd: Option<i32>,
    event_fd: i32,
) -> Result<Command> {
    let address: IpAddr = receiver.host.parse()?;
    anyhow::ensure!(receiver.port > 0, "Invalid receiver port");
    let endpoint = std::net::SocketAddr::new(address, receiver.port).to_string();
    let mut cmd = Command::new(binary);
    cmd.env("CASTER_FPS", q.fps.to_string());
    cmd.args(["-n", "-c", "h264", "-m", &q.bitrate.to_string()]);
    if let Some(fd) = audio_fd {
        cmd.args(["--audio-input", &format!("pipe:{fd}")]);
    }
    anyhow::ensure!(event_fd >= 3, "Invalid transport event descriptor");
    cmd.args(["--event-fd", &event_fd.to_string()]);
    cmd.args([endpoint, "pipe:0".to_owned()]);
    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn receiver(host: &str) -> Receiver {
        Receiver {
            id: "id".into(),
            name: "name;not-a-command".into(),
            host: host.into(),
            port: 8009,
            model: String::new(),
        }
    }
    #[test]
    fn validates_network_address() {
        assert!(
            command(
                Path::new("/sender"),
                &receiver("host;command"),
                Quality::default(),
                None,
                8,
            )
            .is_err()
        );
    }
    #[test]
    fn audio_fd_and_ipv6_are_explicit_arguments() {
        let command = command(
            Path::new("/sender"),
            &receiver("::1"),
            Quality::default(),
            Some(7),
            8,
        )
        .unwrap();
        let args: Vec<_> = command.get_args().map(|s| s.to_str().unwrap()).collect();
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--audio-input", "pipe:7"])
        );
        assert!(args.contains(&"[::1]:8009"));
        assert!(args.windows(2).any(|pair| pair == ["--event-fd", "8"]));
        assert!(!args.contains(&"name;not-a-command"));
    }
}
