use crate::defaults::{
    default_active_connection_retention_days, default_approval_retention_days,
    default_audit_retention_days, default_broker_listen, default_fail_closed,
    default_github_api_base_url, default_metadata_url, default_rollback_max_rows,
    default_rollback_retention_days, default_session_retention_days,
};
use gatebase_core::{AccessSignal, DbEngine};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub admin: AdminConfig,
    #[serde(default)]
    pub metadata: MetadataConfig,
    pub sessions: SessionsConfig,
    pub github: Option<GitHubConfig>,
    pub audit: AuditConfig,
    #[serde(default)]
    pub rollback: RollbackConfig,
    #[serde(default)]
    pub retention: RetentionConfig,
    pub targets: Vec<TargetConfig>,
    pub policies: HashMap<String, PolicyConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdminConfig {
    pub signing_key_file: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub public_url: String,
    #[serde(default = "default_broker_listen")]
    pub broker_listen: SocketAddr,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MetadataBackend {
    Sqlite,
    Postgres,
}

impl Default for MetadataBackend {
    fn default() -> Self {
        Self::Sqlite
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct MetadataConfig {
    #[serde(default)]
    pub backend: MetadataBackend,
    #[serde(default = "default_metadata_url")]
    pub url: String,
}

impl Default for MetadataConfig {
    fn default() -> Self {
        Self {
            backend: MetadataBackend::Sqlite,
            url: default_metadata_url(),
        }
    }
}

impl MetadataConfig {
    #[must_use]
    pub fn effective_url(&self) -> String {
        self.url.clone()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionsConfig {
    pub default_ttl: String,
    pub max_ttl: String,
    pub signing_key_file: PathBuf,
}

impl SessionsConfig {
    pub fn default_ttl_minutes(&self) -> anyhow::Result<i64> {
        parse_minutes(&self.default_ttl)
    }

    pub fn max_ttl_minutes(&self) -> anyhow::Result<i64> {
        parse_minutes(&self.max_ttl)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubConfig {
    pub app_id: String,
    pub installation_id: i64,
    pub private_key_file: PathBuf,
    pub webhook_secret: SecretString,
    #[serde(default = "default_github_api_base_url")]
    pub api_base_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuditConfig {
    #[serde(default = "default_fail_closed")]
    pub fail_closed: bool,
    pub sinks: Vec<AuditSinkConfig>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuditSinkConfig {
    Sqlite,
    Jsonl { path: PathBuf },
}

#[derive(Debug, Clone, Deserialize)]
pub struct RollbackConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_rollback_max_rows")]
    pub max_rows: u64,
    #[serde(default)]
    pub sinks: Vec<RollbackSinkConfig>,
}

impl Default for RollbackConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_rows: default_rollback_max_rows(),
            sinks: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RollbackSinkConfig {
    Sqlite,
    Jsonl { path: PathBuf },
}

#[derive(Debug, Clone, Deserialize)]
pub struct RetentionConfig {
    #[serde(default = "default_audit_retention_days")]
    pub audit_days: u64,
    #[serde(default = "default_rollback_retention_days")]
    pub rollback_days: u64,
    #[serde(default = "default_session_retention_days")]
    pub session_days: u64,
    #[serde(default = "default_approval_retention_days")]
    pub approval_days: u64,
    #[serde(default = "default_active_connection_retention_days")]
    pub active_connection_days: u64,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            audit_days: default_audit_retention_days(),
            rollback_days: default_rollback_retention_days(),
            session_days: default_session_retention_days(),
            approval_days: default_approval_retention_days(),
            active_connection_days: default_active_connection_retention_days(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TargetConfig {
    pub name: String,
    pub engine: DbEngine,
    pub access: TargetAccessConfig,
    pub listen: SocketAddr,
    #[serde(default)]
    pub public_host: Option<String>,
    #[serde(default)]
    pub public_port: Option<u16>,
    pub upstream: String,
    pub database: String,
    pub credentials: CredentialsConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TargetAccessConfig {
    pub github_repo: String,
    #[serde(default = "default_access_token_ttl")]
    pub access_token_ttl: String,
    #[serde(default)]
    pub allow_cli_sessions: bool,
    #[serde(default)]
    pub required_signals: Vec<AccessSignal>,
}

impl TargetAccessConfig {
    pub fn access_token_ttl_minutes(&self) -> anyhow::Result<i64> {
        parse_minutes(&self.access_token_ttl)
    }
}

fn default_access_token_ttl() -> String {
    "5m".to_owned()
}

fn parse_minutes(value: &str) -> anyhow::Result<i64> {
    let minutes = value.strip_suffix('m').unwrap_or(value).parse::<i64>()?;
    anyhow::ensure!(minutes > 0, "duration must be positive: {value}");
    Ok(minutes)
}

#[derive(Debug, Clone, Deserialize)]
pub struct CredentialsConfig {
    pub username: String,
    pub password: SecretString,
}

impl CredentialsConfig {
    #[must_use]
    pub fn username(&self) -> &str {
        &self.username
    }

    #[must_use]
    pub fn password(&self) -> &str {
        self.password.expose_secret()
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PolicyConfig {
    #[serde(default)]
    pub block: Vec<String>,
    #[serde(default)]
    pub require_where: Vec<String>,
    pub max_rows_changed: Option<u64>,
}
