use anyhow::Result;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use chrono::Utc;
use gatebase_config::MetadataConfig;
use gatebase_core::{
    AccessToken, ActiveConnection, AuditEvent, RollbackArtifact, Session, SessionId, User, UserRole,
};
use gatebase_metadata::{
    ActivityEntry, AuditEventFilter, MetadataStore, PruneCutoffs, PruneResult,
};
use rand_core::OsRng;
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

    pub async fn open_metadata(config: &MetadataConfig) -> Result<Self> {
        let metadata = MetadataStore::open(&config.effective_url()).await?;
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

    pub async fn list(&self, limit: Option<u64>, offset: Option<u64>) -> Result<Vec<Session>> {
        self.metadata.list_sessions(limit, offset).await
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

    pub async fn list_active_connections(
        &self,
        limit: Option<u64>,
        offset: Option<u64>,
    ) -> Result<Vec<ActiveConnection>> {
        self.metadata.list_active_connections(limit, offset).await
    }

    pub async fn list_audit_events(&self, filter: AuditEventFilter) -> Result<Vec<AuditEvent>> {
        self.metadata.list_audit_events(filter).await
    }

    pub async fn list_rollback_artifacts(
        &self,
        limit: Option<u64>,
        offset: Option<u64>,
    ) -> Result<Vec<RollbackArtifact>> {
        self.metadata.list_rollback_artifacts(limit, offset).await
    }

    pub async fn find_audit_event(&self, id: &str) -> Result<Option<AuditEvent>> {
        self.metadata.find_audit_event(id).await
    }

    pub async fn list_activity(
        &self,
        limit: Option<u64>,
        offset: Option<u64>,
    ) -> Result<Vec<ActivityEntry>> {
        self.metadata.list_activity(limit, offset).await
    }

    pub async fn find_rollback_artifact(&self, id: &str) -> Result<Option<RollbackArtifact>> {
        self.metadata.find_rollback_artifact(id).await
    }

    pub async fn prune(&self, cutoffs: &PruneCutoffs, dry_run: bool) -> Result<PruneResult> {
        self.metadata.prune(cutoffs, dry_run).await
    }

    pub async fn create_user(
        &self,
        username: String,
        password: &str,
        role: UserRole,
    ) -> Result<User> {
        let user = User {
            id: format!("usr_{}", uuid::Uuid::new_v4().simple()),
            username,
            password_hash: hash_password(password)?,
            role,
            created_at: Utc::now(),
            disabled_at: None,
        };
        self.metadata.create_user(&user).await?;
        Ok(user)
    }

    pub async fn list_users(&self, limit: Option<u64>, offset: Option<u64>) -> Result<Vec<User>> {
        self.metadata.list_users(limit, offset).await
    }

    pub async fn find_user_by_username(&self, username: &str) -> Result<Option<User>> {
        self.metadata.find_user_by_username(username).await
    }

    pub async fn count_users(&self) -> Result<u64> {
        self.metadata.count_users().await
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

pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?
        .to_string();
    Ok(hash)
}

pub fn verify_password(password: &str, password_hash: &str) -> Result<bool> {
    let parsed_hash =
        PasswordHash::new(password_hash).map_err(|error| anyhow::anyhow!(error.to_string()))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}
