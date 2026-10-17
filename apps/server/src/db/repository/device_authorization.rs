//! Programs waiting to be approved from a browser, as rows.
//!
//! See the migration for the shape of the flow and why the token is minted on
//! redemption rather than on approval.

use sea_orm::{
    ActiveModelTrait as _, ColumnTrait as _, DatabaseConnection, EntityTrait as _,
    QueryFilter as _, Set, sea_query::Expr,
};
use tessera_core::{Permission, PermissionSet, UserId};
use time::OffsetDateTime;

use crate::{
    auth::secret::{Secret, fill_random},
    db::entity::device_authorization::{ActiveModel, Column, Entity, Model},
};

/// Letters a user code is made of: consonants only, so no code spells a word,
/// and none of the ones people misread for digits or for each other.
const USER_CODE_ALPHABET: &[u8; 20] = b"BCDFGHJKLMNPQRSTVWXZ";

/// Letters in a user code. 20^8 is about 2.6 x 10^10, against a code that
/// lives for minutes and can only be tried by someone signed in.
pub const USER_CODE_LEN: usize = 8;

/// Anything that can go wrong reading or writing this table.
#[derive(Debug, thiserror::Error)]
pub enum DeviceAuthorizationRepositoryError {
    /// The database refused the operation.
    #[error(transparent)]
    Database(#[from] sea_orm::DbErr),

    /// A stored row does not read back.
    #[error("the stored device authorization `{0}` is not usable")]
    Corrupt(String),
}

/// A request waiting for a person, as the approval page shows it.
#[derive(Debug, Clone)]
pub struct DeviceAuthorization {
    /// The code the person typed, without the dash.
    pub user_code: String,
    /// What the program says it is.
    pub client_name: String,
    /// The address the request came from.
    pub requested_from: String,
    /// When the program asked.
    pub created_at: OffsetDateTime,
    /// When the codes stop working.
    pub expires_at: OffsetDateTime,
}

impl From<Model> for DeviceAuthorization {
    fn from(model: Model) -> Self {
        Self {
            user_code: model.user_code,
            client_name: model.client_name,
            requested_from: model.requested_from,
            created_at: model.created_at,
            expires_at: model.expires_at,
        }
    }
}

/// What polling with a device code finds.
#[derive(Debug, Clone)]
pub enum Redemption {
    /// No such request: never issued, expired, or already redeemed.
    Unknown,
    /// Nobody has answered yet.
    Pending,
    /// The person said no. The row is gone.
    Denied,
    /// The person said yes. The row is gone, and the caller mints the token.
    Approved {
        /// Whose token it will be.
        user_id: UserId,
        /// What it may do.
        permissions: PermissionSet,
        /// What the program called itself, which names the token.
        client_name: String,
    },
}

/// A new user code, with no dash.
fn new_user_code() -> String {
    let mut code = String::with_capacity(USER_CODE_LEN);
    let mut bytes = [0_u8; 16];
    while code.len() < USER_CODE_LEN {
        fill_random(&mut bytes);
        // 256 is not a multiple of 20, so the top of the byte range is thrown
        // away rather than folded in, which would favour the first letters.
        for byte in bytes {
            if byte >= 240 || code.len() == USER_CODE_LEN {
                continue;
            }
            if let Some(&letter) =
                USER_CODE_ALPHABET.get(usize::from(byte) % USER_CODE_ALPHABET.len())
            {
                code.push(char::from(letter));
            }
        }
    }
    code
}

/// A user code as typed, in the form it is stored: upper case, letters only.
///
/// Anything else is dropped, so `wdjb-mjht`, `WDJB MJHT` and `WDJBMJHT` all
/// find the same row.
#[must_use]
pub fn normalize_user_code(typed: &str) -> String {
    typed
        .chars()
        .filter(char::is_ascii_alphabetic)
        .map(|letter| letter.to_ascii_uppercase())
        .collect()
}

/// Device authorizations, as the rest of the server sees them.
#[derive(Debug, Clone)]
pub struct DeviceAuthorizationRepository {
    connection: DatabaseConnection,
}

impl DeviceAuthorizationRepository {
    /// Wraps a connection.
    #[must_use]
    pub const fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    /// Records a request and returns its device code and user code.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceAuthorizationRepositoryError`] if the insert fails,
    /// which includes the user code colliding with a live one. At one in
    /// tens of billions that is left to the program asking again.
    pub async fn open(
        &self,
        client_name: String,
        requested_from: String,
        now: OffsetDateTime,
        expires_at: OffsetDateTime,
    ) -> Result<(Secret, String), DeviceAuthorizationRepositoryError> {
        let device_code = Secret::generate_session();
        let user_code = new_user_code();
        ActiveModel {
            device_code_hash: Set(device_code.digest()),
            user_code: Set(user_code.clone()),
            client_name: Set(client_name),
            requested_from: Set(requested_from),
            user_id: Set(None),
            permissions: Set(None),
            approved_at: Set(None),
            denied_at: Set(None),
            created_at: Set(now),
            expires_at: Set(expires_at),
        }
        .insert(&self.connection)
        .await?;
        Ok((device_code, user_code))
    }

    /// The request a user code names, while it is still waiting for an answer.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceAuthorizationRepositoryError`] if the query fails.
    pub async fn find_pending(
        &self,
        user_code: &str,
        now: OffsetDateTime,
    ) -> Result<Option<DeviceAuthorization>, DeviceAuthorizationRepositoryError> {
        let found = Entity::find()
            .filter(Column::UserCode.eq(user_code))
            .filter(Column::ApprovedAt.is_null())
            .filter(Column::DeniedAt.is_null())
            .filter(Column::ExpiresAt.gt(now))
            .one(&self.connection)
            .await?;
        Ok(found.map(DeviceAuthorization::from))
    }

    /// Approves a waiting request on behalf of `user_id`.
    ///
    /// Returns whether this call answered it. The database decides, so a
    /// request approved and denied at once in two tabs ends up one or the
    /// other.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceAuthorizationRepositoryError`] if the update fails.
    pub async fn approve(
        &self,
        user_code: &str,
        user_id: UserId,
        permissions: &PermissionSet,
        now: OffsetDateTime,
    ) -> Result<bool, DeviceAuthorizationRepositoryError> {
        let permissions = permissions
            .iter()
            .map(Permission::as_str)
            .collect::<Vec<_>>()
            .join(" ");
        self.answer(
            user_code,
            now,
            Entity::update_many()
                .col_expr(Column::UserId, Expr::value(Some(user_id.to_string())))
                .col_expr(Column::Permissions, Expr::value(Some(permissions)))
                .col_expr(Column::ApprovedAt, Expr::value(Some(now))),
        )
        .await
    }

    /// Denies a waiting request on behalf of `user_id`.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceAuthorizationRepositoryError`] if the update fails.
    pub async fn deny(
        &self,
        user_code: &str,
        user_id: UserId,
        now: OffsetDateTime,
    ) -> Result<bool, DeviceAuthorizationRepositoryError> {
        self.answer(
            user_code,
            now,
            Entity::update_many()
                .col_expr(Column::UserId, Expr::value(Some(user_id.to_string())))
                .col_expr(Column::DeniedAt, Expr::value(Some(now))),
        )
        .await
    }

    async fn answer(
        &self,
        user_code: &str,
        now: OffsetDateTime,
        update: sea_orm::UpdateMany<Entity>,
    ) -> Result<bool, DeviceAuthorizationRepositoryError> {
        let result = update
            .filter(Column::UserCode.eq(user_code))
            .filter(Column::ApprovedAt.is_null())
            .filter(Column::DeniedAt.is_null())
            .filter(Column::ExpiresAt.gt(now))
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected == 1)
    }

    /// Looks up a request by the device code the program polls with, and
    /// removes it once it has been answered.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceAuthorizationRepositoryError`] if a query fails or an
    /// approved row does not read back.
    pub async fn redeem(
        &self,
        device_code: &Secret,
        now: OffsetDateTime,
    ) -> Result<Redemption, DeviceAuthorizationRepositoryError> {
        let digest = device_code.digest();
        let Some(row) = Entity::find_by_id(digest.clone())
            .filter(Column::ExpiresAt.gt(now))
            .one(&self.connection)
            .await?
        else {
            return Ok(Redemption::Unknown);
        };
        if row.approved_at.is_none() && row.denied_at.is_none() {
            return Ok(Redemption::Pending);
        }
        // Whoever deletes it is the one poll that gets the answer, so two polls
        // racing cannot both mint a token.
        let deleted = Entity::delete_by_id(digest).exec(&self.connection).await?;
        if deleted.rows_affected != 1 {
            return Ok(Redemption::Unknown);
        }
        if row.denied_at.is_some() {
            return Ok(Redemption::Denied);
        }
        let corrupt = || DeviceAuthorizationRepositoryError::Corrupt(row.user_code.clone());
        let user_id = row
            .user_id
            .as_deref()
            .and_then(|id| UserId::parse(id).ok())
            .ok_or_else(corrupt)?;
        let permissions = row
            .permissions
            .as_deref()
            .unwrap_or_default()
            .split_whitespace()
            .map(str::parse::<Permission>)
            .collect::<Result<PermissionSet, _>>()
            .map_err(|_| corrupt())?;
        Ok(Redemption::Approved {
            user_id,
            permissions,
            client_name: row.client_name,
        })
    }

    /// Removes requests nobody redeemed in time.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceAuthorizationRepositoryError`] if the delete fails.
    pub async fn delete_expired(
        &self,
        now: OffsetDateTime,
    ) -> Result<u64, DeviceAuthorizationRepositoryError> {
        let result = Entity::delete_many()
            .filter(Column::ExpiresAt.lt(now))
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_user_code_uses_only_the_alphabet() {
        for _ in 0..1000 {
            let code = new_user_code();
            assert_eq!(code.len(), USER_CODE_LEN);
            assert!(code.bytes().all(|byte| USER_CODE_ALPHABET.contains(&byte)));
        }
    }

    #[test]
    fn a_typed_code_is_normalised() {
        assert_eq!(normalize_user_code("wdjb-mjht"), "WDJBMJHT");
        assert_eq!(normalize_user_code(" WDJB MJHT "), "WDJBMJHT");
    }
}
