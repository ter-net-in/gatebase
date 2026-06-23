use std::net::SocketAddr;
use std::path::PathBuf;

pub(crate) fn default_broker_listen() -> SocketAddr {
    "127.0.0.1:8080"
        .parse()
        .expect("valid default broker listen")
}

pub(crate) fn default_fail_closed() -> bool {
    true
}

pub(crate) fn default_github_api_base_url() -> String {
    "https://api.github.com".to_owned()
}

pub(crate) fn default_rollback_max_rows() -> u64 {
    100
}

pub(crate) fn default_audit_retention_days() -> u64 {
    90
}

pub(crate) fn default_rollback_retention_days() -> u64 {
    30
}

pub(crate) fn default_session_retention_days() -> u64 {
    30
}

pub(crate) fn default_approval_retention_days() -> u64 {
    30
}

pub(crate) fn default_active_connection_retention_days() -> u64 {
    7
}

pub(crate) fn default_sqlite_path() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".gatebase")
        .join("gatebase.db")
}
