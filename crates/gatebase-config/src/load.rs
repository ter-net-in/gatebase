use crate::Config;
use anyhow::{Context, Result};
use gatebase_core::{AccessSignal, DbEngine};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

impl Config {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        let config: Self = serde_yaml::from_str(&content)
            .with_context(|| format!("failed to parse config {}", path.display()))?;
        config.validate()?;
        config.ensure_state_directory()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(!self.targets.is_empty(), "at least one target is required");
        anyhow::ensure!(
            !self.audit.sinks.is_empty(),
            "at least one audit sink is required"
        );
        let mut repos = HashSet::new();
        for target in &self.targets {
            anyhow::ensure!(
                !target.access.github_repo.trim().is_empty(),
                "target {} access github_repo is required",
                target.name
            );
            anyhow::ensure!(
                repos.insert(target.access.github_repo.as_str()),
                "access github_repo {} is configured on multiple targets",
                target.access.github_repo
            );
            anyhow::ensure!(
                !target.access.required_signals.is_empty() || target.access.allow_cli_sessions,
                "target {} requires issue signals or allow_cli_sessions",
                target.name
            );
            for signal in &target.access.required_signals {
                match signal {
                    AccessSignal::GitHubIssueOpen => {}
                    AccessSignal::GitHubIssueLabels { labels } => anyhow::ensure!(
                        !labels.is_empty(),
                        "target {} github_issue_labels needs labels",
                        target.name
                    ),
                }
            }
        }
        if self
            .targets
            .iter()
            .any(|target| !target.access.required_signals.is_empty())
        {
            anyhow::ensure!(
                self.github.is_some(),
                "GitHub issue access requires github config"
            );
        }
        Ok(())
    }

    fn ensure_state_directory(&self) -> Result<()> {
        let Some(parent) = self.metadata.sqlite_path.parent() else {
            return Ok(());
        };
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create metadata directory {}", parent.display()))?;
        if parent == default_state_dir() {
            set_private_dir_permissions(parent)?;
        }
        Ok(())
    }

    pub fn postgres_targets(&self) -> impl Iterator<Item = &crate::TargetConfig> {
        self.targets
            .iter()
            .filter(|target| target.engine == DbEngine::Postgres)
    }
}

fn default_state_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".gatebase")
}

#[cfg(unix)]
fn set_private_dir_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let permissions = fs::Permissions::from_mode(0o700);
    fs::set_permissions(path, permissions)
        .with_context(|| format!("failed to set permissions on {}", path.display()))
}

#[cfg(not(unix))]
fn set_private_dir_permissions(_path: &Path) -> Result<()> {
    Ok(())
}
