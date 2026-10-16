//! What the panel may see of a backend's location, and when an edit may keep
//! the credentials already stored.
//!
//! # Why an edit can keep them
//!
//! Moving a backend to another bucket or prefix on the same store should not
//! mean digging up the secret key again. So an edit that sends no credentials
//! keeps the stored ones.
//!
//! # Why only on the same store
//!
//! The credentials go wherever the location points. Keeping them for an edit
//! that changed the endpoint would let anyone who may write settings point a
//! backend at a server they run and collect a signed request, or the key
//! itself, on the next upload. So the stored credentials are kept only when
//! the scheme, the server and every option that is not a credential stay as
//! they were; anything else has to send its credentials again.

use std::collections::BTreeMap;

use super::local_path;

/// Substrings that mark an option key as a credential, matched lower case.
///
/// Broad on purpose: wrongly counting an option as a credential only hides it
/// and keeps it on edit, while missing one would send it back to the panel.
const CREDENTIAL_MARKERS: &[&str] = &[
    "key",
    "secret",
    "token",
    "password",
    "credential",
    "service_account",
    "sas",
];

/// Option keys holding an access key ID, in the order they are looked up.
const ACCESS_KEY_OPTIONS: &[&str] = &["access_key_id", "aws_access_key_id"];

/// Schemes whose URL authority names a bucket or container, not a server.
///
/// For these, the server is chosen by the options (`aws_endpoint` and the
/// like), so changing the authority is changing buckets on the same store.
const BUCKET_SCHEMES: &[&str] = &["s3", "s3a", "gs", "az", "azure", "adl", "abfs", "abfss"];

/// Shortest access key ID shown at all. Below this, four characters from each
/// end would be most of it.
const SHORTEST_SHOWN_KEY: usize = 12;

/// Characters kept from each end of a shown access key ID.
const KEY_ENDS: usize = 4;

/// Whether an option key holds a credential.
#[must_use]
pub fn is_credential(key: &str) -> bool {
    let key = key.to_lowercase();
    CREDENTIAL_MARKERS.iter().any(|marker| key.contains(marker))
}

/// Schemes whose URL user is a container name, not a credential:
/// `abfss://container@account.dfs.core.windows.net`. `object_store` reads
/// `az://` the same way.
const CONTAINER_IN_USER_SCHEMES: &[&str] = &["az", "abfs", "abfss"];

/// Whether the URL's user and password are credentials. For `az://` and
/// `abfs(s)://` the user is the container, which has to survive redaction and
/// editing.
fn user_is_credential(url: &url::Url) -> bool {
    !CONTAINER_IN_USER_SCHEMES.contains(&url.scheme())
}

/// Whether the URL carries a credential in its user or password.
fn has_user_credentials(url: &url::Url) -> bool {
    user_is_credential(url) && (!url.username().is_empty() || url.password().is_some())
}

/// The location without its credentials: no user, no password, no query (a
/// presigned URL's signature is there) and no fragment. A local path is shown
/// as it is.
#[must_use]
pub fn redacted(location: &str) -> String {
    if local_path(location).is_some() {
        return location.to_owned();
    }
    let Ok(mut url) = url::Url::parse(location) else {
        // Unparseable, so there is no telling where a credential would be.
        // The scheme alone still says what kind of store it was meant to be.
        return location
            .split_once("://")
            .map(|(scheme, _)| format!("{scheme}://"))
            .unwrap_or_default();
    };
    // Neither fails on a URL that already has an authority, and a URL
    // without one has no user or password to remove.
    if user_is_credential(&url) {
        let _ = url.set_username("");
    }
    let _ = url.set_password(None);
    url.set_query(None);
    url.set_fragment(None);
    url.into()
}

/// Aliases `object_store` accepts for the store's endpoint, in lookup order.
pub const ENDPOINT_KEYS: &[&str] = &[
    "aws_endpoint",
    "aws_endpoint_url",
    "endpoint",
    "endpoint_url",
];
/// Aliases for the region.
pub const REGION_KEYS: &[&str] = &["aws_region", "aws_default_region", "region"];
/// Aliases for allowing plain HTTP to the endpoint.
pub const ALLOW_HTTP_KEYS: &[&str] = &["aws_allow_http", "allow_http"];
/// Aliases for an Azure storage account's name.
pub const ACCOUNT_KEYS: &[&str] = &["azure_storage_account_name", "account_name"];

/// The first of `keys` set in `options`, compared without case.
#[must_use]
pub fn option<'a>(options: &'a BTreeMap<String, String>, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|wanted| {
        options
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(wanted))
            .map(|(_, value)| value.as_str())
    })
}

/// The endpoint option without credentials, for the panel to show and edit:
/// the same redaction as the location, since an endpoint URL can carry a user
/// and password too.
#[must_use]
pub fn redacted_endpoint(options: &BTreeMap<String, String>) -> Option<String> {
    option(options, ENDPOINT_KEYS)
        .map(redacted)
        .filter(|endpoint| !endpoint.is_empty())
}

/// The access key ID, with all but four characters at each end hidden, or
/// `None` when there is none or it is too short to show any of.
#[must_use]
pub fn masked_access_key(location: &str, options: &BTreeMap<String, String>) -> Option<String> {
    let from_url = url::Url::parse(location)
        .ok()
        .filter(|url| local_path(location).is_none() && user_is_credential(url))
        .map(|url| url.username().to_owned())
        .filter(|user| !user.is_empty());
    let from_options = || {
        ACCESS_KEY_OPTIONS.iter().find_map(|wanted| {
            options
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(wanted))
                .map(|(_, value)| value.clone())
        })
    };
    let key = from_url.or_else(from_options)?;

    let chars: Vec<char> = key.chars().collect();
    if chars.len() < SHORTEST_SHOWN_KEY {
        return None;
    }
    let head: String = chars.get(..KEY_ENDS)?.iter().collect();
    let tail: String = chars.get(chars.len() - KEY_ENDS..)?.iter().collect();
    Some(format!("{head}…{tail}"))
}

/// An edit that moved the backend somewhere the stored credentials may not
/// follow, without sending its own.
#[derive(Debug, PartialEq, Eq)]
pub struct CredentialsRequired;

/// The location and options to store for an edit.
///
/// `options` is `None` when the edit did not send any, which keeps the stored
/// ones. When the edit sends no credentials, in the URL or the options, the
/// stored credentials are kept if the backend is still on the same store (see
/// the module documentation); otherwise the edit is refused.
///
/// # Errors
///
/// [`CredentialsRequired`] when credentials would have to be kept for a
/// location on a different store.
pub fn carry_credentials(
    stored_url: &str,
    stored_options: &BTreeMap<String, String>,
    url: String,
    options: Option<BTreeMap<String, String>>,
) -> Result<(String, BTreeMap<String, String>), CredentialsRequired> {
    let options_sent = options.is_some();
    let mut options = options.unwrap_or_else(|| stored_options.clone());

    // A local directory takes no credentials, so there is nothing to keep.
    if local_path(&url).is_some() {
        return Ok((url, options));
    }
    // A URL that will not parse is refused when it is opened, with a better
    // message than this could give.
    let Ok(mut parsed) = url::Url::parse(&url) else {
        return Ok((url, options));
    };

    let sent_user = has_user_credentials(&parsed);
    let sent_option = options_sent && options.keys().any(|key| is_credential(key));
    if sent_user || sent_option {
        if !options_sent {
            // New credentials in the URL replace the stored ones, not sit
            // beside them where one would quietly win over the other.
            options.retain(|key, _| !is_credential(key));
        }
        return Ok((url, options));
    }

    let stored = url::Url::parse(stored_url)
        .ok()
        .filter(|_| local_path(stored_url).is_none());
    let stored_user = stored
        .as_ref()
        .filter(|stored| has_user_credentials(stored));
    let stored_secrets: BTreeMap<_, _> = stored_options
        .iter()
        .filter(|(key, _)| is_credential(key))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    if stored_user.is_none() && stored_secrets.is_empty() {
        return Ok((url, options));
    }

    let same_store = stored
        .as_ref()
        .is_some_and(|stored| same_server(stored, &parsed))
        && without_credentials(&options) == without_credentials(stored_options);
    if !same_store {
        return Err(CredentialsRequired);
    }

    if let Some(stored) = stored_user {
        // Both are already percent-encoded and the setters keep `%` as is, so
        // the stored user and password round-trip unchanged.
        let _ = parsed.set_username(stored.username());
        let _ = parsed.set_password(stored.password());
    }
    options.extend(stored_secrets);
    let url = if stored_user.is_some() {
        parsed.into()
    } else {
        url
    };
    Ok((url, options))
}

/// Whether two locations reach the same server.
///
/// When the host names a bucket, the server is in the options, which the
/// caller compares; otherwise the host is the server.
fn same_server(stored: &url::Url, edited: &url::Url) -> bool {
    if stored.scheme() != edited.scheme() {
        return false;
    }
    (host_is_bucket(stored) && host_is_bucket(edited))
        || (stored.host_str() == edited.host_str()
            && stored.port_or_known_default() == edited.port_or_known_default())
}

/// Whether the URL's host names a bucket or container rather than a server.
///
/// Not when the container is in the user: in
/// `abfss://container@account.dfs.core.windows.net` the host is the storage
/// account, and another account is another store.
fn host_is_bucket(url: &url::Url) -> bool {
    BUCKET_SCHEMES.contains(&url.scheme())
        && (url.username().is_empty() || !CONTAINER_IN_USER_SCHEMES.contains(&url.scheme()))
}

/// The options that place the store, for comparing an edit with what is
/// stored. Endpoints are compared the way the panel sees them, after
/// [`redacted`]: `url` writes `http://host:9000` back as `http://host:9000/`,
/// and an unchanged endpoint sent back from the panel must still match.
fn without_credentials(options: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    options
        .iter()
        .filter(|(key, _)| !is_credential(key))
        .map(|(key, value)| {
            let key = key.to_lowercase();
            let value = if ENDPOINT_KEYS.contains(&key.as_str()) {
                redacted(value).trim_end_matches('/').to_owned()
            } else {
                value.clone()
            };
            (key, value)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn redaction_drops_the_user_the_password_and_the_query() {
        assert_eq!(
            redacted("s3://AKIAEXAMPLE:secret@bucket/prefix?X-Amz-Signature=abc#x"),
            "s3://bucket/prefix"
        );
        assert_eq!(
            redacted("https://user:pw@dav.example.com/files"),
            "https://dav.example.com/files"
        );
    }

    #[test]
    fn a_local_path_is_shown_as_it_is() {
        assert_eq!(redacted("/var/lib/tessera"), "/var/lib/tessera");
        assert_eq!(redacted("file:///data"), "file:///data");
    }

    #[test]
    fn an_unparseable_location_shows_only_its_scheme() {
        assert_eq!(redacted("s3://key:se cret@[bad"), "s3://");
    }

    #[test]
    fn the_access_key_keeps_four_characters_at_each_end() {
        assert_eq!(
            masked_access_key("s3://AKIAIOSFODNN7EXAMPLE:secret@bucket", &BTreeMap::new())
                .as_deref(),
            Some("AKIA…MPLE")
        );
        assert_eq!(
            masked_access_key(
                "s3://bucket",
                &opts(&[("AWS_ACCESS_KEY_ID", "AKIAIOSFODNN7EXAMPLE")])
            )
            .as_deref(),
            Some("AKIA…MPLE")
        );
    }

    #[test]
    fn a_short_or_missing_access_key_is_not_shown() {
        assert_eq!(
            masked_access_key("s3://short:secret@bucket", &BTreeMap::new()),
            None
        );
        assert_eq!(masked_access_key("s3://bucket", &BTreeMap::new()), None);
        assert_eq!(masked_access_key("/data", &BTreeMap::new()), None);
    }

    #[test]
    fn the_endpoint_loses_its_credentials() {
        assert_eq!(
            redacted_endpoint(&opts(&[(
                "AWS_ENDPOINT",
                "http://user:pw@garage:3900/?x=1"
            )]))
            .as_deref(),
            Some("http://garage:3900/")
        );
        assert_eq!(redacted_endpoint(&BTreeMap::new()), None);
    }

    #[test]
    fn an_endpoint_read_back_from_the_panel_keeps_the_credentials() {
        let stored = opts(&[
            ("aws_endpoint", "http://garage:3900"),
            ("aws_secret_access_key", "secret"),
        ]);
        let shown = redacted_endpoint(&stored).unwrap();
        assert_eq!(shown, "http://garage:3900/");
        let (_, options) = carry_credentials(
            "s3://bucket",
            &stored,
            "s3://bucket".to_owned(),
            Some(opts(&[("aws_endpoint", &shown)])),
        )
        .unwrap();
        assert_eq!(
            options.get("aws_secret_access_key").map(String::as_str),
            Some("secret")
        );
    }

    #[test]
    fn an_abfs_container_is_not_a_credential() {
        let location = "abfss://files@acct.dfs.core.windows.net/tessera";
        assert_eq!(redacted(location), location);
        assert_eq!(masked_access_key(location, &BTreeMap::new()), None);

        let stored = opts(&[("azure_storage_account_key", "secret")]);
        let (url, options) = carry_credentials(
            location,
            &stored,
            location.to_owned(),
            Some(BTreeMap::new()),
        )
        .unwrap();
        assert_eq!(url, location);
        assert_eq!(options, stored);
    }

    #[test]
    fn an_az_container_is_not_a_credential_either() {
        let location = "az://files@acct.blob.core.windows.net/tessera";
        assert_eq!(redacted(location), location);
        assert_eq!(masked_access_key(location, &BTreeMap::new()), None);
    }

    /// The account is in the host here, not in the options, so it is compared
    /// as a server would be.
    #[test]
    fn another_azure_account_must_send_its_own_credentials() {
        let stored = opts(&[("azure_storage_account_key", "secret")]);
        assert_eq!(
            carry_credentials(
                "abfss://files@acct.dfs.core.windows.net/tessera",
                &stored,
                "abfss://files@other.dfs.core.windows.net/tessera".to_owned(),
                Some(BTreeMap::new()),
            ),
            Err(CredentialsRequired)
        );
        assert_eq!(
            carry_credentials(
                "abfss://files@acct.dfs.core.windows.net/tessera",
                &stored,
                "abfss://acct/tessera".to_owned(),
                Some(BTreeMap::new()),
            ),
            Err(CredentialsRequired)
        );
    }

    #[test]
    fn another_container_in_the_same_azure_account_keeps_the_credentials() {
        let stored = opts(&[("azure_storage_account_key", "secret")]);
        let (url, options) = carry_credentials(
            "abfss://files@acct.dfs.core.windows.net/tessera",
            &stored,
            "abfss://archive@acct.dfs.core.windows.net/tessera".to_owned(),
            Some(BTreeMap::new()),
        )
        .unwrap();
        assert_eq!(url, "abfss://archive@acct.dfs.core.windows.net/tessera");
        assert_eq!(options, stored);
    }

    #[test]
    fn moving_to_another_bucket_keeps_the_stored_credentials() {
        let (url, options) = carry_credentials(
            "s3://AKIAEXAMPLE:se%2Fcret@old-bucket/a",
            &opts(&[("aws_region", "garage")]),
            "s3://new-bucket/b".to_owned(),
            None,
        )
        .unwrap();
        assert_eq!(url, "s3://AKIAEXAMPLE:se%2Fcret@new-bucket/b");
        assert_eq!(options, opts(&[("aws_region", "garage")]));
    }

    #[test]
    fn credentials_in_the_options_are_kept_too() {
        let stored = opts(&[
            ("aws_endpoint", "https://garage.local"),
            ("aws_secret_access_key", "secret"),
        ]);
        let (_, options) = carry_credentials(
            "s3://old",
            &stored,
            "s3://new".to_owned(),
            Some(opts(&[("aws_endpoint", "https://garage.local")])),
        )
        .unwrap();
        assert_eq!(options, stored);
    }

    #[test]
    fn a_new_endpoint_must_send_its_own_credentials() {
        let stored = opts(&[
            ("aws_endpoint", "https://garage.local"),
            ("aws_secret_access_key", "secret"),
        ]);
        assert_eq!(
            carry_credentials(
                "s3://bucket",
                &stored,
                "s3://bucket".to_owned(),
                Some(opts(&[("aws_endpoint", "https://attacker.example")])),
            ),
            Err(CredentialsRequired)
        );
    }

    #[test]
    fn a_new_server_must_send_its_own_credentials() {
        assert_eq!(
            carry_credentials(
                "https://user:pw@dav.example.com/files",
                &BTreeMap::new(),
                "https://attacker.example/files".to_owned(),
                None,
            ),
            Err(CredentialsRequired)
        );
        assert_eq!(
            carry_credentials(
                "s3://key:secret@bucket",
                &BTreeMap::new(),
                "gs://bucket".to_owned(),
                None
            ),
            Err(CredentialsRequired)
        );
    }

    #[test]
    fn sent_credentials_replace_the_stored_ones() {
        let (url, options) = carry_credentials(
            "s3://bucket",
            &opts(&[("aws_secret_access_key", "old"), ("aws_region", "garage")]),
            "s3://NEWKEY:new@bucket".to_owned(),
            None,
        )
        .unwrap();
        assert_eq!(url, "s3://NEWKEY:new@bucket");
        assert_eq!(options, opts(&[("aws_region", "garage")]));
    }

    #[test]
    fn a_local_directory_needs_nothing_kept() {
        let (url, options) = carry_credentials(
            "s3://key:secret@bucket",
            &BTreeMap::new(),
            "/data".to_owned(),
            None,
        )
        .unwrap();
        assert_eq!(url, "/data");
        assert!(options.is_empty());
    }
}
