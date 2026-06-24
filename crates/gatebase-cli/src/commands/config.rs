use crate::cli::{ConfigArgs, ConfigCommand};
use crate::settings::{save, CliSettings};
use anyhow::Result;
use gatebase_config::Config;
use std::path::PathBuf;

pub(crate) async fn run(args: ConfigArgs) -> Result<()> {
    if let Some(broker) = args.broker {
        let path = save(&CliSettings {
            broker: Some(broker.clone()),
        })?;
        println!("broker {broker}");
        println!("saved {}", path.display());
        return Ok(());
    }
    match args.command {
        Some(ConfigCommand::Check { config }) => verify(config).await,
        None => anyhow::bail!("provide --broker <url> or a config subcommand"),
    }
}

pub(crate) async fn verify(config: PathBuf) -> Result<()> {
    Config::load(config)?;
    println!("config ok");
    Ok(())
}
