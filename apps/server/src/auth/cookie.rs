//! The session cookie.

use axum::http::{HeaderMap, header};
use axum_extra::extract::cookie::{Cookie, SameSite};
use serde::{Deserialize, Serialize};
use time::Duration;

use crate::auth::secret::Secret;

/// Name of the session cookie.
///
/// The `__Host-` prefix is **not** used. Browsers only honour it on HTTPS, and
/// refusing to set a cookie at all on a plain-HTTP LAN deployment
/// would break login for a large share of the people this software is for.
pub const COOKIE_NAME: &str = "tessera_session";

/// When to mark the session cookie `Secure`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecurePolicy {
    /// Set `Secure` when the request arrived over HTTPS.
    ///
    /// The default, and the only one that works for both of this software's
    /// realities: a LAN or Tailscale deployment on plain HTTP, and a public
    /// instance behind TLS.
    #[default]
    Auto,
    /// Always set it. Correct for an instance that is only ever reached over
    /// HTTPS, and will silently break login if that is not true.
    Always,
    /// Never set it.
    Never,
}

impl SecurePolicy {
    /// Whether to mark a cookie `Secure` for a request with these headers.
    ///
    /// # Why this is not simply "read `X-Forwarded-Proto`"
    ///
    /// That header is set by whoever spoke to us last, which on a direct
    /// connection is the client, so trusting it unconditionally lets anyone
    /// claim HTTPS. It is only meaningful when a reverse proxy we control put it
    /// there, and naming it in `server.trusted_proxies` asserts exactly that, so
    /// the answer depends on who connected, not on configuration alone.
    ///
    /// When the header is not trusted the answer is "not secure", and that
    /// direction is chosen. Marking a cookie `Secure` on a plain-HTTP
    /// deployment does not fail loudly: the browser accepts the response,
    /// discards the cookie, and the user sees a login that appears to succeed and
    /// then does nothing. A missing `Secure` flag on a LAN is a smaller problem
    /// than a login that cannot be diagnosed.
    #[must_use]
    pub fn applies(self, headers: &HeaderMap, trust_forwarded_proto: bool) -> bool {
        match self {
            Self::Always => true,
            Self::Never => false,
            Self::Auto => {
                trust_forwarded_proto
                    && headers
                        .get("x-forwarded-proto")
                        .and_then(|value| value.to_str().ok())
                        // A proxy chain may append, giving `https, http`. The
                        // first entry is the one the client actually spoke.
                        .and_then(|value| value.split(',').next())
                        .is_some_and(|proto| proto.trim().eq_ignore_ascii_case("https"))
            }
        }
    }
}

/// Builds the cookie that carries a session.
///
/// `SameSite=Lax`, not `Strict`. Retrieval links get opened from Discord,
/// mail clients and chat apps, and `Strict` withholds the cookie on exactly that
/// kind of top-level navigation, so a logged-in user would arrive logged out.
/// `Lax` still withholds it on cross-site POSTs, the case CSRF cares
/// about.
#[must_use]
pub fn session_cookie(secret: &Secret, max_age: Duration, secure: bool) -> Cookie<'static> {
    Cookie::build((COOKIE_NAME, secret.expose().to_owned()))
        // Unconditional: the session secret must never be readable from
        // JavaScript. That is what turns an XSS into a stolen account.
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .path("/")
        .max_age(max_age)
        .build()
}

/// Builds the cookie that clears a session.
///
/// Attributes must match the one being replaced, since a browser keys cookies by
/// name, domain and path, so a removal with a different `Path` creates a second
/// cookie instead of deleting the first.
#[must_use]
pub fn clearing_cookie(secure: bool) -> Cookie<'static> {
    Cookie::build((COOKIE_NAME, String::new()))
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .path("/")
        .max_age(Duration::ZERO)
        .build()
}

/// Reads the session secret a request presents, if any.
#[must_use]
pub fn session_secret(headers: &HeaderMap) -> Option<Secret> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(Cookie::split_parse_encoded)
        .filter_map(Result::ok)
        .find(|cookie| cookie.name() == COOKIE_NAME)
        .map(|cookie| Secret::from_client(cookie.value().to_owned()))
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;
    use rstest::rstest;

    use super::*;

    fn headers(proto: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(proto) = proto {
            headers.insert(
                "x-forwarded-proto",
                HeaderValue::from_str(proto).expect("valid header"),
            );
        }
        headers
    }

    #[rstest]
    #[case::always_over_http(SecurePolicy::Always, None, false, true)]
    #[case::never_over_https(SecurePolicy::Never, Some("https"), true, false)]
    #[case::auto_trusted_https(SecurePolicy::Auto, Some("https"), true, true)]
    #[case::auto_trusted_http(SecurePolicy::Auto, Some("http"), true, false)]
    #[case::auto_trusted_but_absent(SecurePolicy::Auto, None, true, false)]
    #[case::auto_chain_first_is_https(SecurePolicy::Auto, Some("https, http"), true, true)]
    #[case::auto_case_insensitive(SecurePolicy::Auto, Some("HTTPS"), true, true)]
    fn decides_when_to_mark_secure(
        #[case] policy: SecurePolicy,
        #[case] proto: Option<&str>,
        #[case] trusted: bool,
        #[case] expected: bool,
    ) {
        assert_eq!(policy.applies(&headers(proto), trusted), expected);
    }

    /// The header is client-controlled unless a proxy we trust set it. Believing
    /// it otherwise would let anyone claim HTTPS.
    #[test]
    fn an_untrusted_forwarded_proto_is_ignored() {
        assert!(!SecurePolicy::Auto.applies(&headers(Some("https")), false));
    }

    #[test]
    fn the_session_cookie_is_not_reachable_from_javascript() {
        let secret = Secret::generate_session();
        let cookie = session_cookie(&secret, Duration::hours(1), true);

        assert_eq!(cookie.http_only(), Some(true));
        assert_eq!(cookie.same_site(), Some(SameSite::Lax));
        assert_eq!(cookie.secure(), Some(true));
        assert_eq!(cookie.path(), Some("/"));
        assert_eq!(cookie.value(), secret.expose());
    }

    /// A removal whose attributes differ from the original adds a second cookie
    /// instead of deleting the first, and the user stays logged in.
    #[test]
    fn the_clearing_cookie_matches_the_one_it_replaces() {
        let live = session_cookie(&Secret::generate_session(), Duration::hours(1), true);
        let dead = clearing_cookie(true);

        assert_eq!(dead.name(), live.name());
        assert_eq!(dead.path(), live.path());
        assert_eq!(dead.same_site(), live.same_site());
        assert_eq!(dead.secure(), live.secure());
        assert_eq!(dead.max_age(), Some(Duration::ZERO));
        assert!(dead.value().is_empty());
    }

    #[test]
    fn reads_the_session_secret_from_a_cookie_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("other=1; tessera_session=abc123; another=2"),
        );
        assert_eq!(
            session_secret(&headers).map(|secret| secret.expose().to_owned()),
            Some("abc123".to_owned())
        );
    }

    #[rstest]
    #[case::no_cookies(&[])]
    #[case::other_cookies(&["theme=dark; lang=en"])]
    fn reports_no_secret_when_the_cookie_is_absent(#[case] values: &[&str]) {
        let mut headers = HeaderMap::new();
        for value in values {
            headers.append(
                header::COOKIE,
                HeaderValue::from_str(value).expect("valid header"),
            );
        }
        assert!(session_secret(&headers).is_none());
    }
}
