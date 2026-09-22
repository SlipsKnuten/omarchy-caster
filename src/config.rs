//! Versioned preferences and explicit development-only binary overrides.

use crate::storage::{self, Directory};
use anyhow::{Context, Result};
pub use caster_core::Quality;
use serde::{Deserialize, Serialize};
use std::{env, path::PathBuf};

const CONFIG_VERSION: u32 = 1;
const SENDER_EXECUTABLE: &str = "caster-openscreen";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Preferences {
    #[serde(default)]
    pub receiver_id: String,
    #[serde(default)]
    pub source_kind: caster_core::SourceKind,
    #[serde(default)]
    pub audio: caster_core::AudioMode,
    #[serde(default)]
    pub quality: caster_core::QualityPreset,
    #[serde(default)]
    pub bitrate: caster_core::BitrateChoice,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            receiver_id: String::new(),
            source_kind: caster_core::SourceKind::Window,
            audio: caster_core::AudioMode::Off,
            quality: caster_core::QualityPreset::Auto,
            bitrate: caster_core::BitrateChoice::Auto,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct DevelopmentOverrides {
    #[serde(default)]
    pub sender: Option<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde(default = "config_version")]
    pub version: u32,
    #[serde(default)]
    pub preferences: Preferences,
    #[serde(default)]
    pub development: DevelopmentOverrides,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            preferences: Preferences::default(),
            development: DevelopmentOverrides::default(),
        }
    }
}

// The prototype stored sender at the top level. This compatibility shape is
// read-only and can disappear after the first stable migration.
#[derive(Deserialize)]
struct LegacyConfig {
    sender: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
pub struct Binaries {
    pub sender: PathBuf,
    pub sender_source: &'static str,
}

const fn config_version() -> u32 {
    CONFIG_VERSION
}

pub fn load() -> Result<Config> {
    let path = storage::directory(Directory::Config)?.join("config.json");
    if !path.exists() {
        return Ok(Config::default());
    }
    let config: Config = storage::read(&path)?;
    anyhow::ensure!(
        config.version == CONFIG_VERSION,
        "Unsupported configuration version {}",
        config.version
    );
    Ok(config)
}

pub fn save(config: &Config) -> Result<()> {
    anyhow::ensure!(
        config.version == CONFIG_VERSION,
        "Refusing to write an unsupported configuration version"
    );
    storage::write(
        &storage::directory(Directory::Config)?.join("config.json"),
        config,
    )
}

fn executable_on_path(name: &str) -> Option<PathBuf> {
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|directory| directory.join(name))
            .find(|path| path.is_file())
    })
}

fn checked_sender(path: PathBuf, source: &'static str) -> Result<Binaries> {
    anyhow::ensure!(
        path.is_absolute() && path.is_file(),
        "Open Screen sender is missing: {}",
        path.display()
    );
    Ok(Binaries {
        sender: path,
        sender_source: source,
    })
}

pub fn binaries() -> Result<Binaries> {
    if let Some(path) = env::var_os("CASTER_SENDER") {
        return checked_sender(PathBuf::from(path), "environment");
    }
    if let Some(path) = load()?.development.sender {
        return checked_sender(path, "development override");
    }
    if let Some(path) = executable_on_path(SENDER_EXECUTABLE) {
        return checked_sender(path, "PATH");
    }
    let legacy_path = storage::legacy_file(Directory::Config, "config.json")?;
    if legacy_path.is_file() {
        let legacy: LegacyConfig = storage::read(&legacy_path)
            .context("The legacy omacast-dev configuration is invalid")?;
        return checked_sender(legacy.sender, "legacy compatibility");
    }
    anyhow::bail!(
        "Open Screen sender not found; install {SENDER_EXECUTABLE} or set a development override"
    )
}

pub fn remember(request: &caster_core::Request) -> Result<()> {
    let mut config = load()?;
    config.preferences = Preferences {
        receiver_id: request.receiver.id.clone(),
        source_kind: request.source.kind,
        audio: request.audio,
        quality: request.quality,
        bitrate: request.bitrate,
    };
    save(&config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_do_not_persist_window_identity() {
        let value = serde_json::to_value(Config::default()).unwrap();
        assert!(value["preferences"].get("window_title").is_none());
        assert!(value["preferences"].get("source_id").is_none());
        assert_eq!(value["version"], CONFIG_VERSION);
    }
}
