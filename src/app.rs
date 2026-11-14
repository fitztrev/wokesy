pub mod config;
mod http;
pub mod telemetry;

use tokio::signal::unix::{SignalKind, signal};

use config::Config;

const USER_AGENT: &str = concat!(
    env!("CARGO_PKG_NAME"),
    "/",
    env!("CARGO_PKG_VERSION"),
    " (+",
    env!("CARGO_PKG_REPOSITORY"),
    ")"
);

pub fn http_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder().user_agent(USER_AGENT).build()
}

pub async fn run(_config: Config) -> anyhow::Result<()> {
    tracing::info!("wokesy started");
    shutdown_requested().await;
    tracing::info!("shutdown complete");
    Ok(())
}

async fn shutdown_requested() {
    let terminate = async {
        match signal(SignalKind::terminate()) {
            Ok(mut terminations) => {
                terminations.recv().await;
            }
            Err(error) => {
                tracing::warn!(error = %error, "cannot listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };
    let received = tokio::select! {
        _ = tokio::signal::ctrl_c() => "SIGINT",
        () = terminate => "SIGTERM",
    };
    tracing::info!(signal = received, "shutdown requested");
}
