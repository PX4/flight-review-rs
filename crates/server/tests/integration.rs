//! Integration test: full upload → query → delete lifecycle.
//!
//! Starts a real server on a random port, uploads a ULog fixture,
//! verifies all API endpoints, then cleans up.

use serde_json::Value;
use std::net::TcpListener;
use std::sync::Arc;

/// Find a free port by binding to :0 and reading back the assigned port.
fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Resolve a test fixture path relative to the workspace root.
fn fixture_path(name: &str) -> std::path::PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");

    // First: check local fixtures in the converter crate
    let local = std::path::Path::new(manifest)
        .parent().unwrap()  // crates/
        .parent().unwrap()  // workspace root
        .join("crates/converter/tests/fixtures")
        .join(name);
    if local.exists() {
        return local;
    }

    // Fallback: px4-ulog-rs repo (local dev)
    std::path::Path::new(manifest)
        .parent().unwrap()  // crates/
        .parent().unwrap()  // workspace root
        .parent().unwrap()  // ulog/
        .join("px4-ulog-rs/tests/fixtures")
        .join(name)
}

/// Start the server in the background and return the base URL.
async fn start_server() -> (String, tokio::task::JoinHandle<()>) {
    let port = free_port();
    let base_url = format!("http://127.0.0.1:{}", port);

    let tmp_dir = tempfile::tempdir().unwrap();
    let db_path = tmp_dir.path().join("test.db");
    let storage_path = tmp_dir.path().join("files");
    std::fs::create_dir_all(&storage_path).unwrap();

    let db_url = format!("sqlite://{}", db_path.display());
    let storage_url = format!("file://{}", storage_path.display());

    let db = flight_review_server::db::create_db(&db_url)
        .await
        .expect("failed to create test db");
    let storage = Arc::new(
        flight_review_server::storage::FileStorage::from_url(&storage_url)
            .expect("failed to create test storage"),
    );

    let state = Arc::new(flight_review_server::AppState {
        db,
        storage,
        v1_ulg_prefix: None,
        mapbox_token: None,
        http_client: reqwest::Client::new(),
    });

    use axum::{
        extract::DefaultBodyLimit,
        routing::{get, post},
        Router,
    };
    use tower_http::cors::CorsLayer;

    let app = Router::new()
        .route("/health", get(flight_review_server::api::health::health))
        .route(
            "/api/upload",
            post(flight_review_server::api::upload::upload)
                .layer(DefaultBodyLimit::max(512 * 1024 * 1024)),
        )
        .route("/api/logs", get(flight_review_server::api::logs::list_logs))
        .route(
            "/api/logs/{id}",
            get(flight_review_server::api::logs::get_log)
                .delete(flight_review_server::api::logs::delete_log),
        )
        .route(
            "/api/logs/{id}/data/{filename}",
            get(flight_review_server::api::logs::get_log_file),
        )
        .route(
            "/api/stats",
            get(flight_review_server::api::stats::get_stats),
        )
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port))
        .await
        .unwrap();

    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Wait for server to be ready
    for _ in 0..50 {
        if reqwest::get(format!("{}/health", base_url)).await.is_ok() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    // Keep tmp_dir alive by leaking it (cleaned up on process exit)
    std::mem::forget(tmp_dir);

    (base_url, handle)
}

#[tokio::test]
async fn test_full_lifecycle() {
    let (base_url, _handle) = start_server().await;
    let client = reqwest::Client::new();

    // 1. Health check
    let resp = client
        .get(format!("{}/health", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");

    // 2. Upload a log
    let fixture = fixture_path("sample.ulg");
    assert!(fixture.exists(), "Fixture not found: {:?}", fixture);

    let file_bytes = std::fs::read(&fixture).unwrap();
    let form = reqwest::multipart::Form::new()
        .part(
            "file",
            reqwest::multipart::Part::bytes(file_bytes)
                .file_name("sample.ulg")
                .mime_str("application/octet-stream")
                .unwrap(),
        )
        .text("is_public", "true")
        .text("description", "Integration test upload")
        .text("pilot_name", "CI Bot")
        .text("tags", "test,ci");

    let resp = client
        .post(format!("{}/api/upload", base_url))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "Upload failed");
    let upload: Value = resp.json().await.unwrap();

    let log_id = upload["id"].as_str().expect("no id in upload response");
    let delete_token = upload["delete_token"]
        .as_str()
        .expect("no delete_token in upload response");
    assert_eq!(upload["sys_name"], "PX4");
    assert_eq!(upload["topic_count"], 15);
    assert!(upload["is_public"].as_bool().unwrap());
    assert!(upload["flight_duration_s"].as_f64().unwrap() > 0.0);

    // 3. List logs — should appear
    let resp = client
        .get(format!("{}/api/logs", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let list: Value = resp.json().await.unwrap();
    assert_eq!(list["total"], 1);
    assert_eq!(list["logs"][0]["id"], log_id);
    assert_eq!(list["logs"][0]["description"], "Integration test upload");
    assert_eq!(list["logs"][0]["pilot_name"], "CI Bot");

    // 4. Get single log
    let resp = client
        .get(format!("{}/api/logs/{}", base_url, log_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let log: Value = resp.json().await.unwrap();
    assert_eq!(log["sys_name"], "PX4");
    assert_eq!(log["ver_hw"], "AUAV_X21");
    assert_eq!(log["is_public"], true);
    assert!(log["topic_count"].as_i64().unwrap() >= 15);

    // 5. Get metadata.json
    let resp = client
        .get(format!("{}/api/logs/{}/data/metadata.json", base_url, log_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let metadata: Value = resp.json().await.unwrap();
    assert_eq!(metadata["sys_name"], "PX4");
    assert!(metadata["parameters"].as_object().unwrap().len() > 400);
    assert!(metadata["topics"].as_object().unwrap().len() >= 15);
    // Verify analysis is present
    let analysis = &metadata["analysis"];
    assert!(!analysis.is_null(), "analysis should be present");
    assert!(
        analysis["flight_modes"].as_array().unwrap().len() > 0,
        "should have flight modes"
    );

    // 6. Get Parquet file with Range request
    let resp = client
        .get(format!(
            "{}/api/logs/{}/data/vehicle_attitude.parquet",
            base_url, log_id
        ))
        .header("Range", "bytes=0-1023")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 206, "Should return 206 Partial Content");
    assert_eq!(resp.bytes().await.unwrap().len(), 1024);

    // 7. Get raw .ulg file
    let resp = client
        .get(format!(
            "{}/api/logs/{}/data/sample.ulg",
            base_url, log_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let ulg_size = resp.bytes().await.unwrap().len();
    assert!(ulg_size > 1_000_000, "ULG should be > 1MB");

    // 8. Stats endpoint
    let resp = client
        .get(format!("{}/api/stats?group_by=ver_hw&period=all", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let stats: Value = resp.json().await.unwrap();
    assert_eq!(stats["group_by"], "ver_hw");
    assert!(stats["data"].as_array().unwrap().len() > 0);
    assert_eq!(stats["data"][0]["group"], "AUAV_X21");

    // 9. Search filters
    let resp = client
        .get(format!(
            "{}/api/logs?sys_name=PX4&has_gps=false",
            base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let filtered: Value = resp.json().await.unwrap();
    // sample.ulg may or may not have GPS — just verify the filter doesn't error
    assert!(filtered["total"].as_i64().is_some());

    // 10. Delete with wrong token — should fail
    let resp = client
        .delete(format!(
            "{}/api/logs/{}?token=wrongtoken",
            base_url, log_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "Wrong token should be forbidden");

    // 11. Delete with correct token
    let resp = client
        .delete(format!(
            "{}/api/logs/{}?token={}",
            base_url, log_id, delete_token
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204, "Correct token should succeed");

    // 12. Verify deleted
    let resp = client
        .get(format!("{}/api/logs/{}", base_url, log_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404, "Deleted log should return 404");

    // 13. List should be empty
    let resp = client
        .get(format!("{}/api/logs", base_url))
        .send()
        .await
        .unwrap();
    let list: Value = resp.json().await.unwrap();
    assert_eq!(list["total"], 0);
}
