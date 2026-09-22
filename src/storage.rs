//! XDG paths and atomic private state. No media is written to disk.
use anyhow::{Context, Result};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    env, fs,
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

pub const APP_DIRECTORY: &str = "caster";
const LEGACY_APP_DIRECTORY: &str = "omacast-dev";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Directory {
    Runtime,
    Config,
    State,
}

fn base_directory(kind: Directory) -> Result<PathBuf> {
    let home = PathBuf::from(env::var_os("HOME").context("HOME is not set")?);
    let (key, fallback) = match kind {
        Directory::Runtime => (
            "XDG_RUNTIME_DIR",
            PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() })),
        ),
        Directory::Config => ("XDG_CONFIG_HOME", home.join(".config")),
        Directory::State => ("XDG_STATE_HOME", home.join(".local/state")),
    };
    let base = env::var_os(key).map(PathBuf::from).unwrap_or(fallback);
    anyhow::ensure!(base.is_absolute(), "{key} must be absolute");
    Ok(base)
}

pub fn directory(kind: Directory) -> Result<PathBuf> {
    let path = base_directory(kind)?.join(APP_DIRECTORY);
    fs::create_dir_all(&path)?;
    let metadata = fs::symlink_metadata(&path)?;
    anyhow::ensure!(
        metadata.is_dir()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == unsafe { libc::getuid() },
        "Unsafe application directory"
    );
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    Ok(path)
}

pub fn legacy_file(kind: Directory, name: &str) -> Result<PathBuf> {
    anyhow::ensure!(
        !name.is_empty() && !name.contains('/') && name != "." && name != "..",
        "Invalid state filename"
    );
    Ok(base_directory(kind)?.join(LEGACY_APP_DIRECTORY).join(name))
}

pub fn read<T: DeserializeOwned>(path: &Path) -> Result<T> {
    serde_json::from_slice(&fs::read(path)?)
        .with_context(|| format!("Invalid state: {}", path.display()))
}

pub fn write<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    let result = (|| -> Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        serde_json::to_writer(&mut file, value)?;
        file.flush()?;
        file.sync_all()?;
        fs::rename(&tmp, path)?;
        Ok(())
    })();
    if tmp.exists() {
        let _ = fs::remove_file(tmp);
    }
    result
}
