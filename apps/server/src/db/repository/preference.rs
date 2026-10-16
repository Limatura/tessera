//! Reading and writing one user's own preferences.
//!
//! Storage only. Which names are allowed is decided by
//! [`tessera_core::Appearance`] before a value gets here; this module stores
//! whatever that type accepts and returns the same.

use sea_orm::{ActiveValue::Set, DatabaseConnection, EntityTrait as _, sea_query::OnConflict};
use tessera_core::{Appearance, UserId};
use time::OffsetDateTime;

use crate::db::entity::user_preference::{self, Entity as UserPreferenceEntity};

/// One user's preferences, as the rest of the server sees them.
#[derive(Debug, Clone)]
pub struct PreferenceRepository {
    connection: DatabaseConnection,
}

impl PreferenceRepository {
    /// Wraps a connection.
    #[must_use]
    pub const fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    /// What this user has chosen, empty when nothing.
    ///
    /// A stored value that will not parse is treated as nothing chosen, not an
    /// error. Only a hand edit can produce one, and the cost of being wrong is
    /// that the user sees the instance default until they next choose.
    /// Refusing to answer would break the page for them instead.
    ///
    /// # Errors
    ///
    /// Returns [`sea_orm::DbErr`] if the query fails.
    pub async fn appearance_for(&self, user: UserId) -> Result<Appearance, sea_orm::DbErr> {
        let Some(row) = UserPreferenceEntity::find_by_id(user.to_string())
            .one(&self.connection)
            .await?
        else {
            return Ok(Appearance::default());
        };
        Ok(
            serde_json::from_str(&row.appearance).unwrap_or_else(|error| {
                tracing::warn!(
                    %user,
                    %error,
                    "a stored appearance could not be read and was treated as unset"
                );
                Appearance::default()
            }),
        )
    }

    /// Replaces this user's appearance choices with `appearance`.
    ///
    /// # Errors
    ///
    /// Returns [`sea_orm::DbErr`] if the write fails, or if the value cannot be
    /// serialised.
    pub async fn set_appearance(
        &self,
        user: UserId,
        appearance: &Appearance,
        now: OffsetDateTime,
    ) -> Result<(), sea_orm::DbErr> {
        let encoded = serde_json::to_string(appearance)
            .map_err(|error| sea_orm::DbErr::Custom(error.to_string()))?;
        UserPreferenceEntity::insert(user_preference::ActiveModel {
            user_id: Set(user.to_string()),
            appearance: Set(encoded),
            updated_at: Set(now),
        })
        // One statement, not "look, then insert or update": the same user
        // saving from two devices would otherwise both find nothing and both
        // insert.
        .on_conflict(
            OnConflict::column(user_preference::Column::UserId)
                .update_columns([
                    user_preference::Column::Appearance,
                    user_preference::Column::UpdatedAt,
                ])
                .to_owned(),
        )
        .exec(&self.connection)
        .await?;
        Ok(())
    }
}
