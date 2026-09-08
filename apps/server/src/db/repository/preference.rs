//! Reading and writing one person's own preferences.
//!
//! Storage only. What names are allowed is [`tessera_core::Appearance`]'s
//! decision, and it is made before a value reaches here: this module stores
//! whatever that type says is valid and hands back the same.

use sea_orm::{ActiveValue::Set, DatabaseConnection, EntityTrait as _, sea_query::OnConflict};
use tessera_core::{Appearance, UserId};
use time::OffsetDateTime;

use crate::db::entity::user_preference::{self, Entity as UserPreferenceEntity};

/// One person's preferences, as the rest of the server sees them.
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

    /// What this person has chosen, empty when they have chosen nothing.
    ///
    /// A stored value that will not parse is treated as nothing chosen rather
    /// than as an error. The row can only get that way by hand, and the cost of
    /// being wrong is that somebody sees the instance's default until they next
    /// pick something. Refusing to answer would instead take the page down for
    /// them.
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

    /// Replaces this person's appearance choices with `appearance`.
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
        // One statement, not "look, then insert or update": the same person
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
