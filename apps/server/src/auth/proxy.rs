//! Working out who a request is actually from.
//!
//! # Why this is not one line
//!
//! Every throttle in this service counts against a source address: failed
//! sign-ins, retrieval codes tried and not found, share-password guesses, and
//! how many messages one mailbox may be sent. All of them assume the address is
//! something the caller cannot choose. `X-Forwarded-For` is a header, so on a
//! direct connection it is exactly something the caller chooses.
//!
//! # Why the *rightmost* entry
//!
//! The obvious reading — "the client is the first name in the list" — is wrong
//! even when a reverse proxy really is in front, and this was measured rather
//! than argued. nginx's `$proxy_add_x_forwarded_for` *appends* the address it
//! saw to whatever arrived, so a request that shows up carrying
//! `X-Forwarded-For: 198.51.100.7` leaves the proxy as
//! `198.51.100.7, 10.0.0.5` — the fabricated entry on the left, the real client
//! on the right. Two hundred sign-in attempts, each prepending a different
//! address, were all answered by an instance whose throttle blocks the eleventh.
//!
//! So the walk goes the other way: start at the peer, move leftwards, and stop
//! at the first address that is not one of ours. Everything to the left of that
//! is whatever the client felt like writing, and it is discarded.

use std::net::IpAddr;

use axum::http::HeaderMap;
use ipnet::IpNet;

/// Where a request came from, and whether that answer can be relied on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    /// The address to count attempts against.
    pub source: String,
    /// Whether the thing that connected is a proxy this instance trusts.
    ///
    /// This is what makes `X-Forwarded-Proto` worth reading, and so what decides
    /// whether the session cookie is marked `Secure`.
    pub via_trusted_proxy: bool,
}

/// The proxies an instance is willing to be spoken for by.
#[derive(Debug, Clone, Default)]
pub struct TrustedProxies {
    nets: Vec<IpNet>,
    /// Set by the deprecated `server.trusted_proxy` boolean.
    ///
    /// It means "whatever connects to us is a proxy", which is the one thing a
    /// list cannot express — and deliberately so, since it is the setting that
    /// makes the header forgeable by anyone who can reach the port directly. It
    /// does *not* extend to the rest of the chain: the peer is believed about
    /// the address it saw, and nothing further left is.
    any_peer: bool,
}

impl TrustedProxies {
    /// Reads a list of CIDR blocks, or bare addresses meaning just themselves.
    ///
    /// # Errors
    ///
    /// Returns the offending entry when something in the list is neither.
    pub fn parse(entries: &[String]) -> Result<Self, String> {
        let mut nets = Vec::with_capacity(entries.len());
        for entry in entries {
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }
            // A bare address is the common case in a compose file — one proxy,
            // one address — and writing `/32` after it is a step people forget.
            let parsed = entry
                .parse::<IpNet>()
                .or_else(|_| entry.parse::<IpAddr>().map(IpNet::from));
            nets.push(parsed.map_err(|_| entry.to_owned())?);
        }
        Ok(Self {
            nets,
            any_peer: false,
        })
    }

    /// The deprecated boolean: believe whoever connected, about one hop only.
    #[must_use]
    pub const fn any_peer() -> Self {
        Self {
            nets: Vec::new(),
            any_peer: true,
        }
    }

    /// Whether nothing is trusted, which is the default.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.nets.is_empty() && !self.any_peer
    }

    /// Whether the list contains a block that covers every address.
    ///
    /// `0.0.0.0/0` is the entry somebody reaches for when the real proxy address
    /// is awkward to find, and it does more than the boolean it replaced: every
    /// hop in the chain is then one of ours too, so the walk runs all the way to
    /// the leftmost entry — the one the caller wrote. It is worth saying so out
    /// loud rather than letting it look like a configured proxy.
    #[must_use]
    pub fn covers_everything(&self) -> bool {
        self.nets.iter().any(|net| net.prefix_len() == 0)
    }

    /// Whether an address inside the chain is one of ours.
    fn is_proxy(&self, address: IpAddr) -> bool {
        self.nets.iter().any(|net| net.contains(&address))
    }

    /// Whether the thing on the other end of the socket may speak for somebody.
    fn peer_is_proxy(&self, peer: IpAddr) -> bool {
        self.any_peer || self.is_proxy(peer)
    }

    /// Resolves who a request is from.
    ///
    /// `peer` is `None` when the router is called directly rather than served
    /// over a socket, which is how the tests drive it. There is no address to
    /// report then, and none to trust.
    #[must_use]
    pub fn resolve(&self, peer: Option<IpAddr>, headers: &HeaderMap) -> Origin {
        let Some(peer) = peer else {
            return Origin {
                source: "unknown".to_owned(),
                via_trusted_proxy: false,
            };
        };

        if !self.peer_is_proxy(peer) {
            // Not a proxy of ours, so whatever it says about somebody else is
            // just something it wrote.
            return Origin {
                source: peer.to_string(),
                via_trusted_proxy: false,
            };
        }

        let mut client = peer;
        for hop in forwarded_chain(headers).rev() {
            let Ok(address) = hop.parse::<IpAddr>() else {
                // A chain we cannot read is a chain we stop reading. Keeping the
                // last address we did understand is the conservative end of it.
                break;
            };
            client = address;
            if !self.is_proxy(address) {
                break;
            }
        }

        Origin {
            source: client.to_string(),
            via_trusted_proxy: true,
        }
    }
}

/// The forwarded chain, left to right, across however many headers carried it.
///
/// A chain may arrive as one header with commas or as several headers, and both
/// mean the same thing — so both are read, in the order they were received.
fn forwarded_chain(headers: &HeaderMap) -> impl DoubleEndedIterator<Item = &str> {
    headers
        .get_all("x-forwarded-for")
        .into_iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|hop| !hop.is_empty())
        .collect::<Vec<_>>()
        .into_iter()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(chain: &[&str]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for value in chain {
            headers.append(
                "x-forwarded-for",
                value.parse().expect("a header value in the test"),
            );
        }
        headers
    }

    fn ip(value: &str) -> IpAddr {
        value.parse().expect("an address in the test")
    }

    fn trusting(entries: &[&str]) -> TrustedProxies {
        let owned: Vec<String> = entries.iter().map(|entry| (*entry).to_owned()).collect();
        TrustedProxies::parse(&owned).expect("a list in the test")
    }

    #[test]
    fn nothing_is_trusted_by_default() {
        assert!(TrustedProxies::default().is_empty());
    }

    #[test]
    fn a_bare_address_means_itself() {
        let proxies = trusting(&["10.0.0.5"]);
        assert!(proxies.is_proxy(ip("10.0.0.5")));
        assert!(!proxies.is_proxy(ip("10.0.0.6")));
    }

    #[test]
    fn a_block_covers_its_members() {
        let proxies = trusting(&["10.0.0.0/8", "::1/128"]);
        assert!(proxies.is_proxy(ip("10.9.9.9")));
        assert!(proxies.is_proxy(ip("::1")));
        assert!(!proxies.is_proxy(ip("192.0.2.1")));
    }

    #[test]
    fn something_that_is_neither_is_reported_with_the_entry() {
        let entries = vec!["10.0.0.0/8".to_owned(), "not an address".to_owned()];
        assert_eq!(
            TrustedProxies::parse(&entries).expect_err("should not parse"),
            "not an address"
        );
    }

    /// The default: a header from a stranger is a string a stranger wrote.
    #[test]
    fn an_untrusted_peer_is_not_believed() {
        let origin =
            TrustedProxies::default().resolve(Some(ip("203.0.113.9")), &headers(&["198.51.100.7"]));
        assert_eq!(origin.source, "203.0.113.9");
        assert!(!origin.via_trusted_proxy);
    }

    /// The measured hole. nginx appends, so the real client is on the right and
    /// anything to its left is whatever the caller wrote.
    #[test]
    fn a_prepended_address_is_discarded() {
        let origin = trusting(&["10.0.0.0/8"]).resolve(
            Some(ip("10.0.0.5")),
            &headers(&["198.51.100.7, 203.0.113.9"]),
        );
        assert_eq!(
            origin.source, "203.0.113.9",
            "the rightmost entry is the one the proxy saw"
        );
    }

    /// Two proxies of ours, then the client. Both hops are stepped over.
    #[test]
    fn a_chain_of_our_own_proxies_is_walked_through() {
        let origin = trusting(&["10.0.0.0/8"])
            .resolve(Some(ip("10.0.0.5")), &headers(&["203.0.113.9", "10.0.0.9"]));
        assert_eq!(origin.source, "203.0.113.9");
        assert!(origin.via_trusted_proxy);
    }

    /// Nothing but our own addresses means the request started inside. The
    /// leftmost is as far back as it goes.
    #[test]
    fn a_chain_that_is_all_ours_ends_at_the_leftmost() {
        let origin = trusting(&["10.0.0.0/8"])
            .resolve(Some(ip("10.0.0.5")), &headers(&["10.0.0.1, 10.0.0.2"]));
        assert_eq!(origin.source, "10.0.0.1");
    }

    /// Garbage in the chain stops the walk rather than being skipped over:
    /// skipping would let anyone reach past a proxy by writing something
    /// unparsable in front of the address they wanted to claim.
    #[test]
    fn an_unreadable_hop_stops_the_walk() {
        let origin = trusting(&["10.0.0.0/8"]).resolve(
            Some(ip("10.0.0.5")),
            &headers(&["203.0.113.9, not-an-address, 10.0.0.9"]),
        );
        assert_eq!(origin.source, "10.0.0.9");
    }

    /// The deprecated boolean believes whoever connected, but only about the one
    /// hop it saw — so the chain in front of it is still not walked.
    #[test]
    fn the_deprecated_boolean_believes_one_hop_only() {
        let origin = TrustedProxies::any_peer().resolve(
            Some(ip("10.0.0.5")),
            &headers(&["198.51.100.7, 203.0.113.9"]),
        );
        assert_eq!(origin.source, "203.0.113.9");
        assert!(origin.via_trusted_proxy);
    }

    #[test]
    fn no_peer_means_no_address_and_no_trust() {
        let origin = TrustedProxies::any_peer().resolve(None, &headers(&["198.51.100.7"]));
        assert_eq!(origin.source, "unknown");
        assert!(!origin.via_trusted_proxy);
    }

    /// A proxy that sets the header but leaves it empty must not collapse every
    /// caller onto one throttle key.
    #[test]
    fn an_empty_chain_falls_back_to_the_peer() {
        let origin = trusting(&["10.0.0.0/8"]).resolve(Some(ip("10.0.0.5")), &headers(&[""]));
        assert_eq!(origin.source, "10.0.0.5");
        assert!(origin.via_trusted_proxy);
    }

    #[test]
    fn a_block_that_covers_everything_is_recognised() {
        assert!(trusting(&["0.0.0.0/0"]).covers_everything());
        assert!(trusting(&["::/0"]).covers_everything());
        assert!(!trusting(&["10.0.0.0/8", "127.0.0.1"]).covers_everything());
    }

    /// A chain split across several headers means the same as one with commas.
    #[test]
    fn several_headers_are_one_chain() {
        let origin = trusting(&["10.0.0.0/8"]).resolve(
            Some(ip("10.0.0.5")),
            &headers(&["198.51.100.7", "203.0.113.9"]),
        );
        assert_eq!(origin.source, "203.0.113.9");
    }
}
