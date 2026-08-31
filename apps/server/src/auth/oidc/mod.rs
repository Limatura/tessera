//! Signing in through an identity provider.
//!
//! The shape of the whole thing is in `docs/oidc.md`. What is here is the
//! machinery: what the provider says about itself, and (as the rest lands) the
//! authorisation-code exchange and what may be believed about the result.
//!
//! # What this decides and what it does not
//!
//! The provider decides **who may sign in**. tessera decides **what they may
//! do**. An account admitted here lands in `auth.default_role`, exactly as one
//! that registered with a password, and is moved by an administrator.

pub mod admit;
pub mod claims;
pub mod discovery;
pub mod flow;
pub mod token;

/// Whether a URL may carry an authorisation code and a client secret.
///
/// `https`, or `http` on a loopback address.
///
/// The requirement exists because those two values would otherwise be readable
/// on the wire, and because the exemption from verifying the ID token's
/// signature holds only while the channel is TLS. See `claims.rs`.
///
/// Loopback is the one place both arguments still hold without TLS: the bytes
/// are handed between two processes on one machine and never reach a network.
/// It is also what makes the flow testable end to end, since a test cannot
/// obtain a certificate a client would trust.
///
/// The host is compared against the three spellings of loopback rather than
/// resolved. A name that resolves to `127.0.0.1` today may not tomorrow, and a
/// rule that depends on a resolver is a rule that changes under you.
#[must_use]
pub fn is_private_or_tls(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url.trim()) else {
        return false;
    };
    match parsed.scheme() {
        "https" => true,
        // Compared against the three spellings of loopback rather than
        // resolved. A name that resolves to `127.0.0.1` today may not tomorrow,
        // and a rule that depends on a resolver is a rule that changes under
        // you. Parsing rather than matching on the string keeps
        // `http://127.0.0.1.evil.example` from passing for a loopback host.
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

    /// Everything else is refused, including the near misses that look like
    /// loopback and are not.
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
