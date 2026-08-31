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

use crate::{
    db::entity::setting::{self, Entity as SettingEntity},
    storage::secret::Sealer,
};

/// Settings, as the rest of the server sees them.
#[derive(Debug, Clone)]
pub struct SettingRepository {
    connection: DatabaseConnection,
    /// Opens and closes the values of the keys in `settings::SEALED`.
    ///
    /// Held here so that ciphertext never leaves this type: every caller above
    /// reads plaintext and writes plaintext, and none of them has to remember
    /// which keys are which. Same arrangement as
    /// [`StorageBackendRepository`](super::storage_backend::StorageBackendRepository).
    sealer: Sealer,
}

/// What a sealed value is stored behind.
///
/// Without it a row of ciphertext looks exactly like a row of plaintext, both
/// to a person reading the table and to this code. With it, a value that has
/// the marker and will not open is a key that changed, which is a different
/// problem from a corrupt row and sends an operator somewhere different.
const SEALED_PREFIX: &str = "sealed:";

impl SettingRepository {
    /// Wraps a connection.
    #[must_use]
    pub const fn new(connection: DatabaseConnection, sealer: Sealer) -> Self {
        Self { connection, sealer }
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
                    match self.opened(&row.name, value) {
                        Ok(value) => {
                            stored.insert(row.name, value);
                        }
                        Err(error) => {
                            // Same posture as unparseable JSON above: losing one
                            // override is recoverable, an instance that will not
                            // start is not. Almost always a changed
                            // `storage.secret_key`, and the message says so.
                            tracing::warn!(
                                setting = %row.name,
                                %error,
                                "a stored setting could not be opened and was ignored"
                            );
                        }
                    }
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
        let value = self.closed(name, value)?;
        let encoded = serde_json::to_string(&value)
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
    /// Turns a stored value into what the rest of the server reads.
    ///
    /// A sealed key's value is opened. Anything else is passed through, so a
    /// key that was never sealed costs nothing.
    ///
    /// A sealed key whose stored value has no marker is left alone rather than
    /// refused: it predates this, was written by hand, or the key moved onto
    /// the list after it was stored. Refusing would turn a readable setting
    /// into a boot failure.
    fn opened(
        &self,
        name: &str,
        value: serde_json::Value,
    ) -> Result<serde_json::Value, crate::storage::secret::SecretError> {
        if !crate::settings::is_sealed(name) {
            return Ok(value);
        }
        let Some(sealed) = value.as_str().and_then(|it| it.strip_prefix(SEALED_PREFIX)) else {
            return Ok(value);
        };
        Ok(serde_json::Value::String(self.sealer.open(sealed)?))
    }

    /// Turns a value from the panel into what is stored.
    ///
    /// # Errors
    ///
    /// Refuses when the key is sealed and there is no `storage.secret_key`.
    /// Written in plaintext, it would be the one thing sealing exists to
    /// prevent, and the operator would only find out when the dump leaked.
    fn closed(
        &self,
        name: &str,
        value: &serde_json::Value,
    ) -> Result<serde_json::Value, sea_orm::DbErr> {
        if !crate::settings::is_sealed(name) {
            return Ok(value.clone());
        }
        // Only a string can be sealed. Anything else for one of these keys is a
        // caller mistake, and storing it unsealed would be the wrong repair.
        let Some(plain) = value.as_str() else {
            return Err(sea_orm::DbErr::Custom(format!(
                "`{name}` holds a credential and must be a string"
            )));
        };
        // Clearing one is `clear`, not setting it to nothing, and an empty
        // string is worth storing as itself so that "set to blank" survives a
        // round trip.
        if plain.is_empty() {
            return Ok(value.clone());
        }
        let sealed = self
            .sealer
            .seal(plain)
            .map_err(|error| sea_orm::DbErr::Custom(error.to_string()))?;
        Ok(serde_json::Value::String(format!(
            "{SEALED_PREFIX}{sealed}"
        )))
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DatabaseConfig;

    /// A key of the right shape. Any 32 bytes will do; this is not a secret.
    const KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    async fn repository(secret_key: &str) -> SettingRepository {
        let config = DatabaseConfig {
            url: "sqlite::memory:".to_owned(),
            // One connection: `sqlite::memory:` gives each connection its own
            // database, so a larger pool would silently talk to empty copies.
            max_connections: 1,
            ..Default::default()
        };
        let connection = crate::db::connect(&config).await.expect("pool");
        crate::db::migrate(&connection).await.expect("migrations");
        SettingRepository::new(connection, Sealer::new(secret_key).expect("a usable key"))
    }

    fn now() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_785_000_000).expect("a valid timestamp")
    }

    /// The value comes back as it went in, and what is on disk is not it.
    #[tokio::test]
    async fn a_sealed_setting_round_trips_without_being_stored_in_the_clear() {
        let settings = repository(KEY).await;
        let secret = serde_json::json!("a-relay-password");

        settings
            .set("mail.password", &secret, now())
            .await
            .expect("stored");

        let read = settings.all().await.expect("a read");
        assert_eq!(read.get("mail.password"), Some(&secret));

        // What is actually in the column.
        let row = SettingEntity::find_by_id("mail.password")
            .one(&settings.connection)
            .await
            .expect("a query")
            .expect("the row");
        assert!(
            row.value.contains(SEALED_PREFIX),
            "stored without the marker: {}",
            row.value
        );
        assert!(
            !row.value.contains("a-relay-password"),
            "the credential is in the table in the clear: {}",
            row.value
        );
    }

    /// Everything else is untouched, so a key that was never sealed costs
    /// nothing and reads exactly as it was written.
    #[tokio::test]
    async fn an_ordinary_setting_is_stored_as_itself() {
        let settings = repository(KEY).await;
        let value = serde_json::json!(12);

        settings
            .set("share.code_length", &value, now())
            .await
            .expect("stored");

        let row = SettingEntity::find_by_id("share.code_length")
            .one(&settings.connection)
            .await
            .expect("a query")
            .expect("the row");
        assert_eq!(row.value, "12");
        assert_eq!(
            settings
                .all()
                .await
                .expect("a read")
                .get("share.code_length"),
            Some(&value)
        );
    }

    /// Without a key there is nowhere safe to put one, and plaintext is the one
    /// thing this exists to prevent.
    #[tokio::test]
    async fn storing_a_credential_without_a_key_is_refused() {
        let settings = repository("").await;

        let refused = settings
            .set(
                "mail.password",
                &serde_json::json!("a-relay-password"),
                now(),
            )
            .await;

        assert!(refused.is_err());
        assert!(settings.all().await.expect("a read").is_empty());
    }

    /// A key that changed leaves values nothing can open. Dropped with a
    /// warning rather than failing the read: losing an override is
    /// recoverable, an instance that will not start is not.
    #[tokio::test]
    async fn a_value_that_will_not_open_is_dropped_rather_than_fatal() {
        let settings = repository(KEY).await;
        settings
            .set(
                "mail.password",
                &serde_json::json!("a-relay-password"),
                now(),
            )
            .await
            .expect("stored");
        settings
            .set("share.code_length", &serde_json::json!(12), now())
            .await
            .expect("stored");

        let elsewhere = SettingRepository::new(
            settings.connection.clone(),
            Sealer::new(&"ab".repeat(32)).expect("another usable key"),
        );
        let read = elsewhere.all().await.expect("a read");

        assert_eq!(read.get("mail.password"), None);
        assert_eq!(
            read.get("share.code_length"),
            Some(&serde_json::json!(12)),
            "one unreadable row must not take the others with it"
        );
    }

    /// A row written before this key was sealed has no marker, and is read as
    /// it stands rather than refused.
    #[tokio::test]
    async fn a_value_stored_before_sealing_still_reads() {
        let settings = repository(KEY).await;
        SettingEntity::insert(setting::ActiveModel {
            name: Set("mail.password".to_owned()),
            value: Set("\"from-an-older-version\"".to_owned()),
            updated_at: Set(now()),
        })
        .exec(&settings.connection)
        .await
        .expect("the row");

        assert_eq!(
            settings.all().await.expect("a read").get("mail.password"),
            Some(&serde_json::json!("from-an-older-version"))
        );
    }

    /// Clearing one is `clear`. Setting it to nothing stores nothing, and has
    /// to survive the round trip as nothing rather than as ciphertext.
    #[tokio::test]
    async fn an_empty_credential_stays_empty() {
        let settings = repository(KEY).await;

        settings
            .set("mail.password", &serde_json::json!(""), now())
            .await
            .expect("stored");

        assert_eq!(
            settings.all().await.expect("a read").get("mail.password"),
            Some(&serde_json::json!(""))
        );
    }

    /// Only a string can be sealed, and storing something else unsealed would
    /// be the wrong repair.
    #[tokio::test]
    async fn a_credential_that_is_not_a_string_is_refused() {
        let settings = repository(KEY).await;

        assert!(
            settings
                .set("mail.password", &serde_json::json!(42), now())
                .await
                .is_err()
        );
    }
}
