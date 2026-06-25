use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "gatebase",
    version,
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
        #[command(flatten)]
        args: ConfigArgs,
    },
    Login {
        #[arg(long)]
        broker: Option<String>,
        #[arg(long)]
        username: String,
        #[arg(long)]
        password_stdin: bool,
    },
    Ui {
        #[arg(long)]
        broker: Option<String>,
        #[arg(long)]
        admin_token: Option<String>,
        #[arg(long)]
        port: Option<u16>,
        #[arg(long)]
        no_open: bool,
    },
    Update {
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        force: bool,
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
    Admin {
        #[command(subcommand)]
        command: AdminCommand,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum AdminCommand {
    User {
        #[command(subcommand)]
        command: AdminUserCommand,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum AdminUserCommand {
    Create {
        #[arg(long, conflicts_with = "broker")]
        config: Option<PathBuf>,
        #[arg(long, conflicts_with = "config")]
        broker: Option<String>,
        #[arg(long)]
        admin_token: Option<String>,
        #[arg(long)]
        admin_username: Option<String>,
        #[arg(long)]
        admin_password_stdin: bool,
        #[arg(long)]
        username: String,
        #[arg(long)]
        role: String,
        #[arg(long)]
        password_stdin: bool,
    },
    List {
        #[arg(long, conflicts_with = "broker")]
        config: Option<PathBuf>,
        #[arg(long, conflicts_with = "config")]
        broker: Option<String>,
        #[arg(long)]
        admin_token: Option<String>,
        #[arg(long)]
        admin_username: Option<String>,
        #[arg(long)]
        admin_password_stdin: bool,
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

#[derive(Debug, Args)]
pub(crate) struct ConfigArgs {
    #[arg(long)]
    pub(crate) broker: Option<String>,
    #[command(subcommand)]
    pub(crate) command: Option<ConfigCommand>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum SessionCommand {
    Create {
        #[arg(long)]
        broker: Option<String>,
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
        #[arg(long, conflicts_with = "broker")]
        config: Option<PathBuf>,
        #[arg(long, conflicts_with = "config")]
        broker: Option<String>,
        #[arg(long)]
        admin_token: Option<String>,
    },
    Revoke {
        #[arg(long, conflicts_with = "broker")]
        config: Option<PathBuf>,
        #[arg(long, conflicts_with = "config")]
        broker: Option<String>,
        #[arg(long)]
        admin_token: Option<String>,
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
        admin_token: Option<String>,
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
        #[arg(long, conflicts_with = "broker")]
        config: Option<PathBuf>,
        #[arg(long, conflicts_with = "config")]
        broker: Option<String>,
        #[arg(long)]
        admin_token: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
}
