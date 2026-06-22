use anyhow::Result;
use chrono::Utc;
use gatebase_core::{AccessApproval, ActiveConnection, Session, SessionId};
use gatebase_metadata::MetadataStore;
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

    pub async fn create_access_approval(&self, approval: &AccessApproval) -> Result<()> {
        self.metadata.create_access_approval(approval).await
    }

    pub async fn find_active_access_approval(
        &self,
        repo: &str,
        pull_request: Option<i64>,
        target: &str,
        actor: &str,
        approvers: &[String],
    ) -> Result<Option<AccessApproval>> {
        self.metadata
            .find_active_access_approval(
                repo,
                pull_request,
                target,
                actor,
                approvers,
                Utc::now().to_rfc3339(),
            )
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

    #[must_use]
    pub fn metadata(&self) -> &MetadataStore {
        &self.metadata
    }
}
