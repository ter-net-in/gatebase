use crate::defaults::{default_broker_listen, default_fail_closed, default_github_api_base_url};
use gatebase_core::{AccessSignal, DbEngine};
use serde::Deserialize;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub metadata: MetadataConfig,
    pub sessions: SessionsConfig,
    pub github: Option<GitHubConfig>,
    #[serde(default)]
    pub access: AccessConfig,
    pub audit: AuditConfig,
    pub targets: Vec<TargetConfig>,
    pub policies: HashMap<String, PolicyConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub public_url: String,
    #[serde(default = "default_broker_listen")]
    pub broker_listen: SocketAddr,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MetadataConfig {
    pub sqlite_path: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionsConfig {
    pub default_ttl: String,
    pub max_ttl: String,
    pub signing_key_file: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubConfig {
    pub app_id: String,
    pub installation_id: i64,
    pub private_key_file: PathBuf,
    pub webhook_secret: String,
    #[serde(default = "default_github_api_base_url")]
    pub api_base_url: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AccessConfig {
    #[serde(default)]
    pub allowed_repositories: Vec<String>,
    #[serde(default)]
    pub required_signals: Vec<AccessSignal>,
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
pub struct TargetConfig {
    pub name: String,
    pub engine: DbEngine,
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
pub struct CredentialsConfig {
    pub username_env: String,
    pub password_env: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PolicyConfig {
    #[serde(default)]
    pub block: Vec<String>,
    #[serde(default)]
    pub require_where: Vec<String>,
    pub max_rows_changed: Option<u64>,
}
