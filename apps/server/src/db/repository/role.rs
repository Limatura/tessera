//! Roles and the permissions they carry.

use std::collections::HashMap;

use sea_orm::{
    ColumnTrait as _, DatabaseConnection, EntityTrait as _, PaginatorTrait, QueryFilter as _,
    QueryOrder as _, Set, TransactionTrait as _,
};
use tessera_core::{DomainError, Permission, PermissionSet, RoleId, UserId};
use time::OffsetDateTime;

use crate::db::{
    entity::{
        role::{self, Entity as RoleEntity},
        role_permission::{self, Entity as RolePermissionEntity},
        user_role::{self, Entity as UserRoleEntity},
    },
    repository::user::{CorruptRow, UserRepositoryError},
};

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
    /// Only removal is refused. An operator who wants `member` to stop being
    /// able to delete shares is entitled to say so, and refusing that would be
    /// the software deciding an instance's policy for it.
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

    /// Everything one person is allowed to do.
    ///
    /// # Why an unreadable permission is skipped rather than fatal
    ///
    /// A row naming a permission this version does not have is what a downgrade
    /// leaves behind. Refusing the whole set would lock every one of that
    /// person's requests out, including the ones the rest of their roles do
    /// allow — a strictly worse outcome than ignoring the one word nobody here
    /// can act on. It is logged, because it means somebody is running a mixed
    /// pair of versions.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if a query fails.
    pub async fn permissions_of(&self, user: UserId) -> Result<PermissionSet, UserRepositoryError> {
        let role_ids: Vec<String> = UserRoleEntity::find()
            .filter(user_role::Column::UserId.eq(user.to_string()))
            .all(&self.connection)
            .await?
            .into_iter()
            .map(|row| row.role_id)
            .collect();
        if role_ids.is_empty() {
            return Ok(PermissionSet::default());
        }

        let rows = RolePermissionEntity::find()
            .filter(role_permission::Column::RoleId.is_in(role_ids))
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
        let role_ids: Vec<String> = UserRoleEntity::find()
            .filter(user_role::Column::UserId.eq(user.to_string()))
            .all(&self.connection)
            .await?
            .into_iter()
            .map(|row| row.role_id)
            .collect();
        if role_ids.is_empty() {
            return Ok(Vec::new());
        }

        let models = RoleEntity::find()
            .filter(role::Column::Id.is_in(role_ids))
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
    /// Three queries whatever the number of accounts, rather than two each.
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
        let ids: Vec<String> = users.iter().map(|user| user.id.to_string()).collect();
        let memberships = UserRoleEntity::find()
            .filter(user_role::Column::UserId.is_in(ids))
            .all(&self.connection)
            .await?;

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
            resolved.insert(user.id, Vec::new());
        }
        for membership in &memberships {
            let (Ok(user), Some(role)) = (
                UserId::parse(&membership.user_id),
                by_id.get(&membership.role_id),
            ) else {
                continue;
            };
            if let Some(held) = resolved.get_mut(&user) {
                held.push(role.clone());
            }
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
    /// Used to answer "is this name taken" before writing, so the unique index
    /// is a backstop rather than the thing producing the error message.
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
    /// One query rather than one per role: the listing shows the number beside
    /// every role, and it is the number an operator wants before editing one.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the query fails or a row is not usable.
    pub async fn member_counts(&self) -> Result<HashMap<RoleId, u64>, UserRepositoryError> {
        let mut counts: HashMap<RoleId, u64> = HashMap::new();
        for row in UserRoleEntity::find().all(&self.connection).await? {
            if let Ok(id) = RoleId::parse(&row.role_id) {
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
            UserRoleEntity::find().filter(user_role::Column::RoleId.eq(id.to_string())),
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
        name: &str,
        description: Option<String>,
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
            .col_expr(role::Column::Name, name.into())
            .col_expr(role::Column::Description, description.into())
            .col_expr(role::Column::UpdatedAt, now.into())
            .filter(role::Column::Id.eq(id.to_string()))
            .exec(&transaction)
            .await?;
        transaction.commit().await?;
        Ok(RoleChange::Done)
    }

    /// Creates a role.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if the write fails — including when the
    /// name is taken, which the unique index refuses.
    pub async fn create(
        &self,
        name: String,
        description: Option<String>,
        permissions: &PermissionSet,
        now: OffsetDateTime,
    ) -> Result<Role, UserRepositoryError> {
        let id = RoleId::generate();
        let transaction = self.connection.begin().await?;

        RoleEntity::insert(role::ActiveModel {
            id: Set(id.to_string()),
            name: Set(name.clone()),
            description: Set(description.clone()),
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
            name,
            description,
            is_system: false,
            permissions: permissions.clone(),
            created_at: now,
            updated_at: now,
        })
    }

    /// Replaces what a role allows.
    ///
    /// # Why the whole set rather than add and remove
    ///
    /// The caller is holding a list of ticked boxes. Sending the result and
    /// having this write exactly that is the only version where two operators
    /// editing the same role cannot interleave into a set neither of them
    /// chose.
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
    /// Anybody in it simply stops being in it — the foreign key cascades. That
    /// can leave somebody with no roles at all, which is a real state: signed
    /// in, allowed nothing.
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

    /// Puts somebody in exactly these roles.
    ///
    /// Same reasoning as [`set_permissions`](Self::set_permissions): the caller
    /// holds the resulting list, not a diff.
    ///
    /// # Errors
    ///
    /// Returns [`UserRepositoryError`] if a query fails.
    pub async fn set_roles_of(
        &self,
        user: UserId,
        roles: &[RoleId],
        now: OffsetDateTime,
    ) -> Result<RoleChange, UserRepositoryError> {
        // Every named role must exist before anything is written, or a typo
        // silently removes somebody from the roles they had.
        for id in roles {
            if RoleEntity::find_by_id(id.to_string())
                .one(&self.connection)
                .await?
                .is_none()
            {
                return Ok(RoleChange::NoSuchRole);
            }
        }

        let transaction = self.connection.begin().await?;
        UserRoleEntity::delete_many()
            .filter(user_role::Column::UserId.eq(user.to_string()))
            .exec(&transaction)
            .await?;
        for id in roles {
            UserRoleEntity::insert(user_role::ActiveModel {
                user_id: Set(user.to_string()),
                role_id: Set(id.to_string()),
                granted_at: Set(now),
            })
            .exec(&transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(RoleChange::Done)
    }

    /// How many people hold a permission.
    ///
    /// Asked before taking somebody's last administrator role away, which is
    /// the change that can leave an instance nobody can administer.
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

        let holders: std::collections::HashSet<String> = UserRoleEntity::find()
            .filter(user_role::Column::RoleId.is_in(granting))
            .all(&self.connection)
            .await?
            .into_iter()
            .map(|row| row.user_id)
            // A person in two roles that both grant it is one holder.
            .collect();
        Ok(u64::try_from(holders.len()).unwrap_or(u64::MAX))
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
