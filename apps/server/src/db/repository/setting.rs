//! Reading and writing the settings an administrator has changed.
//!
//! Storage only. What may be stored, and what a stored value means once it is
//! layered under the environment, is [`crate::config`]'s decision. This module
//! would otherwise be a second place that has to agree about the key list.

use std::collections::BTreeMap;

use sea_orm::{
    ActiveValue::Set, ColumnTrait as _, DatabaseConnection, EntityTrait as _, QueryFilter as _,
    sea_query::OnConflict,
};
use time::OffsetDateTime;

use crate::db::entity::setting::{self, Entity as SettingEntity};

/// Settings, as the rest of the server sees them.
#[derive(Debug, Clone)]
pub struct SettingRepository {
    connection: DatabaseConnection,
}

impl SettingRepository {
    /// Wraps a connection.
    #[must_use]
    pub const fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    /// Every stored override, by key.
    ///
    /// A value that will not parse as JSON is left out instead of failing the
    /// call. This is read at start-up, and one unreadable row must not be the
    /// reason a process refuses to boot: losing that override is recoverable,
    /// an instance that will not start is not.
    ///
    /// # Errors
    ///
    /// Returns [`sea_orm::DbErr`] if the query fails.
    pub async fn all(&self) -> Result<BTreeMap<String, serde_json::Value>, sea_orm::DbErr> {
        let mut stored = BTreeMap::new();
        for row in SettingEntity::find().all(&self.connection).await? {
            match serde_json::from_str(&row.value) {
                Ok(value) => {
                    stored.insert(row.name, value);
                }
                Err(error) => {
                    tracing::warn!(
                        setting = %row.name,
                        ?error,
                        "a stored setting is not valid JSON and was ignored"
                    );
                }
            }
        }
        Ok(stored)
    }

    /// Writes one override, replacing whatever was there.
    ///
    /// # Errors
    ///
    /// Returns [`sea_orm::DbErr`] if the write fails, or if the value cannot be
    /// serialised.
    pub async fn set(
        &self,
        name: &str,
        value: &serde_json::Value,
        now: OffsetDateTime,
    ) -> Result<(), sea_orm::DbErr> {
        let encoded = serde_json::to_string(value)
            .map_err(|error| sea_orm::DbErr::Custom(error.to_string()))?;
        SettingEntity::insert(setting::ActiveModel {
            name: Set(name.to_owned()),
            value: Set(encoded),
            updated_at: Set(now),
        })
        // One statement, not "look, then insert or update". Two
        // administrators saving at the same moment would otherwise both find
        // nothing and both insert.
        .on_conflict(
            OnConflict::column(setting::Column::Name)
                .update_columns([setting::Column::Value, setting::Column::UpdatedAt])
                .to_owned(),
        )
        .exec(&self.connection)
        .await?;
        Ok(())
    }

    /// Removes one override, so the setting falls back to the layer below.
    ///
    /// # Errors
    ///
    /// Returns [`sea_orm::DbErr`] if the delete fails.
    pub async fn clear(&self, name: &str) -> Result<bool, sea_orm::DbErr> {
        Ok(SettingEntity::delete_many()
            .filter(setting::Column::Name.eq(name))
            .exec(&self.connection)
            .await?
            .rows_affected
            > 0)
    }

    /// Removes every override. The recovery path when the panel has been used
    /// to make the instance unusable.
    ///
    /// # Errors
    ///
    /// Returns [`sea_orm::DbErr`] if the delete fails.
    pub async fn clear_all(&self) -> Result<u64, sea_orm::DbErr> {
        Ok(SettingEntity::delete_many()
            .exec(&self.connection)
            .await?
            .rows_affected)
    }
}
