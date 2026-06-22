use anyhow::Result;
use async_trait::async_trait;
use gatebase_core::AuditEvent;

#[async_trait]
pub trait AuditSink: Send + Sync {
    async fn write(&self, event: &AuditEvent) -> Result<()>;
}
