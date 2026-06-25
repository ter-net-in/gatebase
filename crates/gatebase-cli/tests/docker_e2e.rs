use anyhow::Context;
use mysql_async::prelude::Queryable;
use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;

const POSTGRES_IMAGE: &str = "postgres:16-alpine";
const MYSQL_IMAGE: &str = "mysql:8.0";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn docker_postgres_and_mysql_proxy_e2e() -> anyhow::Result<()> {
    if std::env::var("GATEBASE_DOCKER_TESTS").as_deref() != Ok("1") {
        eprintln!("skipping Docker E2E test; set GATEBASE_DOCKER_TESTS=1 to run it");
        return Ok(());
    }

    require_docker()?;

    let temp = TempDir::new("gatebase-e2e")?;
    let pg_upstream_port = free_port()?;
    let mysql_upstream_port = free_port()?;
    let broker_port = free_port()?;
    let pg_proxy_port = free_port()?;
    let mysql_proxy_port = free_port()?;

    let suffix = unique_suffix();
    let _postgres = DockerContainer::run(
        format!("gatebase-e2e-pg-{suffix}"),
        vec![
            "run".to_owned(),
            "--rm".to_owned(),
            "-d".to_owned(),
            "--name".to_owned(),
            format!("gatebase-e2e-pg-{suffix}"),
            "-e".to_owned(),
            "POSTGRES_USER=app".to_owned(),
            "-e".to_owned(),
            "POSTGRES_PASSWORD=secret".to_owned(),
            "-e".to_owned(),
            "POSTGRES_DB=app".to_owned(),
            "-p".to_owned(),
            format!("{pg_upstream_port}:5432"),
            POSTGRES_IMAGE.to_owned(),
        ],
    )?;
    let _mysql = DockerContainer::run(
        format!("gatebase-e2e-mysql-{suffix}"),
        vec![
            "run".to_owned(),
            "--rm".to_owned(),
            "-d".to_owned(),
            "--name".to_owned(),
            format!("gatebase-e2e-mysql-{suffix}"),
            "-e".to_owned(),
            "MYSQL_ROOT_PASSWORD=root-secret".to_owned(),
            "-e".to_owned(),
            "MYSQL_DATABASE=app".to_owned(),
            "-e".to_owned(),
            "MYSQL_USER=app".to_owned(),
            "-e".to_owned(),
            "MYSQL_PASSWORD=secret".to_owned(),
            "-p".to_owned(),
            format!("{mysql_upstream_port}:3306"),
            MYSQL_IMAGE.to_owned(),
            "--default-authentication-plugin=mysql_native_password".to_owned(),
        ],
    )?;

    wait_for_postgres(pg_upstream_port).await?;
    wait_for_mysql(mysql_upstream_port).await?;

    fs::write(temp.path().join("session.key"), "test-signing-secret")?;
    let config_path = write_config(
        temp.path(),
        broker_port,
        pg_proxy_port,
        mysql_proxy_port,
        pg_upstream_port,
        mysql_upstream_port,
    )?;

    let bin = gatebase_bin();
    let mut broker = ChildProcess::spawn(
        &bin,
        ["broker", "--config", config_path.to_str().unwrap()],
        &[
            ("PG_UPSTREAM_USER", "app"),
            ("PG_UPSTREAM_PASSWORD", "secret"),
            ("MYSQL_UPSTREAM_USER", "app"),
            ("MYSQL_UPSTREAM_PASSWORD", "secret"),
        ],
    )?;
    wait_for_http_health(broker_port).await?;

    let mut pg_proxy = ChildProcess::spawn(
        &bin,
        [
            "proxy",
            "postgres",
            "--config",
            config_path.to_str().unwrap(),
        ],
        &[
            ("PG_UPSTREAM_USER", "app"),
            ("PG_UPSTREAM_PASSWORD", "secret"),
            ("MYSQL_UPSTREAM_USER", "app"),
            ("MYSQL_UPSTREAM_PASSWORD", "secret"),
        ],
    )?;
    let mut mysql_proxy = ChildProcess::spawn(
        &bin,
        ["proxy", "mysql", "--config", config_path.to_str().unwrap()],
        &[
            ("PG_UPSTREAM_USER", "app"),
            ("PG_UPSTREAM_PASSWORD", "secret"),
            ("MYSQL_UPSTREAM_USER", "app"),
            ("MYSQL_UPSTREAM_PASSWORD", "secret"),
        ],
    )?;
    wait_for_tcp(SocketAddr::from(([127, 0, 0, 1], pg_proxy_port))).await?;
    wait_for_tcp(SocketAddr::from(([127, 0, 0, 1], mysql_proxy_port))).await?;

    exercise_postgres_proxy(&bin, &config_path, broker_port, pg_proxy_port)
        .await
        .context("Postgres proxy E2E failed")?;
    exercise_mysql_proxy(&bin, &config_path, mysql_proxy_port, mysql_upstream_port)
        .await
        .context("MySQL proxy E2E failed")?;

    let audit_response = http_request(
        broker_port,
        "GET /api/audit/events?target=prod-pg&limit=20 HTTP/1.1",
        None,
    )?;
    assert!(
        audit_response.contains("SELECT 1"),
        "broker audit API should return audit events"
    );

    let audit_cli = Command::new(&bin)
        .args([
            "audit",
            "list",
            "--broker",
            &format!("http://127.0.0.1:{broker_port}"),
            "--target",
            "prod-pg",
            "--limit",
            "20",
            "--json",
        ])
        .output()?;
    anyhow::ensure!(
        audit_cli.status.success(),
        "audit CLI failed: {}",
        String::from_utf8_lossy(&audit_cli.stderr)
    );
    assert!(
        String::from_utf8_lossy(&audit_cli.stdout).contains("SELECT 1"),
        "audit CLI should print broker audit events"
    );

    broker.kill();
    pg_proxy.kill();
    mysql_proxy.kill();

    let audit = fs::read_to_string(temp.path().join("audit.jsonl"))?;
    assert!(
        audit.contains("SELECT 1"),
        "audit should contain allowed Postgres query"
    );
    assert!(
        audit.contains("SELECT 2"),
        "audit should contain allowed MySQL query"
    );
    assert!(
        audit.contains("DROP TABLE"),
        "audit should contain blocked query"
    );
    let rollback = fs::read_to_string(temp.path().join("rollback.jsonl"))?;
    assert!(
        rollback.contains("gatebase_rollback_e2e"),
        "rollback artifacts should contain covered Postgres table"
    );
    assert!(
        rollback.contains("gatebase_mysql_rollback_e2e"),
        "rollback artifacts should contain covered MySQL table"
    );
    assert!(
        rollback.contains("before"),
        "rollback artifacts should contain Postgres before-image row"
    );
    assert!(
        rollback.contains("mysql-before"),
        "rollback artifacts should contain MySQL before-image row"
    );
    assert!(
        rollback.contains("inverse_sql"),
        "rollback artifacts should contain inverse SQL"
    );

    Ok(())
}

async fn exercise_postgres_proxy(
    bin: &Path,
    config_path: &Path,
    broker_port: u16,
    proxy_port: u16,
) -> anyhow::Result<()> {
    let session = create_session(bin, config_path, "prod-pg").context("create Postgres session")?;
    let config = format!(
        "host=127.0.0.1 port={proxy_port} user=alice password={} dbname=app sslmode=disable",
        session.token
    );
    let (client, connection) = tokio_postgres::connect(&config, tokio_postgres::NoTls)
        .await
        .context("connect tokio-postgres through Gatebase")?;
    tokio::spawn(async move {
        let _ = connection.await;
    });

    let rows = client
        .simple_query("SELECT 1")
        .await
        .context("run Postgres SELECT 1 through Gatebase")?;
    assert!(rows.iter().any(|message| matches!(
        message,
        tokio_postgres::SimpleQueryMessage::Row(row) if row.get(0) == Some("1")
    )));

    let row = client
        .query_one("SELECT 'extended-ok'", &[])
        .await
        .context("run extended Postgres SELECT through Gatebase")?;
    let value: String = row.get(0);
    assert_eq!(value, "extended-ok");

    let extended_blocked = client.execute("DROP TABLE definitely_blocked", &[]).await;
    assert!(
        extended_blocked.is_err(),
        "blocked extended Postgres SQL must return an error"
    );

    let blocked = client.simple_query("DROP TABLE definitely_blocked").await;
    assert!(
        blocked.is_err(),
        "blocked Postgres SQL must return an error"
    );

    client
        .simple_query("CREATE TABLE gatebase_rollback_e2e (id integer PRIMARY KEY, name text)")
        .await
        .context("create rollback E2E table")?;
    client
        .simple_query("INSERT INTO gatebase_rollback_e2e (id, name) VALUES (1, 'before')")
        .await
        .context("insert rollback E2E row")?;
    client
        .simple_query("UPDATE gatebase_rollback_e2e SET name = 'after' WHERE id IN (1)")
        .await
        .context("run rollback-covered Postgres update")?;
    client
        .simple_query("DELETE FROM gatebase_rollback_e2e WHERE id IN (1)")
        .await
        .context("run rollback-covered Postgres delete")?;

    revoke_session(broker_port, &session.id)?;
    sleep(Duration::from_secs(2)).await;
    let revoked = client.simple_query("SELECT 1").await;
    assert!(
        revoked.is_err(),
        "revoked Postgres session must disconnect active connection"
    );
    Ok(())
}

async fn exercise_mysql_proxy(
    bin: &Path,
    config_path: &Path,
    proxy_port: u16,
    upstream_port: u16,
) -> anyhow::Result<()> {
    let upstream_opts = mysql_async::OptsBuilder::default()
        .ip_or_hostname("127.0.0.1")
        .tcp_port(upstream_port)
        .user(Some("app"))
        .pass(Some("secret"))
        .db_name(Some("app"));
    let mut upstream = mysql_async::Conn::new(upstream_opts)
        .await
        .context("connect direct MySQL upstream for rollback setup")?;
    upstream
        .query_drop("CREATE TABLE gatebase_mysql_rollback_e2e (id integer PRIMARY KEY, name text)")
        .await
        .context("create MySQL rollback E2E table")?;
    upstream
        .query_drop("INSERT INTO gatebase_mysql_rollback_e2e (id, name) VALUES (1, 'mysql-before')")
        .await
        .context("insert MySQL rollback E2E row")?;
    upstream.disconnect().await?;

    let session = create_session(bin, config_path, "prod-mysql").context("create MySQL session")?;
    let opts = mysql_async::OptsBuilder::default()
        .ip_or_hostname("127.0.0.1")
        .tcp_port(proxy_port)
        .user(Some("alice"))
        .pass(Some(session.token))
        .db_name(Some("app"))
        .enable_cleartext_plugin(true);
    let mut conn = mysql_async::Conn::new(opts)
        .await
        .context("connect mysql_async through Gatebase")?;
    let value: Option<u8> = conn
        .query_first("SELECT 2")
        .await
        .context("run MySQL SELECT 2 through Gatebase")?;
    assert_eq!(value, Some(2));

    conn.query_drop("UPDATE gatebase_mysql_rollback_e2e SET name = 'mysql-after' WHERE id IN (1)")
        .await
        .context("run rollback-covered MySQL update")?;
    conn.query_drop("DELETE FROM gatebase_mysql_rollback_e2e WHERE id IN (1)")
        .await
        .context("run rollback-covered MySQL delete")?;
    let blocked = conn.query_drop("DROP TABLE definitely_blocked").await;
    assert!(blocked.is_err(), "blocked MySQL SQL must return an error");
    conn.disconnect().await?;
    Ok(())
}

struct CreatedSession {
    id: String,
    token: String,
}

fn create_session(bin: &Path, config_path: &Path, target: &str) -> anyhow::Result<CreatedSession> {
    let output = Command::new(bin)
        .args([
            "session",
            "create-local",
            "--config",
            config_path.to_str().unwrap(),
            "--target",
            target,
            "--actor",
            "alice",
        ])
        .output()
        .context("run gatebase session create-local")?;
    anyhow::ensure!(
        output.status.success(),
        "create-local failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout)?;
    let connection_string = stdout
        .lines()
        .find_map(|line| line.strip_prefix("connection_string "))
        .ok_or_else(|| anyhow::anyhow!("missing connection_string in create-local output"))?;
    let id = stdout
        .lines()
        .find_map(|line| line.strip_prefix("session_id "))
        .ok_or_else(|| anyhow::anyhow!("missing session_id in create-local output"))?
        .to_owned();
    let token = connection_string
        .split_once("//")
        .and_then(|(_, rest)| rest.split_once(':'))
        .and_then(|(_, rest)| rest.split_once('@'))
        .map(|(password, _)| password.to_owned())
        .ok_or_else(|| anyhow::anyhow!("failed to extract token from connection string"))?;
    Ok(CreatedSession { id, token })
}

fn revoke_session(broker_port: u16, session_id: &str) -> anyhow::Result<()> {
    let response = raw_http_request(
        broker_port,
        &format!("POST /api/sessions/{session_id}/revoke HTTP/1.1"),
        None,
    )?;
    let (head, _) = split_http_response(&response)?;
    anyhow::ensure!(head.contains(" 204 "), "unexpected HTTP response: {head}");
    Ok(())
}

async fn wait_for_postgres(port: u16) -> anyhow::Result<()> {
    let config = format!("host=127.0.0.1 port={port} user=app password=secret dbname=app");
    retry("Postgres", || async {
        let (client, connection) = tokio_postgres::connect(&config, tokio_postgres::NoTls).await?;
        tokio::spawn(async move {
            let _ = connection.await;
        });
        client.simple_query("SELECT 1").await?;
        Ok(())
    })
    .await
}

async fn wait_for_mysql(port: u16) -> anyhow::Result<()> {
    retry("MySQL", || async {
        let opts = mysql_async::OptsBuilder::default()
            .ip_or_hostname("127.0.0.1")
            .tcp_port(port)
            .user(Some("app"))
            .pass(Some("secret"))
            .db_name(Some("app"));
        let mut conn = mysql_async::Conn::new(opts).await?;
        conn.query_drop("SELECT 1").await?;
        conn.disconnect().await?;
        Ok(())
    })
    .await
}

async fn wait_for_http_health(port: u16) -> anyhow::Result<()> {
    retry("broker", || async move {
        let response = http_request(port, "GET /healthz HTTP/1.1", None)?;
        anyhow::ensure!(
            response == "ok",
            "unexpected broker health response: {response}"
        );
        Ok(())
    })
    .await
}

async fn wait_for_tcp(addr: SocketAddr) -> anyhow::Result<()> {
    retry("tcp listener", || async move {
        TcpStream::connect_timeout(&addr, Duration::from_millis(200))?;
        Ok(())
    })
    .await
}

async fn retry<F, Fut>(name: &str, mut f: F) -> anyhow::Result<()>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<()>>,
{
    let mut last_error = None;
    for _ in 0..90 {
        match f().await {
            Ok(()) => return Ok(()),
            Err(error) => last_error = Some(error),
        }
        sleep(Duration::from_secs(1)).await;
    }
    Err(anyhow::anyhow!(
        "timed out waiting for {name}: {}",
        last_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| "unknown error".to_owned())
    ))
}

fn write_config(
    dir: &Path,
    broker_port: u16,
    pg_proxy_port: u16,
    mysql_proxy_port: u16,
    pg_upstream_port: u16,
    mysql_upstream_port: u16,
) -> anyhow::Result<PathBuf> {
    let sqlite_path = dir.join("gatebase.db");
    let signing_key_file = dir.join("session.key");
    let audit_path = dir.join("audit.jsonl");
    let rollback_path = dir.join("rollback.jsonl");
    let config = format!(
        r#"server:
  public_url: "http://127.0.0.1:{broker_port}"
  broker_listen: "127.0.0.1:{broker_port}"

metadata:
  backend: "sqlite"
  url: "sqlite://{}?mode=rwc"

sessions:
  default_ttl: "15m"
  max_ttl: "30m"
  signing_key_file: "{}"

github:
  app_id: "local-dev"
  installation_id: 123456
  private_key_file: "{}"
  webhook_secret: "local-dev-secret"

audit:
  fail_closed: true
  sinks:
    - type: "sqlite"
    - type: "jsonl"
      path: "{}"

rollback:
  enabled: true
  max_rows: 10
  sinks:
    - type: "sqlite"
    - type: "jsonl"
      path: "{}"

targets:
  - name: "prod-pg"
    engine: "postgres"
    access:
      github_repo: "gatebase/gatebase-pg"
      allow_cli_sessions: true
    listen: "127.0.0.1:{pg_proxy_port}"
    upstream: "127.0.0.1:{pg_upstream_port}"
    database: "app"
    credentials:
      username: "${{PG_UPSTREAM_USER}}"
      password: "${{PG_UPSTREAM_PASSWORD}}"
  - name: "prod-mysql"
    engine: "mysql"
    access:
      github_repo: "gatebase/gatebase-mysql"
      allow_cli_sessions: true
    listen: "127.0.0.1:{mysql_proxy_port}"
    upstream: "127.0.0.1:{mysql_upstream_port}"
    database: "app"
    credentials:
      username: "${{MYSQL_UPSTREAM_USER}}"
      password: "${{MYSQL_UPSTREAM_PASSWORD}}"

policies:
  default:
    block:
      - "drop_database"
      - "drop_table"
      - "truncate"
      - "alter_system"
    require_where:
      - "update"
      - "delete"
    max_rows_changed: 1000
"#,
        yaml_path(&sqlite_path),
        yaml_path(&signing_key_file),
        yaml_path(&dir.join("github.pem")),
        yaml_path(&audit_path),
        yaml_path(&rollback_path),
    );
    let path = dir.join("gatebase.yaml");
    fs::write(&path, config)?;
    Ok(path)
}

fn http_request(
    port: u16,
    request_line: &str,
    body: Option<(&str, String)>,
) -> anyhow::Result<String> {
    let response = raw_http_request(port, request_line, body)?;
    let (head, body) = split_http_response(&response)?;
    anyhow::ensure!(head.contains(" 200 "), "unexpected HTTP response: {head}");
    Ok(body.to_owned())
}

fn raw_http_request(
    port: u16,
    request_line: &str,
    body: Option<(&str, String)>,
) -> anyhow::Result<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    let (headers, body) = match body {
        Some((content_type, body)) => (
            format!("{content_type}\r\nContent-Length: {}\r\n", body.len()),
            body,
        ),
        None => ("Content-Length: 0\r\n".to_owned(), String::new()),
    };
    write!(
        stream,
        "{request_line}\r\nHost: 127.0.0.1\r\nConnection: close\r\n{headers}\r\n{body}"
    )?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response)
}

fn split_http_response(response: &str) -> anyhow::Result<(&str, &str)> {
    response
        .split_once("\r\n\r\n")
        .ok_or_else(|| anyhow::anyhow!("invalid HTTP response"))
}

fn require_docker() -> anyhow::Result<()> {
    let status = Command::new("docker")
        .arg("version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    anyhow::ensure!(status.success(), "docker is not available");
    Ok(())
}

fn free_port() -> anyhow::Result<u16> {
    Ok(TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port())
}

fn gatebase_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_gatebase"))
}

fn yaml_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{}-{nanos}", std::process::id())
}

struct DockerContainer {
    name: String,
}

impl DockerContainer {
    fn run(name: String, args: Vec<String>) -> anyhow::Result<Self> {
        let status = Command::new("docker").args(args).status()?;
        anyhow::ensure!(status.success(), "failed to start Docker container {name}");
        Ok(Self { name })
    }
}

impl Drop for DockerContainer {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["rm", "-f", &self.name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

struct ChildProcess {
    child: Child,
}

impl ChildProcess {
    fn spawn<const N: usize, const M: usize>(
        bin: &Path,
        args: [&str; N],
        envs: &[(&str, &str); M],
    ) -> anyhow::Result<Self> {
        let mut command = Command::new(bin);
        command
            .args(args)
            .envs(envs.iter().copied())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        Ok(Self {
            child: command.spawn()?,
        })
    }

    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for ChildProcess {
    fn drop(&mut self) {
        self.kill();
    }
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> anyhow::Result<Self> {
        let path = std::env::temp_dir().join(format!("{prefix}-{}", unique_suffix()));
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
