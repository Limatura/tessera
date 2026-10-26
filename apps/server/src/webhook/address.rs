//! Keeping an account's webhooks off this instance's own network.
//!
//! A webhook is a request this server makes to a URL someone else chose. An
//! account could point one at `localhost`, the router, or a cloud provider's
//! metadata address, and use the server to reach what only it can. So an
//! account's webhooks only go to public addresses: checked when one is added,
//! and again on every send, after the name is resolved, so a name that
//! resolves somewhere else by then is still caught.
//!
//! An administrator's webhooks are not limited. Whoever runs the instance
//! already reaches its network, and a receiver on it is a reasonable thing to
//! want.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use url::{Host, Url};

/// Whether an address is on the public internet.
#[must_use]
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_v4(ip),
        IpAddr::V6(ip) => match ip.to_ipv4_mapped() {
            Some(v4) => is_public_v4(v4),
            None => is_public_v6(ip),
        },
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [first, second, ..] = ip.octets();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_multicast()
        // "This network", shared address space for carrier NAT, benchmarking,
        // and everything from 240 up.
        || first == 0
        || (first == 100 && (64..128).contains(&second))
        || (first == 198 && (18..20).contains(&second))
        || first >= 240)
}

const fn is_public_v6(ip: Ipv6Addr) -> bool {
    let [first, second, ..] = ip.segments();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || ip.is_unique_local()
        || ip.is_unicast_link_local()
        // Documentation.
        || (first == 0x2001 && second == 0x0db8))
}

/// Why a URL is not one an account's webhook may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// Its host is an address that is not public, or a name for this machine.
    Private,
}

/// Checks the parts of a URL that can be checked without resolving it.
///
/// # Errors
///
/// [`Refused::Private`] when the host is a non-public address or `localhost`.
pub fn check(url: &Url) -> Result<(), Refused> {
    match url.host() {
        Some(Host::Ipv4(ip)) if !is_public(IpAddr::V4(ip)) => Err(Refused::Private),
        Some(Host::Ipv6(ip)) if !is_public(IpAddr::V6(ip)) => Err(Refused::Private),
        Some(Host::Domain(name)) => {
            let name = name.trim_end_matches('.').to_ascii_lowercase();
            if name == "localhost" || name.ends_with(".localhost") {
                Err(Refused::Private)
            } else {
                Ok(())
            }
        }
        _ => Ok(()),
    }
}

/// Resolves names to their public addresses only, and fails a name that has
/// none.
///
/// What makes the check hold on every send: the connection is made to an
/// address this returned, so a name that resolved to a public address when
/// the webhook was added and to a private one now is refused now.
#[derive(Debug, Clone, Copy, Default)]
pub struct PublicOnly;

impl Resolve for PublicOnly {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let public: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0))
                .await?
                .filter(|address| is_public(address.ip()))
                .collect();
            if public.is_empty() {
                return Err(format!("{host} has no public address").into());
            }
            let addresses: Addrs = Box::new(public.into_iter());
            Ok(addresses)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_and_special_addresses_are_not_public() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "255.255.255.255",
            "::1",
            "fd00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "::ffff:192.168.0.1",
        ] {
            assert!(
                !is_public(ip.parse().unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))),
                "{ip}"
            );
        }
        for ip in ["1.1.1.1", "162.159.135.232", "2606:4700::1111"] {
            assert!(
                is_public(ip.parse().unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))),
                "{ip}"
            );
        }
    }

    #[test]
    fn a_url_naming_this_machine_is_refused() {
        for url in [
            "http://127.0.0.1:8080/hook",
            "http://[::1]/hook",
            "http://localhost/hook",
            "http://app.localhost./hook",
            "http://192.168.1.10/hook",
        ] {
            let url = Url::parse(url).ok();
            assert_eq!(url.as_ref().map(check), Some(Err(Refused::Private)));
        }
        let url = Url::parse("https://discord.com/api/webhooks/1/abc").ok();
        assert_eq!(url.as_ref().map(check), Some(Ok(())));
    }
}
