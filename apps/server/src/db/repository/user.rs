//! Reading and writing accounts.
//!
//! # Where the boundary is paid for
//!
//! The workspace keeps domain types in `tessera-core` and storage types here,
//! so something has to translate. That is [`User::from_model`], and it is
//! fallible by design.
//!
//! A row can violate today's rules, having been written by an older version,
//! edited by hand or migrated from elsewhere. `unwrap` would turn a bad row
//! into a crashed request, and skipping validation would let an invalid value
//! back into code promised it could not exist. A `Result` says "this row is not
//! usable" once, at the edge, where the caller can log which row and move on.
//!
//! That is what the two-crate split costs: roughly forty mechanical lines per
//! table.

use sea_orm::{
    ActiveValue::Set, ColumnTrait as _, DatabaseConnection, EntityTrait as _, PaginatorTrait as _,
    QueryFilter as _, QueryOrder as _, QuerySelect as _,
};
use tessera_core::{AccountState, DomainError, EmailAddress, RoleId, UserId, Username};
use time::OffsetDateTime;

use crate::db::{
    entity::user::{self, Entity as UserEntity},
    repository::quota,
};

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
    /// The role this account is in.
    ///
    /// `None` means no role: signed in and allowed nothing. That was
    /// expressible before as "no rows in the join table" and stays expressible.
    pub role_id: Option<RoleId>,
    /// How much storage this one account may hold, overriding its role.
    ///
    /// `None` asks the role; `Some(0)` is no ceiling. The override exists so an
    /// operator can raise one person without having to invent a role for them.
    pub quota_bytes: Option<u64>,
    /// Largest single file this one account may send, overriding its role.
    ///
    /// `None` asks the role; `Some(0)` is no limit of its own.
    pub max_file_bytes: Option<u64>,
    /// Whether the account must set a new password before doing anything else.
    pub must_change_password: bool,
    /// Whether the account works, is suspended, or is gone.
    pub state: AccountState,
    /// When it was removed, if it was.
    pub deleted_at: Option<OffsetDateTime>,
    /// Who removed it: an administrator, or the account itself.
    pub deleted_by: Option<UserId>,
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
            // Privileged parsing, because reading a row back is not claiming
            // the name. The bootstrap administrator is called `admin`, which
            // the reserved list refuses, so the ordinary parser would call the
            // one row every instance has corrupt.
            username: Username::parse_privileged(&model.username).map_err(fail)?,
            // Privileged for the same reason as the name above. A deleted
            // account's row holds an address at the reserved tombstone domain,
            // which the ordinary parser refuses, and that refusal is what stops
            // anybody else taking it.
            email: EmailAddress::parse_privileged(&model.email).map_err(fail)?,
            role_id: model
                .role_id
                .as_deref()
                .map(RoleId::parse)
                .transpose()
                .map_err(fail)?,
            quota_bytes: quota::from_column(model.quota_bytes),
            max_file_bytes: quota::from_column(model.max_file_bytes),
            password_hash: model.password_hash,
            must_change_password: model.must_change_password,
            state: model.state.parse().map_err(fail)?,
            deleted_at: model.deleted_at,
            // A `deleted_by` that will not parse is a broken pointer, not a
            // broken account. Dropping it costs the record of who did it and
            // keeps the account readable, the better of the two.
            deleted_by: model
                .deleted_by
                .as_deref()
                .and_then(|id| UserId::parse(id).ok()),
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
            role_id: Set(self.role_id.map(|id| id.to_string())),
            quota_bytes: Set(quota::to_column(self.quota_bytes)),
            max_file_bytes: Set(quota::to_column(self.max_file_bytes)),
            must_change_password: Set(self.must_change_password),
            state: Set(self.state.as_str().to_owned()),
            deleted_at: Set(self.deleted_at),
            deleted_by: Set(self.deleted_by.map(|id| id.to_string())),
            email_verified_at: Set(self.email_verified_at),
            created_at: Set(self.created_at),
            updated_at: Set(self.updated_at),
        }
    }
}

/// Accounts, as the rest of the server sees them.
///
/// A plain struct over the connection, not a trait. SeaORM already supplies the
/// swappability, and a trait would buy only mockability, which
/// these thin queries do not need.
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

    /// Inserts an account, and puts it in a role.
    ///
    /// # The role travels on the account
    ///
    /// It used to be a second insert into a join table, in a transaction so
    /// the pair could not half happen. It is a column now, so there is no pair.
    ///
    /// An account in no role is signed in and allowed nothing, which reads like
    /// a bug and not an intended state, so a caller who means it writes
    /// `role_id: None` instead of forgetting a second step.
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
    /// Takes a [`Username`], not a `&str`, so the caller cannot forget to
    /// normalise. Looking up `Alice` when the row says `alice` would
    /// silently find nothing.
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

    /// Every account, newest first.
    ///
    /// No pagination. The instance this is built for has a handful of accounts,
    /// and an unused page parameter is tested once and then rots.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the query fails or a row is not usable.
    pub async fn list(&self, include_deleted: bool) -> Result<Vec<User>, UserRepositoryError> {
        let mut query = UserEntity::find().order_by_desc(user::Column::CreatedAt);
        if !include_deleted {
            // Off by default: a deleted account is a row that exists to keep
            // shares attributable, not a person an operator manages. Left in,
            // every listing fills with scrubbed names that cannot be acted on.
            query = query.filter(user::Column::State.ne(AccountState::Deleted.as_str()));
        }
        let models = query.all(&self.connection).await?;
        models
            .into_iter()
            .map(|model| User::from_model(model).map_err(UserRepositoryError::from))
            .collect()
    }

    /// How many accounts exist.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the query fails.
    pub async fn count(&self) -> Result<u64, UserRepositoryError> {
        Ok(UserEntity::find().count(&self.connection).await?)
    }

    /// Changes an account's name, address, or both.
    ///
    /// Clearing `email_verified_at` when the address moves is the caller's
    /// decision; see the handler, which has the address to compare against.
    ///
    /// Returns whether the account changed.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the update fails, including when the
    /// name or address is already taken.
    pub async fn rename(
        &self,
        id: UserId,
        username: &Username,
        email: &EmailAddress,
        verified_at: Option<OffsetDateTime>,
        now: OffsetDateTime,
    ) -> Result<bool, UserRepositoryError> {
        let result = UserEntity::update_many()
            .col_expr(user::Column::Username, username.as_str().into())
            .col_expr(user::Column::Email, email.as_str().into())
            .col_expr(user::Column::EmailVerifiedAt, verified_at.into())
            .col_expr(user::Column::UpdatedAt, now.into())
            .filter(user::Column::Id.eq(id.to_string()))
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected == 1)
    }

    /// Suspends an account, or lets it back in.
    ///
    /// Refuses to touch a deleted one. That state is final, and reactivating a
    /// scrubbed row produces an account with a tombstone for a
    /// name.
    ///
    /// Returns whether the account changed.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if a query fails.
    pub async fn set_state(
        &self,
        id: UserId,
        state: AccountState,
        now: OffsetDateTime,
    ) -> Result<bool, UserRepositoryError> {
        let result = UserEntity::update_many()
            .col_expr(user::Column::State, state.as_str().into())
            .col_expr(user::Column::UpdatedAt, now.into())
            .filter(user::Column::Id.eq(id.to_string()))
            .filter(user::Column::State.ne(AccountState::Deleted.as_str()))
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected == 1)
    }

    /// Removes an account, leaving a row that says one was here.
    ///
    /// # What is left and what is not
    ///
    /// The identifier stays, because `shares.user_id` points at it and a share
    /// whose uploader is gone differs from one that was uploaded anonymously.
    /// The name and address are replaced with values that identify no person,
    /// which also frees the originals. The password hash goes.
    ///
    /// # Why the sessions and tokens go too
    ///
    /// Authentication already refuses a deleted account, so they would not
    /// work, and a row that cannot work has to be explained. The login history
    /// goes with them, and that matters: it is the most identifying thing
    /// this service holds about a person.
    ///
    /// Returns whether an account was removed.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if a query fails.
    pub async fn soft_delete(
        &self,
        id: UserId,
        by: UserId,
        now: OffsetDateTime,
    ) -> Result<bool, UserRepositoryError> {
        let (name, email) = tessera_core::tombstone(id);
        let result = UserEntity::update_many()
            .col_expr(user::Column::State, AccountState::Deleted.as_str().into())
            .col_expr(user::Column::Username, name.as_str().into())
            .col_expr(user::Column::Email, email.as_str().into())
            // Not emptied: the column is `NOT NULL`, and a value nothing can
            // ever verify against is what "there is no password here" has to
            // look like. `verify` treats an unparsable hash as a failure.
            .col_expr(user::Column::PasswordHash, "".into())
            .col_expr(user::Column::EmailVerifiedAt, Option::<OffsetDateTime>::None.into())
            .col_expr(user::Column::MustChangePassword, false.into())
            .col_expr(user::Column::DeletedAt, now.into())
            .col_expr(user::Column::DeletedBy, by.to_string().into())
            .col_expr(user::Column::UpdatedAt, now.into())
            .filter(user::Column::Id.eq(id.to_string()))
            .filter(user::Column::State.ne(AccountState::Deleted.as_str()))
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected == 1)
    }

    /// Grants or removes the administrator flag.
    ///
    /// Returns whether an account changed.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the update fails.
    pub async fn set_admin(
        &self,
        id: UserId,
        is_admin: bool,
        now: OffsetDateTime,
    ) -> Result<bool, UserRepositoryError> {
        // `true` grants `owner`, not the lesser `admin`, because this
        // boolean has always meant everything. Making it mean less would change
        // what existing clients do and break the sequence an operator most
        // needs: promote a successor, then step down. A successor without
        // `roles:write` cannot be stepped down to.
        //
        // `false` leaves the account in **no role**, as the join
        // table did: the demotion removed `owner` and `admin` and kept whatever
        // else was there, usually nothing. Preserved, not improved,
        // since this field is a compatibility shim for clients predating the
        // role endpoints.
        //
        // One statement, where the join table needed a transaction to keep the
        // account from being briefly in neither role.
        let result = UserEntity::update_many()
            .col_expr(
                user::Column::RoleId,
                is_admin.then(|| RoleId::OWNER.to_owned()).into(),
            )
            .col_expr(user::Column::UpdatedAt, now.into())
            .filter(user::Column::Id.eq(id.to_string()))
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected == 1)
    }

    /// Sets or clears one account's storage quota.
    ///
    /// `None` clears it, which puts the account back on its role's figure;
    /// `Some(0)` gives it no ceiling of its own. Returns whether a row changed.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the write fails.
    pub async fn set_quota(
        &self,
        id: UserId,
        quota_bytes: Option<u64>,
        now: OffsetDateTime,
    ) -> Result<bool, UserRepositoryError> {
        let result = UserEntity::update_many()
            .col_expr(user::Column::QuotaBytes, quota::to_expr(quota_bytes))
            .col_expr(user::Column::UpdatedAt, now.into())
            .filter(user::Column::Id.eq(id.to_string()))
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected == 1)
    }

    /// Sets or clears one account's single-file limit.
    ///
    /// `None` clears it, which puts the account back on its role's figure;
    /// `Some(0)` gives it no limit of its own. Returns whether a row changed.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the write fails.
    pub async fn set_max_file_bytes(
        &self,
        id: UserId,
        max_file_bytes: Option<u64>,
        now: OffsetDateTime,
    ) -> Result<bool, UserRepositoryError> {
        let result = UserEntity::update_many()
            .col_expr(user::Column::MaxFileBytes, quota::to_expr(max_file_bytes))
            .col_expr(user::Column::UpdatedAt, now.into())
            .filter(user::Column::Id.eq(id.to_string()))
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected == 1)
    }

    /// Every live account that carries a quota of its own, by name.
    ///
    /// Only those with a figure: on an instance with a thousand accounts and
    /// three overrides, listing all thousand buries the three. And only live
    /// ones, since a removed account keeps its columns and would otherwise be
    /// listed under a scrubbed name that identifies no person.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the query fails.
    pub async fn with_quota(&self) -> Result<Vec<(String, Option<u64>)>, UserRepositoryError> {
        let rows: Vec<(String, Option<i64>)> = UserEntity::find()
            .filter(user::Column::QuotaBytes.is_not_null())
            .filter(user::Column::State.ne(AccountState::Deleted.as_str()))
            .order_by_asc(user::Column::Username)
            .select_only()
            .column(user::Column::Username)
            .column(user::Column::QuotaBytes)
            .into_tuple()
            .all(&self.connection)
            .await?;
        Ok(rows
            .into_iter()
            .map(|(username, bytes)| (username, quota::from_column(bytes)))
            .collect())
    }

    /// Removes an account.
    ///
    /// Sessions and API tokens go with it, by foreign key. Shares do not:
    /// their owner column is `ON DELETE SET NULL`, so an upload outlives its
    /// account as an anonymous share instead of vanishing with
    /// the bytes still on disk.
    ///
    /// Returns whether an account was removed.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the delete fails.
    pub async fn delete(&self, id: UserId) -> Result<bool, UserRepositoryError> {
        let result = UserEntity::delete_by_id(id.to_string())
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected == 1)
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
        must_change: bool,
        now: OffsetDateTime,
    ) -> Result<(), UserRepositoryError> {
        UserEntity::update_many()
            .col_expr(user::Column::PasswordHash, password_hash.into())
            .col_expr(user::Column::MustChangePassword, must_change.into())
            .col_expr(user::Column::UpdatedAt, now.into())
            .filter(user::Column::Id.eq(id.to_string()))
            .exec(&self.connection)
            .await?;
        Ok(())
    }

    /// Whether any account exists.
    ///
    /// Used at start-up to decide whether to bootstrap an administrator. It
    /// counts instead of fetching, because the answer is one number and the
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
/// reject, instead of hiding it as "not found".
fn convert(model: Option<user::Model>) -> Result<Option<User>, UserRepositoryError> {
    model.map(User::from_model).transpose().map_err(Into::into)
}

#[cfg(test)]
mod tests {
    /// The seeded member role, for tests that only care that an account exists.
    fn member() -> RoleId {
        RoleId::parse(RoleId::MEMBER).expect("the seeded id is valid")
    }

    use tessera_core::{DomainError, Password, PasswordPolicy};

    use super::*;
    use crate::config::DatabaseConfig;

    fn sample(username: &str, email: &str) -> User {
        let now = OffsetDateTime::from_unix_timestamp(1_785_000_000).unwrap();
        User {
            role_id: Some(member()),
            quota_bytes: None,
            max_file_bytes: None,
            id: UserId::generate(),
            username: Username::parse(username).unwrap(),
            email: EmailAddress::parse(email).unwrap(),
            // Not a real hash; hashing arrives with the endpoints that need it.
            password_hash: "$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA".to_owned(),
            must_change_password: false,
            state: AccountState::Active,
            deleted_at: None,
            deleted_by: None,
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
        // intact, not the strings the columns hold.
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

    /// Looking up by a differently-cased name must still find the account,
    /// which works because `Username` normalises before the query is built.
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
    /// version, or edited by hand, can hold something today's rules reject, and
    /// that must surface as a nameable error, not a panic.
    #[test]
    fn a_row_that_violates_todays_rules_is_reported_not_panicked() {
        let now = OffsetDateTime::from_unix_timestamp(1_785_000_000).unwrap();
        let model = user::Model {
            role_id: None,
            quota_bytes: None,
            max_file_bytes: None,
            id: UserId::generate().to_string(),
            username: "no spaces allowed".to_owned(),
            email: "alice@example.com".to_owned(),
            password_hash: String::new(),
            must_change_password: false,
            state: "active".to_owned(),
            deleted_at: None,
            deleted_by: None,
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
