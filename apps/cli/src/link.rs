//! Reading what a person pastes: a whole link, or a bare code.

use tessera_crypto::ShareKey;
use url::Url;

use crate::{Error, Result};

/// A share, as a person identified it.
#[derive(Debug)]
pub struct Target {
    /// The instance it lives on.
    pub server: Url,
    /// Its retrieval code.
    pub code: String,
    /// The key from after `#`, when the link carried one.
    pub key: Option<ShareKey>,
}

/// Reads `input` as a link, or as a code on `server`.
///
/// # Errors
///
/// [`Error::Usage`] for a link that is not a share's, a key after `#` that does
/// not decode, or a bare code with no server to look it up on.
pub fn parse(input: &str, server: Option<&Url>) -> Result<Target> {
    let Ok(url) = Url::parse(input) else {
        let server = server.ok_or_else(|| {
            Error::Usage(format!(
                "`{input}` is not a link; give the whole link, or the code with --server"
            ))
        })?;
        return Ok(Target {
            server: server.clone(),
            code: input.trim().to_owned(),
            key: None,
        });
    };

    let mut segments = url.path_segments().into_iter().flatten();
    let code = match (segments.next(), segments.next(), segments.next()) {
        (Some("s"), Some(code), None) if !code.is_empty() => code.to_owned(),
        _ => {
            return Err(Error::Usage(format!(
                "`{input}` is not a share link; those look like https://host/s/CODE"
            )));
        }
    };
    let key = url
        .fragment()
        .filter(|fragment| !fragment.is_empty())
        .map(|fragment| {
            ShareKey::parse(fragment).map_err(|_| {
                Error::Usage(
                    "the part of the link after `#` is not a key; was it cut off?".to_owned(),
                )
            })
        })
        .transpose()?;

    let mut server = url;
    server.set_path("/");
    server.set_query(None);
    server.set_fragment(None);
    Ok(Target { server, code, key })
}

/// An instance as a person names it: a whole URL, or only its host, which
/// means HTTPS.
///
/// # Errors
///
/// When it is neither.
pub fn instance(text: &str) -> Result<Url> {
    let text = text.trim();
    let written = if text.contains("://") {
        text.to_owned()
    } else {
        format!("https://{text}")
    };
    Url::parse(&written)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https") && url.host().is_some())
        .ok_or_else(|| {
            Error::Usage(format!(
                "`{text}` is not an instance; give its address, like share.example.com"
            ))
        })
}

/// The link to a share, with its key after `#` when there is one to carry.
#[must_use]
pub fn to_share(server: &Url, code: &str, key: Option<&ShareKey>) -> String {
    let mut link = server.clone();
    link.set_path(&format!("/s/{code}"));
    link.set_query(None);
    link.set_fragment(key.map(ShareKey::encode).as_deref());
    link.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_instance_can_be_named_by_its_host() {
        let read = |text| instance(text).expect(text).to_string();
        assert_eq!(read("share.example.com"), "https://share.example.com/");
        assert_eq!(
            read("share.example.com:8443"),
            "https://share.example.com:8443/"
        );
        assert_eq!(read("http://localhost:3000"), "http://localhost:3000/");
        assert_eq!(
            read(" https://share.example.com/ "),
            "https://share.example.com/"
        );
        assert!(instance("ftp://share.example.com").is_err());
        assert!(instance("").is_err());
    }

    const KEY: &str = "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc";

    #[test]
    fn a_whole_link_gives_the_server_the_code_and_the_key() {
        let target = parse(&format!("https://files.example/s/0ZPQ4M7K#{KEY}"), None).unwrap();
        assert_eq!(target.server.as_str(), "https://files.example/");
        assert_eq!(target.code, "0ZPQ4M7K");
        assert_eq!(target.key.unwrap().encode(), KEY);
    }

    #[test]
    fn a_link_round_trips() {
        let server = Url::parse("http://127.0.0.1:12923").unwrap();
        let key = ShareKey::parse(KEY).unwrap();
        let link = to_share(&server, "0ZPQ4M7K", Some(&key));
        assert_eq!(link, format!("http://127.0.0.1:12923/s/0ZPQ4M7K#{KEY}"));
        assert_eq!(parse(&link, None).unwrap().key.unwrap().encode(), KEY);
    }

    #[test]
    fn a_bare_code_needs_a_server() {
        assert!(matches!(parse("0ZPQ4M7K", None), Err(Error::Usage(_))));
        let server = Url::parse("https://files.example").unwrap();
        let target = parse("0ZPQ4M7K", Some(&server)).unwrap();
        assert_eq!(target.code, "0ZPQ4M7K");
        assert!(target.key.is_none());
    }

    #[test]
    fn a_cut_off_key_is_said_to_be_one() {
        let refusal = parse("https://files.example/s/0ZPQ4M7K#BwcHBwcH", None).unwrap_err();
        assert!(refusal.to_string().contains("cut off"), "{refusal}");
    }

    #[test]
    fn a_link_that_is_not_a_share_is_refused() {
        assert!(parse("https://files.example/admin", None).is_err());
    }
}
