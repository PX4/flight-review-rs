pub mod api;
pub mod db;
pub mod storage;

/// Shared application state passed to all handlers via axum's State extractor.
///
/// The `db` and `storage` fields are placeholders until the LogStore trait
/// and storage layer are implemented in tasks 14-15.
pub struct AppState {
    // pub db: Box<dyn LogStore>,
    // pub storage: Arc<dyn ObjectStore>,
    // pub converter_config: ConverterConfig,
}
