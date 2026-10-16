//! Blob storage abstraction.
//!
//! Everything above this module sees `Arc<dyn ObjectStore>` and nothing else,
//! so swapping a local directory for an S3 gateway is a configuration change.
//! See `docs/storage.md`.
//!
//! # Why one URL instead of one struct per backend
//!
//! An earlier shape had `backend: Local | S3` plus an `S3Config` block, so
//! every new backend cost a config enum variant, a struct, a validation branch
//! and a builder arm. `object_store::parse_url_opts` already does that dispatch
//! and accepts arbitrary backend options, so a user can reach settings we never
//! thought to expose.
//!
//! The cost is that a bad option is reported by the backend, not by our own
//! validation. The set of things people want to point at is larger than the
//! set we can anticipate.

use std::{collections::BTreeMap, path::Path, sync::Arc};

use object_store::{ObjectStore, local::LocalFileSystem, multipart::MultipartStore};

/// Environment prefixes considered as backend credentials.
///
/// Matching the prefix is not enough to forward a variable. See
/// [`is_backend_option`].
const CREDENTIAL_ENV_PREFIXES: [&str; 3] = ["AWS_", "GOOGLE_", "AZURE_"];

/// Failure to construct a store.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// The backing store rejected the configuration.
    #[error("could not open the object store: {0}")]
    Backend(#[from] object_store::Error),

    /// A backend's location is not a URL and not a usable path.
    #[error("the backend's location is not usable: {0}")]
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

/// Where a backend is and how to open it: a stored row's URL and options.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Target {
    /// A bare path for a local directory, or an `object_store` URL.
    pub url: String,
    /// Backend-specific options, passed through as-is. The standard `AWS_*`,
    /// `GOOGLE_*` and `AZURE_*` environment variables are merged in underneath,
    /// so these win where both are set.
    pub options: BTreeMap<String, String>,
}

impl Target {
    /// A backend at `url` with no options of its own.
    #[must_use]
    pub fn at(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            options: BTreeMap::new(),
        }
    }
}

/// Builds the object store `target` describes.
///
/// A bare path is treated as a local directory and created if missing, so a
/// first run needs no manual setup. Anything with a scheme is handed to
/// `object_store`.
///
/// # Errors
///
/// Returns [`StorageError`] if the directory cannot be prepared, the URL cannot
/// be parsed, or the backend rejects the options, including when the scheme
/// needs a cargo feature this binary was built without.
pub fn build(target: &Target) -> Result<Arc<dyn ObjectStore>, StorageError> {
    Ok(build_with_multipart(target)?.store)
}

/// A [`MultipartStore`] handle that can sit in a `Debug` struct.
///
/// `dyn MultipartStore` does not implement `Debug`, and what it would print is
/// the store's configuration, not anything about the process holding it, so the
/// wrapper prints its own name and nothing else.
#[derive(Clone)]
pub struct Multipart(pub Arc<dyn MultipartStore>);

impl core::fmt::Debug for Stores {
    /// Names what the store can do, not what it is. Neither trait object has
    /// anything printable, and the capabilities are the part anyone reading
    /// a log wants.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Stores")
            .field("multipart", &self.multipart.is_some())
            .field("put_multipart", &self.put_multipart)
            // `store` is a trait object with nothing printable behind it.
            .finish_non_exhaustive()
    }
}

impl core::fmt::Debug for Multipart {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("Multipart(..)")
    }
}

/// A store, and the same store seen as a [`MultipartStore`] when it is one.
///
/// `None` on the second means resumable uploads there append to a partial file
/// instead. See `upload::sink`.
#[cfg(any(feature = "storage-s3", feature = "storage-gcs"))]
pub mod clock;
pub mod first_write;
pub mod health;
pub mod in_flight;
pub mod location;
pub mod placement;
pub mod split;
pub mod stitch;

/// Every open backend, by the name `file_segments.backend` records.
///
/// Both views of each, because a resumable upload has to hold a multipart open
/// against the backend it was *placed* on. Keeping only the plain store would
/// force every resumable upload to the default one, and record a file somewhere
/// its bytes are not.
pub type Backends = std::collections::HashMap<String, Stores>;

/// The open stores and the settings they were opened from.
///
/// One value, not two, because they have to agree. A store without its capacity
/// is placed against the wrong ceiling, and a setting without its
/// store against a backend that can take nothing. Built and swapped together.
#[derive(Debug, Clone, Default)]
pub struct Pool {
    /// Stores that opened, by name.
    pub open: Backends,
    /// Every backend's row, whether or not it opened.
    pub configured: Vec<crate::db::repository::storage_backend::StorageBackend>,
    /// Whether each is answering, as last seen. Carried from one pool to the
    /// next.
    pub health: health::Health,
}

impl Pool {
    /// The row a backend was opened from.
    #[must_use]
    pub fn row(
        &self,
        name: &str,
    ) -> Option<&crate::db::repository::storage_backend::StorageBackend> {
        self.configured.iter().find(|backend| backend.name == name)
    }

    /// The directory a backend writes to, when it is local.
    ///
    /// A resumable upload into a local directory appends to a partial file
    /// there, since a directory has no multipart upload to keep open.
    #[must_use]
    pub fn local_root(&self, name: &str) -> Option<std::path::PathBuf> {
        self.row(name)
            .and_then(|backend| local_path(&backend.url))
            .map(std::path::PathBuf::from)
    }
}

/// Builds the store for every stored backend.
///
/// Every backend is a row. A local directory is a row like any bucket, written
/// first on a fresh instance by [`seed`], and an operator who wants only a
/// bucket can remove it.
///
/// A backend that fails to build (a deleted bucket, options for a feature this
/// binary lacks, an unwritable directory) is logged and skipped, not fatal.
/// Otherwise one unreachable bucket would take the others down, and an
/// instance that no longer uses its seeded directory would fail to start once
/// that directory is gone.
///
/// Each store notes how calls to it go in `health`, which forgets the backends
/// no longer stored, and a backend that fails to open is noted there too.
#[must_use]
pub fn build_all(
    stored: &[crate::db::repository::storage_backend::StorageBackend],
    health: &health::Health,
) -> Backends {
    health.retain(stored.iter().map(|backend| backend.name.as_str()));
    let mut all = Backends::with_capacity(stored.len());
    for backend in stored {
        let target = Target {
            url: backend.url.clone(),
            options: backend.options.clone(),
        };
        match build_with_multipart(&target) {
            Ok(mut opened) => {
                opened.store = health.observe(&backend.name, opened.store);
                all.insert(backend.name.clone(), opened);
            }
            Err(error) => {
                tracing::error!(
                    name = backend.name,
                    %error,
                    "could not open a backend; shares held there will answer 503"
                );
                health.record_failure(&backend.name, &error);
            }
        }
    }
    all
}

/// Writes the first backend on an instance with none: a local directory,
/// `blobs` under `server.data_dir`, which receives new content.
///
/// Only when the table is empty, so an operator who replaced it with a bucket
/// does not get it back on the next start. Named [`DEFAULT_BACKEND`], the name
/// every file recorded before backends were rows refers to, so their bytes are
/// found where they always were.
///
/// # Errors
///
/// When the table cannot be read or written.
pub async fn seed(
    backends: &crate::db::repository::storage_backend::StorageBackendRepository,
    data_dir: &str,
    now: time::OffsetDateTime,
) -> Result<bool, crate::db::repository::storage_backend::StorageBackendError> {
    if !backends.all().await?.is_empty() {
        return Ok(false);
    }
    let url = Path::new(data_dir)
        .join("blobs")
        .to_string_lossy()
        .into_owned();
    backends
        .put(
            &crate::db::repository::storage_backend::StorageBackend {
                name: DEFAULT_BACKEND.to_owned(),
                url,
                options: BTreeMap::new(),
                is_default: true,
                capacity_bytes: None,
                read_only: false,
                direct_access: false,
            },
            now,
        )
        .await?;
    Ok(true)
}

/// A store, the same store seen as a [`MultipartStore`] when it is one, and
/// whether it can be written to a piece at a time.
///
/// `multipart: None` means resumable uploads there append to a partial file
/// instead. See `upload::sink`.
///
/// The two flags answer different questions, so both are here.
/// `multipart` is about [`MultipartStore`], the *part-level* trait that lets an
/// upload's identifier outlive the request that opened it. `put_multipart` is
/// about [`ObjectStoreExt::put_multipart`](object_store::ObjectStoreExt::put_multipart),
/// which every store here implements except the HTTP/WebDAV one, whose only upload verb is a
/// whole-body `PUT`. `LocalFileSystem` shows they differ: no part-level view, but
/// `put_multipart` works fine.
#[derive(Clone)]
pub struct Stores {
    /// The store itself.
    pub store: Arc<dyn ObjectStore>,
    /// Its part-level view, when the scheme has one.
    pub multipart: Option<Multipart>,
    /// Whether [`ObjectStoreExt::put_multipart`](object_store::ObjectStoreExt::put_multipart)
    /// works here. False only for the HTTP/WebDAV store; see the type's own documentation.
    pub put_multipart: bool,
    /// The same store as a [`Signer`](object_store::signer::Signer), when a
    /// client may get a URL to write one part of a multipart upload itself.
    ///
    /// S3 and GCS only. Both sign the `partNumber` and `uploadId` into the URL,
    /// which is what makes a presigned part possible. Azure signs URLs too, but
    /// a SAS covers the blob, not the request, and writing blocks there needs a
    /// different recipe nothing here implements yet. A local directory and
    /// WebDAV have nothing to sign: the bytes pass through this server either
    /// way.
    #[cfg(any(feature = "storage-s3", feature = "storage-gcs"))]
    pub signer: Option<Arc<dyn object_store::signer::Signer>>,
}

impl Stores {
    /// A store with no part-level view whose `put_multipart` works: a local
    /// directory, and what tests want when they only need somewhere to put
    /// bytes.
    #[must_use]
    pub fn plain(store: Arc<dyn ObjectStore>) -> Self {
        Self {
            store,
            multipart: None,
            put_multipart: true,
            #[cfg(any(feature = "storage-s3", feature = "storage-gcs"))]
            signer: None,
        }
    }

    /// A store whose only upload verb is a whole-body write.
    #[must_use]
    pub fn whole_body_only(store: Arc<dyn ObjectStore>) -> Self {
        Self {
            store,
            multipart: None,
            put_multipart: false,
            #[cfg(any(feature = "storage-s3", feature = "storage-gcs"))]
            signer: None,
        }
    }
}

/// The name of the backend [`seed`] writes on a fresh instance.
///
/// An ordinary row, which an operator may rename, repoint or remove. The name
/// matters only because files recorded before backends were rows refer to it,
/// so the seeded backend is where those bytes are found.
pub const DEFAULT_BACKEND: &str = "default";

/// The same store, plus a handle to it as a [`MultipartStore`] when it is one.
///
/// # Why this is separate from [`build`]
///
/// A resumable upload writes one object across several requests, which needs
/// the store's own multipart identifier to outlive the call that made it. That
/// is [`MultipartStore`], a *different trait* from [`ObjectStore`]: `aws`,
/// `azure` and `gcp` implement both, `LocalFileSystem` only the first, and the
/// crate exposes no downcast to ask an `Arc<dyn ObjectStore>` which it is.
///
/// So the decision is made here, where the concrete type still exists.
/// `parse_url_opts` erases it, so the three schemes that can do multipart are
/// built from their own builders, the same `with_url` plus `with_config`
/// `parse_url_opts` uses internally, and everything else falls through to it.
///
/// # Errors
///
/// As [`build`].
pub fn build_with_multipart(target: &Target) -> Result<Stores, StorageError> {
    if let Some(path) = local_path(&target.url) {
        // A local filesystem has no multipart concept to expose. Resumable
        // uploads there append to a partial file instead; see `upload::sink`.
        return Ok(Stores::plain(build_local(&path)?));
    }

    let url = url::Url::parse(&target.url)
        .map_err(|e| StorageError::Location(format!("`{}`: {e}", target.url)))?;
    let options = backend_options(target);

    if let Some(pair) = build_multipart_capable(&url, &options)? {
        return Ok(pair);
    }

    let (store, prefix) = object_store::parse_url_opts(&url, options)?;

    // `parse_url_opts` splits the URL into a store and the path *within* it
    // and hands both back. Dropping the second value silently ignores the
    // prefix, so `s3://bucket/tessera` would write to the bucket root.
    let shared: Arc<dyn ObjectStore> = if prefix.as_ref().is_empty() {
        Arc::from(store)
    } else {
        Arc::new(object_store::prefix::PrefixStore::new(store, prefix))
    };

    // The one scheme here whose only upload verb is a whole-body `PUT`. Asked
    // by scheme, not by trying, because `put_multipart` fails at the
    // point of use, by which time an upload is half-read and the placer has
    // committed to a backend that cannot finish it.
    //
    // Not a limitation of WebDAV, which takes a chunked `PUT` of any size.
    // `object_store`'s `PutPayload` is an array of `Bytes` and cannot express a
    // stream: `apache/arrow-rs-object-store#281`, with work in progress in
    // `apache/arrow-rs-object-store#817`. When that lands and reaches the HTTP
    // store, this becomes `true`.
    Ok(if whole_body_only(&url) {
        Stores::whole_body_only(shared)
    } else {
        Stores::plain(shared)
    })
}

/// Whether the store for `url` can only be written a whole object at a time.
///
/// Separate and pure so the answer is testable without opening anything.
fn whole_body_only(url: &url::Url) -> bool {
    object_store::ObjectStoreScheme::parse(url)
        .is_ok_and(|(scheme, _)| matches!(scheme, object_store::ObjectStoreScheme::Http))
}

/// Applies the prefix to a store that is both an [`ObjectStore`] and a
/// [`MultipartStore`], keeping both views of it.
///
/// `PrefixStore` implements both traits, so wrapping does not cost the second
/// one. See `apache/arrow-rs-object-store#586`.
#[cfg(feature = "storage-azure")]
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
/// Its own function because the coercion has to happen at a return position:
/// `Arc::clone` infers its parameter from the annotation, so naming the target
/// type on the binding asks for an `&Arc<dyn ObjectStore>` that does not exist.
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
    Stores {
        store: same,
        multipart: Some(Multipart(shared)),
        put_multipart: true,
        #[cfg(any(feature = "storage-s3", feature = "storage-gcs"))]
        signer: None,
    }
}

/// As [`prefixed_pair`], for a store that can also sign part uploads.
///
/// A separate function, not a flag, because the extra view needs the store to
/// implement [`Signer`](object_store::signer::Signer), and Azure's does but is
/// not usable here. See [`Stores::signer`].
#[cfg(any(feature = "storage-s3", feature = "storage-gcs"))]
fn prefixed_pair_signing<S>(
    store: S,
    prefix: object_store::path::Path,
) -> (Stores, Arc<dyn object_store::signer::Signer>)
where
    S: ObjectStore + MultipartStore + object_store::signer::Signer,
{
    // The signer needs the prefix too. `PrefixStore` forwards `signed_url_opts`
    // with its prefix applied, so a URL signed through it names the same object
    // the multipart view opened.
    if prefix.as_ref().is_empty() {
        let shared = Arc::new(store);
        let signer = as_signer(&shared);
        return (both_views(shared), signer);
    }
    let shared = Arc::new(object_store::prefix::PrefixStore::new(store, prefix));
    let signer = as_signer(&shared);
    (both_views(shared), signer)
}

/// The signer view of a store, coerced at a return position for the reason
/// [`both_views`] gives.
#[cfg(any(feature = "storage-s3", feature = "storage-gcs"))]
fn as_signer<S: object_store::signer::Signer>(
    shared: &Arc<S>,
) -> Arc<dyn object_store::signer::Signer> {
    let same: Arc<S> = Arc::clone(shared);
    same
}

/// Attaches the signer to the pair it was built alongside.
#[cfg(any(feature = "storage-s3", feature = "storage-gcs"))]
fn signed((stores, signer): (Stores, Arc<dyn object_store::signer::Signer>)) -> Stores {
    Stores {
        signer: Some(signer),
        ..stores
    }
}

/// Returns the local directory `location` names, or `None` if it names a remote
/// store.
///
/// A value with no `://` is a path. `file://` is also accepted, since it is
/// what people write once they have seen the other schemes, and `file:///data`
/// means the *absolute* path `/data`, so the third slash has to survive.
pub(crate) fn local_path(location: &str) -> Option<String> {
    let Some(rest) = location.strip_prefix("file://") else {
        return (!location.contains("://")).then(|| location.to_owned());
    };

    // `file:///data` → `/data` (absolute). `file://data` has `data` as a URL
    // authority and not a path, and that is not something to guess at.
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
    // working directory: a systemd unit and a `cargo run` must resolve alike.
    let root = std::fs::canonicalize(path).map_err(|source| StorageError::LocalRoot {
        path: path.display().to_string(),
        source,
    })?;
    Ok(Arc::new(LocalFileSystem::new_with_prefix(root)?))
}

/// Merges configured options with the standard cloud credential variables.
///
/// The backend's own option wins: it was set explicitly, even if an ambient
/// `AWS_ACCESS_KEY_ID` disagrees.
fn backend_options(target: &Target) -> BTreeMap<String, String> {
    let mut options: BTreeMap<String, String> = std::env::vars()
        .map(|(key, value)| (key.to_lowercase(), value))
        .filter(|(key, _)| is_backend_option(key))
        .collect();
    for (key, value) in &target.options {
        options.insert(key.to_lowercase(), value.clone());
    }
    options
}

/// Whether a lowercased environment variable name is one `object_store` will
/// accept as a configuration key.
///
/// The prefix alone is not a good enough filter, and getting it wrong is not
/// cosmetic: `object_store` rejects an unrecognised key with
/// `UnknownConfigurationKey` instead of ignoring it, so forwarding everything
/// starting with `AWS_` stops the server booting on any machine with an
/// ordinary AWS CLI setup (`AWS_PROFILE`, `AWS_PAGER`, `AWS_SDK_LOAD_CONFIG`).
///
/// Rather than maintain a list that would drift from `object_store`'s, ask its
/// own parsers. Each backend's key enum is only compiled in with its feature,
/// so a build without `storage-s3` never forwards `aws_*`.
fn is_backend_option(lowercased_key: &str) -> bool {
    // Checked first and unconditionally, so `lowercased_key` is used in every
    // feature configuration, including one with no cloud backend where all the
    // blocks below are compiled away.
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
/// `None` when the scheme is not one of those, or its feature was not compiled
/// in. Either way the caller falls back to `parse_url_opts`, which produces the
/// same store without the second handle.
///
/// Each arm mirrors `parse_url_opts`'s own `builder_opts!`: start from the URL,
/// then fold the options in by their string names, skipping any the backend
/// does not recognise. Same tolerance, so older configurations still work.
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
            Ok(Some(signed(prefixed_pair_signing(
                builder.build()?,
                prefix,
            ))))
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
            Ok(Some(signed(prefixed_pair_signing(
                builder.build()?,
                prefix,
            ))))
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
        // Everything else (the HTTP/WebDAV backend, memory, and any scheme a
        // later `object_store` adds) has no multipart to offer, so the ordinary
        // path builds it.
        _ => Ok(None),
    }
}

/// The same, for a build with no multipart-capable backend compiled in.
///
/// Every arm of the other one is behind a feature, so with none of them there
/// is nothing to match on, and `None` already means "cannot do multipart".
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

/// Every backend as the placer sees it: its limit, its usage, and whether it
/// still accepts new content.
///
/// Usage is summed from the segments this instance wrote. See
/// [`ShareRepository::bytes_per_backend`](crate::db::repository::share::ShareRepository::bytes_per_backend).
///
/// A configured backend that failed to open is left out: it can serve nothing,
/// so placing an upload there would only delay the failure.
#[must_use]
pub fn candidates(
    stored: &[crate::db::repository::storage_backend::StorageBackend],
    open: &Backends,
    used: &Usage,
) -> Vec<placement::Candidate> {
    let taken = |name: &str| used.get(name).copied().unwrap_or(0);
    stored
        .iter()
        .filter(|backend| open.contains_key(&backend.name))
        .map(|backend| placement::Candidate {
            name: backend.name.clone(),
            capacity_bytes: backend.capacity_bytes,
            used_bytes: taken(&backend.name),
            read_only: backend.read_only,
            is_default: backend.is_default,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use object_store::{ObjectStoreExt as _, path::Path as ObjectPath};

    /// Which schemes can only be written a whole object at a time.
    ///
    /// Pinned here because the answer decides where the placer sends a large
    /// file, and getting it wrong is invisible until an upload is half read.
    #[rstest::rstest]
    #[case::webdav("http://dav.example/share", true)]
    #[case::webdav_tls("https://dav.example/share", true)]
    #[case::s3("s3://bucket", false)]
    #[case::azure("azure://container", false)]
    #[case::gcs("gs://bucket", false)]
    #[case::memory("memory://", false)]
    fn a_scheme_says_whether_it_takes_only_whole_objects(
        #[case] url: &str,
        #[case] expected: bool,
    ) {
        let parsed = url::Url::parse(url).expect("a url");
        assert_eq!(whole_body_only(&parsed), expected);
    }
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
        let config = Target::at(root.display().to_string());

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
    /// This does **not** prove the prefix is applied, and no unit test here
    /// can: `PrefixStore` rewrites keys on the way down and is transparent from
    /// outside, so the only observable difference is in the backing store's key
    /// space, which `memory://` does not expose to a second handle. An S3
    /// integration test would cover it for real.
    #[tokio::test]
    async fn a_prefixed_url_still_round_trips() {
        let config = Target::at("memory:///tessera/uploads");
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
        let config = Target {
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
    /// `object_store` rejects an unknown key outright, so a developer machine
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
        let config = Target::at("definitely-not-a-scheme://bucket");
        assert!(build(&config).is_err());
    }
}
