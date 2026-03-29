use axum::{routing::get, Router};
use clap::Parser;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use flight_review_server::api;

/// Flight Review v2 — HTTP API server
#[derive(Parser, Debug)]
#[command(version, about)]
struct Config {
    /// Database connection URL
    #[arg(long, default_value = "sqlite:///data/flight-review.db")]
    db: String,

    /// Object-storage URL (file:// or s3://)
    #[arg(long, default_value = "file:///data/files")]
    storage: String,

    /// Port to listen on
    #[arg(long, default_value_t = 8080)]
    port: u16,

    /// Host / bind address
    #[arg(long, default_value = "0.0.0.0")]
    host: String,
}

#[tokio::main]
async fn main() {
    let config = Config::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tracing::info!(
        "flight-review-server v{}",
        env!("CARGO_PKG_VERSION")
    );
    tracing::info!("db:      {}", config.db);
    tracing::info!("storage: {}", config.storage);
    tracing::info!("listen:  {}:{}", config.host, config.port);

    let app = Router::new().route("/health", get(api::health::health));

    let addr = format!("{}:{}", config.host, config.port);
    let listener = TcpListener::bind(&addr)
        .await
        .expect("failed to bind listener");

    tracing::info!("server listening on {addr}");
    axum::serve(listener, app)
        .await
        .expect("server error");
}
