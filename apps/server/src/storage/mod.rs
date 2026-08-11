//! Blob storage abstraction.
//!
//! Everything above this module sees `Arc<dyn ObjectStore>` and nothing else, so
//! swapping a homelab's local directory for an S3 gateway is a configuration
//! change rather than a code change.
//!
//! # Why one URL instead of one struct per backend
//!
//! An earlier shape had `backend: Local | S3` plus an `S3Config` block, which
//! meant every new backend cost a config enum variant, a struct, a validation
//! branch and a builder arm. `object_store::parse_url_opts` already does that
//! dispatch, and it accepts arbitrary backend options — so a user can reach
//! settings we never thought to expose, without waiting for a release.
//!
//! The cost is that a bad option is reported by the backend rather than by our
//! own validation. That is the right trade for a self-hosted tool: the set of
//! things people want to point at is larger than the set we can anticipate.

use std::{collections::BTreeMap, path::Path, sync::Arc};

use object_store::{ObjectStore, local::LocalFileSystem};

use crate::config::StorageConfig;

/// Environment prefixes considered as backend credentials.
///
/// Matching the prefix is not enough to forward a variable — see
/// [`is_backend_option`].
const CREDENTIAL_ENV_PREFIXES: [&str; 3] = ["AWS_", "GOOGLE_", "AZURE_"];

/// Failure to construct a store.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// The backing store rejected the configuration.
    #[error("could not open the object store: {0}")]
    Backend(#[from] object_store::Error),

    /// `storage.url` is not a URL and not a usable path.
    #[error("`storage.url` is not usable: {0}")]
    Location(String),

    /// The local root directory could not be created or read.
    #[error("could not prepare the local storage directory `{path}`: {source}")]
    LocalRoot {
        /// Directory we tried to use.
        path: String,
        /// Underlying filesystem error.
        source: std::io::Error,
    },
}

/// Builds the object store described by `config`.
///
/// A bare path is treated as a local directory and created if missing, so a
/// first run needs no manual setup. Anything with a scheme is handed to
/// `object_store`.
///
/// # Errors
///
/// Returns [`StorageError`] if the directory cannot be prepared, the URL cannot
/// be parsed, or the backend rejects the options — including when the scheme
/// needs a cargo feature this binary was built without.
pub fn build(config: &StorageConfig) -> Result<Arc<dyn ObjectStore>, StorageError> {
    if let Some(path) = local_path(&config.url) {
        return build_local(&path);
    }

    let url = url::Url::parse(&config.url)
        .map_err(|e| StorageError::Location(format!("`{}`: {e}", config.url)))?;
    let (store, prefix) = object_store::parse_url_opts(&url, backend_options(config))?;

    // `parse_url_opts` splits the URL into a store and the path *within* it, and
    // hands both back. Dropping the second value would silently ignore the
    // prefix — `s3://bucket/tessera` would write to the bucket root, mixed in
    // with whatever else lives there.
    Ok(if prefix.as_ref().is_empty() {
        Arc::from(store)
    } else {
        Arc::new(object_store::prefix::PrefixStore::new(store, prefix))
    })
}

/// Returns the local directory `location` names, or `None` if it names a remote
/// store.
///
/// A value with no `://` is a path. `file://` is also accepted, because it is
/// what people write once they have seen the other schemes — and `file:///data`
/// means the *absolute* path `/data`, so the third slash has to survive.
fn local_path(location: &str) -> Option<String> {
    let Some(rest) = location.strip_prefix("file://") else {
        return (!location.contains("://")).then(|| location.to_owned());
    };

    // `file:///data` → `/data` (absolute). `file://data` has `data` as a URL
    // authority rather than a path, which is not something to guess at.
    rest.strip_prefix('/')
        .map(|path| format!("/{path}"))
        .filter(|path| path != "/")
}

fn build_local(path: &str) -> Result<Arc<dyn ObjectStore>, StorageError> {
    let path = Path::new(path);
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

/// Merges configured options with the standard cloud credential variables.
///
/// Configuration wins: an operator who wrote the key into `tessera.toml` meant
/// it, even if the ambient environment disagrees.
fn backend_options(config: &StorageConfig) -> BTreeMap<String, String> {
    let mut options: BTreeMap<String, String> = std::env::vars()
        .map(|(key, value)| (key.to_lowercase(), value))
        .filter(|(key, _)| is_backend_option(key))
        .collect();
    for (key, value) in &config.options {
        options.insert(key.to_lowercase(), value.clone());
    }
    options
}

/// Whether a lowercased environment variable name is one `object_store` will
/// accept as a configuration key.
///
/// The prefix alone is not a good enough filter, and getting this wrong is not
/// cosmetic: `object_store` rejects an unrecognised key with
/// `UnknownConfigurationKey` rather than ignoring it, so forwarding everything
/// that starts with `AWS_` means any machine with an ordinary AWS CLI setup —
/// `AWS_PROFILE`, `AWS_PAGER`, `AWS_SDK_LOAD_CONFIG` — cannot start the server.
///
/// Rather than maintain a list of accepted names that would drift from
/// `object_store`'s, ask its own parsers. Each backend's key enum is only
/// compiled in with its feature, so a build without `storage-s3` simply never
/// forwards `aws_*`.
fn is_backend_option(lowercased_key: &str) -> bool {
    // Checked first, and unconditionally, so that `lowercased_key` is used in
    // every feature configuration — including one with no cloud backend, where
    // all the blocks below are compiled away.
    let looks_like_a_credential = CREDENTIAL_ENV_PREFIXES
        .iter()
        .any(|prefix| lowercased_key.starts_with(&prefix.to_ascii_lowercase()));
    if !looks_like_a_credential {
        return false;
    }

    #[cfg(feature = "storage-s3")]
    if <object_store::aws::AmazonS3ConfigKey as std::str::FromStr>::from_str(lowercased_key).is_ok()
    {
        return true;
    }
    #[cfg(feature = "storage-gcs")]
    if <object_store::gcp::GoogleConfigKey as std::str::FromStr>::from_str(lowercased_key).is_ok() {
        return true;
    }
    #[cfg(feature = "storage-azure")]
    if <object_store::azure::AzureConfigKey as std::str::FromStr>::from_str(lowercased_key).is_ok()
    {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use object_store::{ObjectStoreExt as _, path::Path as ObjectPath};
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::bare_relative("data/blobs", Some("data/blobs"))]
    #[case::bare_absolute("/var/lib/tessera/blobs", Some("/var/lib/tessera/blobs"))]
    // Regression: an earlier version trimmed the leading slash and turned this
    // absolute path into a relative one, quietly writing to ./var/lib/tessera.
    #[case::file_url_is_absolute("file:///var/lib/tessera", Some("/var/lib/tessera"))]
    #[case::file_url_root_only("file:///", None)]
    #[case::s3("s3://bucket/prefix", None)]
    #[case::gcs("gs://bucket", None)]
    #[case::azure("az://container", None)]
    #[case::webdav("https://dav.example.com/files", None)]
    #[case::memory("memory:///", None)]
    fn distinguishes_local_paths_from_remote_urls(
        #[case] location: &str,
        #[case] expected: Option<&str>,
    ) {
        assert_eq!(local_path(location).as_deref(), expected);
    }

    #[tokio::test]
    async fn a_bare_path_creates_its_root_and_round_trips() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("nested/blobs");
        let config = StorageConfig {
            url: root.display().to_string(),
            ..StorageConfig::default()
        };

        let store = build(&config).unwrap();
        assert!(root.is_dir(), "the root should have been created");

        let key = ObjectPath::from("probe.txt");
        store
            .put(&key, bytes::Bytes::from_static(b"hello").into())
            .await
            .unwrap();
        let read = store.get(&key).await.unwrap().bytes().await.unwrap();
        assert_eq!(&read[..], b"hello");
    }

    /// A prefixed URL must still produce a working store.
    ///
    /// This does **not** prove the prefix is applied, and no unit test here can:
    /// `PrefixStore` is transparent from the outside — it rewrites keys on the
    /// way down — so the only observable difference is in the backing store's
    /// key space, which `memory://` does not expose to a second handle. That the
    /// prefix reaches `PrefixStore` is verified by reading `build`, and will be
    /// covered for real by an S3 integration test when one exists.
    #[tokio::test]
    async fn a_prefixed_url_still_round_trips() {
        let config = StorageConfig {
            url: "memory:///tessera/uploads".to_owned(),
            ..StorageConfig::default()
        };
        let store = build(&config).unwrap();

        let key = ObjectPath::from("a.bin");
        store
            .put(&key, bytes::Bytes::from_static(b"x").into())
            .await
            .unwrap();
        assert!(store.get(&key).await.is_ok());
    }

    #[test]
    fn configured_options_beat_the_ambient_environment() {
        let config = StorageConfig {
            url: "s3://bucket".to_owned(),
            options: [("AWS_REGION".to_owned(), "from-config".to_owned())]
                .into_iter()
                .collect(),
        };
        let options = backend_options(&config);
        assert_eq!(
            options.get("aws_region").map(String::as_str),
            Some("from-config")
        );
    }

    /// Regression: every `AWS_*` variable used to be forwarded, and
    /// `object_store` rejects an unknown key outright — so a developer machine
    /// with a normal AWS CLI profile could not start the server.
    #[rstest]
    #[case::real_credential("aws_access_key_id", cfg!(feature = "storage-s3"))]
    #[case::real_setting("aws_region", cfg!(feature = "storage-s3"))]
    #[case::cli_only_profile("aws_profile", false)]
    #[case::cli_only_pager("aws_pager", false)]
    #[case::cli_only_loader("aws_sdk_load_config", false)]
    #[case::unrelated("path", false)]
    #[case::unrelated_prefixed("awsome_thing", false)]
    fn only_forwards_keys_object_store_understands(#[case] key: &str, #[case] expected: bool) {
        assert_eq!(is_backend_option(key), expected, "key `{key}`");
    }

    #[test]
    fn a_scheme_this_build_lacks_is_an_error_not_a_panic() {
        // Which schemes are compiled in depends on features; whichever way this
        // build was configured, an unusable URL must surface as an error.
        let config = StorageConfig {
            url: "definitely-not-a-scheme://bucket".to_owned(),
            ..StorageConfig::default()
        };
        assert!(build(&config).is_err());
    }
}
