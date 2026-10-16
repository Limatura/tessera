//! Roles and the permissions they carry.

use std::collections::HashMap;

use sea_orm::{
    ColumnTrait as _, DatabaseConnection, EntityTrait as _, PaginatorTrait, QueryFilter as _,
    QueryOrder as _, QuerySelect as _, Set, TransactionTrait as _,
};
use tessera_core::{DomainError, Permission, PermissionSet, RoleId, UserId};
use time::OffsetDateTime;

use crate::db::{
    entity::{
        role::{self, Entity as RoleEntity},
        role_permission::{self, Entity as RolePermissionEntity},
        user::{self, Entity as UserEntity},
    },
    repository::{
        quota,
        user::{CorruptRow, UserRepositoryError},
    },
};

/// Everything an edit replaces.
///
/// The whole set, not a change to it. See [`RoleRepository::update`].
#[derive(Debug)]
pub struct RoleFields<'a> {
    /// What an operator types and reads.
    pub name: &'a str,
    /// What it is for.
    pub description: Option<String>,
    /// How much storage each account in it may hold, or `None` for the
    /// instance ceiling.
    pub quota_bytes: Option<u64>,
    /// Largest single file each account in it may send, or `None` for
    /// `server.max_body_bytes`.
    pub max_file_bytes: Option<u64>,
    /// Everything it allows.
    pub permissions: &'a PermissionSet,
}

/// One role, with everything it allows.
#[derive(Debug, Clone)]
pub struct Role {
    /// Row identifier.
    pub id: RoleId,
    /// What an operator types and reads.
    pub name: String,
    /// What it is for.
    pub description: Option<String>,
    /// Whether removing it would leave the instance unadministrable.
    pub is_system: bool,
    /// How much storage each account in this role may hold.
    ///
    /// `None` asks the instance ceiling; `Some(0)` is no ceiling at all. They
    /// are not the same answer, so this cannot collapse to a plain number.
    pub quota_bytes: Option<u64>,
    /// Largest single file each account in this role may send.
    ///
    /// `None` asks `server.max_body_bytes`; `Some(0)` is no limit of its own.
    pub max_file_bytes: Option<u64>,
    /// What it allows.
    pub permissions: PermissionSet,
    /// When it was created.
    pub created_at: OffsetDateTime,
    /// When its permissions last changed.
    pub updated_at: OffsetDateTime,
}

impl Role {
    fn from_parts(model: &role::Model, permissions: PermissionSet) -> Result<Self, CorruptRow> {
        Ok(Self {
            id: RoleId::parse(&model.id).map_err(|source| CorruptRow {
                id: model.id.clone(),
                source,
            })?,
            name: model.name.clone(),
            description: model.description.clone(),
            quota_bytes: quota::from_column(model.quota_bytes),
            max_file_bytes: quota::from_column(model.max_file_bytes),
            is_system: model.is_system,
            permissions,
            created_at: model.created_at,
            updated_at: model.updated_at,
        })
    }
}

/// What went wrong while changing a role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleChange {
    /// It was changed.
    Done,
    /// No role has that identifier.
    NoSuchRole,
    /// It is one of the seeded roles and may not be removed.
    ///
    /// Only removal is refused. An operator who wants `member` to stop deleting
    /// shares may say so; refusing would be the software deciding an instance's
    /// policy.
    Seeded,
}

/// Roles, as the rest of the server sees them.
#[derive(Debug, Clone)]
pub struct RoleRepository {
    connection: DatabaseConnection,
}

impl RoleRepository {
    /// Wraps a connection.
    #[must_use]
    pub const fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    /// The role one account is in, if any.
    ///
    /// One column on `users` since the join table went, so this is a lookup by
    /// primary key.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the query fails.
    async fn role_of(&self, user: UserId) -> Result<Option<String>, UserRepositoryError> {
        Ok(UserEntity::find_by_id(user.to_string())
            .one(&self.connection)
            .await?
            .and_then(|model| model.role_id))
    }

    /// Everything one person is allowed to do.
    ///
    /// # Why an unreadable permission is skipped instead of fatal
    ///
    /// A row naming a permission this version does not have is what a downgrade
    /// leaves behind. Refusing the whole set would lock the person out of every
    /// request, including the ones their role does allow. That is worse than
    /// ignoring one unrecognised word. Logged, because it means a mixed pair of
    /// versions is running.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if a query fails.
    pub async fn permissions_of(&self, user: UserId) -> Result<PermissionSet, UserRepositoryError> {
        let Some(role_id) = self.role_of(user).await? else {
            return Ok(PermissionSet::default());
        };

        let rows = RolePermissionEntity::find()
            .filter(role_permission::Column::RoleId.eq(role_id))
            .all(&self.connection)
            .await?;
        Ok(rows
            .iter()
            .filter_map(|row| match row.permission.parse::<Permission>() {
                Ok(permission) => Some(permission),
                Err(DomainError::Malformed { .. }) => {
                    tracing::warn!(
                        permission = %row.permission,
                        role = %row.role_id,
                        "a role grants a permission this version does not have"
                    );
                    None
                }
                Err(_) => None,
            })
            .collect())
    }

    /// Every role, with its permissions, by name.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if a query fails or a row is not usable.
    pub async fn list(&self) -> Result<Vec<Role>, UserRepositoryError> {
        let models = RoleEntity::find()
            .order_by_asc(role::Column::Name)
            .all(&self.connection)
            .await?;
        let mut permissions = self.permissions_by_role().await?;
        models
            .iter()
            .map(|model| {
                let held = permissions.remove(&model.id).unwrap_or_default();
                Ok(Role::from_parts(model, held)?)
            })
            .collect()
    }

    /// The roles one person is in.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if a query fails or a row is not usable.
    pub async fn of_user(&self, user: UserId) -> Result<Vec<Role>, UserRepositoryError> {
        let Some(role_id) = self.role_of(user).await? else {
            return Ok(Vec::new());
        };

        let models = RoleEntity::find()
            .filter(role::Column::Id.eq(role_id))
            .order_by_asc(role::Column::Name)
            .all(&self.connection)
            .await?;
        let mut permissions = self.permissions_by_role().await?;
        models
            .iter()
            .map(|model| {
                let held = permissions.remove(&model.id).unwrap_or_default();
                Ok(Role::from_parts(model, held)?)
            })
            .collect()
    }

    /// The roles of many people at once.
    ///
    /// Three queries whatever the number of accounts, instead of two each.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if a query fails or a row is not usable.
    pub async fn of_users(
        &self,
        users: &[crate::db::repository::user::User],
    ) -> Result<HashMap<UserId, Vec<Role>>, UserRepositoryError> {
        if users.is_empty() {
            return Ok(HashMap::new());
        }
        // No query for the memberships: every account carries its own role now,
        // and the caller already has the accounts.
        let models = RoleEntity::find()
            .order_by_asc(role::Column::Name)
            .all(&self.connection)
            .await?;
        let mut permissions = self.permissions_by_role().await?;
        let mut by_id: HashMap<String, Role> = HashMap::with_capacity(models.len());
        for model in &models {
            let held = permissions.remove(&model.id).unwrap_or_default();
            by_id.insert(model.id.clone(), Role::from_parts(model, held)?);
        }

        let mut resolved: HashMap<UserId, Vec<Role>> = HashMap::with_capacity(users.len());
        for user in users {
            // Still a list in the answer, because that is what the responses
            // are shaped like and one role fits in a list of one. A role the
            // account names but that no longer exists comes back as no roles,
            // as the name says.
            let held = user
                .role_id
                .and_then(|id| by_id.get(&id.to_string()).cloned())
                .map(|role| vec![role])
                .unwrap_or_default();
            resolved.insert(user.id, held);
        }
        Ok(resolved)
    }

    /// One query for every role's permissions, keyed by role.
    async fn permissions_by_role(
        &self,
    ) -> Result<HashMap<String, PermissionSet>, UserRepositoryError> {
        let rows = RolePermissionEntity::find().all(&self.connection).await?;
        let mut grouped: HashMap<String, Vec<Permission>> = HashMap::new();
        for row in &rows {
            if let Ok(permission) = row.permission.parse::<Permission>() {
                grouped
                    .entry(row.role_id.clone())
                    .or_default()
                    .push(permission);
            }
        }
        Ok(grouped
            .into_iter()
            .map(|(role_id, permissions)| (role_id, permissions.into_iter().collect()))
            .collect())
    }

    /// One role, by identifier.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if a query fails or a row is not usable.
    pub async fn find_by_id(&self, id: RoleId) -> Result<Option<Role>, UserRepositoryError> {
        let Some(model) = RoleEntity::find_by_id(id.to_string())
            .one(&self.connection)
            .await?
        else {
            return Ok(None);
        };
        let held = self.permissions_of_role(&model.id).await?;
        Ok(Some(Role::from_parts(&model, held)?))
    }

    /// One role, by the name an operator typed.
    ///
    /// Answers "is this name taken" before writing, so the unique index is a
    /// backstop, not the source of the error message.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if a query fails or a row is not usable.
    pub async fn find_by_name(&self, name: &str) -> Result<Option<Role>, UserRepositoryError> {
        let Some(model) = RoleEntity::find()
            .filter(role::Column::Name.eq(name))
            .one(&self.connection)
            .await?
        else {
            return Ok(None);
        };
        let held = self.permissions_of_role(&model.id).await?;
        Ok(Some(Role::from_parts(&model, held)?))
    }

    async fn permissions_of_role(
        &self,
        role_id: &str,
    ) -> Result<PermissionSet, UserRepositoryError> {
        Ok(RolePermissionEntity::find()
            .filter(role_permission::Column::RoleId.eq(role_id))
            .all(&self.connection)
            .await?
            .iter()
            .filter_map(|row| row.permission.parse::<Permission>().ok())
            .collect())
    }

    /// How many accounts are in each role.
    ///
    /// One query, not one per role. The listing shows it beside every
    /// role, and it is what an operator wants before editing one.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the query fails or a row is not usable.
    pub async fn member_counts(&self) -> Result<HashMap<RoleId, u64>, UserRepositoryError> {
        let mut counts: HashMap<RoleId, u64> = HashMap::new();
        // Only the column, not whole accounts: this reads every row in the
        // table, and an instance with a lot of them should not be pulling
        // password hashes across to count.
        let rows: Vec<Option<String>> = UserEntity::find()
            .select_only()
            .column(user::Column::RoleId)
            .into_tuple()
            .all(&self.connection)
            .await?;
        for row in rows.into_iter().flatten() {
            if let Ok(id) = RoleId::parse(&row) {
                *counts.entry(id).or_insert(0) += 1;
            }
        }
        Ok(counts)
    }

    /// How many accounts are in one role.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the query fails.
    pub async fn member_count(&self, id: RoleId) -> Result<u64, UserRepositoryError> {
        Ok(PaginatorTrait::count(
            UserEntity::find().filter(user::Column::RoleId.eq(id.to_string())),
            &self.connection,
        )
        .await?)
    }

    /// Renames a role and replaces what it allows.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if a query fails.
    pub async fn update(
        &self,
        id: RoleId,
        fields: RoleFields<'_>,
        now: OffsetDateTime,
    ) -> Result<RoleChange, UserRepositoryError> {
        let RoleFields {
            name,
            description,
            quota_bytes,
            max_file_bytes,
            permissions,
        } = fields;
        if RoleEntity::find_by_id(id.to_string())
            .one(&self.connection)
            .await?
            .is_none()
        {
            return Ok(RoleChange::NoSuchRole);
        }

        let transaction = self.connection.begin().await?;
        RolePermissionEntity::delete_many()
            .filter(role_permission::Column::RoleId.eq(id.to_string()))
            .exec(&transaction)
            .await?;
        insert_permissions(&transaction, id, permissions).await?;
        RoleEntity::update_many()
            .col_expr(role::Column::Name, name.into())
            .col_expr(role::Column::Description, description.into())
            .col_expr(role::Column::QuotaBytes, quota::to_expr(quota_bytes))
            .col_expr(role::Column::MaxFileBytes, quota::to_expr(max_file_bytes))
            .col_expr(role::Column::UpdatedAt, now.into())
            .filter(role::Column::Id.eq(id.to_string()))
            .exec(&transaction)
            .await?;
        transaction.commit().await?;
        Ok(RoleChange::Done)
    }

    /// Sets or clears one role's storage quota.
    ///
    /// `None` clears it, which puts the role back on the instance ceiling;
    /// `Some(0)` gives it no ceiling of its own. Returns whether a row changed.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the write fails.
    pub async fn set_quota(
        &self,
        id: RoleId,
        quota_bytes: Option<u64>,
        now: OffsetDateTime,
    ) -> Result<bool, UserRepositoryError> {
        let result = RoleEntity::update_many()
            .col_expr(role::Column::QuotaBytes, quota::to_expr(quota_bytes))
            .col_expr(role::Column::UpdatedAt, now.into())
            .filter(role::Column::Id.eq(id.to_string()))
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected == 1)
    }

    /// Sets or clears one role's single-file limit.
    ///
    /// `None` clears it, which puts the role back on `server.max_body_bytes`;
    /// `Some(0)` gives it no limit of its own. Returns whether a row changed.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the write fails.
    pub async fn set_max_file_bytes(
        &self,
        id: RoleId,
        max_file_bytes: Option<u64>,
        now: OffsetDateTime,
    ) -> Result<bool, UserRepositoryError> {
        let result = RoleEntity::update_many()
            .col_expr(role::Column::MaxFileBytes, quota::to_expr(max_file_bytes))
            .col_expr(role::Column::UpdatedAt, now.into())
            .filter(role::Column::Id.eq(id.to_string()))
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected == 1)
    }

    /// Creates a role.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the write fails, including when the
    /// unique index refuses a name that is taken.
    pub async fn create(
        &self,
        fields: RoleFields<'_>,
        now: OffsetDateTime,
    ) -> Result<Role, UserRepositoryError> {
        // The same bundle `update` takes: one shape for "everything a role is",
        // so a field added to one path cannot be forgotten on the other.
        let RoleFields {
            name,
            description,
            quota_bytes,
            max_file_bytes,
            permissions,
        } = fields;
        let id = RoleId::generate();
        let transaction = self.connection.begin().await?;

        RoleEntity::insert(role::ActiveModel {
            id: Set(id.to_string()),
            name: Set(name.to_owned()),
            description: Set(description.clone()),
            quota_bytes: Set(quota::to_column(quota_bytes)),
            max_file_bytes: Set(quota::to_column(max_file_bytes)),
            is_system: Set(false),
            created_at: Set(now),
            updated_at: Set(now),
        })
        .exec(&transaction)
        .await?;
        insert_permissions(&transaction, id, permissions).await?;
        transaction.commit().await?;

        Ok(Role {
            id,
            name: name.to_owned(),
            description,
            quota_bytes,
            max_file_bytes,
            is_system: false,
            permissions: permissions.clone(),
            created_at: now,
            updated_at: now,
        })
    }

    /// Replaces what a role allows.
    ///
    /// # Why the whole set, and not add and remove
    ///
    /// The caller is holding a list of ticked boxes. Writing exactly that is
    /// the only version where two operators editing the same role cannot
    /// interleave into a set neither chose.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if a query fails.
    pub async fn set_permissions(
        &self,
        id: RoleId,
        permissions: &PermissionSet,
        now: OffsetDateTime,
    ) -> Result<RoleChange, UserRepositoryError> {
        if RoleEntity::find_by_id(id.to_string())
            .one(&self.connection)
            .await?
            .is_none()
        {
            return Ok(RoleChange::NoSuchRole);
        }

        let transaction = self.connection.begin().await?;
        RolePermissionEntity::delete_many()
            .filter(role_permission::Column::RoleId.eq(id.to_string()))
            .exec(&transaction)
            .await?;
        insert_permissions(&transaction, id, permissions).await?;
        RoleEntity::update_many()
            .col_expr(role::Column::UpdatedAt, now.into())
            .filter(role::Column::Id.eq(id.to_string()))
            .exec(&transaction)
            .await?;
        transaction.commit().await?;
        Ok(RoleChange::Done)
    }

    /// Removes a role, unless it is one of the seeded ones.
    ///
    /// Every member stops being one, by cascade. That can leave an account with
    /// no role at all: a real state, signed in and allowed nothing.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if a query fails.
    pub async fn delete(&self, id: RoleId) -> Result<RoleChange, UserRepositoryError> {
        let Some(model) = RoleEntity::find_by_id(id.to_string())
            .one(&self.connection)
            .await?
        else {
            return Ok(RoleChange::NoSuchRole);
        };
        if model.is_system {
            return Ok(RoleChange::Seeded);
        }
        RoleEntity::delete_by_id(id.to_string())
            .exec(&self.connection)
            .await?;
        Ok(RoleChange::Done)
    }

    /// Puts an account in a role, or in none.
    ///
    /// `None` is signed in and allowed nothing: a real state, and the only way
    /// to express what "no rows in the join table" used to mean.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if a query fails.
    pub async fn set_role_of(
        &self,
        user: UserId,
        role: Option<RoleId>,
        now: OffsetDateTime,
    ) -> Result<RoleChange, UserRepositoryError> {
        // The role must exist before anything is written, or a typo silently
        // strips the role the account already had.
        if let Some(id) = role
            && RoleEntity::find_by_id(id.to_string())
                .one(&self.connection)
                .await?
                .is_none()
        {
            return Ok(RoleChange::NoSuchRole);
        }

        // One statement, where the join table needed a transaction to replace a
        // set without a moment of having none.
        UserEntity::update_many()
            .col_expr(user::Column::RoleId, role.map(|id| id.to_string()).into())
            .col_expr(user::Column::UpdatedAt, now.into())
            .filter(user::Column::Id.eq(user.to_string()))
            .exec(&self.connection)
            .await?;
        Ok(RoleChange::Done)
    }

    /// How many people hold a permission.
    ///
    /// Asked before taking away an account's last administrator role, the
    /// change that can leave an instance with no administrator at all.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if a query fails.
    pub async fn holders_of(&self, permission: Permission) -> Result<u64, UserRepositoryError> {
        let granting: Vec<String> = RolePermissionEntity::find()
            .filter(role_permission::Column::Permission.eq(permission.as_str()))
            .all(&self.connection)
            .await?
            .into_iter()
            .map(|row| row.role_id)
            .collect();
        if granting.is_empty() {
            return Ok(0);
        }

        // One role each, so counting accounts is counting holders. No account
        // is in two granting roles to deduplicate.
        Ok(PaginatorTrait::count(
            UserEntity::find().filter(user::Column::RoleId.is_in(granting)),
            &self.connection,
        )
        .await?)
    }
}

async fn insert_permissions<C: sea_orm::ConnectionTrait>(
    connection: &C,
    id: RoleId,
    permissions: &PermissionSet,
) -> Result<(), UserRepositoryError> {
    for permission in permissions.iter() {
        RolePermissionEntity::insert(role_permission::ActiveModel {
            role_id: Set(id.to_string()),
            permission: Set(permission.as_str().to_owned()),
        })
        .exec(connection)
        .await?;
    }
    Ok(())
}
