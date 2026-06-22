use crate::Config;
use anyhow::{Context, Result};
use gatebase_core::DbEngine;
use std::fs;
use std::path::Path;

impl Config {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        let config: Self = serde_yaml::from_str(&content)
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
        Ok(())
    }

    pub fn postgres_targets(&self) -> impl Iterator<Item = &crate::TargetConfig> {
        self.targets
            .iter()
            .filter(|target| target.engine == DbEngine::Postgres)
    }
}
