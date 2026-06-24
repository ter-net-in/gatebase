use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct CliSettings {
    pub(crate) broker: Option<String>,
}

pub(crate) fn load() -> Result<CliSettings> {
    let path = path()?;
    if !path.exists() {
        return Ok(CliSettings::default());
    }
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

pub(crate) fn save(settings: &CliSettings) -> Result<PathBuf> {
    let path = path()?;
    let parent = path
        .parent()
        .context("settings path does not have parent directory")?;
    std::fs::create_dir_all(parent)?;
    std::fs::write(&path, serde_json::to_vec_pretty(settings)?)?;
    Ok(path)
}

pub(crate) fn broker(explicit: Option<String>) -> Result<String> {
    explicit
        .or(load()?.broker)
        .context("provide --broker or run gatebase config --broker <url>")
}

pub(crate) fn broker_or_localhost(explicit: Option<String>) -> Result<String> {
    Ok(explicit
        .or(load()?.broker)
        .unwrap_or_else(|| "http://127.0.0.1:8080".to_owned()))
}

fn path() -> Result<PathBuf> {
    Ok(
        PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?)
            .join(".config")
            .join("gatebase")
            .join("config.json"),
    )
}
