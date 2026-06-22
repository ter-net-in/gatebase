use crate::AuditSink;
use anyhow::Result;
use async_trait::async_trait;
use gatebase_core::AuditEvent;
use gatebase_metadata::MetadataStore;

pub struct SqliteAuditSink {
    metadata: MetadataStore,
}

impl SqliteAuditSink {
    pub async fn new(metadata: MetadataStore) -> Result<Self> {
        metadata.migrate().await?;
        Ok(Self { metadata })
    }
}

#[async_trait]
impl AuditSink for SqliteAuditSink {
    async fn write(&self, event: &AuditEvent) -> Result<()> {
        self.metadata.write_audit_event(event).await
    }
}
