use anyhow::{Context, Result};
use gatebase_config::TargetConfig;
use std::env;

pub(crate) fn upstream_config(target: &TargetConfig) -> Result<String> {
    let username = env::var(&target.credentials.username_env)
        .with_context(|| format!("missing {}", target.credentials.username_env))?;
    let password = env::var(&target.credentials.password_env)
        .with_context(|| format!("missing {}", target.credentials.password_env))?;
    let (host, port) = split_host_port(&target.upstream);
    let mut config = format!(
        "host={} dbname={} user={} password={}",
        host, target.database, username, password
    );
    if let Some(port) = port {
        config.push_str(&format!(" port={port}"));
    }
    Ok(config)
}

fn split_host_port(upstream: &str) -> (&str, Option<u16>) {
    upstream
        .rsplit_once(':')
        .and_then(|(host, port)| port.parse().ok().map(|port| (host, Some(port))))
        .unwrap_or((upstream, None))
}
