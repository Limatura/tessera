//! Registrations waiting on their address, as rows.

use sea_orm::{
    ColumnTrait as _, DatabaseConnection, EntityTrait as _, QueryFilter as _, Set,
    sea_query::OnConflict,
};
use tessera_core::{EmailAddress, PendingRegistrationId, Username};
use time::OffsetDateTime;

use crate::{
    auth::secret::Secret,
    db::{
        entity::pending_registration::{ActiveModel, Column, Entity as PendingEntity, Model},
        repository::user::{CorruptRow, UserRepositoryError},
    },
};

/// A registration nobody has confirmed yet.
#[derive(Debug, Clone)]
pub struct PendingRegistration {
    /// Row identifier.
    pub id: PendingRegistrationId,
    /// The name asked for, which is checked for real only at confirmation.
    pub username: Username,
    /// The address being confirmed.
    pub email: EmailAddress,
    /// Already hashed, carried through the wait.
    pub password_hash: String,
    /// When the link stops working.
    pub expires_at: OffsetDateTime,
}

impl PendingRegistration {
    /// Whether this link may still be followed.
    #[must_use]
    pub fn is_usable_at(&self, now: OffsetDateTime) -> bool {
        self.expires_at > now
    }

    fn from_model(model: &Model) -> Result<Self, UserRepositoryError> {
        let fail = |source| CorruptRow {
            id: model.id.clone(),
            source,
        };
        Ok(Self {
            id: PendingRegistrationId::parse(&model.id).map_err(fail)?,
            // Privileged parsing: a name that was legal when it was asked for
            // should still be readable now, even if the reserved list has grown
            // since. Whether it may actually be taken is decided at
            // confirmation, against today's rules.
            username: Username::parse_privileged(&model.username).map_err(fail)?,
            email: EmailAddress::parse(&model.email).map_err(fail)?,
            password_hash: model.password_hash.clone(),
            expires_at: model.expires_at,
        })
    }
}

/// Pending registrations, as the rest of the server sees them.
#[derive(Debug, Clone)]
pub struct PendingRegistrationRepository {
    connection: DatabaseConnection,
}

impl PendingRegistrationRepository {
    /// Wraps a connection.
    #[must_use]
    pub const fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    /// Records a registration and returns the secret to put in the email.
    ///
    /// Replaces any existing registration for the same address rather than
    /// adding a second: two live links in one mailbox means the older one keeps
    /// working after somebody has already used the newer, and there is no
    /// reason for that to be possible.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the write fails.
    pub async fn start(
        &self,
        username: &Username,
        email: &EmailAddress,
        password_hash: String,
        now: OffsetDateTime,
        expires_at: OffsetDateTime,
    ) -> Result<Secret, UserRepositoryError> {
        let secret = Secret::generate_session();
        let row = ActiveModel {
            id: Set(PendingRegistrationId::generate().to_string()),
            username: Set(username.as_str().to_owned()),
            email: Set(email.as_str().to_owned()),
            password_hash: Set(password_hash),
            secret_hash: Set(secret.digest()),
            created_at: Set(now),
            expires_at: Set(expires_at),
        };

        PendingEntity::insert(row)
            .on_conflict(
                // Everything but the id, so a second attempt is the one that
                // counts — including a different username or password, because
                // the usual reason to register again is having got something
                // wrong the first time.
                OnConflict::column(Column::Email)
                    .update_columns([
                        Column::Username,
                        Column::PasswordHash,
                        Column::SecretHash,
                        Column::CreatedAt,
                        Column::ExpiresAt,
                    ])
                    .to_owned(),
            )
            .exec(&self.connection)
            .await?;
        Ok(secret)
    }

    /// Finds the registration a secret refers to, expired or not.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the query fails.
    pub async fn find_by_secret(
        &self,
        secret: &Secret,
    ) -> Result<Option<PendingRegistration>, UserRepositoryError> {
        let found = PendingEntity::find()
            .filter(Column::SecretHash.eq(secret.digest()))
            .one(&self.connection)
            .await?;
        found
            .as_ref()
            .map(PendingRegistration::from_model)
            .transpose()
    }

    /// Finds the registration waiting on an address, if there is one.
    ///
    /// There is at most one — see [`PendingRegistrationRepository::start`].
    ///
    /// Nothing in the request path calls this: registration answers the same
    /// way whether or not a row exists, so looking one up could only be a way
    /// to make the answers differ. It is here because the token exists nowhere
    /// but the email, so this is the only way to observe that a registration
    /// was recorded at all — in a test, or by an operator with a database
    /// prompt and somebody telling them the message never arrived.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the query fails.
    pub async fn find_by_email(
        &self,
        email: &EmailAddress,
    ) -> Result<Option<PendingRegistration>, UserRepositoryError> {
        let found = PendingEntity::find()
            .filter(Column::Email.eq(email.as_str()))
            .one(&self.connection)
            .await?;
        found
            .as_ref()
            .map(PendingRegistration::from_model)
            .transpose()
    }

    /// Removes a registration once it has become an account.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the delete fails.
    pub async fn finish(&self, id: PendingRegistrationId) -> Result<bool, UserRepositoryError> {
        let result = PendingEntity::delete_by_id(id.to_string())
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected == 1)
    }

    /// Removes registrations nobody confirmed in time.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the delete fails.
    pub async fn delete_expired(&self, now: OffsetDateTime) -> Result<u64, UserRepositoryError> {
        let result = PendingEntity::delete_many()
            .filter(Column::ExpiresAt.lt(now))
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected)
    }
}
