//! Sending mail, when the instance is set up to.
//!
//! # The whole module is optional twice over
//!
//! Once at compile time (the `email` cargo feature, on by default because it
//! costs 37 KB) and once at run time (an empty `mail.host`, the default). Neither is the usual case
//! for the audience this is built for: one person on a NAS gets back into a forgotten account with
//! `reset-password` on the box, and never configures a relay at all.
//!
//! That is why [`Mailer::from_config`] returns `None` instead of failing, and
//! why every caller treats `None` as "this instance does not do that" and not
//! as an error to report.

/// Anything that stopped a message being handed to the relay.
#[derive(Debug, thiserror::Error)]
pub enum MailError {
    /// The address, subject or body was not something a message can be built
    /// from.
    #[error("could not build the message: {0}")]
    Build(String),

    /// The relay refused it, or could not be reached.
    #[error("could not send the message: {0}")]
    Send(String),

    /// `mail.tls` is `none` and the relay is not on a private network.
    #[error("refusing to send mail unencrypted to {0}, which is not a private or loopback address")]
    NotPrivate(String),
}

/// One outgoing message.
#[derive(Debug, Clone)]
pub struct Letter {
    /// Who it goes to.
    pub to: String,
    /// Subject line.
    pub subject: String,
    /// Plain text body.
    ///
    /// Text only, and that is a decision, not a shortcut. A password
    /// reset is one sentence and a link, HTML adds a way for it to render as a
    /// blank page in some clients, and a mail with a *hidden* link is exactly
    /// the shape people are told to distrust.
    pub body: String,
}

#[cfg(feature = "email")]
mod smtp {
    use std::net::{IpAddr, SocketAddr};

    use lettre::{
        AsyncSmtpTransport, AsyncTransport as _, Message, Tokio1Executor,
        message::header::ContentType, transport::smtp::authentication::Credentials,
    };

    use super::{Letter, MailError};
    use crate::config::{MailConfig, MailTls};

    /// A configured relay.
    #[derive(Clone)]
    pub struct Mailer {
        route: Route,
        from: String,
    }

    /// How a message reaches the relay.
    #[derive(Clone)]
    enum Route {
        /// Encrypted, one way or the other. Built once and reused.
        Tls(AsyncSmtpTransport<Tokio1Executor>),
        /// In the clear. Built per message, against the address the host
        /// resolves to at that moment, once that address is known to be
        /// private. See [`private_address`].
        Plain {
            host: String,
            port: u16,
            credentials: Option<Credentials>,
        },
    }

    impl std::fmt::Debug for Mailer {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            // The transport holds the credentials, so it does not get printed.
            formatter
                .debug_struct("Mailer")
                .field("from", &self.from)
                .finish_non_exhaustive()
        }
    }

    impl Mailer {
        /// Builds a relay from configuration, or `None` when there is none.
        ///
        /// # Errors
        ///
        /// When the host is set but unusable as a relay address.
        pub fn from_config(config: &MailConfig) -> Result<Option<Self>, MailError> {
            if !config.is_configured() {
                return Ok(None);
            }

            let credentials = (!config.username.is_empty())
                .then(|| Credentials::new(config.username.clone(), config.password.clone()));

            // By default, what the ports mean: 465 is implicit TLS from the
            // first byte, and submission on 587 opens in clear and upgrades.
            // Getting this the wrong way round produces a handshake error that
            // reads as a certificate problem, which is why it can be set.
            let builder = match config.tls.on(config.port) {
                MailTls::Implicit => AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host),
                MailTls::None => {
                    return Ok(Some(Self {
                        route: Route::Plain {
                            host: config.host.clone(),
                            port: config.port,
                            credentials,
                        },
                        from: config.from.clone(),
                    }));
                }
                MailTls::Auto | MailTls::Starttls => {
                    AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)
                }
            }
            .map_err(|error| MailError::Build(error.to_string()))?;

            let mut builder = builder.port(config.port);
            if let Some(credentials) = credentials {
                builder = builder.credentials(credentials);
            }

            Ok(Some(Self {
                route: Route::Tls(builder.build()),
                from: config.from.clone(),
            }))
        }

        /// Hands a message to the relay.
        ///
        /// # Errors
        ///
        /// [`MailError`] when the message cannot be built or the relay refuses.
        pub async fn send(&self, letter: Letter) -> Result<(), MailError> {
            let message =
                Message::builder()
                    .from(self.from.parse().map_err(|_| {
                        MailError::Build(format!("bad from address `{}`", self.from))
                    })?)
                    .to(letter
                        .to
                        .parse()
                        .map_err(|_| MailError::Build(format!("bad recipient `{}`", letter.to)))?)
                    .subject(letter.subject)
                    .header(ContentType::TEXT_PLAIN)
                    .body(letter.body)
                    .map_err(|error| MailError::Build(error.to_string()))?;

            let sent = match &self.route {
                Route::Tls(transport) => transport.send(message).await,
                Route::Plain {
                    host,
                    port,
                    credentials,
                } => {
                    let address = private_address(host, *port).await?;
                    let mut builder = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(
                        address.ip().to_string(),
                    )
                    .port(address.port());
                    if let Some(credentials) = credentials {
                        builder = builder.credentials(credentials.clone());
                    }
                    builder.build().send(message).await
                }
            };
            sent.map(|_| ())
                .map_err(|error| MailError::Send(error.to_string()))
        }
    }

    /// Where `host` resolves to, provided every address it resolves to is
    /// private or loopback.
    ///
    /// Resolved here rather than checked once in configuration, because a
    /// relay is usually named, `mailpit` or `postfix`, and a name can point
    /// somewhere else tomorrow. The address checked is the address connected
    /// to, so it cannot change in between.
    async fn private_address(host: &str, port: u16) -> Result<SocketAddr, MailError> {
        let addresses: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
            .await
            .map_err(|error| MailError::Send(format!("cannot resolve {host}: {error}")))?
            .collect();
        match addresses.first() {
            Some(first) if addresses.iter().all(|address| is_private(address.ip())) => Ok(*first),
            _ => Err(MailError::NotPrivate(host.to_owned())),
        }
    }

    /// Loopback, RFC 1918, link-local, or an IPv6 unique local address.
    fn is_private(ip: IpAddr) -> bool {
        match ip {
            IpAddr::V4(ip) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
            IpAddr::V6(ip) => match ip.to_ipv4_mapped() {
                Some(v4) => is_private(IpAddr::V4(v4)),
                None => ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local(),
            },
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn only_this_network_counts_as_private() {
            for ip in [
                "127.0.0.1",
                "10.1.2.3",
                "172.16.0.9",
                "192.168.1.10",
                "169.254.1.1",
                "::1",
                "fd00::1",
                "fe80::1",
                "::ffff:192.168.1.10",
            ] {
                assert!(is_private(ip.parse().expect("an address")), "{ip}");
            }
            for ip in ["8.8.8.8", "172.32.0.1", "2001:db8::1", "::ffff:8.8.8.8"] {
                assert!(!is_private(ip.parse().expect("an address")), "{ip}");
            }
        }

        #[tokio::test]
        #[ignore = "wants Mailpit on 127.0.0.1:11025"]
        async fn plain_mail_reaches_a_local_relay() {
            let config = MailConfig {
                host: "localhost".to_owned(),
                port: 11025,
                tls: MailTls::None,
                from: "tessera@example.com".to_owned(),
                public_url: "http://localhost".to_owned(),
                ..MailConfig::default()
            };
            let mailer = Mailer::from_config(&config)
                .expect("config")
                .expect("a mailer");
            mailer
                .send(Letter {
                    to: "someone@example.com".to_owned(),
                    subject: "plain".to_owned(),
                    body: "hello".to_owned(),
                })
                .await
                .expect("sent");
        }

        #[tokio::test]
        async fn a_public_relay_is_refused_in_the_clear() {
            assert!(private_address("localhost", 25).await.is_ok());
            assert!(matches!(
                private_address("8.8.8.8", 25).await,
                Err(MailError::NotPrivate(_))
            ));
        }
    }
}

#[cfg(not(feature = "email"))]
mod smtp {
    use super::{Letter, MailError};
    use crate::config::MailConfig;

    /// A relay, on a build with the `email` feature off.
    ///
    /// Constructing one is impossible, so every path that would send is dead
    /// code the optimiser removes. The type exists so the rest of the server
    /// compiles either way instead of being littered with `cfg`.
    #[derive(Debug, Clone)]
    pub struct Mailer(std::convert::Infallible);

    impl Mailer {
        /// Always `None`: this build cannot send mail.
        ///
        /// # Errors
        ///
        /// Never.
        pub const fn from_config(_config: &MailConfig) -> Result<Option<Self>, MailError> {
            Ok(None)
        }

        /// Unreachable: there is no way to hold one of these.
        ///
        /// # Errors
        ///
        /// Never.
        pub async fn send(&self, _letter: Letter) -> Result<(), MailError> {
            match self.0 {}
        }
    }
}

pub use smtp::Mailer;

/// Composes the message a password reset sends.
///
/// Kept here, not in the handler, so the wording is in one place and can be
/// read without the routing around it.
#[must_use]
pub fn password_reset_letter(to: &str, link: &str, minutes: u64) -> Letter {
    Letter {
        to: to.to_owned(),
        subject: "Reset your tessera password".to_owned(),
        body: format!(
            "Somebody asked to reset the password for this address.\n\n\
             Open this link to choose a new one:\n\n{link}\n\n\
             It works once and stops working in {minutes} minutes.\n\n\
             If this was not you, nothing has happened yet and you can ignore this — \
             the link cannot be used without opening it.\n"
        ),
    }
}

/// Composes the message that proves an account can receive mail at its
/// address.
///
/// Requested by a signed-in account, but the wording still tells a reader who
/// did not request it that ignoring it changes nothing.
#[must_use]
pub fn verification_letter(to: &str, link: &str, hours: u64) -> Letter {
    Letter {
        to: to.to_owned(),
        subject: "Confirm your tessera email address".to_owned(),
        body: format!(
            "Somebody signed in to tessera asked to confirm that this address is theirs.\n\n\
             Open this link to confirm it:\n\n{link}\n\n\
             It works once and stops working in {hours} hours.\n\n\
             If this was not you, ignore this — nothing changes unless the link is opened.\n"
        ),
    }
}

/// Composes the message that confirms a new registration.
///
/// The account does not exist until this link is followed, and the wording says
/// so. A reader who did not ask for this needs to know that ignoring it is
/// enough, or they go looking for an account to delete.
#[must_use]
pub fn registration_letter(to: &str, link: &str, hours: u64) -> Letter {
    Letter {
        to: to.to_owned(),
        subject: "Confirm your tessera account".to_owned(),
        body: format!(
            "Somebody asked to create a tessera account with this address.\n\n\
             Open this link to finish:\n\n{link}\n\n\
             It works once and stops working in {hours} hours.\n\n\
             If this was not you, ignore this — no account exists yet, and none \
             will be created unless the link is opened.\n"
        ),
    }
}

/// Composes the message sent when the address already has an account.
///
/// # Why this is sent at all
///
/// So that registration can answer the same way whichever it is. Telling the
/// caller "that address is taken" turns the form into a way of asking who has
/// an account here, a question about who uses this instance, answerable by
/// anyone.
///
/// # Why it mentions probing
///
/// A single one of these is noise: a person mistyping, or forgetting they had
/// signed up. Several is a signal, and the account holder is the only one in a
/// position to notice the difference. Saying nothing would mean the one person
/// who could tell never finds out.
#[must_use]
pub fn already_registered_letter(to: &str, reset_path: &str) -> Letter {
    Letter {
        to: to.to_owned(),
        subject: "Someone tried to register with your address".to_owned(),
        body: format!(
            "Somebody tried to create a tessera account with this address, but it \
             already has one.\n\n\
             Nothing has changed. No account was created, none was modified, and \
             nobody was told whether this address is registered — this message is \
             the only place that is said, and it goes only to you.\n\n\
             If it was you and you have forgotten the password, you can set a new \
             one here:\n\n{reset_path}\n\n\
             If it was not you, somebody is checking whether this address has an \
             account here. One message is probably a mistake. Several is somebody \
             trying repeatedly, and worth knowing about.\n"
        ),
    }
}

/// Whether this build can send at all.
///
/// Used at start-up to say something when the configuration asks for mail the
/// binary cannot deliver, a combination that is otherwise silent.
#[must_use]
pub const fn compiled_in() -> bool {
    cfg!(feature = "email")
}
