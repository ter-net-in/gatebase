use crate::cli::ProxyCommand;
use anyhow::Result;
use gatebase_config::Config;

pub(crate) async fn run(command: ProxyCommand) -> Result<()> {
    match command {
        ProxyCommand::Postgres { config } => {
            gatebase_proxy_postgres::run(Config::load(config)?).await
        }
        ProxyCommand::Mysql { config } => gatebase_proxy_mysql::run(Config::load(config)?).await,
    }
}
