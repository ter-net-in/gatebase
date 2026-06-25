use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use std::env;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs;

const REPO: &str = "ter-net-in/gatebase";
const BIN: &str = "gatebase";

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
}

pub(crate) async fn run(version: Option<String>, force: bool) -> Result<()> {
    let current = env!("CARGO_PKG_VERSION");
    let version = match version {
        Some(version) => version.trim_start_matches('v').to_owned(),
        None => latest_version().await?,
    };
    if version == current && !force {
        println!("gatebase {current} already installed");
        return Ok(());
    }

    let triple = target_triple()?;
    let archive = format!("gatebase-{version}-{triple}.tar.gz");
    let url = format!("https://github.com/{REPO}/releases/download/v{version}/{archive}");
    let current_exe = env::current_exe().context("resolve current executable")?;
    let temp_dir = make_temp_dir()?;

    let result = install_update(&url, &archive, &temp_dir, &current_exe).await;
    let cleanup = fs::remove_dir_all(&temp_dir).await;
    result?;
    cleanup.ok();

    println!("updated gatebase {current} -> {version}");
    Ok(())
}

async fn latest_version() -> Result<String> {
    let release: GitHubRelease = reqwest::Client::new()
        .get(format!(
            "https://api.github.com/repos/{REPO}/releases/latest"
        ))
        .header(
            "user-agent",
            concat!("gatebase/", env!("CARGO_PKG_VERSION")),
        )
        .send()
        .await
        .context("fetch latest release")?
        .error_for_status()
        .context("fetch latest release")?
        .json()
        .await
        .context("parse latest release")?;
    Ok(release.tag_name.trim_start_matches('v').to_owned())
}

async fn install_update(
    url: &str,
    archive: &str,
    temp_dir: &Path,
    current_exe: &Path,
) -> Result<()> {
    let archive_path = temp_dir.join(archive);
    let bytes = reqwest::Client::new()
        .get(url)
        .header(
            "user-agent",
            concat!("gatebase/", env!("CARGO_PKG_VERSION")),
        )
        .send()
        .await
        .with_context(|| format!("download {url}"))?
        .error_for_status()
        .with_context(|| format!("download {url}"))?
        .bytes()
        .await
        .with_context(|| format!("download {url}"))?;
    fs::write(&archive_path, bytes)
        .await
        .with_context(|| format!("write {}", archive_path.display()))?;

    let status = Command::new("tar")
        .arg("-xzf")
        .arg(&archive_path)
        .arg("-C")
        .arg(temp_dir)
        .status()
        .context("run tar")?;
    if !status.success() {
        bail!("tar failed with status {status}");
    }

    let new_bin = temp_dir.join(BIN);
    replace_current_exe(&new_bin, current_exe).await
}

async fn replace_current_exe(new_bin: &Path, current_exe: &Path) -> Result<()> {
    match fs::copy(new_bin, current_exe).await {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == ErrorKind::PermissionDenied => {
            let status = Command::new("sudo")
                .arg("install")
                .arg("-m")
                .arg("0755")
                .arg(new_bin)
                .arg(current_exe)
                .status()
                .context("run sudo install")?;
            if status.success() {
                Ok(())
            } else {
                bail!("sudo install failed with status {status}")
            }
        }
        Err(error) => Err(error).with_context(|| format!("replace {}", current_exe.display())),
    }
}

fn make_temp_dir() -> Result<PathBuf> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock before unix epoch")?
        .as_millis();
    let dir = env::temp_dir().join(format!("gatebase-update-{}-{millis}", std::process::id()));
    std::fs::create_dir(&dir).with_context(|| format!("create {}", dir.display()))?;
    Ok(dir)
}

fn target_triple() -> Result<&'static str> {
    match (env::consts::OS, env::consts::ARCH) {
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-gnu"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        (os, arch) => Err(anyhow!("unsupported platform {os}/{arch}")),
    }
}
