//! Looking a new password up in the passwords known to have leaked.
//!
//! # What leaves this process
//!
//! Five characters of the password's SHA-1, in a URL. The service answers with
//! every suffix it holds under that prefix, tens of thousands of them, and the
//! match is found here. Nothing that identifies the password is sent, which is
//! the whole point of the range API and the reason this can be on at all.
//!
//! # Why a failure is not a refusal
//!
//! This sits on registration, on changing a password and on finishing a reset.
//! All three have to work when somebody else's service does not, so anything
//! short of "this password is in the list" lets the password through, with a
//! line in the log. See [`PwnedConfig`](crate::config::PwnedConfig).

use std::time::Duration;

use sha1::{Digest as _, Sha1};

use crate::{
    error::{Error, Result},
    state::AppState,
};

/// How long the lookup may take before the password is let through.
///
/// Short: somebody is waiting on a form, and the fallback is to accept.
const TIMEOUT: Duration = Duration::from_secs(3);

/// How many characters of the hash go in the URL. Fixed by the API.
const PREFIX: usize = 5;

/// Refuses a password that appears in the breach list.
///
/// # Errors
///
/// [`Error::BadRequest`] when the password is in the list. Never for anything
/// else: a service that cannot be reached, answers oddly, or takes too long
/// leaves the password accepted.
pub async fn refuse_if_breached(state: &AppState, password: &tessera_core::Password) -> Result<()> {
    let config = state.config().auth.pwned.clone();
    if !config.enabled {
        return Ok(());
    }
    let digest = hex(&Sha1::digest(password.expose().as_bytes()));
    let (prefix, suffix) = digest.split_at_checked(PREFIX).unwrap_or((&digest, ""));

    match lookup(&config.url, prefix).await {
        Ok(body) => {
            if seen(&body, suffix) {
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

/// Asks the service for every hash under `prefix`.
async fn lookup(base: &str, prefix: &str) -> Result<String, reqwest::Error> {
    reqwest::Client::builder()
        .timeout(TIMEOUT)
        .build()?
        .get(format!("{}/range/{prefix}", base.trim_end_matches('/')))
        .header("user-agent", concat!("tessera/", env!("CARGO_PKG_VERSION")))
        // Answers are padded with hashes that were never breached, so the size
        // of the response says nothing about how many real matches it holds.
        .header("add-padding", "true")
        .send()
        .await?
        .error_for_status()?
        .text()
        .await
}

/// Whether `suffix` is in the answer with a count above zero.
///
/// Each line is `SUFFIX:COUNT`. The padding entries the header asks for come
/// back with a count of zero, and a password matching one of those has not
/// been breached.
fn seen(body: &str, suffix: &str) -> bool {
    body.lines().any(|line| {
        let line = line.trim();
        let (candidate, count) = line.split_once(':').unwrap_or((line, "0"));
        candidate.eq_ignore_ascii_case(suffix) && count.trim().parse::<u64>().unwrap_or(0) > 0
    })
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

    /// The one hash everybody knows: `password` is `5BAA6…`, and the API is
    /// asked for `5BAA6` and told nothing else.
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

    /// Padding. A response asked to be padded carries hashes with a count of
    /// zero, and treating one as a match would refuse a password nobody has
    /// ever leaked.
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
