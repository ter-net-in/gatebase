use std::net::SocketAddr;

pub(crate) fn default_broker_listen() -> SocketAddr {
    "127.0.0.1:8080"
        .parse()
        .expect("valid default broker listen")
}

pub(crate) fn default_fail_closed() -> bool {
    true
}

pub(crate) fn default_github_api_base_url() -> String {
    "https://api.github.com".to_owned()
}
