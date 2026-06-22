use anyhow::Result;
use gatebase_config::TargetConfig;
use tokio::net::TcpListener;

pub async fn listen(target: &TargetConfig) -> Result<TcpListener> {
    let listener = TcpListener::bind(target.listen).await?;
    tracing::info!(target = %target.name, listen = %target.listen, "proxy listening");
    Ok(listener)
}
