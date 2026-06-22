use crate::cli::ConfigCommand;
use anyhow::Result;
use gatebase_config::Config;

pub(crate) async fn run(command: ConfigCommand) -> Result<()> {
    match command {
        ConfigCommand::Check { config } => {
            Config::load(config)?;
            println!("config ok");
            Ok(())
        }
    }
}
