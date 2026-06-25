use anyhow::Result;
use gatebase_config::TargetConfig;
use mysql_async::OptsBuilder;

pub(crate) fn upstream_opts(target: &TargetConfig) -> Result<OptsBuilder> {
    let username = target.credentials.username();
    let password = target.credentials.password();
    let (host, port) = split_host_port(&target.upstream);
    let mut builder = OptsBuilder::default()
        .ip_or_hostname(host.to_owned())
        .user(Some(username.to_owned()))
        .pass(Some(password.to_owned()))
        .db_name(Some(target.database.clone()));
    if let Some(port) = port {
        builder = builder.tcp_port(port);
    }
    Ok(builder)
}

fn split_host_port(upstream: &str) -> (&str, Option<u16>) {
    upstream
        .rsplit_once(':')
        .and_then(|(host, port)| port.parse().ok().map(|port| (host, Some(port))))
        .unwrap_or((upstream, None))
}
