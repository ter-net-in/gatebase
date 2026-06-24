use crate::cli::{AdminCommand, AdminUserCommand};
use crate::settings;
use anyhow::{Context, Result};
use gatebase_config::Config;
use gatebase_core::UserRole;
use gatebase_session::{verify_password, SessionStore};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Debug, Serialize)]
struct AdminLoginRequest {
    username: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct AdminLoginResponse {
    token: String,
    username: String,
    role: String,
}

#[derive(Debug, Serialize)]
struct CreateUserRequest {
    username: String,
    password: String,
    role: String,
}

#[derive(Debug, Deserialize)]
struct UserResponse {
    id: String,
    username: String,
    role: String,
    created_at: String,
    disabled_at: Option<String>,
}

struct CreateUserArgs {
    config: Option<PathBuf>,
    broker: Option<String>,
    admin_token: Option<String>,
    admin_username: Option<String>,
    admin_password_stdin: bool,
    username: String,
    role: String,
    password_stdin: bool,
}

pub(crate) async fn run(command: AdminCommand) -> Result<()> {
    match command {
        AdminCommand::User { command } => user(command).await,
    }
}

pub(crate) async fn login(
    broker: Option<String>,
    username: String,
    password_stdin: bool,
) -> Result<()> {
    let broker = settings::broker(broker)?;
    let password = read_login_password(password_stdin)?;
    let response: AdminLoginResponse = post_json(
        &broker,
        "/api/admin/login",
        None,
        &AdminLoginRequest { username, password },
    )
    .await?;
    let mut config = settings::load()?;
    config.admin_token = Some(response.token.clone());
    let path = settings::save(&config)?;
    println!("username {}", response.username);
    println!("role {}", response.role);
    println!("saved {}", path.display());
    Ok(())
}

async fn user(command: AdminUserCommand) -> Result<()> {
    match command {
        AdminUserCommand::Create {
            config,
            broker,
            admin_token,
            admin_username,
            admin_password_stdin,
            username,
            role,
            password_stdin,
        } => {
            create_user(CreateUserArgs {
                config,
                broker,
                admin_token,
                admin_username,
                admin_password_stdin,
                username,
                role,
                password_stdin,
            })
            .await
        }
        AdminUserCommand::List {
            config,
            broker,
            admin_token,
            admin_username,
            admin_password_stdin,
        } => {
            list_users(
                config,
                broker,
                admin_token,
                admin_username,
                admin_password_stdin,
            )
            .await
        }
    }
}

async fn create_user(args: CreateUserArgs) -> Result<()> {
    anyhow::ensure!(args.password_stdin, "provide --password-stdin");
    if let Some(config) = args.config {
        anyhow::ensure!(
            args.broker.is_none(),
            "provide exactly one of --config or --broker"
        );
        let store = open_store(config).await?;
        let count = store.count_users().await?;
        let role = UserRole::from_str(&args.role).map_err(anyhow::Error::msg)?;
        anyhow::ensure!(
            count > 0 || role == UserRole::Admin,
            "first user must have admin role"
        );
        let password = if count == 0 {
            read_stdin_secret()?
        } else {
            let (admin_password, password) =
                read_admin_and_user_passwords(args.admin_password_stdin)?;
            authenticate_local_admin(&store, args.admin_username, &admin_password).await?;
            password
        };
        let user = store.create_user(args.username, &password, role).await?;
        println!("id {}", user.id);
        println!("username {}", user.username);
        println!("role {}", user.role);
    } else {
        let broker = settings::broker(args.broker)?;
        let password = read_stdin_secret()?;
        let token = settings::admin_token(args.admin_token)?;
        let user: UserResponse = post_json(
            &broker,
            "/api/admin/users",
            Some(&token),
            &CreateUserRequest {
                username: args.username,
                password,
                role: args.role,
            },
        )
        .await?;
        print_user(&user);
    }
    Ok(())
}

async fn list_users(
    config: Option<PathBuf>,
    broker: Option<String>,
    admin_token: Option<String>,
    admin_username: Option<String>,
    admin_password_stdin: bool,
) -> Result<()> {
    if let Some(config) = config {
        anyhow::ensure!(
            broker.is_none(),
            "provide exactly one of --config or --broker"
        );
        let store = open_store(config).await?;
        if store.count_users().await? > 0 {
            anyhow::ensure!(admin_password_stdin, "provide --admin-password-stdin");
            let admin_password = read_stdin_secret()?;
            authenticate_local_admin(&store, admin_username, &admin_password).await?;
        }
        for user in store.list_users(None, None).await? {
            println!(
                "{}\t{}\t{}\t{}\t{}",
                user.id,
                user.username,
                user.role,
                user.created_at.to_rfc3339(),
                user.disabled_at
                    .map(|time| time.to_rfc3339())
                    .unwrap_or_else(|| "-".to_owned())
            );
        }
    } else if let Some(broker) = broker {
        let broker = settings::broker(Some(broker))?;
        let token = settings::admin_token(admin_token)?;
        let users: Vec<UserResponse> = get_json(&broker, "/api/admin/users", &token).await?;
        for user in users {
            print_user(&user);
        }
    } else {
        let broker = settings::broker(None)?;
        let token = settings::admin_token(admin_token)?;
        let users: Vec<UserResponse> = get_json(&broker, "/api/admin/users", &token).await?;
        for user in users {
            print_user(&user);
        }
    }
    Ok(())
}

async fn open_store(config: PathBuf) -> Result<SessionStore> {
    let config = Config::load(config)?;
    SessionStore::open(&config.metadata.sqlite_path).await
}

fn read_stdin_secret() -> Result<String> {
    let mut value = String::new();
    std::io::stdin().read_to_string(&mut value)?;
    Ok(value.trim_end_matches(['\r', '\n']).to_owned())
}

fn read_login_password(password_stdin: bool) -> Result<String> {
    use std::io::IsTerminal;
    if password_stdin {
        read_stdin_secret()
    } else if std::io::stdin().is_terminal() {
        Ok(rpassword::prompt_password("Password: ")?)
    } else {
        anyhow::bail!("provide --password-stdin or run from a terminal")
    }
}

fn read_admin_and_user_passwords(admin_password_stdin: bool) -> Result<(String, String)> {
    anyhow::ensure!(admin_password_stdin, "provide --admin-password-stdin");
    let mut value = String::new();
    std::io::stdin().read_to_string(&mut value)?;
    let (admin_password, password) = value
        .split_once('\n')
        .context("stdin must contain admin password, newline, then new user password")?;
    Ok((
        admin_password.trim_end_matches('\r').to_owned(),
        password.trim_end_matches(['\r', '\n']).to_owned(),
    ))
}

async fn authenticate_local_admin(
    store: &SessionStore,
    admin_username: Option<String>,
    admin_password: &str,
) -> Result<()> {
    let username = admin_username.context("provide --admin-username")?;
    let user = store
        .find_user_by_username(&username)
        .await?
        .context("invalid admin username or password")?;
    anyhow::ensure!(
        user.disabled_at.is_none(),
        "invalid admin username or password"
    );
    anyhow::ensure!(user.role == UserRole::Admin, "admin role required");
    anyhow::ensure!(
        verify_password(admin_password, &user.password_hash)?,
        "invalid admin username or password"
    );
    Ok(())
}

fn print_user(user: &UserResponse) {
    println!(
        "{}\t{}\t{}\t{}\t{}",
        user.id,
        user.username,
        user.role,
        user.created_at,
        user.disabled_at.as_deref().unwrap_or("-")
    );
}

async fn post_json<T, R>(broker: &str, path: &str, token: Option<&str>, body: &T) -> Result<R>
where
    T: Serialize,
    R: for<'de> Deserialize<'de>,
{
    let url = format!("{}{}", broker.trim_end_matches('/'), path);
    let mut request = reqwest::Client::new().post(&url).json(body);
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .with_context(|| format!("failed to connect to broker {broker}"))?;
    let status = response.status();
    let body = response.text().await?;
    anyhow::ensure!(status.is_success(), "broker request failed: {body}");
    Ok(serde_json::from_str(&body)?)
}

async fn get_json<R>(broker: &str, path: &str, token: &str) -> Result<R>
where
    R: for<'de> Deserialize<'de>,
{
    let url = format!("{}{}", broker.trim_end_matches('/'), path);
    let response = reqwest::Client::new()
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .with_context(|| format!("failed to connect to broker {broker}"))?;
    let status = response.status();
    let body = response.text().await?;
    anyhow::ensure!(status.is_success(), "broker request failed: {body}");
    Ok(serde_json::from_str(&body)?)
}
