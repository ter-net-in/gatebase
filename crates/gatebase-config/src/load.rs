use crate::Config;
use crate::MetadataBackend;
use anyhow::{Context, Result};
use gatebase_core::{AccessSignal, DbEngine};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

impl Config {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        let mut value: serde_yaml::Value = serde_yaml::from_str(&content)
            .with_context(|| format!("failed to parse config {}", path.display()))?;
        expand_env_refs_in_value(&mut value)?;
        let config: Self = serde_yaml::from_value(value)
            .with_context(|| format!("failed to parse config {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(!self.targets.is_empty(), "at least one target is required");
        anyhow::ensure!(
            !self.audit.sinks.is_empty(),
            "at least one audit sink is required"
        );
        let metadata_url = self.metadata.effective_url();
        match self.metadata.backend {
            MetadataBackend::Sqlite => anyhow::ensure!(
                metadata_url.starts_with("sqlite://"),
                "metadata backend sqlite requires url starting with sqlite://"
            ),
            MetadataBackend::Postgres => anyhow::ensure!(
                metadata_url.starts_with("postgres://")
                    || metadata_url.starts_with("postgresql://"),
                "metadata backend postgres requires url starting with postgres:// or postgresql://"
            ),
        }
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

    pub fn postgres_targets(&self) -> impl Iterator<Item = &crate::TargetConfig> {
        self.targets
            .iter()
            .filter(|target| target.engine == DbEngine::Postgres)
    }
}

fn expand_env_refs(input: &str) -> Result<String> {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("${") {
        output.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find('}') else {
            anyhow::bail!("unterminated environment reference in config");
        };
        let name = &after_start[..end];
        anyhow::ensure!(!name.is_empty(), "empty environment reference in config");
        let value = std::env::var(name)
            .with_context(|| format!("missing environment variable {name} referenced by config"))?;
        anyhow::ensure!(
            !value.is_empty(),
            "environment variable {name} referenced by config is empty"
        );
        output.push_str(&value);
        rest = &after_start[end + 1..];
    }
    output.push_str(rest);
    Ok(output)
}

fn expand_env_refs_in_value(value: &mut serde_yaml::Value) -> Result<()> {
    match value {
        serde_yaml::Value::String(string) => {
            *string = expand_env_refs(string)?;
        }
        serde_yaml::Value::Sequence(values) => {
            for value in values {
                expand_env_refs_in_value(value)?;
            }
        }
        serde_yaml::Value::Mapping(values) => {
            for value in values.values_mut() {
                expand_env_refs_in_value(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}
