use anyhow::Result;
use gatebase_config::TargetConfig;
use tokio::net::TcpStream;

pub async fn connect_upstream(target: &TargetConfig) -> Result<TcpStream> {
    let stream = TcpStream::connect(&target.upstream).await?;
    Ok(stream)
}
