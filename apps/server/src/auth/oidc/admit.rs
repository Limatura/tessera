//! Turning a provider's claims into a name and a decision.
//!
//! Everything here is a pure function of the claims and the configuration. The
//! parts that need the database, and their order, are in `api/oidc.rs`.

use tessera_core::Username;

use super::claims::{self, Claims};
use crate::config::OidcConfig;

/// Whether the provider's answer meets the configured group requirement.
///
/// With no `required_group`, everyone the provider admits is admitted. With
/// one, the group must appear in the configured claim.
///
/// It fails closed in every way: a claim that is absent, empty or of an
/// unrecognised shape yields no groups, which never contain the required one.
/// A provider that silently stops releasing the claim locks users out, which
/// is the right direction to fail in.
#[must_use]
pub fn is_in_required_group(claims: &Claims, config: &OidcConfig) -> bool {
    let required = config.required_group.trim();
    if required.is_empty() {
        return true;
    }
    let found = claims::groups(claims.rest.get(config.group_claim.trim()));
    found.iter().any(|group| group == required)
}

/// A name for an account being created from a provider's claims.
///
/// Tries the provider's name for the user, then the address's local part,
/// then a fixed stem. Each candidate is coerced into a valid username instead
/// of refused: a first-time user cannot fix a name the provider chose, so an
/// unusable one has to be made usable here.
///
/// `taken` resolves collisions. It is asked in order, so the first free name
/// wins, and the suffix is a number because a random tail is unrecognisable
/// to its owner.
pub fn username_for(
    identity: &crate::auth::oauth::Identity,
    mut taken: impl FnMut(&Username) -> bool,
) -> Option<Username> {
    let email_local = identity
        .email
        .as_deref()
        .and_then(|address| address.split('@').next())
        .unwrap_or_default()
        .to_owned();

    let candidates = [
        identity.preferred_username.clone().unwrap_or_default(),
        email_local,
        identity.name.clone().unwrap_or_default(),
        "user".to_owned(),
    ];

    for candidate in &candidates {
        let Some(stem) = squeeze(candidate) else {
            continue;
        };
        if let Ok(name) = Username::parse(&stem)
            && !taken(&name)
        {
            return Some(name);
        }
        // Numbered, not random: `alice2` is recognisably the same person, a
        // random tail is not. The ceiling is a stopping condition, not a limit
        // anyone is expected to reach.
        for suffix in 2..=99_u32 {
            let mut numbered = stem.clone();
            numbered.truncate(Username::MAX_LENGTH - suffix.to_string().len());
            numbered.push_str(&suffix.to_string());
            if let Ok(name) = Username::parse(&numbered)
                && !taken(&name)
            {
                return Some(name);
            }
        }
    }
    None
}

/// Coerces a candidate into something [`Username::parse`] might accept.
///
/// Lowercases, drops disallowed characters, and trims to length. Returns
/// nothing when nothing usable is left, so the caller moves to the next
/// candidate instead of registering an empty name.
fn squeeze(raw: &str) -> Option<String> {
    let kept: String = raw
        .trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    // A name must start with a letter or digit, and trailing separators are
    // typical leftovers from an address local part.
    let trimmed = kept
        .trim_start_matches(|c: char| !c.is_ascii_alphanumeric())
        .trim_end_matches(['_', '-']);
    if trimmed.is_empty() {
        return None;
    }
    let mut stem: String = trimmed.chars().take(Username::MAX_LENGTH).collect();
    // Too short alone, though a numbered form would pass. Padding keeps a
    // two-letter provider name usable.
    while stem.chars().count() < Username::MIN_LENGTH {
        stem.push('0');
    }
    Some(stem)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn claims(fields: &serde_json::Value) -> Claims {
        let mut base = serde_json::json!({
            "iss": "https://id.example",
            "sub": "sub-1",
            "aud": "tessera",
            "exp": 1_785_000_300_i64,
        });
        for (key, value) in fields.as_object().expect("an object") {
            base[key] = value.clone();
        }
        serde_json::from_value(base).expect("claims")
    }

    fn config() -> OidcConfig {
        OidcConfig::default()
    }

    /// The claims above, converted to the shape everything downstream of the
    /// provider understands.
    fn identity(fields: &serde_json::Value) -> crate::auth::oauth::Identity {
        let claims = claims(fields);
        crate::auth::oauth::Identity {
            subject: claims.sub,
            email: claims.email,
            email_verified: claims.email_verified.unwrap_or(false),
            preferred_username: claims.preferred_username,
            name: claims.name,
        }
    }

    fn free(_name: &Username) -> bool {
        false
    }

    #[test]
    fn with_no_required_group_everyone_the_provider_admits_is_admitted() {
        assert!(is_in_required_group(
            &claims(&serde_json::json!({})),
            &config()
        ));
    }

    #[test]
    fn the_required_group_has_to_be_in_the_configured_claim() {
        let mut config = config();
        config.required_group = "tessera-users".to_owned();
        config.group_claim = "groups".to_owned();

        assert!(is_in_required_group(
            &claims(&serde_json::json!({"groups": ["staff", "tessera-users"]})),
            &config
        ));
        assert!(!is_in_required_group(
            &claims(&serde_json::json!({"groups": ["staff"]})),
            &config
        ));
    }

    /// A provider that stops releasing the claim locks users out instead of
    /// letting everyone in, which is the right direction to fail in.
    #[test]
    fn a_missing_claim_admits_nobody() {
        let mut config = config();
        config.required_group = "tessera-users".to_owned();
        config.group_claim = "groups".to_owned();

        assert!(!is_in_required_group(
            &claims(&serde_json::json!({})),
            &config
        ));
        assert!(!is_in_required_group(
            &claims(&serde_json::json!({"groups": []})),
            &config
        ));
        assert!(!is_in_required_group(
            &claims(&serde_json::json!({"groups": "not-a-list-we-know"})),
            &config
        ));
    }

    /// The claim is configurable because providers disagree about its name,
    /// and some use a URI.
    #[test]
    fn the_claim_is_looked_up_by_its_configured_name() {
        let mut config = config();
        config.required_group = "admins".to_owned();
        config.group_claim = "https://authentik/groups".to_owned();

        assert!(is_in_required_group(
            &claims(&serde_json::json!({"https://authentik/groups": ["admins"]})),
            &config
        ));
        assert!(!is_in_required_group(
            &claims(&serde_json::json!({"groups": ["admins"]})),
            &config
        ));
    }

    #[test]
    fn the_providers_own_name_is_preferred() {
        let name = username_for(
            &identity(&serde_json::json!({
                "preferred_username": "alice",
                "email": "someone.else@example.com",
            })),
            free,
        )
        .expect("a name");

        assert_eq!(name.as_str(), "alice");
    }

    #[test]
    fn the_address_is_the_next_best_thing() {
        let name = username_for(
            &identity(&serde_json::json!({"email": "alice.smith@example.com"})),
            free,
        )
        .expect("a name");

        assert_eq!(name.as_str(), "alicesmith");
    }

    /// A first-time user cannot fix a name the provider chose, so an unusable
    /// one is made usable, not refused.
    #[test]
    fn a_name_that_would_be_refused_is_squeezed_into_shape() {
        for (raw, expected) in [
            ("Alice Smith", "alicesmith"),
            ("_alice", "alice"),
            ("alice@example.com", "aliceexamplecom"),
            ("Ali", "ali"),
            ("a", "a00"),
            ("王小明", "user"),
        ] {
            let name = username_for(
                &identity(&serde_json::json!({"preferred_username": raw})),
                free,
            )
            .expect("a name");
            assert_eq!(name.as_str(), expected, "for {raw}");
        }
    }

    /// A reserved name is refused by `Username::parse`, so it falls through to
    /// the next candidate instead of being handed out.
    #[test]
    fn a_reserved_name_is_not_handed_out() {
        let name = username_for(
            &identity(&serde_json::json!({
                "preferred_username": "admin",
                "email": "someone@example.com",
            })),
            free,
        )
        .expect("a name");

        assert_ne!(name.as_str(), "admin");
    }

    #[test]
    fn a_taken_name_is_numbered_rather_than_randomised() {
        let existing: BTreeSet<String> = ["alice", "alice2"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();

        let name = username_for(
            &identity(&serde_json::json!({"preferred_username": "alice"})),
            |candidate| existing.contains(candidate.as_str()),
        )
        .expect("a name");

        assert_eq!(name.as_str(), "alice3");
    }

    /// A very long provider name still leaves room for the number.
    #[test]
    fn a_long_name_makes_room_for_its_suffix() {
        let long = "a".repeat(60);
        let existing: BTreeSet<String> = ["a".repeat(Username::MAX_LENGTH)].into_iter().collect();

        let name = username_for(
            &identity(&serde_json::json!({"preferred_username": long})),
            |candidate| existing.contains(candidate.as_str()),
        )
        .expect("a name");

        assert!(name.as_str().chars().count() <= Username::MAX_LENGTH);
        assert!(name.as_str().ends_with('2'));
    }

    /// With nothing usable anywhere, a name is still produced: refusing here
    /// would leave a user who cannot sign in and cannot do anything about it.
    #[test]
    fn there_is_always_a_last_resort() {
        let name = username_for(&identity(&serde_json::json!({})), free).expect("a name");

        assert_eq!(name.as_str(), "user");
    }

    /// Unless every numbered form is taken too. That is a real answer, better
    /// than handing a name to the wrong person.
    #[test]
    fn everything_taken_yields_nothing() {
        assert!(username_for(&identity(&serde_json::json!({})), |_taken| true).is_none());
    }
}
