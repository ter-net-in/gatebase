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
    Audit {
        #[command(subcommand)]
        command: AuditCommand,
    },
    Maintenance {
        #[command(subcommand)]
        command: MaintenanceCommand,
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
        token: String,
    },
    CreateLocal {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        target: String,
        #[arg(long)]
        actor: String,
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
pub(crate) enum AuditCommand {
    List {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        broker: Option<String>,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        target: Option<String>,
        #[arg(long)]
        decision: Option<String>,
        #[arg(long, default_value_t = 100)]
        limit: u64,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum MaintenanceCommand {
    Prune {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
}
