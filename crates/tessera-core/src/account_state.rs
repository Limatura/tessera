//! Where an account is in its life.

use core::{fmt, str::FromStr};

use crate::{EmailAddress, UserId, Username, error::DomainError};

/// Whether an account works, is suspended, or is gone.
///
/// # Why deletion is a state and not a missing row
///
/// `shares.user_id` is `ON DELETE SET NULL`, so removing the row would turn
/// every share that account uploaded into one indistinguishable from an
/// anonymous upload. "Nobody ever owned this" and "the owner is gone" are
/// different facts, and once merged they cannot be told apart again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum AccountState {
    /// Signs in, uploads, does whatever its roles allow.
    #[default]
    Active,
    /// Suspended: the person is stopped, their content is not.
    ///
    /// Their shares keep serving — whoever holds one of those links did nothing
    /// wrong. Stopping the content too is what a share's own `disabled_at` is
    /// for, and an administrator can reach every share to set it.
    Disabled,
    /// Gone. The row survives to keep the shares attributable and to record
    /// that an account was here; everything identifying has been scrubbed.
    Deleted,
}

impl AccountState {
    /// The value stored in the `state` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
            Self::Deleted => "deleted",
        }
    }

    /// Whether an account in this state may authenticate.
    ///
    /// One question asked in one place, rather than each call site deciding
    /// which states it minds — the two that are not `Active` are alike in every
    /// way that matters to a request.
    #[must_use]
    pub const fn may_sign_in(self) -> bool {
        matches!(self, Self::Active)
    }
}

impl fmt::Display for AccountState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for AccountState {
    type Err = DomainError;

    /// Reads a state back from a row.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::Malformed`] for anything else. Refused rather
    /// than defaulted to `Active`: a value this version does not recognise is
    /// what a downgrade leaves behind, and treating it as active would let a
    /// suspended account back in.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "active" => Ok(Self::Active),
            "disabled" => Ok(Self::Disabled),
            "deleted" => Ok(Self::Deleted),
            _ => Err(DomainError::Malformed {
                field: "state",
                reason: "must be `active`, `disabled` or `deleted`",
            }),
        }
    }
}

/// The name and address a deleted account is left with.
///
/// # Why values rather than `NULL`
///
/// Both columns are `NOT NULL` and carry a unique index. Making them nullable
/// would mean rebuilding the table on SQLite for a case that a value handles
/// just as well — and a value keeps the index doing its job.
///
/// # What they are for
///
/// Freeing the originals, so somebody else may register that name and that
/// address. What is left identifies nobody: the identifier was already public
/// to anyone who could see the account, and `.invalid` is reserved precisely so
/// that it can never be a real domain (RFC 2606).
///
/// # Why these two are themselves reserved
///
/// The identifier is public — every response naming an account carries one. So
/// without reserving them, a person could read somebody else's, take the name
/// and address that account's deletion will one day need, and the unique index
/// would then refuse the deletion. The account becomes undeletable, by its
/// owner and by an administrator alike. Measured before it was fixed:
/// `UNIQUE constraint failed: users.username`.
///
/// Which is why this builds them through the privileged parsers. It is the one
/// caller allowed to produce what everybody else is refused.
#[must_use]
pub fn tombstone(id: UserId) -> (Username, EmailAddress) {
    let name = Username::parse_privileged(&format!("{}{id}", crate::username::TOMBSTONE_PREFIX))
        .unwrap_or_else(|_| unreachable!("a ULID behind the tombstone prefix is a valid name"));
    let email = EmailAddress::parse_privileged(&format!("{id}@{}", crate::email::TOMBSTONE_DOMAIN))
        .unwrap_or_else(|_| unreachable!("a ULID at the tombstone domain is a valid address"));
    (name, email)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_survives_a_round_trip() {
        for state in [
            AccountState::Active,
            AccountState::Disabled,
            AccountState::Deleted,
        ] {
            assert_eq!(state.as_str().parse::<AccountState>(), Ok(state), "{state}");
        }
    }

    /// A value this version does not know is what a downgrade leaves behind.
    /// Reading it as `Active` would let a suspended account sign in.
    #[test]
    fn an_unknown_state_is_refused_rather_than_assumed_active() {
        assert!("banished".parse::<AccountState>().is_err());
        assert!(String::new().parse::<AccountState>().is_err());
    }

    #[test]
    fn only_an_active_account_may_sign_in() {
        assert!(AccountState::Active.may_sign_in());
        assert!(!AccountState::Disabled.may_sign_in());
        assert!(!AccountState::Deleted.may_sign_in());
    }

    /// The scrubbed values have to parse, or the row they are written to stops
    /// being readable and the account becomes a corrupt row instead of a
    /// deleted one.
    #[test]
    fn a_tombstone_is_readable_and_says_nothing_about_anybody() {
        let id = UserId::generate();
        let (name, email) = tombstone(id);
        assert!(name.to_string().starts_with("gone-"));
        assert!(email.to_string().ends_with("@deleted.invalid"));
        assert_eq!(Username::parse_privileged(&name.to_string()), Ok(name));
        assert_eq!(
            EmailAddress::parse_privileged(&email.to_string()),
            Ok(email)
        );
    }

    /// The two halves of the fix, tied together.
    ///
    /// If the parsers ever stop refusing what this produces, somebody can take
    /// a tombstone that is not theirs and make that account undeletable.
    #[test]
    fn nobody_else_can_take_a_tombstone() {
        let (name, email) = tombstone(UserId::generate());
        assert!(
            Username::parse(&name.to_string()).is_err(),
            "the name a deletion needs must not be claimable"
        );
        assert!(
            EmailAddress::parse(&email.to_string()).is_err(),
            "the address a deletion needs must not be claimable"
        );
    }

    /// Two deleted accounts must not collide on the unique index.
    #[test]
    fn two_tombstones_differ() {
        let (first, first_email) = tombstone(UserId::generate());
        let (second, second_email) = tombstone(UserId::generate());
        assert_ne!(first, second);
        assert_ne!(first_email, second_email);
    }
}
