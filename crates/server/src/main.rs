use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Router,
};
use clap::{Args, Parser, Subcommand};
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tracing_subscriber::EnvFilter;

use flight_review_server::{api, db, storage::FileStorage, AppState};

#[derive(Parser)]
#[command(version, about = "Flight Review v2 server")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start the HTTP server
    Serve(ServeConfig),
    /// Import logs from a v1 Flight Review database
    Migrate(MigrateConfig),
}

#[derive(Args)]
struct ServeConfig {
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

    /// Prefix where v1 .ulg files live in storage (e.g., `flight_review/log_files`).
    /// Enables lazy conversion of unconverted logs on first view.
    #[arg(long)]
    v1_ulg_prefix: Option<String>,
}

#[derive(Args)]
struct MigrateConfig {
    /// Path to v1 logs.sqlite database
    #[arg(long)]
    v1_db: String,

    /// v2 database URL
    #[arg(long, default_value = "sqlite:///data/flight-review.db")]
    db: String,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    match cli.command {
        Command::Serve(config) => run_server(config).await,
        #[cfg(feature = "sqlite")]
        Command::Migrate(config) => run_migrate(config).await,
        #[cfg(not(feature = "sqlite"))]
        Command::Migrate(_) => {
            eprintln!("Error: the 'migrate' command requires the 'sqlite' feature.");
            std::process::exit(1);
        }
    }
}

async fn run_server(config: ServeConfig) {
    tracing::info!(
        "flight-review-server v{}",
        env!("CARGO_PKG_VERSION")
    );
    tracing::info!("db:      {}", config.db);
    tracing::info!("storage: {}", config.storage);
    tracing::info!("listen:  {}:{}", config.host, config.port);
    if let Some(ref prefix) = config.v1_ulg_prefix {
        tracing::info!("v1 ULG prefix: {}", prefix);
    }

    let db = db::create_db(&config.db)
        .await
        .expect("failed to connect to database");
    let storage = Arc::new(
        FileStorage::from_url(&config.storage).expect("failed to init storage"),
    );

    let state = Arc::new(AppState {
        db,
        storage,
        v1_ulg_prefix: config.v1_ulg_prefix,
    });

    let app = Router::new()
        .route("/health", get(api::health::health))
        .route("/api/upload", post(api::upload::upload)
            .layer(DefaultBodyLimit::max(512 * 1024 * 1024))) // 512 MB
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

#[cfg(feature = "sqlite")]
async fn run_migrate(config: MigrateConfig) {
    use chrono::{NaiveDateTime, TimeZone, Utc};
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use sqlx::Row;
    use std::str::FromStr;
    use uuid::Uuid;

    tracing::info!("Migrating from v1 database: {}", config.v1_db);
    tracing::info!("Target v2 database: {}", config.db);

    // Open v1 SQLite (read-only)
    let v1_opts = SqliteConnectOptions::from_str(&config.v1_db)
        .expect("invalid v1 database path")
        .read_only(true);
    let v1_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(v1_opts)
        .await
        .expect("failed to open v1 database");

    // Open v2 DB
    let v2_db = db::create_db(&config.db)
        .await
        .expect("failed to connect to v2 database");

    // Query all v1 rows with LEFT JOIN
    let rows = sqlx::query(
        "SELECT \
            l.Id, l.Date, l.Description, l.OriginalFilename, l.Source, l.Public, l.Token, l.Type, \
            l.WindSpeed, l.Rating, l.Feedback, l.VideoUrl, \
            g.Duration, g.MavType, g.Estimator, g.AutostartId, g.Hardware, g.Software, \
            g.SoftwareVersion, g.NumLoggedErrors, g.NumLoggedWarnings, g.FlightModes, \
            g.FlightModeDurations, g.UUID, g.StartTime \
         FROM Logs l \
         LEFT JOIN LogsGenerated g ON l.Id = g.Id"
    )
    .fetch_all(&v1_pool)
    .await
    .expect("failed to query v1 database");

    let total = rows.len();
    tracing::info!("Found {} records in v1 database", total);

    let mut imported = 0u64;
    let mut skipped = 0u64;

    for (i, row) in rows.iter().enumerate() {
        // Parse UUID
        let id_str: String = row.try_get("Id").unwrap_or_default();
        let id = match Uuid::parse_str(&id_str) {
            Ok(id) => id,
            Err(e) => {
                tracing::warn!("Skipping row with invalid UUID '{}': {}", id_str, e);
                skipped += 1;
                continue;
            }
        };

        // Check if already exists in v2
        match v2_db.get(id).await {
            Ok(Some(_)) => {
                skipped += 1;
                continue;
            }
            Err(e) => {
                tracing::warn!("Error checking for existing record {}: {}", id, e);
                skipped += 1;
                continue;
            }
            Ok(None) => {} // proceed
        }

        // Parse created_at from Logs.Date
        let date_str: String = row.try_get("Date").unwrap_or_default();
        let created_at = chrono::DateTime::parse_from_rfc3339(&date_str)
            .map(|dt| dt.with_timezone(&Utc))
            .or_else(|_| {
                NaiveDateTime::parse_from_str(&date_str, "%Y-%m-%d %H:%M:%S")
                    .map(|ndt| Utc.from_utc_datetime(&ndt))
            })
            .or_else(|_| {
                NaiveDateTime::parse_from_str(&date_str, "%Y-%m-%d")
                    .map(|ndt| Utc.from_utc_datetime(&ndt))
            })
            .unwrap_or_else(|_| Utc::now());

        let filename: String = row
            .try_get("OriginalFilename")
            .unwrap_or_else(|_| format!("{}.ulg", id));

        let is_public: bool = row.try_get::<i32, _>("Public").unwrap_or(0) == 1;

        let delete_token: String = row
            .try_get("Token")
            .unwrap_or_else(|_| Uuid::new_v4().simple().to_string());

        // LogsGenerated fields (all optional since it's a LEFT JOIN)
        let hardware: Option<String> = row.try_get("Hardware").ok().flatten();
        let mav_type: Option<String> = row.try_get("MavType").ok().flatten();
        let software_version: Option<String> = row.try_get("SoftwareVersion").ok().flatten();
        let duration_str: Option<String> = row.try_get("Duration").ok().flatten();

        let sys_name = hardware.clone().or(mav_type);
        let ver_hw = hardware;
        let ver_sw_release_str = software_version;
        let flight_duration_s = duration_str
            .as_deref()
            .and_then(|s| s.parse::<f64>().ok());

        // Pilot-provided context fields from v1 Logs table
        let description: Option<String> = row.try_get("Description").ok().flatten();
        let source: Option<String> = row.try_get("Source").ok().flatten();
        let wind_speed: Option<String> = row.try_get("WindSpeed").ok().flatten();
        let rating_str: Option<String> = row.try_get("Rating").ok().flatten();
        let rating: Option<i32> = rating_str.as_deref().and_then(|s| s.parse::<i32>().ok());
        let feedback: Option<String> = row.try_get("Feedback").ok().flatten();
        let video_url: Option<String> = row.try_get("VideoUrl").ok().flatten();

        let record = db::LogRecord {
            id,
            filename,
            created_at,
            file_size: 0,
            sys_name,
            ver_hw,
            ver_sw_release_str,
            flight_duration_s,
            topic_count: 0,
            lat: None,
            lon: None,
            is_public,
            delete_token,
            description,
            wind_speed,
            rating,
            feedback,
            video_url,
            source,
            pilot_name: None,
            vehicle_name: None,
            tags: None,
            location_name: None,
            mission_type: None,
        };

        match v2_db.insert(&record).await {
            Ok(()) => imported += 1,
            Err(e) => {
                tracing::warn!("Failed to insert {}: {}", id, e);
                skipped += 1;
            }
        }

        if (i + 1) % 1000 == 0 {
            tracing::info!("Progress: {}/{} processed ({} imported, {} skipped)", i + 1, total, imported, skipped);
        }
    }

    tracing::info!(
        "Migration complete: {} imported, {} skipped, {} total in v1",
        imported,
        skipped,
        total
    );
}
