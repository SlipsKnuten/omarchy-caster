//! XDG paths and atomic private state. No media is written to disk.
use anyhow::{Context, Result, bail};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    env, fs,
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

pub fn directory(kind: &str) -> Result<PathBuf> {
    let home = PathBuf::from(env::var_os("HOME").context("HOME is not set")?);
    let (key, fallback) = match kind {
        "runtime" => (
            "XDG_RUNTIME_DIR",
            PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() })),
        ),
        "config" => ("XDG_CONFIG_HOME", home.join(".config")),
        "state" => ("XDG_STATE_HOME", home.join(".local/state")),
        _ => bail!("Unknown XDG directory"),
    };
    let base = env::var_os(key).map(PathBuf::from).unwrap_or(fallback);
    anyhow::ensure!(base.is_absolute(), "{key} must be absolute");
    let path = base.join("omacast-dev");
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
