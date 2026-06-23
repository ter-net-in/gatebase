use crate::audit::build_sinks;
use crate::connection::{handle_connection, ConnectionParams};
use crate::rollback::build_rollback_sinks;
use anyhow::{Context, Result};
use gatebase_config::Config;
use gatebase_core::DbEngine;
use gatebase_proxy_core::listen;
use gatebase_session::{SessionIssuer, SessionStore};
use std::fs;

pub async fn run(config: Config) -> Result<()> {
    let signing_secret = fs::read(&config.sessions.signing_key_file).with_context(|| {
        format!(
            "failed to read signing key {}",
            config.sessions.signing_key_file.display()
        )
    })?;
    let store = SessionStore::open(&config.metadata.sqlite_path).await?;
    let issuer = SessionIssuer::new(&signing_secret);
    let sinks = build_sinks(&config, &store).await?;
    let rollback_sinks = build_rollback_sinks(&config.rollback, &store).await?;

    for target in config
        .targets
        .iter()
        .filter(|target| target.engine == DbEngine::Mysql)
    {
        let listener = listen(target).await?;
        let target = target.clone();
        let policy = config.policies.get("default").cloned().unwrap_or_default();
        let sinks = sinks.clone();
        let rollback = config.rollback.clone();
        let rollback_sinks = rollback_sinks.clone();
        let store = store.clone();
        let issuer = issuer.clone();
        let fail_closed = config.audit.fail_closed;
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        let target = target.clone();
                        let policy = policy.clone();
                        let sinks = sinks.clone();
                        let rollback = rollback.clone();
                        let rollback_sinks = rollback_sinks.clone();
                        let store = store.clone();
                        let issuer = issuer.clone();
                        tokio::spawn(async move {
                            if let Err(error) = handle_connection(
                                stream,
                                ConnectionParams {
                                    target,
                                    policy,
                                    sinks,
                                    rollback,
                                    rollback_sinks,
                                    store,
                                    issuer,
                                    fail_closed,
                                },
                            )
                            .await
                            {
                                tracing::warn!(%addr, %error, "connection failed");
                            }
                        });
                    }
                    Err(error) => tracing::error!(%error, "accept failed"),
                }
            }
        });
    }

    tokio::signal::ctrl_c().await?;
    Ok(())
}
