mod access;
mod config;
mod proxy;
mod session;

use crate::cli::{Cli, Command};
use anyhow::Result;

pub(crate) async fn dispatch(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Broker { config } => {
            gatebase_broker::run(gatebase_config::Config::load(config)?).await
        }
        Command::Proxy { command } => proxy::run(command).await,
        Command::Config { command } => config::run(command).await,
        Command::Session { command } => session::run(command).await,
        Command::Access { command } => access::run(command).await,
    }
}
