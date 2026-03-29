use bytes::Bytes;
use object_store::local::LocalFileSystem;
use object_store::path::Path as ObjectPath;
use object_store::ObjectStore;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("storage error: {0}")]
    ObjectStore(#[from] object_store::Error),
    #[error("unsupported storage URL: {0}")]
    UnsupportedUrl(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct FileStorage {
    store: Arc<dyn ObjectStore>,
}

impl FileStorage {
    /// Create storage from a URL:
    /// - `file:///path/to/dir` -> LocalFileSystem
    /// - `s3://bucket/prefix`  -> AmazonS3 (requires `s3` feature)
    pub fn from_url(url: &str) -> Result<Self, StorageError> {
        if url.starts_with("file://") {
            let path = url.strip_prefix("file://").unwrap();
            std::fs::create_dir_all(path)?;
            let store =
                LocalFileSystem::new_with_prefix(path).map_err(object_store::Error::from)?;
            Ok(Self {
                store: Arc::new(store),
            })
        } else if url.starts_with("s3://") {
            #[cfg(feature = "s3")]
            {
                let without_scheme = url.strip_prefix("s3://").unwrap();
                let (bucket, prefix) =
                    without_scheme.split_once('/').unwrap_or((without_scheme, ""));
                let s3 = object_store::aws::AmazonS3Builder::from_env()
                    .with_bucket_name(bucket)
                    .build()?;
                if prefix.is_empty() {
                    Ok(Self {
                        store: Arc::new(s3),
                    })
                } else {
                    let store = object_store::prefix::PrefixStore::new(s3, prefix);
                    Ok(Self {
                        store: Arc::new(store),
                    })
                }
            }
            #[cfg(not(feature = "s3"))]
            {
                Err(StorageError::UnsupportedUrl(
                    "S3 support not compiled in. Enable the 's3' feature.".to_string(),
                ))
            }
        } else {
            Err(StorageError::UnsupportedUrl(url.to_string()))
        }
    }

    /// Get the underlying ObjectStore for direct access.
    pub fn inner(&self) -> &Arc<dyn ObjectStore> {
        &self.store
    }

    // ── Path helpers ──────────────────────────────────────────────────

    fn log_prefix(log_id: Uuid) -> ObjectPath {
        ObjectPath::from(log_id.to_string())
    }

    fn log_file_path(log_id: Uuid, filename: &str) -> ObjectPath {
        ObjectPath::from(format!("{}/{}", log_id, filename))
    }

    // ── Convenience methods ───────────────────────────────────────────

    /// Store a file for a log.
    pub async fn put_file(
        &self,
        log_id: Uuid,
        filename: &str,
        data: Bytes,
    ) -> Result<(), StorageError> {
        let path = Self::log_file_path(log_id, filename);
        self.store.put(&path, data.into()).await?;
        Ok(())
    }

    /// Get a file for a log.
    pub async fn get_file(
        &self,
        log_id: Uuid,
        filename: &str,
    ) -> Result<Bytes, StorageError> {
        let path = Self::log_file_path(log_id, filename);
        let result = self.store.get(&path).await?;
        Ok(result.bytes().await?)
    }

    /// Get a byte range of a file (for HTTP Range requests / DuckDB-WASM).
    pub async fn get_range(
        &self,
        log_id: Uuid,
        filename: &str,
        range: std::ops::Range<u64>,
    ) -> Result<Bytes, StorageError> {
        let path = Self::log_file_path(log_id, filename);
        Ok(self.store.get_range(&path, range).await?)
    }

    /// List files for a log.
    pub async fn list_files(&self, log_id: Uuid) -> Result<Vec<String>, StorageError> {
        use futures::TryStreamExt;
        let prefix = Self::log_prefix(log_id);
        let mut files = Vec::new();
        let mut stream = self.store.list(Some(&prefix));
        while let Some(meta) = stream.try_next().await? {
            if let Some(name) = meta.location.filename() {
                files.push(name.to_string());
            }
        }
        Ok(files)
    }

    /// Delete all files for a log.
    pub async fn delete_log_files(&self, log_id: Uuid) -> Result<(), StorageError> {
        use futures::TryStreamExt;
        let prefix = Self::log_prefix(log_id);
        let mut stream = self.store.list(Some(&prefix));
        while let Some(meta) = stream.try_next().await? {
            self.store.delete(&meta.location).await?;
        }
        Ok(())
    }
}
