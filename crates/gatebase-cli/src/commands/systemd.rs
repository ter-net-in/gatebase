use crate::cli::SystemdCommand;
use anyhow::{bail, Context, Result};
use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs;

const UNIT_DIR: &str = "/etc/systemd/system";
const SERVICES: &[Service] = &[
    Service {
        name: "gatebase-broker",
        description: "Gatebase broker",
        args: &["broker", "--config"],
        after: "network-online.target",
    },
    Service {
        name: "gatebase-proxy-postgres",
        description: "Gatebase Postgres proxy",
        args: &["proxy", "postgres", "--config"],
        after: "network-online.target gatebase-broker.service",
    },
    Service {
        name: "gatebase-proxy-mysql",
        description: "Gatebase MySQL proxy",
        args: &["proxy", "mysql", "--config"],
        after: "network-online.target gatebase-broker.service",
    },
];

struct Service {
    name: &'static str,
    description: &'static str,
    args: &'static [&'static str],
    after: &'static str,
}

pub(crate) async fn run(command: SystemdCommand) -> Result<()> {
    match command {
        SystemdCommand::Install {
            config,
            bin,
            enable,
            start,
        } => install(config, bin, enable, start).await,
    }
}

async fn install(config: PathBuf, bin: Option<PathBuf>, enable: bool, start: bool) -> Result<()> {
    let bin = bin.unwrap_or(env::current_exe().context("resolve current executable")?);
    let bin = bin
        .canonicalize()
        .with_context(|| format!("resolve {}", bin.display()))?;
    let config = config
        .canonicalize()
        .with_context(|| format!("resolve {}", config.display()))?;

    for service in SERVICES {
        let unit = unit_file(service, &bin, &config);
        let path = Path::new(UNIT_DIR).join(format!("{}.service", service.name));
        install_unit(&path, unit).await?;
        println!("installed {}", path.display());
    }

    systemctl(["daemon-reload"])?;
    let service_units: Vec<String> = SERVICES
        .iter()
        .map(|service| format!("{}.service", service.name))
        .collect();
    if enable && start {
        let mut args = vec!["enable".to_owned(), "--now".to_owned()];
        args.extend(service_units.clone());
        systemctl(args)?;
    } else {
        if enable {
            let mut args = vec!["enable".to_owned()];
            args.extend(service_units.clone());
            systemctl(args)?;
        }
        if start {
            let mut args = vec!["start".to_owned()];
            args.extend(service_units);
            systemctl(args)?;
        }
    }

    println!("systemd units ready");
    Ok(())
}

fn unit_file(service: &Service, bin: &Path, config: &Path) -> String {
    let exec_args = service.args.join(" ");
    format!(
        "[Unit]\n\
Description={}\n\
Wants=network-online.target\n\
After={}\n\n\
[Service]\n\
Type=simple\n\
ExecStart={} {} {}\n\
Restart=on-failure\n\
RestartSec=5s\n\n\
[Install]\n\
WantedBy=multi-user.target\n",
        service.description,
        service.after,
        systemd_escape_path(bin),
        exec_args,
        systemd_escape_path(config),
    )
}

async fn install_unit(path: &Path, content: String) -> Result<()> {
    let temp = temp_path(
        path.file_name()
            .unwrap_or_else(|| OsStr::new("gatebase.service")),
    )?;
    fs::write(&temp, content)
        .await
        .with_context(|| format!("write {}", temp.display()))?;
    let status = Command::new("sudo")
        .arg("install")
        .arg("-m")
        .arg("0644")
        .arg(&temp)
        .arg(path)
        .status()
        .context("run sudo install")?;
    fs::remove_file(&temp).await.ok();
    if status.success() {
        Ok(())
    } else {
        bail!("sudo install failed with status {status}")
    }
}

fn systemctl<I, S>(args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let status = Command::new("sudo")
        .arg("systemctl")
        .args(args)
        .status()
        .context("run sudo systemctl")?;
    if status.success() {
        Ok(())
    } else {
        bail!("systemctl failed with status {status}")
    }
}

fn temp_path(name: &OsStr) -> Result<PathBuf> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock before unix epoch")?
        .as_millis();
    Ok(env::temp_dir().join(format!(
        "gatebase-{}-{millis}-{}",
        std::process::id(),
        name.to_string_lossy()
    )))
}

fn systemd_escape_path(path: &Path) -> String {
    path.display().to_string().replace(' ', "\\x20")
}
