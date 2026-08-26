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

use object_store::{ObjectStore, local::LocalFileSystem, multipart::MultipartStore};

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
    Ok(build_with_multipart(config)?.0)
}

/// A [`MultipartStore`] handle that can sit in a `Debug` struct.
///
/// `dyn MultipartStore` does not implement `Debug`, and what it would print is
/// the store's configuration rather than anything about the process holding it
/// — so the wrapper prints its own name and nothing else.
#[derive(Clone)]
pub struct Multipart(pub Arc<dyn MultipartStore>);

impl core::fmt::Debug for Multipart {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("Multipart(..)")
    }
}

/// A store, and the same store seen as a [`MultipartStore`] when it is one.
///
/// `None` on the second means resumable uploads there append to a partial file
/// instead — see `upload::sink`.
pub mod placement;
pub mod secret;

/// Every open backend, by the name `file_segments.backend` records.
///
/// Both views of each, because a resumable upload has to hold a multipart open
/// against the backend it was *placed* on — keeping only the plain store would
/// force every resumable upload to the default one, and a file recorded
/// somewhere its bytes are not is the exact failure this whole change exists to
/// prevent.
pub type Backends = std::collections::HashMap<String, Stores>;

/// The open stores and the settings they were opened from.
///
/// One value rather than two, because they have to agree: a store without its
/// capacity would be placed against the wrong ceiling, and a setting without
/// its store would be placed against a backend that cannot take anything. They
/// are built together and swapped together.
#[derive(Debug, Clone, Default)]
pub struct Pool {
    /// Stores that opened, by name.
    pub open: Backends,
    /// What the stored ones were configured with. The one from `storage.url`
    /// is not here — it has no row.
    pub configured: Vec<crate::db::repository::storage_backend::StorageBackend>,
}

/// Builds the store for every configured backend.
///
/// The one from `storage.url` is always present under [`DEFAULT_BACKEND`],
/// whatever the table says: it is where everything written before backends
/// existed lives, and an instance that could not open it would be unable to
/// serve its own history.
///
/// A stored backend that will not build — a bucket that has gone, options this
/// binary was compiled without — is logged and left out rather than fatal. The
/// alternative is a process that refuses to start because one of five buckets
/// is unreachable, which takes the other four down with it.
///
/// # Errors
///
/// Only when the default itself cannot be built, which is the same failure the
/// single-store version had.
pub fn build_all(
    config: &StorageConfig,
    stored: &[crate::db::repository::storage_backend::StorageBackend],
) -> Result<(Backends, Stores), StorageError> {
    let default = build_with_multipart(config)?;
    let mut all = Backends::with_capacity(stored.len() + 1);
    all.insert(DEFAULT_BACKEND.to_owned(), default.clone());

    for backend in stored {
        if backend.name == DEFAULT_BACKEND {
            tracing::warn!(
                name = DEFAULT_BACKEND,
                "a stored backend shadows the one from storage.url; the configured one wins"
            );
            continue;
        }
        let described = StorageConfig {
            url: backend.url.clone(),
            options: backend.options.clone(),
            ..StorageConfig::default()
        };
        match build_with_multipart(&described) {
            Ok(opened) => {
                all.insert(backend.name.clone(), opened);
            }
            Err(error) => tracing::error!(
                name = backend.name,
                %error,
                "could not open a stored backend; shares held there will answer 503"
            ),
        }
    }
    Ok((all, default))
}

/// A store, and the same store seen as a [`MultipartStore`] when it is one.
///
/// `None` on the second means resumable uploads there append to a partial file
/// instead — see `upload::sink`.
pub type Stores = (Arc<dyn ObjectStore>, Option<Multipart>);

/// The name given to the backend built from `storage.url`.
///
/// Every instance has this one, and until an operator configures another it is
/// the only one. Rows written before backends were recorded read as this,
/// because there was nowhere else the bytes could have been.
pub const DEFAULT_BACKEND: &str = "default";

/// The same store, plus a handle to it as a [`MultipartStore`] when it is one.
///
/// # Why this is separate from [`build`]
///
/// A resumable upload writes one object across several requests, which needs
/// the store's own multipart identifier to outlive the call that made it. That
/// is [`MultipartStore`], and it is a *different trait* from [`ObjectStore`]:
/// `aws`, `azure` and `gcp` implement both, `LocalFileSystem` implements only
/// the first, and there is no way to ask an `Arc<dyn ObjectStore>` which it is
/// — the crate exposes no downcast.
///
/// So the decision has to be made where the concrete type still exists, which
/// is here. `parse_url_opts` erases it, so the three schemes that can do
/// multipart are built from their own builders — the same `with_url` plus
/// `with_config` that `parse_url_opts` does internally — and everything else
/// falls through to it unchanged.
///
/// # Errors
///
/// As [`build`].
pub fn build_with_multipart(config: &StorageConfig) -> Result<Stores, StorageError> {
    if let Some(path) = local_path(&config.url) {
        // A local filesystem has no multipart concept to expose. Resumable
        // uploads there append to a partial file instead — see `upload::sink`.
        return Ok((build_local(&path)?, None));
    }

    let url = url::Url::parse(&config.url)
        .map_err(|e| StorageError::Location(format!("`{}`: {e}", config.url)))?;
    let options = backend_options(config);

    if let Some(pair) = build_multipart_capable(&url, &options)? {
        return Ok(pair);
    }

    let (store, prefix) = object_store::parse_url_opts(&url, options)?;

    // `parse_url_opts` splits the URL into a store and the path *within* it, and
    // hands both back. Dropping the second value would silently ignore the
    // prefix — `s3://bucket/tessera` would write to the bucket root, mixed in
    // with whatever else lives there.
    Ok((
        if prefix.as_ref().is_empty() {
            Arc::from(store)
        } else {
            Arc::new(object_store::prefix::PrefixStore::new(store, prefix))
        },
        None,
    ))
}

/// Applies the prefix to a store that is both an [`ObjectStore`] and a
/// [`MultipartStore`], keeping both views of it.
///
/// `PrefixStore` implements both traits, so wrapping does not cost the second
/// one — see `apache/arrow-rs-object-store#586`.
#[cfg(any(
    feature = "storage-s3",
    feature = "storage-gcs",
    feature = "storage-azure"
))]
fn prefixed_pair<S>(store: S, prefix: object_store::path::Path) -> Stores
where
    S: ObjectStore + MultipartStore,
{
    // One `Arc`, two views of it. Not two stores: a second client would mean a
    // second connection pool and a second chance for the two to be configured
    // differently.
    if prefix.as_ref().is_empty() {
        return both_views(Arc::new(store));
    }
    both_views(Arc::new(object_store::prefix::PrefixStore::new(
        store, prefix,
    )))
}

/// Two trait objects over one `Arc`.
///
/// Written as its own function because the coercion has to happen at a return
/// position: `Arc::clone` infers its parameter from the annotation, so naming
/// the target type on the binding asks it for an `&Arc<dyn ObjectStore>` that
/// nobody has.
#[cfg(any(
    feature = "storage-s3",
    feature = "storage-gcs",
    feature = "storage-azure"
))]
fn both_views<S>(shared: Arc<S>) -> Stores
where
    S: ObjectStore + MultipartStore,
{
    let same = Arc::clone(&shared);
    (same, Some(Multipart(shared)))
}

/// Returns the local directory `location` names, or `None` if it names a remote
/// store.
///
/// A value with no `://` is a path. `file://` is also accepted, because it is
/// what people write once they have seen the other schemes — and `file:///data`
/// means the *absolute* path `/data`, so the third slash has to survive.
pub(crate) fn local_path(location: &str) -> Option<String> {
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
/// The configured option wins: somebody who set `storage.options` meant it,
/// even if an ambient `AWS_ACCESS_KEY_ID` in the environment disagrees.
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

#[cfg(any(
    feature = "storage-s3",
    feature = "storage-gcs",
    feature = "storage-azure"
))]
/// Builds the store for a scheme that can hold a multipart upload open.
///
/// `None` when the scheme is not one of those, or the feature for it was not
/// compiled in — either way the caller falls back to `parse_url_opts`, which
/// produces the same store without the second handle.
///
/// Each arm mirrors `parse_url_opts`'s own `builder_opts!`: start from the URL,
/// then fold the options in by their string names, skipping any the backend
/// does not recognise. Same tolerance, so a configuration that worked before
/// this function existed still works.
fn build_multipart_capable(
    url: &url::Url,
    options: &BTreeMap<String, String>,
) -> Result<Option<Stores>, StorageError> {
    use object_store::ObjectStoreScheme;

    let Ok((scheme, prefix)) = ObjectStoreScheme::parse(url) else {
        return Ok(None);
    };

    match scheme {
        #[cfg(feature = "storage-s3")]
        ObjectStoreScheme::AmazonS3 => {
            let builder = options.iter().fold(
                object_store::aws::AmazonS3Builder::new().with_url(url.to_string()),
                |builder, (key, value)| match key.to_ascii_lowercase().parse() {
                    Ok(key) => builder.with_config(key, value),
                    Err(_) => builder,
                },
            );
            Ok(Some(prefixed_pair(builder.build()?, prefix)))
        }
        #[cfg(feature = "storage-gcs")]
        ObjectStoreScheme::GoogleCloudStorage => {
            let builder = options.iter().fold(
                object_store::gcp::GoogleCloudStorageBuilder::new().with_url(url.to_string()),
                |builder, (key, value)| match key.to_ascii_lowercase().parse() {
                    Ok(key) => builder.with_config(key, value),
                    Err(_) => builder,
                },
            );
            Ok(Some(prefixed_pair(builder.build()?, prefix)))
        }
        #[cfg(feature = "storage-azure")]
        ObjectStoreScheme::MicrosoftAzure => {
            let builder = options.iter().fold(
                object_store::azure::MicrosoftAzureBuilder::new().with_url(url.to_string()),
                |builder, (key, value)| match key.to_ascii_lowercase().parse() {
                    Ok(key) => builder.with_config(key, value),
                    Err(_) => builder,
                },
            );
            Ok(Some(prefixed_pair(builder.build()?, prefix)))
        }
        // Everything else — the HTTP/WebDAV backend, memory, and any scheme a
        // later version of `object_store` adds — has no multipart to offer, so
        // the ordinary path builds it.
        _ => Ok(None),
    }
}

/// The same, for a build with no multipart-capable backend compiled in.
///
/// Every arm of the other one is behind a feature, so with none of them there
/// is nothing to match on — and a store that cannot do multipart is exactly
/// what `None` means.
#[cfg(not(any(
    feature = "storage-s3",
    feature = "storage-gcs",
    feature = "storage-azure"
)))]
const fn build_multipart_capable(
    _url: &url::Url,
    _options: &BTreeMap<String, String>,
) -> Result<Option<Stores>, StorageError> {
    Ok(None)
}

/// How much each backend is holding, by name.
pub type Usage = std::collections::HashMap<String, u64>;

/// Every backend as the placer sees it: its ceiling, what it holds, and whether
/// it still takes new content.
///
/// The stored ones carry their own figures; the one from `storage.url` has no
/// row, so its figures come from the configuration. Usage is summed from the
/// segments this instance wrote — see
/// [`ShareRepository::bytes_per_backend`](crate::db::repository::share::ShareRepository::bytes_per_backend).
///
/// A backend that is configured but did not open is left out: it can serve
/// nothing, so placing an upload there would only move the failure later.
#[must_use]
pub fn candidates(
    config: &StorageConfig,
    stored: &[crate::db::repository::storage_backend::StorageBackend],
    open: &Backends,
    used: &Usage,
) -> Vec<placement::Candidate> {
    let mut all = Vec::with_capacity(stored.len() + 1);
    let taken = |name: &str| used.get(name).copied().unwrap_or(0);
    let any_default = stored.iter().any(|backend| backend.is_default);

    if open.contains_key(DEFAULT_BACKEND) {
        all.push(placement::Candidate {
            name: DEFAULT_BACKEND.to_owned(),
            capacity_bytes: Some(config.capacity_bytes),
            used_bytes: taken(DEFAULT_BACKEND),
            read_only: false,
            // Only when nothing else claims it. An instance that has never
            // configured a second backend still has a preference, and it is
            // this one.
            is_default: !any_default,
        });
    }
    for backend in stored {
        if backend.name == DEFAULT_BACKEND || !open.contains_key(&backend.name) {
            continue;
        }
        all.push(placement::Candidate {
            name: backend.name.clone(),
            capacity_bytes: backend.capacity_bytes,
            used_bytes: taken(&backend.name),
            read_only: backend.read_only,
            is_default: backend.is_default,
        });
    }
    all
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
            ..StorageConfig::default()
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
