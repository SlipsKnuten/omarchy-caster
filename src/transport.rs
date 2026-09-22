//! Stable process boundary to the tested C++ Open Screen implementation.
use crate::{config::Quality, discovery::Receiver};
use anyhow::Result;
use std::{net::IpAddr, path::Path, process::Command};

pub fn command(
    binary: &Path,
    receiver: &Receiver,
    q: Quality,
    audio_fd: Option<i32>,
) -> Result<Command> {
    let address: IpAddr = receiver.host.parse()?;
    anyhow::ensure!(receiver.port > 0, "Invalid receiver port");
    let endpoint = std::net::SocketAddr::new(address, receiver.port).to_string();
    let mut cmd = Command::new(binary);
    cmd.args(["-n", "-c", "h264", "-m", &q.bitrate.to_string()]);
    if let Some(fd) = audio_fd {
        cmd.args(["--audio-input", &format!("pipe:{fd}")]);
    }
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
                None
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
        )
        .unwrap();
        let args: Vec<_> = command.get_args().map(|s| s.to_str().unwrap()).collect();
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--audio-input", "pipe:7"])
        );
        assert!(args.contains(&"[::1]:8009"));
        assert!(!args.contains(&"name;not-a-command"));
    }
}
