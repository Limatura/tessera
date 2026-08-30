//! Identities, and the sign-ins that are away at the provider.
//!
//! Two tables with one thing in common: both exist because a provider's answer
//! arrives as a request we did not make, and both are how that request is tied
//! back to something we did.

use sea_orm::{
    ColumnTrait as _, DatabaseConnection, EntityTrait as _, QueryFilter as _, QueryOrder as _, Set,
};
use tessera_core::UserId;
use time::OffsetDateTime;

use crate::{
    auth::oidc::flow::Purpose,
    db::{
        entity::{
            identity::{
                ActiveModel as IdentityActive, Column as IdentityColumn, Entity as IdentityEntity,
            },
            oidc_flow::{ActiveModel as FlowActive, Column as FlowColumn, Entity as FlowEntity},
        },
        repository::user::{CorruptRow, UserRepositoryError},
    },
};

/// A sign-in waiting for the browser to come back.
#[derive(Debug, Clone)]
pub struct Flow {
    /// What the callback proved it belongs to.
    pub state: String,
    /// Expected inside the returned token.
    pub nonce: String,
    /// The PKCE secret to send to the token endpoint.
    pub verifier: String,
    /// Signing in, or attaching a provider to an account.
    pub purpose: Purpose,
    /// Whose account a `link` attaches to.
    pub user_id: Option<UserId>,
    /// An invitation to spend once the sign-in succeeds.
    pub invite_code: Option<String>,
    /// When this stops being answerable.
    pub expires_at: OffsetDateTime,
}

/// One way an account signs in through a provider.
#[derive(Debug, Clone)]
pub struct Identity {
    /// Which provider vouched for this.
    pub provider: String,
    /// The provider's own identifier for the person.
    pub subject: String,
    /// The account it signs in as.
    pub user_id: UserId,
    /// When it was attached.
    pub created_at: OffsetDateTime,
}

/// Provider sign-ins, as the rest of the server sees them.
#[derive(Debug, Clone)]
pub struct OidcRepository {
    connection: DatabaseConnection,
}

impl OidcRepository {
    /// Wraps a connection.
    #[must_use]
    pub const fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    /// Records a sign-in that is about to leave for the provider.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the insert fails.
    pub async fn begin(&self, flow: &Flow, now: OffsetDateTime) -> Result<(), UserRepositoryError> {
        let row = FlowActive {
            state: Set(flow.state.clone()),
            nonce: Set(flow.nonce.clone()),
            verifier: Set(flow.verifier.clone()),
            purpose: Set(flow.purpose.as_str().to_owned()),
            user_id: Set(flow.user_id.map(|id| id.to_string())),
            invite_code: Set(flow.invite_code.clone()),
            created_at: Set(now),
            expires_at: Set(flow.expires_at),
        };
        FlowEntity::insert(row).exec(&self.connection).await?;
        Ok(())
    }

    /// Takes a sign-in back, and only once.
    ///
    /// The row is deleted whether or not it had expired, and the flow is
    /// returned only if this call was the one that deleted it. That makes the
    /// database the referee: a callback replayed, or opened twice from a
    /// history entry, finds nothing the second time. Doing the same with a read
    /// and then a delete leaves a window where both attempts see the row.
    ///
    /// Expiry is the caller's to judge, so it can say "this took too long"
    /// rather than "no such sign-in".
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the query or the delete fails.
    pub async fn consume(&self, state: &str) -> Result<Option<Flow>, UserRepositoryError> {
        let Some(model) = FlowEntity::find_by_id(state).one(&self.connection).await? else {
            return Ok(None);
        };
        let removed = FlowEntity::delete_many()
            .filter(FlowColumn::State.eq(state))
            .exec(&self.connection)
            .await?;
        if removed.rows_affected == 0 {
            return Ok(None);
        }

        let user_id = model
            .user_id
            .as_deref()
            .map(UserId::parse)
            .transpose()
            .map_err(|source| CorruptRow {
                id: model.state.clone(),
                source,
            })?;
        Ok(Some(Flow {
            state: model.state,
            nonce: model.nonce,
            verifier: model.verifier,
            purpose: Purpose::from_stored(&model.purpose),
            user_id,
            invite_code: model.invite_code,
            expires_at: model.expires_at,
        }))
    }

    /// Removes sign-ins nobody came back for.
    ///
    /// Returns how many went. A browser closed on the provider's login page
    /// leaves a row that nothing will ever present again.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the delete fails.
    pub async fn sweep_expired(&self, now: OffsetDateTime) -> Result<u64, UserRepositoryError> {
        let removed = FlowEntity::delete_many()
            .filter(FlowColumn::ExpiresAt.lte(now))
            .exec(&self.connection)
            .await?;
        Ok(removed.rows_affected)
    }

    /// The account a provider's subject signs in as, if it is known here.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the query fails.
    pub async fn find_identity(
        &self,
        provider: &str,
        subject: &str,
    ) -> Result<Option<Identity>, UserRepositoryError> {
        let found = IdentityEntity::find_by_id((provider.to_owned(), subject.to_owned()))
            .one(&self.connection)
            .await?;
        found
            .map(|model| {
                let user_id = UserId::parse(&model.user_id).map_err(|source| CorruptRow {
                    id: format!("{}:{}", model.provider, model.subject),
                    source,
                })?;
                Ok(Identity {
                    provider: model.provider,
                    subject: model.subject,
                    user_id,
                    created_at: model.created_at,
                })
            })
            .transpose()
    }

    /// Attaches a provider's subject to an account.
    ///
    /// The primary key is `(provider, subject)`, so a subject already attached
    /// to a different account is refused by the database rather than silently
    /// moved. That is the failure worth being loud about: it means two people
    /// are being told they are the same one.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the insert fails.
    pub async fn link(
        &self,
        provider: &str,
        subject: &str,
        user_id: UserId,
        now: OffsetDateTime,
    ) -> Result<(), UserRepositoryError> {
        let row = IdentityActive {
            provider: Set(provider.to_owned()),
            subject: Set(subject.to_owned()),
            user_id: Set(user_id.to_string()),
            created_at: Set(now),
        };
        IdentityEntity::insert(row).exec(&self.connection).await?;
        Ok(())
    }

    /// Every way one account signs in through a provider.
    ///
    /// Ordered oldest first, so the list reads the way it was built.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the query fails.
    pub async fn identities_of(
        &self,
        user_id: UserId,
    ) -> Result<Vec<Identity>, UserRepositoryError> {
        let rows = IdentityEntity::find()
            .filter(IdentityColumn::UserId.eq(user_id.to_string()))
            .order_by_asc(IdentityColumn::CreatedAt)
            .all(&self.connection)
            .await?;
        Ok(rows
            .into_iter()
            .map(|model| Identity {
                provider: model.provider,
                subject: model.subject,
                user_id,
                created_at: model.created_at,
            })
            .collect())
    }

    /// Detaches a provider from an account.
    ///
    /// Returns whether a row went. Whether it was allowed to is decided above
    /// this: an account with no password and no other identity would have no
    /// way back in, and that check needs the credentials table too.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the delete fails.
    pub async fn unlink(
        &self,
        user_id: UserId,
        provider: &str,
    ) -> Result<bool, UserRepositoryError> {
        let removed = IdentityEntity::delete_many()
            .filter(IdentityColumn::UserId.eq(user_id.to_string()))
            .filter(IdentityColumn::Provider.eq(provider))
            .exec(&self.connection)
            .await?;
        Ok(removed.rows_affected > 0)
    }
}

#[cfg(test)]
mod tests {
    use tessera_core::{AccountState, EmailAddress, RoleId, Username};
    use time::Duration;

    use super::*;
    use crate::{config::DatabaseConfig, db::repository::user::User};

    async fn repositories() -> (OidcRepository, crate::db::repository::user::UserRepository) {
        let config = DatabaseConfig {
            url: "sqlite::memory:".to_owned(),
            // One connection: `sqlite::memory:` gives each connection its own
            // database, so a larger pool would silently talk to empty copies.
            max_connections: 1,
            ..Default::default()
        };
        let connection = crate::db::connect(&config).await.expect("pool");
        crate::db::migrate(&connection).await.expect("migrations");
        (
            OidcRepository::new(connection.clone()),
            crate::db::repository::user::UserRepository::new(connection),
        )
    }

    fn now() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_785_000_000).expect("a valid timestamp")
    }

    async fn account(
        users: &crate::db::repository::user::UserRepository,
        username: &str,
    ) -> UserId {
        let user = User {
            role_id: Some(RoleId::parse(RoleId::MEMBER).expect("the seeded id is valid")),
            quota_bytes: None,
            max_file_bytes: None,
            id: UserId::generate(),
            username: Username::parse(username).expect("a valid username"),
            email: EmailAddress::parse(&format!("{username}@example.com"))
                .expect("a valid address"),
            must_change_password: false,
            state: AccountState::Active,
            deleted_at: None,
            deleted_by: None,
            email_verified_at: None,
            created_at: now(),
            updated_at: now(),
        };
        users.insert(&user, None).await.expect("the account");
        user.id
    }

    fn flow(state: &str) -> Flow {
        Flow {
            state: state.to_owned(),
            nonce: "nonce".to_owned(),
            verifier: "verifier".to_owned(),
            purpose: Purpose::Login,
            user_id: None,
            invite_code: None,
            expires_at: now() + Duration::minutes(10),
        }
    }

    #[tokio::test]
    async fn a_sign_in_comes_back_with_what_it_left_with() {
        let (oidc, _users) = repositories().await;
        oidc.begin(&flow("state-1"), now()).await.expect("begun");

        let taken = oidc.consume("state-1").await.expect("a query").expect("it");

        assert_eq!(taken.nonce, "nonce");
        assert_eq!(taken.verifier, "verifier");
        assert_eq!(taken.purpose, Purpose::Login);
    }

    /// The whole reason the delete decides: a callback opened twice, or
    /// replayed from history, must only work once.
    #[tokio::test]
    async fn a_sign_in_can_only_be_taken_once() {
        let (oidc, _users) = repositories().await;
        oidc.begin(&flow("state-2"), now()).await.expect("begun");

        assert!(oidc.consume("state-2").await.expect("a query").is_some());
        assert!(oidc.consume("state-2").await.expect("a query").is_none());
    }

    /// An expired flow is still returned, so the caller can say "this took too
    /// long" rather than "no such sign-in". It is still removed.
    #[tokio::test]
    async fn an_expired_sign_in_is_returned_and_removed() {
        let (oidc, _users) = repositories().await;
        let mut expired = flow("state-3");
        expired.expires_at = now() - Duration::minutes(1);
        oidc.begin(&expired, now()).await.expect("begun");

        let taken = oidc.consume("state-3").await.expect("a query").expect("it");

        assert!(taken.expires_at < now());
        assert!(oidc.consume("state-3").await.expect("a query").is_none());
    }

    #[tokio::test]
    async fn a_link_remembers_whose_account_it_is_for() {
        let (oidc, users) = repositories().await;
        let user_id = account(&users, "alice").await;
        let mut linking = flow("state-4");
        linking.purpose = Purpose::Link;
        linking.user_id = Some(user_id);
        linking.invite_code = Some("code".to_owned());
        oidc.begin(&linking, now()).await.expect("begun");

        let taken = oidc.consume("state-4").await.expect("a query").expect("it");

        assert_eq!(taken.purpose, Purpose::Link);
        assert_eq!(taken.user_id, Some(user_id));
        assert_eq!(taken.invite_code.as_deref(), Some("code"));
    }

    #[tokio::test]
    async fn the_sweep_takes_the_abandoned_and_leaves_the_rest() {
        let (oidc, _users) = repositories().await;
        let mut abandoned = flow("old");
        abandoned.expires_at = now() - Duration::minutes(1);
        oidc.begin(&abandoned, now()).await.expect("begun");
        oidc.begin(&flow("current"), now()).await.expect("begun");

        assert_eq!(oidc.sweep_expired(now()).await.expect("a sweep"), 1);
        assert!(oidc.consume("current").await.expect("a query").is_some());
    }

    #[tokio::test]
    async fn an_identity_is_found_by_what_the_provider_calls_the_person() {
        let (oidc, users) = repositories().await;
        let user_id = account(&users, "alice").await;
        oidc.link("authentik", "sub-1", user_id, now())
            .await
            .expect("linked");

        let found = oidc
            .find_identity("authentik", "sub-1")
            .await
            .expect("a query")
            .expect("it");

        assert_eq!(found.user_id, user_id);
        // A subject is only meaningful within its own issuer.
        assert!(
            oidc.find_identity("keycloak", "sub-1")
                .await
                .expect("a query")
                .is_none()
        );
    }

    /// Two accounts claiming the same subject means somebody is about to be
    /// signed in as the wrong person. The primary key refuses it.
    #[tokio::test]
    async fn one_subject_cannot_be_claimed_by_two_accounts() {
        let (oidc, users) = repositories().await;
        let alice = account(&users, "alice").await;
        let bob = account(&users, "bob").await;
        oidc.link("authentik", "sub-1", alice, now())
            .await
            .expect("linked");

        assert!(oidc.link("authentik", "sub-1", bob, now()).await.is_err());
        let found = oidc
            .find_identity("authentik", "sub-1")
            .await
            .expect("a query")
            .expect("it");
        assert_eq!(found.user_id, alice);
    }

    #[tokio::test]
    async fn unlinking_removes_one_way_in_and_leaves_the_others() {
        let (oidc, users) = repositories().await;
        let user_id = account(&users, "alice").await;
        oidc.link("authentik", "sub-1", user_id, now())
            .await
            .expect("linked");
        oidc.link("keycloak", "sub-2", user_id, now())
            .await
            .expect("linked");

        assert!(oidc.unlink(user_id, "authentik").await.expect("unlinked"));
        assert!(!oidc.unlink(user_id, "authentik").await.expect("a query"));

        let left = oidc.identities_of(user_id).await.expect("a query");
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].provider, "keycloak");
    }

    /// The way back into an instance whose provider went away: `reset-password`
    /// on an account that never had one creates the credential row rather than
    /// finding nothing to update.
    #[tokio::test]
    async fn an_account_with_no_password_can_be_given_one() {
        let (_oidc, users) = repositories().await;
        let user_id = account(&users, "alice").await;
        assert!(users.password_of(user_id).await.expect("a query").is_none());

        users
            .set_password(
                user_id,
                "$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA".to_owned(),
                true,
                now(),
            )
            .await
            .expect("the password");

        assert!(users.password_of(user_id).await.expect("a query").is_some());
    }

    /// Unlinking is scoped to the account asking, so one person cannot detach
    /// another person's provider by naming it.
    #[tokio::test]
    async fn unlinking_only_touches_the_account_that_asked() {
        let (oidc, users) = repositories().await;
        let alice = account(&users, "alice").await;
        let bob = account(&users, "bob").await;
        oidc.link("authentik", "sub-1", alice, now())
            .await
            .expect("linked");

        assert!(!oidc.unlink(bob, "authentik").await.expect("a query"));
        assert_eq!(oidc.identities_of(alice).await.expect("a query").len(), 1);
    }
}
