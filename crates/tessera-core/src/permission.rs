//! What an account is allowed to do.
//!
//! # Why an enum and not a table
//!
//! Every permission that exists corresponds to a check somewhere in this
//! program. A row naming one the code does not check grants nothing; a check
//! against a permission with no row can never pass. So the set is decided by
//! the code, and writing it down twice would only create the question of which
//! copy is right.
//!
//! Roles, on the other hand, are rows: which permissions belong together is a
//! decision each instance makes, and the useful ones are not the ones anybody
//! designs up front.
//!
//! # Why `resource:verb`
//!
//! The string is what a database row and an API response carry, so it has to
//! survive a round trip and stay readable to anyone staring at a table
//! wondering why an account cannot do something.

use core::{fmt, str::FromStr};

use crate::error::DomainError;

/// One thing an account may be allowed to do.
///
/// Ordered as the surface is: the account's own things first, then the ones
/// that reach other people's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Permission {
    /// Upload something.
    SharesCreate,
    /// See one's own shares.
    SharesRead,
    /// Delete one's own shares.
    SharesDelete,
    /// Mint and revoke one's own API tokens.
    TokensManage,
    /// See every share on the instance, not only one's own.
    SharesReadAny,
    /// Remove any share on the instance.
    ///
    /// The only way to reach a share whose uploader is gone: ownership is what
    /// the delete route checks, and a deleted account owns nothing.
    SharesDeleteAny,
    /// List every account.
    UsersRead,
    /// Change an account that is not one's own.
    UsersWrite,
    /// Remove an account.
    UsersDelete,
    /// See the invite codes and how used up they are.
    InvitesRead,
    /// Mint an invite code.
    InvitesCreate,
    /// Revoke an invite code.
    InvitesDelete,
    /// See what roles exist and who holds them.
    RolesRead,
    /// Change what a role may do, and who holds it.
    RolesWrite,
    /// Read the instance-wide counts.
    StatsRead,
    /// See how the instance is configured, and where each value came from.
    ///
    /// Separate from writing, because the risks differ. The list names every
    /// knob this instance has, which suits an account that answers questions
    /// about it without being able to turn anything.
    SettingsRead,
    /// Change how the instance is configured.
    SettingsWrite,
    /// Read the server's own log.
    ///
    /// Its own permission, not part of reading settings: the log names the
    /// addresses, accounts and shares behind every request, which says far more
    /// about other people than the list of knobs does.
    LogsRead,
}

impl Permission {
    /// Every permission, in a stable order.
    ///
    /// Used to seed the administrator role and to answer "what could a role
    /// possibly contain", so it has to stay exhaustive. A variant added without
    /// being added here is a permission that can never be granted.
    pub const ALL: [Self; 18] = [
        Self::SharesCreate,
        Self::SharesRead,
        Self::SharesDelete,
        Self::TokensManage,
        Self::SharesReadAny,
        Self::SharesDeleteAny,
        Self::UsersRead,
        Self::UsersWrite,
        Self::UsersDelete,
        Self::InvitesRead,
        Self::InvitesCreate,
        Self::InvitesDelete,
        Self::RolesRead,
        Self::RolesWrite,
        Self::StatsRead,
        Self::SettingsRead,
        Self::SettingsWrite,
        Self::LogsRead,
    ];

    /// The spelling stored in a row and sent in a response.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SharesCreate => "shares:create",
            Self::SharesRead => "shares:read",
            Self::SharesDelete => "shares:delete",
            Self::TokensManage => "tokens:manage",
            Self::SharesReadAny => "shares:read:any",
            Self::SharesDeleteAny => "shares:delete:any",
            Self::UsersRead => "users:read",
            Self::UsersWrite => "users:write",
            Self::UsersDelete => "users:delete",
            Self::InvitesRead => "invites:read",
            Self::InvitesCreate => "invites:create",
            Self::InvitesDelete => "invites:delete",
            Self::RolesRead => "roles:read",
            Self::RolesWrite => "roles:write",
            Self::StatsRead => "stats:read",
            Self::SettingsRead => "settings:read",
            Self::SettingsWrite => "settings:write",
            Self::LogsRead => "logs:read",
        }
    }

    /// Whether this permission reaches beyond the holder's own things.
    ///
    /// Decides nothing, since the checks name permissions directly. It answers
    /// what an operator really asks of a role they are about to grant: does
    /// this let them touch other people?
    #[must_use]
    pub const fn is_privileged(self) -> bool {
        !matches!(
            self,
            Self::SharesCreate | Self::SharesRead | Self::SharesDelete | Self::TokensManage
        )
    }
}

impl fmt::Display for Permission {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for Permission {
    type Err = DomainError;

    /// Reads a permission back from a row.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::Malformed`] for a string no permission uses. A
    /// row naming one this version does not have is what a downgrade leaves
    /// behind, and refusing it beats silently granting nothing, which looks to
    /// an operator like the role stopped working.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|permission| permission.as_str() == value)
            .ok_or(DomainError::Malformed {
                field: "permission",
                rule: "unknown_permission",
                reason: "is not a permission this version has",
            })
    }
}

/// The permissions an account holds, across every role it is in.
///
/// A set, not a list. Two roles that both grant `shares:create` grant it
/// once, and the answer to "may they" must not depend on how many roles say so.
///
/// Backed by a sorted `Vec`, not a `HashSet`, because it holds at most
/// [`Permission::ALL`] and is asked far more often than built: a linear scan of
/// thirteen `u8`-sized values beats hashing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PermissionSet(Vec<Permission>);

impl PermissionSet {
    /// Everything: what the administrator role holds.
    #[must_use]
    pub fn all() -> Self {
        Self(Permission::ALL.to_vec())
    }

    /// Whether this set allows something.
    #[must_use]
    pub fn allows(&self, permission: Permission) -> bool {
        self.0.contains(&permission)
    }

    /// Whether it allows every one of them.
    #[must_use]
    pub fn allows_all(&self, permissions: &[Permission]) -> bool {
        permissions
            .iter()
            .all(|permission| self.allows(*permission))
    }

    /// Whether nothing is allowed.
    ///
    /// An account in no role at all: signed in and able to do nothing. Worth
    /// naming; see the note on the sign-in response.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The permissions, in [`Permission::ALL`] order.
    ///
    /// Ordered so that a response and a snapshot do not change because two
    /// roles were loaded in a different order.
    pub fn iter(&self) -> impl Iterator<Item = Permission> + '_ {
        self.0.iter().copied()
    }
}

impl FromIterator<Permission> for PermissionSet {
    fn from_iter<I: IntoIterator<Item = Permission>>(iter: I) -> Self {
        let mut permissions: Vec<Permission> = iter.into_iter().collect();
        permissions.sort_unstable();
        permissions.dedup();
        Self(permissions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A permission that cannot be read back from its own spelling would be one
    /// that stops working the moment it is stored.
    #[test]
    fn every_permission_survives_a_round_trip() {
        for permission in Permission::ALL {
            assert_eq!(
                permission.as_str().parse::<Permission>(),
                Ok(permission),
                "{permission}"
            );
        }
    }

    /// Two permissions sharing a spelling would make one of them unreachable,
    /// and `ALL` missing one would make it ungrantable.
    #[test]
    fn the_list_is_complete_and_has_no_duplicates() {
        let mut spellings: Vec<&str> = Permission::ALL.iter().map(|p| p.as_str()).collect();
        let count = spellings.len();
        spellings.sort_unstable();
        spellings.dedup();
        assert_eq!(spellings.len(), count, "two permissions share a spelling");
    }

    #[test]
    fn a_permission_this_version_does_not_have_is_refused() {
        assert!("shares:teleport".parse::<Permission>().is_err());
        assert!(String::new().parse::<Permission>().is_err());
    }

    /// Roles overlap by design, and holding one twice is not holding it more.
    #[test]
    fn the_same_permission_from_two_roles_is_held_once() {
        let set: PermissionSet = [
            Permission::SharesCreate,
            Permission::SharesCreate,
            Permission::UsersRead,
        ]
        .into_iter()
        .collect();
        assert_eq!(set.iter().count(), 2);
        assert!(set.allows(Permission::SharesCreate));
    }

    #[test]
    fn the_administrator_set_allows_everything() {
        let all = PermissionSet::all();
        for permission in Permission::ALL {
            assert!(all.allows(permission), "{permission}");
        }
    }

    /// The distinction an operator is actually asking about.
    #[test]
    fn only_the_permissions_that_reach_other_people_are_privileged() {
        assert!(!Permission::SharesCreate.is_privileged());
        assert!(!Permission::TokensManage.is_privileged());
        assert!(Permission::UsersDelete.is_privileged());
        assert!(Permission::RolesWrite.is_privileged());
    }
}
