use axum::{
    routing::{get, post},
    Router,
};
use clap::Parser;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tracing_subscriber::EnvFilter;

use flight_review_server::{api, db, storage::FileStorage, AppState};

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

    let db = db::create_db(&config.db)
        .await
        .expect("failed to connect to database");
    let storage = Arc::new(
        FileStorage::from_url(&config.storage).expect("failed to init storage"),
    );

    let state = Arc::new(AppState { db, storage });

    let app = Router::new()
        .route("/health", get(api::health::health))
        .route("/api/upload", post(api::upload::upload))
        .route("/api/logs", get(api::logs::list_logs))
        .route(
            "/api/logs/{id}",
            get(api::logs::get_log).delete(api::logs::delete_log),
        )
        .route(
            "/api/logs/{id}/data/{filename}",
            get(api::logs::get_log_file),
        )
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("{}:{}", config.host, config.port);
    let listener = TcpListener::bind(&addr)
        .await
        .expect("failed to bind listener");

    tracing::info!("server listening on {addr}");
    axum::serve(listener, app)
        .await
        .expect("server error");
}
