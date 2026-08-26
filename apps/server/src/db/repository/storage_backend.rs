//! Storage backends, and the sealing that keeps their credentials out of a dump.

use sea_orm::{
    ActiveValue::Set, ColumnTrait as _, DatabaseConnection, EntityTrait as _, QueryFilter as _,
    sea_query::OnConflict,
};
use time::OffsetDateTime;

use crate::{
    db::entity::storage_backend::{self, Entity as StorageBackendEntity},
    storage::secret::{Sealer, SecretError},
};

/// A configured backend, opened.
#[derive(Debug, Clone)]
pub struct StorageBackend {
    /// What `file_segments.backend` names.
    pub name: String,
    /// Where it is, credentials and all.
    pub url: String,
    /// Backend-specific options, as `object_store` takes them.
    pub options: std::collections::BTreeMap<String, String>,
    /// Whether new uploads go here.
    pub is_default: bool,
    /// How much it may hold, or `None` for no limit.
    pub capacity_bytes: Option<u64>,
    /// Whether it still takes new content. Reads are unaffected — a full
    /// backend and a retired one both keep serving what they already hold.
    pub read_only: bool,
}

/// Anything that can go wrong reading or writing a backend.
#[derive(Debug, thiserror::Error)]
pub enum StorageBackendError {
    /// The database refused the operation.
    #[error(transparent)]
    Database(#[from] sea_orm::DbErr),

    /// The credential could not be sealed or opened.
    #[error(transparent)]
    Secret(#[from] SecretError),

    /// Removing it would leave content nothing can reach.
    ///
    /// A `SET NULL` is not available — there is nowhere else for those bytes to
    /// be — and cascading would delete somebody's files to tidy up a
    /// configuration row. So the operator is told what it is holding and left
    /// to drain it.
    #[error("`{name}` still holds {files} file(s); move them off it before removing it")]
    StillInUse {
        /// Which backend.
        name: String,
        /// How many segments still name it.
        files: u64,
    },

    /// A stored row's options are not a JSON object of strings.
    #[error("`{name}` has options this version cannot read")]
    Malformed {
        /// Which backend.
        name: String,
    },
}

/// Reads and writes `storage_backends`.
#[derive(Debug, Clone)]
pub struct StorageBackendRepository {
    connection: DatabaseConnection,
    sealer: Sealer,
}

impl StorageBackendRepository {
    /// Binds the repository to a connection and the key that opens its rows.
    #[must_use]
    pub const fn new(connection: DatabaseConnection, sealer: Sealer) -> Self {
        Self { connection, sealer }
    }

    /// Every configured backend, opened.
    ///
    /// # Errors
    ///
    /// Returns [`StorageBackendError`] if the query fails, or if a row will not
    /// open with this process's key — which is worth failing on rather than
    /// skipping, because a backend silently missing is a set of shares that
    /// silently stop working.
    pub async fn all(&self) -> Result<Vec<StorageBackend>, StorageBackendError> {
        StorageBackendEntity::find()
            .all(&self.connection)
            .await?
            .into_iter()
            .map(|model| self.open(model))
            .collect()
    }

    /// Adds a backend, or replaces one of the same name.
    ///
    /// # Errors
    ///
    /// [`SecretError::NoKey`] when this process has no key — refused rather
    /// than stored in plaintext.
    pub async fn put(
        &self,
        backend: &StorageBackend,
        now: OffsetDateTime,
    ) -> Result<(), StorageBackendError> {
        let options = serde_json::to_string(&backend.options).map_err(|_| {
            StorageBackendError::Malformed {
                name: backend.name.clone(),
            }
        })?;
        let row = storage_backend::ActiveModel {
            name: Set(backend.name.clone()),
            url: Set(self.sealer.seal(&backend.url)?),
            options: Set(self.sealer.seal(&options)?),
            is_default: Set(backend.is_default),
            capacity_bytes: Set(crate::db::repository::quota::to_column(
                backend.capacity_bytes,
            )),
            read_only: Set(backend.read_only),
            created_at: Set(now),
            updated_at: Set(now),
        };
        StorageBackendEntity::insert(row)
            .on_conflict(
                OnConflict::column(storage_backend::Column::Name)
                    .update_columns([
                        storage_backend::Column::Url,
                        storage_backend::Column::Options,
                        storage_backend::Column::IsDefault,
                        storage_backend::Column::CapacityBytes,
                        storage_backend::Column::ReadOnly,
                        storage_backend::Column::UpdatedAt,
                    ])
                    .to_owned(),
            )
            .exec(&self.connection)
            .await?;
        Ok(())
    }

    /// Removes a backend by name. Returns whether there was one.
    ///
    /// Refuses while any segment still names it — see
    /// [`StorageBackendError::StillInUse`].
    ///
    /// # Errors
    ///
    /// Returns [`StorageBackendError`] if the delete fails or the backend is
    /// still holding content.
    pub async fn remove(&self, name: &str) -> Result<bool, StorageBackendError> {
        // Checked here rather than left to a foreign key, because there is no
        // foreign key: `file_segments.backend` is a name so that a reference to
        // a backend nobody configured any more stays readable. That makes this
        // the only place the check can live.
        let held = crate::db::repository::share::ShareRepository::new(self.connection.clone())
            .segments_on_backend(name)
            .await
            .map_err(|_| StorageBackendError::StillInUse {
                name: name.to_owned(),
                files: 0,
            })?;
        if held > 0 {
            return Err(StorageBackendError::StillInUse {
                name: name.to_owned(),
                files: held,
            });
        }
        Ok(StorageBackendEntity::delete_by_id(name.to_owned())
            .exec(&self.connection)
            .await?
            .rows_affected
            == 1)
    }

    /// Makes one backend the destination for new uploads.
    ///
    /// Clears the flag everywhere first, in one transaction with setting it:
    /// two defaults is a state nothing downstream knows how to read, and it is
    /// reachable by two operators pressing the same button.
    ///
    /// # Errors
    ///
    /// Returns [`StorageBackendError`] if a write fails.
    pub async fn set_default(
        &self,
        name: &str,
        now: OffsetDateTime,
    ) -> Result<bool, StorageBackendError> {
        use sea_orm::TransactionTrait as _;

        let transaction = self.connection.begin().await?;
        StorageBackendEntity::update_many()
            .col_expr(storage_backend::Column::IsDefault, false.into())
            .exec(&transaction)
            .await?;
        let chosen = StorageBackendEntity::update_many()
            .col_expr(storage_backend::Column::IsDefault, true.into())
            .col_expr(storage_backend::Column::UpdatedAt, now.into())
            .filter(storage_backend::Column::Name.eq(name.to_owned()))
            .exec(&transaction)
            .await?;
        transaction.commit().await?;
        Ok(chosen.rows_affected == 1)
    }

    /// Takes the flag off every backend.
    ///
    /// How the one from `storage.url` becomes the destination again: it has no
    /// row to set, so it is the default by nothing else claiming it.
    ///
    /// # Errors
    ///
    /// Returns [`StorageBackendError`] if the write fails.
    pub async fn clear_default(&self, now: OffsetDateTime) -> Result<(), StorageBackendError> {
        StorageBackendEntity::update_many()
            .col_expr(storage_backend::Column::IsDefault, false.into())
            .col_expr(storage_backend::Column::UpdatedAt, now.into())
            .exec(&self.connection)
            .await?;
        Ok(())
    }

    fn open(&self, model: storage_backend::Model) -> Result<StorageBackend, StorageBackendError> {
        let url = self.sealer.open(&model.url)?;
        let options = serde_json::from_str(&self.sealer.open(&model.options)?).map_err(|_| {
            StorageBackendError::Malformed {
                name: model.name.clone(),
            }
        })?;
        Ok(StorageBackend {
            name: model.name,
            url,
            options,
            is_default: model.is_default,
            capacity_bytes: crate::db::repository::quota::from_column(model.capacity_bytes),
            read_only: model.read_only,
        })
    }
}
