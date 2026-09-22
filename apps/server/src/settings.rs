//! Which settings the admin panel may change, and where a value came from.
//!
//! # The rule this module exists to keep
//!
//! **Anything stored in the database takes effect without a restart.** The
//! panel never has to say "saved, now restart the server", because a setting
//! that would need one is not offered.
//!
//! That is a promise about this list, not about the settings. A setting
//! is excluded when the running process cannot pick it up, and there are only
//! three reasons for that:
//!
//! - the socket is already bound (`server.host`, `server.port`)
//! - the pool is already built, and reading this table needs it (`database.*`)
//! - the log subscriber is installed once, before anything else (`telemetry.*`)
//!
//! Those stay in the environment, where a deployment sets them anyway.
//!
//! # Why secrets are excluded too
//!
//! A stored secret is plaintext in a table and comes back out of whatever
//! endpoint reads settings. Neither is worth it for values a deployment already
//! has a better place for.

use std::collections::BTreeMap;

use crate::config::Config;

/// Keys that cannot take effect until the process restarts.
///
/// Prefixes: `database` covers `database.url`, `database.max_connections` and
/// anything added under it later. A new setting is hot unless it lands under
/// one of these, the right default, since everything the request path
/// reads goes through the swapped configuration.
const FIXED_AT_BOOT: &[&str] = &[
    // The listener is already bound.
    "server.host",
    "server.port",
    // Reading the settings table needs the pool this describes.
    "database",
    // The subscriber is installed before the database is even opened.
    "telemetry",
    // Baked into the middleware stack when the router is built. `axum` reads
    // these when the layer is constructed, not per request, so a change would
    // sit in the table looking applied.
    //
    // `max_body_bytes` is the worst of the three, because half of it *would*
    // follow: the hand-rolled check in the upload path reads the current
    // configuration and `DefaultBodyLimit` does not. An operator raising it
    // would find large uploads still refused by a layer whose number no longer
    // matches the panel's.
    "server.cors_allowed_origins",
    "server.max_body_bytes",
    // The SPA is read off disk once and embedded in the router's state.
    "server.assets_dir",
    // Read once to build the sealer every stored backend is opened with.
    // Changing it at runtime would leave the process holding backends it can no
    // longer re-seal, and the panel reporting a key that opens nothing.
    "security.secret_key",
    // Whether the browsable docs are mounted is decided while building the
    // router. The contract at `/openapi.json` is served either way.
    "features.docs",
];

/// Keys that hold a credential, and so are never stored here.
///
/// Whole keys, not prefixes. `mail.host` is not a secret and an operator has
/// every reason to change it from the panel, so excluding all of `mail`
/// would cost more than it protects.
const SECRETS: &[&str] = &[
    // Only read on first run, and only to create the first account. Storing it
    // would keep a password in a table for ever to do a job already done.
    "auth.admin_password",
    // Both carry credentials in the URL: `postgres://user:pass@…`,
    // `s3://key:secret@…`.
    "database.url",
    "storage.url",
    // Opens every stored backend credential. A panel that could read it back
    // would make the encryption decorative.
    "security.secret_key",
];

/// Keys whose value is encrypted before it is stored, and never read back out.
///
/// # Why these are stored at all when the others are not
///
/// [`SECRETS`] holds keys that must not reach this table in any form. These are
/// different: they are credentials an operator has every reason to set from the
/// panel, and the alternative is worse. `mail.host` is already changeable and
/// `mail.password` was not, so configuring a relay meant doing half of it in the
/// panel and half in the environment, then restarting for the half that could
/// not be done in the panel.
///
/// Recoverable and stored means a database dump is a credential leak, unless
/// what opens it lives somewhere the dump does not. That is
/// [`Sealer`](crate::security::secret::Sealer), keyed by `security.secret_key`
/// from the environment. Without a key, storing one of these is refused rather
/// than written in plaintext, which is what adding a storage backend already
/// does.
///
/// # What "never read back" means
///
/// The panel may write these and may be told whether one is set. It is never
/// given the value, so an endpoint that lists settings cannot hand a relay
/// password to anybody who can read settings.
const SEALED: &[&str] = &[
    // The identity provider's own secret.
    "auth.oidc.client_secret",
    // The relay's credentials. See above for why these are here and not in
    // `SECRETS`.
    "mail.username",
    "mail.password",
    // The same thing for the two providers that have no discovery document.
    "auth.github.client_secret",
    "auth.discord.client_secret",
    // Whatever the scanning service takes as a credential.
    "scan.authorization",
];

/// Whether a key's value is encrypted at rest and withheld from the panel.
#[must_use]
pub fn is_sealed(key: &str) -> bool {
    SEALED.contains(&key)
}

/// Why a setting is not offered in the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// It cannot take effect until the process restarts.
    FixedAtBoot,
    /// It holds a credential.
    Secret,
    /// There is no such setting.
    Unknown,
}

/// Whether `key` may be stored, and why not when it may not.
///
/// # Errors
///
/// Returns the [`Refusal`] that applies.
pub fn admits(key: &str) -> Result<(), Refusal> {
    if !keys().iter().any(|known| known == key) {
        return Err(Refusal::Unknown);
    }
    if SECRETS.contains(&key) {
        return Err(Refusal::Secret);
    }
    if FIXED_AT_BOOT
        .iter()
        .any(|fixed| key == *fixed || key.starts_with(&format!("{fixed}.")))
    {
        return Err(Refusal::FixedAtBoot);
    }
    Ok(())
}

/// Every setting this binary has, as dotted keys.
///
/// Read off the default configuration, not written down. A written list goes
/// stale the first time a field is added without it, and the symptom is a
/// setting the panel silently cannot see.
#[must_use]
pub fn keys() -> Vec<String> {
    let mut found = Vec::new();
    // A `Config` that will not serialise is a bug in this crate, not a runtime
    // condition, but the key list is read while deciding what an operator may
    // change, so returning nothing is the safe way to be wrong: the panel
    // offers no settings instead of the wrong ones.
    let json = serde_json::to_value(Config::default()).unwrap_or(serde_json::Value::Null);
    walk("", &json, &mut found);
    found
}

/// The keys the panel may change.
#[must_use]
pub fn changeable() -> Vec<String> {
    keys()
        .into_iter()
        .filter(|key| admits(key).is_ok())
        .collect()
}

fn walk(prefix: &str, value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        // A nested object is a group of settings, not a setting. A map whose
        // own keys are data never reaches here, because it is not an object:
        // `server.cors_allowed_origins` is a list, not a group.
        serde_json::Value::Object(fields) => {
            for (name, nested) in fields {
                let key = if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{prefix}.{name}")
                };
                walk(&key, nested, out);
            }
        }
        _ => out.push(prefix.to_owned()),
    }
}

/// Drops anything the panel is not allowed to have changed.
///
/// Applied to what comes *out* of the database as well as what goes in. A row
/// can predate a key moving onto one of the lists above, or arrive by hand
/// through `sqlite3`, and neither may reintroduce a value this binary has
/// decided not to honour.
#[must_use]
pub fn admitted_only(
    stored: BTreeMap<String, serde_json::Value>,
) -> BTreeMap<String, serde_json::Value> {
    stored
        .into_iter()
        .filter(|(key, _)| match admits(key) {
            Ok(()) => true,
            Err(refusal) => {
                tracing::warn!(setting = %key, ?refusal, "ignoring a stored setting");
                false
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{Refusal, admits, changeable, describe, is_sealed, keys};

    /// The list is derived, so this is really asking whether `Config` still
    /// serialises the way the walk expects.
    #[test]
    fn the_keys_come_from_the_configuration_itself() {
        let found = keys();
        assert!(found.contains(&"share.code_length".to_owned()));
        assert!(
            found.contains(&"auth.hashing.memory_kib".to_owned()),
            "nested"
        );
        assert!(
            !found.iter().any(|key| key.ends_with('.')),
            "a group should never be listed as a setting: {found:?}"
        );
    }

    /// The whole promise of the table: if the panel offers it, changing it
    /// works. Anything the running process cannot pick up has to be refused
    /// here, or the panel will report a save that did nothing.
    #[test]
    fn what_the_process_cannot_pick_up_is_refused() {
        for key in [
            "server.port",
            "server.host",
            "database.url",
            "database.max_connections",
            "telemetry.format",
            // Found by moving the configuration behind a swap and letting the
            // compiler point at every read that happens once. Each of these is
            // consumed while the router is built.
            "server.cors_allowed_origins",
            "server.max_body_bytes",
            "server.assets_dir",
            "features.docs",
        ] {
            assert!(
                matches!(admits(key), Err(Refusal::FixedAtBoot | Refusal::Secret)),
                "`{key}` cannot take effect without a restart and must not be offered"
            );
        }
    }

    #[test]
    fn credentials_that_cannot_be_stored_at_all_are_refused() {
        for key in ["auth.admin_password", "storage.url", "security.secret_key"] {
            assert!(
                matches!(admits(key), Err(Refusal::Secret | Refusal::FixedAtBoot)),
                "`{key}` holds a credential and must not be stored"
            );
        }
    }

    /// The other kind: written from the panel, encrypted at rest, and never
    /// handed back. Being editable is the point, so these must not be refused.
    #[test]
    fn sealed_credentials_are_storable_and_never_returned() {
        for key in ["mail.password", "mail.username", "auth.oidc.client_secret"] {
            assert_eq!(admits(key), Ok(()), "`{key}` must be settable");
            assert!(is_sealed(key), "`{key}` must be sealed");
        }
    }

    /// The key that opens the others can never be one of them.
    #[test]
    fn the_sealing_key_is_not_itself_sealed() {
        assert!(!is_sealed("security.secret_key"));
        assert!(admits("security.secret_key").is_err());
    }

    /// A sealed value never leaves as itself. What a screen gets is whether one
    /// is set.
    #[test]
    fn a_sealed_value_is_described_as_a_boolean() {
        let overrides = BTreeMap::from([(
            "mail.password".to_owned(),
            serde_json::json!("a-relay-password"),
        )]);

        let described = describe(&overrides);
        let view = described
            .iter()
            .find(|view| view.key == "mail.password")
            .expect("the setting");

        assert!(view.secret);
        assert!(view.editable);
        assert_eq!(view.value, serde_json::json!(true));
        assert!(
            !described
                .iter()
                .any(|view| view.value.as_str() == Some("a-relay-password")),
            "a credential reached the description"
        );
    }

    /// And an unset one says so, rather than looking configured.
    #[test]
    fn an_unset_sealed_value_says_it_is_unset() {
        let view = describe(&BTreeMap::new());
        let found = view
            .iter()
            .find(|view| view.key == "mail.password")
            .expect("the setting");

        assert_eq!(found.value, serde_json::json!(false));
    }

    #[test]
    fn a_key_that_does_not_exist_is_refused() {
        assert_eq!(admits("share.nonsense"), Err(Refusal::Unknown));
    }

    /// The ordinary settings, the reason the feature exists, have to be in.
    #[test]
    fn the_settings_an_operator_actually_changes_are_offered() {
        let offered = changeable();
        for key in [
            "share.code_length",
            "share.max_text_bytes",
            "share.access_log.enabled",
            "auth.session_ttl_hours",
            "features.anonymous_upload",
            "mail.host",
        ] {
            assert!(
                offered.contains(&key.to_owned()),
                "`{key}` should be offered"
            );
        }
    }
}

/// Which layer a setting's value came from.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Unset; this is what the binary ships with.
    Default,
    /// An environment variable.
    Environment,
    /// Changed in the admin panel.
    Panel,
}

/// One setting, as a screen needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    /// Dotted key.
    pub key: String,
    /// What the instance is using.
    pub value: serde_json::Value,
    /// Which layer supplied it.
    pub source: Source,
    /// Whether the panel may change it.
    pub editable: bool,
    /// Why not, when it may not.
    pub refusal: Option<Refusal>,
    /// Whether this holds a credential the panel may write and never read.
    ///
    /// When true, `value` says whether one is set and never what it is. A
    /// screen needs that much to draw "configured" beside a field it is asking
    /// somebody to fill in again.
    pub secret: bool,
}

/// Every setting, with its value and where that value came from.
///
/// # Why the source is worked out by comparison
///
/// figment can name the provider that supplied a key, but only as a display
/// string ("`TESSERA_` environment variable(s)"), and matching on that would
/// tie this to how a dependency phrases itself. Building the stack one layer at
/// a time and seeing where the answer changes needs no such agreement.
///
/// An environment variable set to the value the default already had reports as
/// the default, the honest answer: nothing observable distinguishes them.
#[must_use]
pub fn describe(overrides: &BTreeMap<String, serde_json::Value>) -> Vec<View> {
    let defaults = Config::defaults();
    let with_env = Config::defaults().merge(Config::env_provider());
    let stored = admitted_only(overrides.clone());

    keys()
        .into_iter()
        .map(|key| {
            let refusal = admits(&key).err();
            let at = |figment: &figment::Figment| figment.find_value(&key).ok();

            let (value, source) = stored.get(&key).map_or_else(
                || {
                    let base = at(&defaults);
                    let env = at(&with_env);
                    let source = if env == base {
                        Source::Default
                    } else {
                        Source::Environment
                    };
                    (
                        env.and_then(|value| serde_json::to_value(value).ok())
                            .unwrap_or(serde_json::Value::Null),
                        source,
                    )
                },
                |value| (value.clone(), Source::Panel),
            );

            // A credential is named, never handed back. `database.url` and
            // `storage.url` carry a password inside them, and this list is read
            // by an endpoint and printed by a command, both of which end up
            // pasted into issues. Whoever needs the value has the environment
            // it came from.
            let value = if refusal == Some(Refusal::Secret) {
                serde_json::Value::Null
            } else {
                value
            };

            // A sealed key is editable, so it is not caught above, and by the
            // time it reaches here it has been opened. What a screen needs is
            // whether one is set, which is a boolean; the value itself would
            // be a relay password in a list somebody pastes into an issue.
            let secret = is_sealed(&key);
            let value = if secret {
                serde_json::Value::Bool(value.as_str().is_some_and(|held| !held.trim().is_empty()))
            } else {
                value
            };

            View {
                key,
                value,
                source,
                editable: refusal.is_none(),
                refusal,
                secret,
            }
        })
        .collect()
}

#[cfg(test)]
mod describe_tests {
    use std::collections::BTreeMap;

    use super::{Source, describe};

    /// The panel shows where a value came from so that "I set this and it did
    /// not change" has an answer on the screen instead of in a support thread.
    #[test]
    fn each_layer_is_named_correctly() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("TESSERA_SHARE__TICKET_TTL_HOURS", "9");

            let mut stored = BTreeMap::new();
            stored.insert("share.max_text_bytes".to_owned(), 4096.into());

            let described = describe(&stored);
            let find = |key: &str| {
                described
                    .iter()
                    .find(|view| view.key == key)
                    .unwrap_or_else(|| panic!("{key} should be described"))
            };

            assert_eq!(find("share.ticket_ttl_hours").source, Source::Environment);
            assert_eq!(find("share.max_text_bytes").source, Source::Panel);
            assert_eq!(find("share.inline_threshold_bytes").source, Source::Default);
            Ok(())
        });
    }

    /// The value shown has to be the one in force, or the screen is describing
    /// an instance that does not exist.
    #[test]
    fn the_value_shown_is_the_one_in_force() {
        figment::Jail::expect_with(|_jail| {
            let mut stored = BTreeMap::new();
            stored.insert("share.code_length".to_owned(), 11.into());

            let described = describe(&stored);
            let view = described
                .iter()
                .find(|view| view.key == "share.code_length")
                .expect("described");
            assert_eq!(view.value, serde_json::json!(11), "the panel's value wins");
            Ok(())
        });
    }

    /// Settings the panel cannot change still appear, marked. Hiding them
    /// prompts the question of where the port setting went.
    #[test]
    fn what_cannot_be_changed_is_still_listed() {
        figment::Jail::expect_with(|_jail| {
            let described = describe(&BTreeMap::new());

            let port = described
                .iter()
                .find(|view| view.key == "server.port")
                .expect("the port should be listed even though it is fixed");
            assert!(!port.editable);
            assert!(port.refusal.is_some());

            let ordinary = described
                .iter()
                .find(|view| view.key == "share.code_length")
                .expect("described");
            assert!(ordinary.editable);
            Ok(())
        });
    }
}

#[cfg(test)]
mod secrecy_tests {
    use std::collections::BTreeMap;

    use super::{Refusal, describe};

    /// A credential must not come back out of the description. Both readers,
    /// the admin endpoint and the `settings list` command, produce output that
    /// ends up pasted into issues, and `database.url` carries the password.
    #[test]
    fn the_value_of_a_secret_is_withheld() {
        figment::Jail::expect_with(|jail| {
            jail.set_env(
                "TESSERA_DATABASE__URL",
                "postgres://tessera:hunter2@localhost/tessera",
            );

            let described = describe(&BTreeMap::new());
            let url = described
                .iter()
                .find(|view| view.key == "database.url")
                .expect("listed");

            assert_eq!(url.refusal, Some(Refusal::Secret));
            assert!(url.value.is_null(), "the value must not be handed back");
            assert!(
                !format!("{described:?}").contains("hunter2"),
                "the password must not appear anywhere in the description"
            );
            Ok(())
        });
    }

    /// And the ordinary settings still carry their values, or the screen is
    /// useless.
    #[test]
    fn an_ordinary_setting_still_shows_its_value() {
        figment::Jail::expect_with(|_jail| {
            let described = describe(&BTreeMap::new());
            let code = described
                .iter()
                .find(|view| view.key == "share.code_length")
                .expect("listed");
            assert_eq!(code.value, serde_json::json!(8));
            Ok(())
        });
    }
}
