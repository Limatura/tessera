//! Signing in through an identity provider.
//!
//! The overall design is in `docs/oidc.md`. This is the machinery: the
//! provider's discovery document, the authorisation-code exchange, and what
//! may be believed about the result.
//!
//! # What this decides and what it does not
//!
//! The provider decides **who may sign in**; tessera decides **what they may
//! do**. An admitted account lands in `auth.default_role`, like one registered
//! with a password, and is moved by an administrator.

pub mod admit;
pub mod claims;
pub mod discovery;
pub mod flow;
pub mod jwks;
pub mod logout;
pub mod token;

/// Whether a URL may carry an authorisation code and a client secret.
///
/// `https`, or `http` on a loopback address.
///
/// Required because those values would otherwise be readable on the wire, and
/// because skipping the ID token signature check holds only while the channel
/// is TLS. See `claims.rs`.
///
/// Loopback is the one place both still hold without TLS: the bytes pass
/// between two processes on one machine and never reach a network. It also
/// makes the flow testable end to end, since a test cannot get a certificate a
/// client would trust.
///
/// The host is compared against the three loopback spellings, not resolved. A
/// name resolving to `127.0.0.1` today may not tomorrow, and a rule depending
/// on a resolver can change under you.
#[must_use]
pub fn is_private_or_tls(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url.trim()) else {
        return false;
    };
    match parsed.scheme() {
        "https" => true,
        // Parsing, not string matching, keeps `http://127.0.0.1.evil.example`
        // from passing as a loopback host.
        "http" => matches!(parsed.host_str(), Some("127.0.0.1" | "localhost" | "[::1]")),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::is_private_or_tls;

    #[test]
    fn https_is_always_allowed() {
        assert!(is_private_or_tls("https://id.example"));
        assert!(is_private_or_tls("https://id.example/realms/main"));
    }

    #[test]
    fn http_on_loopback_is_allowed() {
        for url in [
            "http://127.0.0.1",
            "http://127.0.0.1:8080",
            "http://localhost:9000/realms/main",
            "http://[::1]:8080",
            "http://[::1]",
        ] {
            assert!(is_private_or_tls(url), "{url} was refused");
        }
    }

    /// Everything else is refused, including near misses that look like
    /// loopback but are not.
    #[test]
    fn http_anywhere_else_is_refused() {
        for url in [
            "http://id.example",
            "http://127.0.0.1.example.com",
            "http://localhost.example.com",
            "http://192.168.1.10:9000",
            "http://user@127.0.0.1.evil.example",
            "ftp://127.0.0.1",
            "",
        ] {
            assert!(!is_private_or_tls(url), "{url} was allowed");
        }
    }
}
