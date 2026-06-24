use crate::settings;
use anyhow::{Context, Result};
use axum::body::Body;
use axum::http::{header, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use axum::Router;
use rust_embed::RustEmbed;
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(RustEmbed)]
#[folder = "ui/dist"]
struct Assets;

/// API path prefixes the proxy is allowed to forward. Read-only surface only.
const ALLOWED_PREFIXES: &[&str] = &[
    "api/sessions",
    "api/audit/events",
    "api/admin/users",
    "api/admin/me",
    "api/rollbacks",
    "api/connections",
    "api/activity",
];

struct ProxyState {
    broker: String,
    token: String,
    client: reqwest::Client,
}

pub(crate) async fn run(
    broker: Option<String>,
    admin_token: Option<String>,
    port: Option<u16>,
    no_open: bool,
) -> Result<()> {
    let broker = settings::broker(broker)?;
    let token = settings::admin_token(admin_token)?;
    let state = Arc::new(ProxyState {
        broker: broker.trim_end_matches('/').to_owned(),
        token,
        client: reqwest::Client::new(),
    });

    let app = Router::new()
        .route("/api/*path", any(proxy))
        .fallback(static_handler)
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port.unwrap_or(7777)));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;
    let url = format!("http://{addr}");
    println!("gatebase ui on {url} (broker {broker})");
    if !no_open {
        open_browser(&url);
    }
    axum::serve(listener, app).await?;
    Ok(())
}

/// Forward GET requests on the allowlist to the broker with the bearer token
/// injected. Everything else is rejected so the local server stays read-only.
async fn proxy(
    axum::extract::State(state): axum::extract::State<Arc<ProxyState>>,
    method: Method,
    uri: Uri,
) -> Response {
    if method != Method::GET {
        return (StatusCode::METHOD_NOT_ALLOWED, "gatebase ui is read-only").into_response();
    }
    let path = uri.path().trim_start_matches('/');
    let allowed = ALLOWED_PREFIXES
        .iter()
        .any(|prefix| path == *prefix || path.starts_with(&format!("{prefix}/")));
    if !allowed {
        return (StatusCode::FORBIDDEN, "path not allowed").into_response();
    }

    let mut target = format!("{}/{}", state.broker, path);
    if let Some(query) = uri.query() {
        target.push('?');
        target.push_str(query);
    }
    match state
        .client
        .get(&target)
        .bearer_auth(&state.token)
        .send()
        .await
    {
        Ok(response) => {
            let status =
                StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let body = response.bytes().await.unwrap_or_default();
            (status, [(header::CONTENT_TYPE, "application/json")], body).into_response()
        }
        Err(error) => (StatusCode::BAD_GATEWAY, format!("broker error: {error}")).into_response(),
    }
}

/// Serve the embedded SPA. Unknown paths fall back to index.html for
/// client-side routing.
async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    if let Some(asset) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return (
            [(header::CONTENT_TYPE, mime.as_ref().to_owned())],
            Body::from(asset.data.into_owned()),
        )
            .into_response();
    }
    match Assets::get("index.html") {
        Some(asset) => (
            [(header::CONTENT_TYPE, "text/html".to_owned())],
            Body::from(asset.data.into_owned()),
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            "ui assets not embedded; run bun build in crates/gatebase-cli/ui",
        )
            .into_response(),
    }
}

fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", url])
        .spawn();
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let _ = url;
}
