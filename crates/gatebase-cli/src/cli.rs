use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "gatebase",
    about = "Approved, temporary access to production databases"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    Broker {
        #[arg(long)]
        config: PathBuf,
    },
    Proxy {
        #[command(subcommand)]
        command: ProxyCommand,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    Session {
        #[command(subcommand)]
        command: SessionCommand,
    },
    Access {
        #[command(subcommand)]
        command: AccessCommand,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum ProxyCommand {
    Postgres {
        #[arg(long)]
        config: PathBuf,
    },
    Mysql {
        #[arg(long)]
        config: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum ConfigCommand {
    Check {
        #[arg(long)]
        config: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum SessionCommand {
    Create {
        #[arg(long, default_value = "http://127.0.0.1:8080")]
        broker: String,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        repo: String,
        #[arg(long)]
        pull_request: Option<i64>,
        #[arg(long)]
        target: String,
    },
    List {
        #[arg(long)]
        config: PathBuf,
    },
    Revoke {
        #[arg(long)]
        config: PathBuf,
        id: String,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum AccessCommand {
    Approve {
        #[arg(long, default_value = "http://127.0.0.1:8080")]
        broker: String,
        #[arg(long)]
        repo: String,
        #[arg(long)]
        pull_request: Option<i64>,
        #[arg(long)]
        target: String,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        approver: String,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        ttl_minutes: Option<i64>,
    },
}
