//! Checking a new password against known leaked passwords.
//!
//! # What leaves this process
//!
//! Five characters of the password's SHA-1, in a URL. The service returns
//! every suffix it holds under that prefix, tens of thousands, and the match
//! happens here. Nothing identifying the password is sent; that is what the
//! range API is for, and why this can be enabled at all.
//!
//! # Why a failure is not a refusal
//!
//! This runs on registration, password change and reset completion, all of
//! which must work when a third-party service does not. Anything short of
//! "this password is in the list" lets the password through, with a log
//! line. See [`PwnedConfig`](crate::config::PwnedConfig).

use std::time::Duration;

use sha1::{Digest as _, Sha1};

use crate::{
    error::{Error, Result},
    state::AppState,
};

/// How long the lookup may take before the password is let through.
///
/// Short: a user is waiting on a form, and the fallback is to accept.
const TIMEOUT: Duration = Duration::from_secs(3);

/// How many characters of the hash go in the URL. Fixed by the API.
const PREFIX: usize = 5;

/// Refuses a password that appears in the breach list.
///
/// # Errors
///
/// [`Error::BadRequest`] when the password is in the list. Never otherwise: if
/// the service is unreachable, answers oddly or is too slow, the password is
/// accepted.
pub async fn refuse_if_breached(state: &AppState, password: &tessera_core::Password) -> Result<()> {
    let config = state.config().auth.pwned.clone();
    if !config.enabled {
        return Ok(());
    }
    let digest = hex(&Sha1::digest(password.expose().as_bytes()));
    let (prefix, suffix) = digest.split_at_checked(PREFIX).unwrap_or((&digest, ""));

    match lookup(&config.url, prefix).await {
        Ok(body) => {
            if breached(&body).any(|candidate| candidate.eq_ignore_ascii_case(suffix)) {
                return Err(Error::bad_request(
                    "this password has appeared in a data breach; choose a different one",
                ));
            }
            Ok(())
        }
        Err(error) => {
            // Logged, not raised: see the module docs.
            tracing::warn!(%error, "could not check a password against the breach list");
            Ok(())
        }
    }
}

/// Every breached suffix under a five-character `prefix`, uppercase, for a
/// client that finishes the comparison itself.
///
/// The same k-anonymity the check above relies on, one hop further out: the
/// browser sends this server only the prefix, and this server sends the
/// service only the prefix.
///
/// # Errors
///
/// When the service is unreachable or answers oddly. Unlike the check, this
/// has no fallback, so it reports the error; the check still runs when the
/// form is submitted.
pub async fn range(state: &AppState, prefix: &str) -> Result<Vec<String>, reqwest::Error> {
    let url = state.config().auth.pwned.url.clone();
    let body = lookup(&url, &prefix.to_ascii_uppercase()).await?;
    Ok(breached(&body).map(str::to_ascii_uppercase).collect())
}

/// Whether `prefix` is what the range API takes: five hex characters.
#[must_use]
pub fn is_prefix(prefix: &str) -> bool {
    prefix.len() == PREFIX && prefix.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Asks the service for every hash under `prefix`.
async fn lookup(base: &str, prefix: &str) -> Result<String, reqwest::Error> {
    reqwest::Client::builder()
        .timeout(TIMEOUT)
        .build()?
        .get(format!("{}/range/{prefix}", base.trim_end_matches('/')))
        .header("user-agent", concat!("tessera/", env!("CARGO_PKG_VERSION")))
        // Responses are padded with never-breached hashes, so the response size
        // reveals nothing about how many real matches it holds.
        .header("add-padding", "true")
        .send()
        .await?
        .error_for_status()?
        .text()
        .await
}

/// The breached suffixes in a response.
///
/// Each line is `SUFFIX:COUNT`. The padding entries requested by the header
/// come back with a count of zero, and matching one of those does not mean
/// the password was breached.
fn breached(body: &str) -> impl Iterator<Item = &str> {
    body.lines().filter_map(|line| {
        let line = line.trim();
        let (candidate, count) = line.split_once(':')?;
        (count.trim().parse::<u64>().unwrap_or(0) > 0).then_some(candidate.trim())
    })
}

#[cfg(test)]
fn seen(body: &str, suffix: &str) -> bool {
    breached(body).any(|candidate| candidate.eq_ignore_ascii_case(suffix))
}

/// Uppercase hex, the spelling the API answers in.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02X}");
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one hash everyone knows: `password` is `5BAA6…`, and only `5BAA6`
    /// is sent to the API.
    #[test]
    fn only_five_characters_of_the_hash_identify_the_password() {
        let digest = hex(&Sha1::digest(b"password"));
        assert_eq!(digest, "5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8");
        let (prefix, suffix) = digest.split_at(PREFIX);
        assert_eq!(prefix, "5BAA6");
        assert_eq!(suffix, "1E4C9B93F3F0682250B6CF8331B7EE68FD8");
    }

    #[test]
    fn a_match_with_a_count_is_a_breach() {
        let body =
            "1E4C9B93F3F0682250B6CF8331B7EE68FD8:9659365\r\n0018A45C4D1DEF81644B54AB7F969B88D65:1";
        assert!(seen(body, "1E4C9B93F3F0682250B6CF8331B7EE68FD8"));
    }

    /// Padding. A padded response carries hashes with a count of zero, and
    /// treating one as a match would refuse a password that was never leaked.
    #[test]
    fn a_match_with_no_count_is_padding() {
        assert!(!seen(
            "1E4C9B93F3F0682250B6CF8331B7EE68FD8:0",
            "1E4C9B93F3F0682250B6CF8331B7EE68FD8"
        ));
    }

    #[test]
    fn a_suffix_that_is_not_there_is_not_a_breach() {
        assert!(!seen(
            "0018A45C4D1DEF81644B54AB7F969B88D65:1",
            "1E4C9B93F3F0682250B6CF8331B7EE68FD8"
        ));
    }

    /// The service answers in uppercase, but a self-hosted one need not.
    #[test]
    fn case_does_not_decide_a_match() {
        assert!(seen(
            "1e4c9b93f3f0682250b6cf8331b7ee68fd8:2",
            "1E4C9B93F3F0682250B6CF8331B7EE68FD8"
        ));
    }
}
