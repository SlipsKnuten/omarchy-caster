//! Typed systemd user-service boundary for the on-demand session worker.

use anyhow::{Context, Result};
use caster_core::{SessionState, Status, adapters::SessionSupervisor};
use std::{path::Path, process::Command};

const UNIT: &str = caster_core::constants::SYSTEMD_UNIT;

pub struct SystemdSupervisor;

pub fn active() -> bool {
    Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", UNIT])
        .status()
        .is_ok_and(|status| status.success())
}

pub fn start(executable: &Path) -> Result<()> {
    let path = executable
        .to_str()
        .context("Executable path is not valid UTF-8")?;
    anyhow::ensure!(
        !path.contains(['\n', '\r', '%', '"', '\\']),
        "Unsupported executable path"
    );
    let cleanup = format!("--property=ExecStopPost=\"{path}\" recover");
    let result = Command::new("systemd-run")
        .args([
            "--user",
            "--unit=caster-session",
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
        "Cannot start session worker: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(())
}

pub fn stop() -> Result<()> {
    if !active() {
        return Ok(());
    }
    let result = Command::new("systemctl")
        .args(["--user", "stop", UNIT])
        .output()?;
    anyhow::ensure!(
        result.status.success(),
        "Cannot stop session worker: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(())
}

impl SessionSupervisor for SystemdSupervisor {
    fn id(&self) -> &'static str {
        "systemd-user"
    }

    fn is_active(&self) -> caster_core::adapters::AdapterResult<bool> {
        Ok(active())
    }

    fn stop(&self) -> caster_core::adapters::AdapterResult<Status> {
        stop().map_err(|error| {
            caster_core::adapters::AdapterError::new("supervisor.stop", format!("{error:#}"))
        })?;
        Ok(Status {
            state: SessionState::Idle,
            detail: "Stopped".into(),
            ..Status::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_name_is_a_service() {
        assert_eq!(UNIT, "caster-session.service");
    }
}
