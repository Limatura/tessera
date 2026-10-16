//! Passkeys and their challenges, as rows.

use sea_orm::{
    ActiveModelTrait as _, ColumnTrait as _, DatabaseConnection, EntityTrait as _,
    PaginatorTrait as _, QueryFilter as _, QueryOrder as _, Set, sea_query::Expr,
};
use tessera_core::UserId;
use time::OffsetDateTime;

use crate::{
    auth::{
        secret::Secret,
        webauthn::{self, NewCredential},
    },
    db::entity::{passkey, webauthn_challenge},
};

/// Anything that can go wrong reading or writing these tables.
#[derive(Debug, thiserror::Error)]
pub enum PasskeyRepositoryError {
    /// The database refused the operation.
    #[error(transparent)]
    Database(#[from] sea_orm::DbErr),

    /// A stored row does not read back.
    #[error("the stored passkey row `{0}` is not usable")]
    Corrupt(String),
}

/// A registered passkey, as the server knows it.
#[derive(Debug, Clone)]
pub struct Passkey {
    /// Row identifier, a ULID.
    pub id: String,
    /// Whose it is.
    pub user_id: UserId,
    /// The authenticator's id for it.
    pub credential_id: Vec<u8>,
    /// Its COSE public key.
    pub public_key: Vec<u8>,
    /// The signature counter when last seen.
    pub sign_count: u32,
    /// What the owner calls it.
    pub name: String,
    /// Whether it may sync to the owner's other devices.
    pub backup_eligible: bool,
    /// Whether it has.
    pub backed_up: bool,
    /// When it was registered.
    pub created_at: OffsetDateTime,
    /// When it last signed in.
    pub last_used_at: Option<OffsetDateTime>,
}

impl Passkey {
    fn from_model(model: passkey::Model) -> Result<Self, PasskeyRepositoryError> {
        let corrupt = || PasskeyRepositoryError::Corrupt(model.id.clone());
        Ok(Self {
            user_id: UserId::parse(&model.user_id).map_err(|_| corrupt())?,
            credential_id: webauthn::decode(&model.credential_id).map_err(|_| corrupt())?,
            public_key: webauthn::decode(&model.public_key).map_err(|_| corrupt())?,
            sign_count: u32::try_from(model.sign_count).map_err(|_| corrupt())?,
            id: model.id,
            name: model.name,
            backup_eligible: model.backup_eligible,
            backed_up: model.backed_up,
            created_at: model.created_at,
            last_used_at: model.last_used_at,
        })
    }
}

/// What a challenge was issued for, so one cannot be spent on the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChallengePurpose {
    /// Adding a passkey to a signed-in account.
    Register,
    /// Signing in with one.
    SignIn,
}

impl ChallengePurpose {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Register => "register",
            Self::SignIn => "sign_in",
        }
    }
}

/// The passkey tables, as the rest of the server sees them.
#[derive(Debug, Clone)]
pub struct PasskeyRepository {
    connection: DatabaseConnection,
}

/// What a credential id is looked up by.
fn credential_hash(id: &[u8]) -> String {
    Secret::from_client(webauthn::encode(id)).digest()
}

impl PasskeyRepository {
    /// Wraps a connection.
    #[must_use]
    pub const fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    /// The account's passkeys, oldest first.
    ///
    /// # Errors
    ///
    /// Returns [`PasskeyRepositoryError`] if the query fails or a row is corrupt.
    pub async fn list(&self, user: UserId) -> Result<Vec<Passkey>, PasskeyRepositoryError> {
        passkey::Entity::find()
            .filter(passkey::Column::UserId.eq(user.to_string()))
            .order_by_asc(passkey::Column::CreatedAt)
            .all(&self.connection)
            .await?
            .into_iter()
            .map(Passkey::from_model)
            .collect()
    }

    /// How many passkeys the account has.
    ///
    /// # Errors
    ///
    /// Returns [`PasskeyRepositoryError`] if the query fails.
    pub async fn count(&self, user: UserId) -> Result<u64, PasskeyRepositoryError> {
        Ok(passkey::Entity::find()
            .filter(passkey::Column::UserId.eq(user.to_string()))
            .count(&self.connection)
            .await?)
    }

    /// The passkey with this credential id, whoever's it is.
    ///
    /// # Errors
    ///
    /// Returns [`PasskeyRepositoryError`] if the query fails or the row is
    /// corrupt.
    pub async fn find_by_credential(
        &self,
        credential_id: &[u8],
    ) -> Result<Option<Passkey>, PasskeyRepositoryError> {
        passkey::Entity::find()
            .filter(passkey::Column::CredentialHash.eq(credential_hash(credential_id)))
            .one(&self.connection)
            .await?
            .map(Passkey::from_model)
            .transpose()
    }

    /// Stores a passkey that passed registration.
    ///
    /// # Errors
    ///
    /// Returns [`PasskeyRepositoryError`] if the insert fails, including when
    /// the credential is already registered.
    pub async fn add(
        &self,
        user: UserId,
        credential: &NewCredential,
        name: &str,
        now: OffsetDateTime,
    ) -> Result<Passkey, PasskeyRepositoryError> {
        let row = passkey::ActiveModel {
            id: Set(ulid::Ulid::generate().to_string()),
            user_id: Set(user.to_string()),
            credential_hash: Set(credential_hash(&credential.id)),
            credential_id: Set(webauthn::encode(&credential.id)),
            public_key: Set(webauthn::encode(&credential.public_key)),
            sign_count: Set(i64::from(credential.sign_count)),
            name: Set(name.to_owned()),
            backup_eligible: Set(credential.backup_eligible),
            backed_up: Set(credential.backed_up),
            created_at: Set(now),
            last_used_at: Set(None),
        }
        .insert(&self.connection)
        .await?;
        Passkey::from_model(row)
    }

    /// Renames one of the account's passkeys. Returns whether it was theirs.
    ///
    /// # Errors
    ///
    /// Returns [`PasskeyRepositoryError`] if the update fails.
    pub async fn rename(
        &self,
        user: UserId,
        id: &str,
        name: &str,
    ) -> Result<bool, PasskeyRepositoryError> {
        let result = passkey::Entity::update_many()
            .col_expr(passkey::Column::Name, Expr::value(name))
            .filter(passkey::Column::Id.eq(id))
            .filter(passkey::Column::UserId.eq(user.to_string()))
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected == 1)
    }

    /// Removes one of the account's passkeys. Returns whether it was theirs.
    ///
    /// # Errors
    ///
    /// Returns [`PasskeyRepositoryError`] if the delete fails.
    pub async fn remove(&self, user: UserId, id: &str) -> Result<bool, PasskeyRepositoryError> {
        let result = passkey::Entity::delete_many()
            .filter(passkey::Column::Id.eq(id))
            .filter(passkey::Column::UserId.eq(user.to_string()))
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected == 1)
    }

    /// Records a sign-in: the new counter, whether it has synced since, and
    /// when.
    ///
    /// # Errors
    ///
    /// Returns [`PasskeyRepositoryError`] if the update fails.
    pub async fn record_use(
        &self,
        id: &str,
        sign_count: u32,
        backed_up: bool,
        now: OffsetDateTime,
    ) -> Result<(), PasskeyRepositoryError> {
        passkey::Entity::update_many()
            .col_expr(
                passkey::Column::SignCount,
                Expr::value(i64::from(sign_count)),
            )
            .col_expr(passkey::Column::BackedUp, Expr::value(backed_up))
            .col_expr(passkey::Column::LastUsedAt, Expr::value(Some(now)))
            .filter(passkey::Column::Id.eq(id))
            .exec(&self.connection)
            .await?;
        Ok(())
    }

    /// Issues a challenge and returns it, for the browser to sign.
    ///
    /// # Errors
    ///
    /// Returns [`PasskeyRepositoryError`] if the insert fails.
    pub async fn open_challenge(
        &self,
        purpose: ChallengePurpose,
        user: Option<UserId>,
        now: OffsetDateTime,
        expires_at: OffsetDateTime,
    ) -> Result<String, PasskeyRepositoryError> {
        let challenge = webauthn::new_challenge();
        webauthn_challenge::ActiveModel {
            secret_hash: Set(Secret::from_client(challenge.clone()).digest()),
            purpose: Set(purpose.as_str().to_owned()),
            user_id: Set(user.map(|user| user.to_string())),
            created_at: Set(now),
            expires_at: Set(expires_at),
        }
        .insert(&self.connection)
        .await?;
        Ok(challenge)
    }

    /// Spends a challenge the browser echoed back, if it was issued for this
    /// purpose and has not expired.
    ///
    /// `Some` with the account a registration was for (`None` inside for a
    /// sign-in), or `None` when there is no such challenge. Spent even when the
    /// answer goes on to fail, so each challenge is tried once.
    ///
    /// # Errors
    ///
    /// Returns [`PasskeyRepositoryError`] if a query fails or the row is
    /// corrupt.
    pub async fn take_challenge(
        &self,
        challenge: &str,
        purpose: ChallengePurpose,
        now: OffsetDateTime,
    ) -> Result<Option<Option<UserId>>, PasskeyRepositoryError> {
        let digest = Secret::from_client(challenge.to_owned()).digest();
        let Some(row) = webauthn_challenge::Entity::find_by_id(digest.clone())
            .filter(webauthn_challenge::Column::Purpose.eq(purpose.as_str()))
            .filter(webauthn_challenge::Column::ExpiresAt.gt(now))
            .one(&self.connection)
            .await?
        else {
            return Ok(None);
        };
        // Whoever deletes it is the one answer that counts.
        let deleted = webauthn_challenge::Entity::delete_by_id(digest)
            .exec(&self.connection)
            .await?;
        if deleted.rows_affected != 1 {
            return Ok(None);
        }
        let user = row
            .user_id
            .as_deref()
            .map(|id| {
                UserId::parse(id)
                    .map_err(|_| PasskeyRepositoryError::Corrupt(row.secret_hash.clone()))
            })
            .transpose()?;
        Ok(Some(user))
    }

    /// Removes challenges nobody answered.
    ///
    /// # Errors
    ///
    /// Returns [`PasskeyRepositoryError`] if the delete fails.
    pub async fn delete_expired_challenges(
        &self,
        now: OffsetDateTime,
    ) -> Result<u64, PasskeyRepositoryError> {
        let result = webauthn_challenge::Entity::delete_many()
            .filter(webauthn_challenge::Column::ExpiresAt.lte(now))
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected)
    }
}
