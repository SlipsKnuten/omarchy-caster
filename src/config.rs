use crate::storage;
use anyhow::{Context, Result};
pub use caster_core::Quality;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
