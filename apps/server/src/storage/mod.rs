//! Blob storage abstraction.
//!
//! Everything above this module sees `Arc<dyn ObjectStore>` and nothing else, so
//! swapping a homelab's local directory for an S3 gateway is a configuration
//! change rather than a code change.

use std::sync::Arc;

use object_store::{ObjectStore, local::LocalFileSystem};

use crate::config::{StorageBackend, StorageConfig};

/// Failure to construct a store.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// The backing store rejected the configuration.
    #[error("could not open the object store: {0}")]
    Backend(#[from] object_store::Error),

    /// The local root directory could not be created or read.
    #[error("could not prepare the local storage directory `{path}`: {source}")]
    LocalRoot {
        /// Directory we tried to use.
        path: String,
        /// Underlying filesystem error.
        source: std::io::Error,
    },

    /// The configuration selects a backend this binary was not built with.
    #[error("this binary was built without the `{0}` cargo feature")]
    MissingFeature(&'static str),
}

/// Builds the object store described by `config`.
///
/// For [`StorageBackend::Local`] the root directory is created if missing, so a
/// first run needs no manual setup.
///
/// # Errors
///
/// Returns [`StorageError`] if the directory cannot be prepared, the remote
/// endpoint is misconfigured, or the selected backend was not compiled in.
pub fn build(config: &StorageConfig) -> Result<Arc<dyn ObjectStore>, StorageError> {
    match config.backend {
        StorageBackend::Local => build_local(config),
        StorageBackend::S3 => build_s3(config),
    }
}

fn build_local(config: &StorageConfig) -> Result<Arc<dyn ObjectStore>, StorageError> {
    let path = &config.local_path;
    std::fs::create_dir_all(path).map_err(|source| StorageError::LocalRoot {
        path: path.display().to_string(),
        source,
    })?;
    // Canonicalise so the store's prefix is stable regardless of the process's
    // working directory — a systemd unit and a `cargo run` must resolve alike.
    let root = std::fs::canonicalize(path).map_err(|source| StorageError::LocalRoot {
        path: path.display().to_string(),
        source,
    })?;
    Ok(Arc::new(LocalFileSystem::new_with_prefix(root)?))
}

#[cfg(feature = "s3")]
fn build_s3(config: &StorageConfig) -> Result<Arc<dyn ObjectStore>, StorageError> {
    // Credentials come from the standard AWS environment variables so they never
    // have to be written into a config file.
    let mut builder = object_store::aws::AmazonS3Builder::from_env()
        .with_bucket_name(&config.s3.bucket)
        .with_allow_http(config.s3.allow_http);
    if !config.s3.region.is_empty() {
        builder = builder.with_region(&config.s3.region);
    }
    if let Some(endpoint) = &config.s3.endpoint {
        builder = builder.with_endpoint(endpoint);
    }
    Ok(Arc::new(builder.build()?))
}

#[cfg(not(feature = "s3"))]
fn build_s3(_config: &StorageConfig) -> Result<Arc<dyn ObjectStore>, StorageError> {
    Err(StorageError::MissingFeature("s3"))
}

#[cfg(test)]
mod tests {
    use object_store::{ObjectStoreExt as _, path::Path as ObjectPath};

    use super::*;

    #[tokio::test]
    async fn local_backend_creates_its_root_and_round_trips() {
        let temp = tempfile::tempdir().unwrap();
        let config = StorageConfig {
            local_path: temp.path().join("nested/blobs"),
            ..StorageConfig::default()
        };

        let store = build(&config).unwrap();
        assert!(config.local_path.is_dir());

        let key = ObjectPath::from("probe.txt");
        store
            .put(&key, bytes::Bytes::from_static(b"hello").into())
            .await
            .unwrap();
        let read = store.get(&key).await.unwrap().bytes().await.unwrap();
        assert_eq!(&read[..], b"hello");
    }

    #[test]
    fn s3_backend_without_the_feature_reports_the_missing_feature() {
        let config = StorageConfig {
            backend: StorageBackend::S3,
            ..StorageConfig::default()
        };
        if cfg!(feature = "s3") {
            // With the feature on, an empty bucket name is what fails — and that
            // is configuration validation's job, not this module's.
            return;
        }
        assert!(matches!(
            build(&config),
            Err(StorageError::MissingFeature("s3"))
        ));
    }
}
