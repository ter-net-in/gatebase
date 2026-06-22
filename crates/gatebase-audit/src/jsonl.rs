use crate::AuditSink;
use anyhow::Result;
use async_trait::async_trait;
use gatebase_core::AuditEvent;
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;

pub struct JsonlAuditSink {
    path: PathBuf,
}

impl JsonlAuditSink {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

#[async_trait]
impl AuditSink for JsonlAuditSink {
    async fn write(&self, event: &AuditEvent) -> Result<()> {
        let line = serde_json::to_string(event)?;
        if let Some(parent) = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await?;
        file.write_all(line.as_bytes()).await?;
        file.write_all(b"\n").await?;
        Ok(())
    }
}
