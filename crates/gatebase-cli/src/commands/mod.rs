mod admin;
mod audit;
mod config;
mod maintenance;
mod proxy;
mod session;
mod ui;

use crate::cli::{Cli, Command};
use anyhow::Result;

pub(crate) async fn dispatch(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Broker { config } => {
            gatebase_broker::run(gatebase_config::Config::load(config)?).await
        }
        Command::Proxy { command } => proxy::run(command).await,
        Command::Config { args } => config::run(args).await,
        Command::Login {
            broker,
            username,
            password_stdin,
        } => admin::login(broker, username, password_stdin).await,
        Command::Ui {
            broker,
            admin_token,
            port,
            no_open,
        } => ui::run(broker, admin_token, port, no_open).await,
        Command::Session { command } => session::run(command).await,
        Command::Audit { command } => audit::run(command).await,
        Command::Maintenance { command } => maintenance::run(command).await,
        Command::Admin { command } => admin::run(command).await,
    }
}
