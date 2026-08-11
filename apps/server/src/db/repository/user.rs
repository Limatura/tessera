//! Reading and writing accounts.
//!
//! # Where the boundary is paid for
//!
//! The workspace keeps domain types in `tessera-core` and storage types here, so
//! something has to translate. That translation is [`User::from_model`], and it
//! is fallible.
//!
//! Being fallible is the point. A row can violate today's rules — written by an
//! older version, edited by hand, migrated from elsewhere — and the alternatives
//! are worse: `unwrap` turns a bad row into a crashed request, and skipping
//! validation lets an invalid value back into code that was promised it could
//! not exist. A `Result` says "this row is not usable" once, at the edge, where
//! the caller can log which row and move on.
//!
//! This is what the two-crate split costs in practice, and it is worth writing
//! down: roughly forty lines per table, most of it mechanical.

use sea_orm::{
    ActiveValue::Set, ColumnTrait as _, DatabaseConnection, EntityTrait as _, QueryFilter as _,
};
use tessera_core::{DomainError, EmailAddress, UserId, Username};
use time::OffsetDateTime;

use crate::db::entity::user::{self, Entity as UserEntity};

/// An account, in domain terms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    /// Stable identifier.
    pub id: UserId,
    /// Normalised account name.
    pub username: Username,
    /// Normalised address.
    pub email: EmailAddress,
    /// PHC-format Argon2 string.
    pub password_hash: String,
    /// Whether the account may reach the administrative endpoints.
    pub is_admin: bool,
    /// Whether the account must set a new password before doing anything else.
    pub must_change_password: bool,
    /// When the address was proven reachable, if it ever was.
    pub email_verified_at: Option<OffsetDateTime>,
    /// When the account was created.
    pub created_at: OffsetDateTime,
    /// When the account last changed.
    pub updated_at: OffsetDateTime,
}

/// A stored row that today's rules reject.
#[derive(Debug, thiserror::Error)]
#[error("user row `{id}` is not usable: {source}")]
pub struct CorruptRow {
    /// Primary key of the offending row, so an operator can find it.
    pub id: String,
    /// Which rule it failed.
    #[source]
    pub source: DomainError,
}

impl User {
    /// Converts a stored row into domain types.
    ///
    /// # Errors
    ///
    /// Returns [`CorruptRow`] if any column fails the rule its type enforces.
    pub fn from_model(model: user::Model) -> Result<Self, CorruptRow> {
        let fail = |source: DomainError| CorruptRow {
            id: model.id.clone(),
            source,
        };

        Ok(Self {
            id: UserId::parse(&model.id).map_err(fail)?,
            // Privileged parsing: reading a row back is not claiming the name.
            // The bootstrap administrator is called `admin`, which the reserved
            // list refuses — so the ordinary parser would declare the one row
            // every instance has to be corrupt.
            username: Username::parse_privileged(&model.username).map_err(fail)?,
            email: EmailAddress::parse(&model.email).map_err(fail)?,
            password_hash: model.password_hash,
            is_admin: model.is_admin,
            must_change_password: model.must_change_password,
            email_verified_at: model.email_verified_at,
            created_at: model.created_at,
            updated_at: model.updated_at,
        })
    }

    /// The row to write for this account.
    fn to_active_model(&self) -> user::ActiveModel {
        user::ActiveModel {
            id: Set(self.id.to_string()),
            username: Set(self.username.as_str().to_owned()),
            email: Set(self.email.as_str().to_owned()),
            password_hash: Set(self.password_hash.clone()),
            is_admin: Set(self.is_admin),
            must_change_password: Set(self.must_change_password),
            email_verified_at: Set(self.email_verified_at),
            created_at: Set(self.created_at),
            updated_at: Set(self.updated_at),
        }
    }
}

/// Accounts, as the rest of the server sees them.
///
/// A plain struct over the connection rather than a trait: the swappability a
/// port would provide is already supplied by SeaORM, and a trait here would buy
/// only mockability — which these methods, being thin queries, do not need.
#[derive(Debug, Clone)]
pub struct UserRepository {
    connection: DatabaseConnection,
}

/// Anything that can go wrong reading or writing an account.
#[derive(Debug, thiserror::Error)]
pub enum UserRepositoryError {
    /// The database refused the operation.
    #[error(transparent)]
    Database(#[from] sea_orm::DbErr),

    /// A stored row does not satisfy today's rules.
    #[error(transparent)]
    Corrupt(#[from] CorruptRow),
}

impl UserRepository {
    /// Wraps a connection.
    #[must_use]
    pub const fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    /// Inserts an account.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the username or address is already
    /// taken, or the insert otherwise fails.
    pub async fn insert(&self, user: &User) -> Result<(), UserRepositoryError> {
        UserEntity::insert(user.to_active_model())
            .exec(&self.connection)
            .await?;
        Ok(())
    }

    /// Finds an account by identifier.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the query fails or the stored row is
    /// not usable.
    pub async fn find_by_id(&self, id: UserId) -> Result<Option<User>, UserRepositoryError> {
        convert(
            UserEntity::find_by_id(id.to_string())
                .one(&self.connection)
                .await?,
        )
    }

    /// Finds an account by name.
    ///
    /// Takes a [`Username`] rather than a `&str` so the caller cannot forget to
    /// normalise — looking up `Alice` when the row says `alice` would silently
    /// find nothing.
    ///
    /// # Errors
    ///
    /// See [`UserRepository::find_by_id`].
    pub async fn find_by_username(
        &self,
        username: &Username,
    ) -> Result<Option<User>, UserRepositoryError> {
        convert(
            UserEntity::find()
                .filter(user::Column::Username.eq(username.as_str()))
                .one(&self.connection)
                .await?,
        )
    }

    /// Finds an account by address.
    ///
    /// # Errors
    ///
    /// See [`UserRepository::find_by_id`].
    pub async fn find_by_email(
        &self,
        email: &EmailAddress,
    ) -> Result<Option<User>, UserRepositoryError> {
        convert(
            UserEntity::find()
                .filter(user::Column::Email.eq(email.as_str()))
                .one(&self.connection)
                .await?,
        )
    }

    /// Replaces an account's password hash and clears the "must change" flag.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the update fails.
    pub async fn set_password(
        &self,
        id: UserId,
        password_hash: String,
        now: OffsetDateTime,
    ) -> Result<(), UserRepositoryError> {
        UserEntity::update_many()
            .col_expr(user::Column::PasswordHash, password_hash.into())
            .col_expr(user::Column::MustChangePassword, false.into())
            .col_expr(user::Column::UpdatedAt, now.into())
            .filter(user::Column::Id.eq(id.to_string()))
            .exec(&self.connection)
            .await?;
        Ok(())
    }

    /// Whether any account exists.
    ///
    /// Used at start-up to decide whether to bootstrap an administrator, which
    /// is why it counts rather than fetching: the answer is one number and the
    /// rows may be unreadable by today's rules without that mattering.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the query fails.
    pub async fn any_exist(&self) -> Result<bool, UserRepositoryError> {
        Ok(UserEntity::find().one(&self.connection).await?.is_some())
    }
}

/// Turns an optional row into an optional account, surfacing a row today's rules
/// reject rather than hiding it as "not found".
fn convert(model: Option<user::Model>) -> Result<Option<User>, UserRepositoryError> {
    model.map(User::from_model).transpose().map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use tessera_core::{DomainError, Password, PasswordPolicy};

    use super::*;
    use crate::config::DatabaseConfig;

    fn sample(username: &str, email: &str) -> User {
        let now = OffsetDateTime::from_unix_timestamp(1_785_000_000).unwrap();
        User {
            id: UserId::generate(),
            username: Username::parse(username).unwrap(),
            email: EmailAddress::parse(email).unwrap(),
            // Not a real hash; hashing arrives with the endpoints that need it.
            password_hash: "$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA".to_owned(),
            is_admin: false,
            must_change_password: false,
            email_verified_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    async fn repository() -> UserRepository {
        let config = DatabaseConfig {
            url: "sqlite::memory:".to_owned(),
            // One connection: `sqlite::memory:` gives each connection its own
            // database, so a larger pool would silently talk to empty copies.
            max_connections: 1,
            ..Default::default()
        };
        let connection = crate::db::connect(&config).await.expect("pool");
        crate::db::migrate(&connection).await.expect("migrations");
        UserRepository::new(connection)
    }

    #[tokio::test]
    async fn an_account_round_trips_through_the_database() {
        let repository = repository().await;
        let user = sample("alice", "alice@example.com");

        repository.insert(&user).await.unwrap();

        // Every lookup path must return the same account, with the domain types
        // intact rather than the strings the columns hold.
        let by_id = repository.find_by_id(user.id).await.unwrap().unwrap();
        let by_name = repository
            .find_by_username(&user.username)
            .await
            .unwrap()
            .unwrap();
        let by_email = repository
            .find_by_email(&user.email)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(by_id, user);
        assert_eq!(by_name, user);
        assert_eq!(by_email, user);
    }

    #[tokio::test]
    async fn missing_accounts_are_none_rather_than_an_error() {
        let repository = repository().await;
        assert!(
            repository
                .find_by_id(UserId::generate())
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            repository
                .find_by_username(&Username::parse("nobody").unwrap())
                .await
                .unwrap()
                .is_none()
        );
    }

    /// Looking up by a differently-cased name must still find the account —
    /// which it does because `Username` normalises before the query is built.
    #[tokio::test]
    async fn lookup_is_insensitive_to_how_the_name_was_typed() {
        let repository = repository().await;
        let user = sample("alice", "alice@example.com");
        repository.insert(&user).await.unwrap();

        let typed = Username::parse("  ALICE ").unwrap();
        assert_eq!(
            repository.find_by_username(&typed).await.unwrap(),
            Some(user)
        );
    }

    #[tokio::test]
    async fn the_unique_index_rejects_a_duplicate_username() {
        let repository = repository().await;
        repository
            .insert(&sample("alice", "a@example.com"))
            .await
            .unwrap();

        let clash = sample("alice", "different@example.com");
        assert!(repository.insert(&clash).await.is_err());
    }

    #[tokio::test]
    async fn the_unique_index_rejects_a_duplicate_email() {
        let repository = repository().await;
        repository
            .insert(&sample("alice", "shared@example.com"))
            .await
            .unwrap();

        let clash = sample("bob", "shared@example.com");
        assert!(repository.insert(&clash).await.is_err());
    }

    #[tokio::test]
    async fn any_exist_answers_before_and_after_the_first_account() {
        let repository = repository().await;
        assert!(!repository.any_exist().await.unwrap());
        repository
            .insert(&sample("alice", "alice@example.com"))
            .await
            .unwrap();
        assert!(repository.any_exist().await.unwrap());
    }

    /// The reason `from_model` returns a `Result`. A row written by an older
    /// version, or edited by hand, can hold something today's rules reject —
    /// and that must surface as a nameable error rather than a panic.
    #[test]
    fn a_row_that_violates_todays_rules_is_reported_not_panicked() {
        let now = OffsetDateTime::from_unix_timestamp(1_785_000_000).unwrap();
        let model = user::Model {
            id: UserId::generate().to_string(),
            username: "no spaces allowed".to_owned(),
            email: "alice@example.com".to_owned(),
            password_hash: String::new(),
            is_admin: false,
            must_change_password: false,
            email_verified_at: None,
            created_at: now,
            updated_at: now,
        };

        let error = User::from_model(model).unwrap_err();
        assert!(matches!(error.source, DomainError::Malformed { .. }));
        // The message must name the row, or an operator cannot go and fix it.
        assert!(error.to_string().contains(&error.id));
    }

    /// Passwords never reach the database in the clear. This is a reminder in
    /// test form: `User` has no plaintext field for one to occupy.
    #[test]
    fn the_stored_account_has_nowhere_to_put_a_plaintext_password() {
        let password = Password::parse("a passphrase here", PasswordPolicy::default()).unwrap();
        let rendered = format!("{:?}", sample("alice", "alice@example.com"));
        assert!(!rendered.contains(password.expose()));
        assert!(rendered.contains("password_hash"));
    }
}
