use anyhow::Result;
use chrono::Utc;
use gatebase_core::{AccessToken, ActiveConnection, AuditEvent, Session, SessionId};
use gatebase_metadata::{AuditEventFilter, MetadataStore, PruneCutoffs, PruneResult};
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct SessionStore {
    metadata: MetadataStore,
}

impl SessionStore {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let metadata = MetadataStore::open_sqlite(path).await?;
        Ok(Self { metadata })
    }

    pub async fn migrate(&self) -> Result<()> {
        self.metadata.migrate().await
    }

    pub async fn create(&self, session: &Session) -> Result<()> {
        self.metadata.create_session(session).await
    }

    pub async fn revoke(&self, session_id: &SessionId) -> Result<()> {
        self.metadata
            .revoke_session(session_id, Utc::now().to_rfc3339())
            .await
    }

    pub async fn get(&self, session_id: &SessionId) -> Result<Option<Session>> {
        self.metadata.get_session(session_id).await
    }

    pub async fn list(&self) -> Result<Vec<Session>> {
        self.metadata.list_sessions().await
    }

    pub async fn create_access_token(&self, token: &AccessToken) -> Result<()> {
        self.metadata.create_access_token(token).await
    }

    pub async fn find_active_access_token(
        &self,
        repo: &str,
        issue: i64,
        target: &str,
    ) -> Result<Option<AccessToken>> {
        self.metadata
            .find_active_access_token(repo, issue, target, Utc::now().to_rfc3339())
            .await
    }

    pub async fn consume_access_token(&self, token: &str) -> Result<Option<AccessToken>> {
        self.metadata
            .consume_access_token(&hash_access_token(token), Utc::now().to_rfc3339())
            .await
    }

    pub async fn create_active_connection(&self, connection: &ActiveConnection) -> Result<()> {
        self.metadata.create_active_connection(connection).await
    }

    pub async fn close_active_connection(&self, id: &str) -> Result<()> {
        self.metadata
            .close_active_connection(id, Utc::now().to_rfc3339())
            .await
    }

    pub async fn list_active_connections(&self) -> Result<Vec<ActiveConnection>> {
        self.metadata.list_active_connections().await
    }

    pub async fn list_audit_events(&self, filter: AuditEventFilter) -> Result<Vec<AuditEvent>> {
        self.metadata.list_audit_events(filter).await
    }

    pub async fn prune(&self, cutoffs: &PruneCutoffs, dry_run: bool) -> Result<PruneResult> {
        self.metadata.prune(cutoffs, dry_run).await
    }

    #[must_use]
    pub fn metadata(&self) -> &MetadataStore {
        &self.metadata
    }
}

#[must_use]
pub fn hash_access_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}
