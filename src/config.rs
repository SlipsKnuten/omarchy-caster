use crate::storage;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Quality {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate: u32,
}
impl Default for Quality {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 30,
            bitrate: 6_000_000,
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct Binaries {
    pub sender: PathBuf,
    pub window_capture: PathBuf,
}
pub fn binaries() -> Result<Binaries> {
    let config: Binaries = storage::read(&storage::directory("config")?.join("config.json"))
        .context(
            "Configure sender and window_capture paths in ~/.config/omacast-dev/config.json",
        )?;
    for path in [&config.sender, &config.window_capture] {
        anyhow::ensure!(
            path.is_absolute() && path.is_file(),
            "Native binary missing: {}",
            path.display()
        );
    }
    Ok(config)
}
