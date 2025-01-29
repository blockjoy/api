use std::collections::{HashMap, HashSet};

use chrono::DateTime;
use chrono::Utc;
use diesel::dsl;
use diesel::prelude::*;
use diesel::result::Error::NotFound;
use diesel_async::RunQueryDsl;
use displaydoc::Display;
use thiserror::Error;

use crate::auth::rbac::BlockjoyRole;
use crate::auth::rbac::OrgRole;
use crate::auth::rbac::ViewRole;
use crate::auth::rbac::{Perm, Role};
use crate::auth::resource::{OrgId, UserId};
use crate::database::Conn;
use crate::grpc::Status;

use super::schema::{permissions, role_permissions, roles, user_roles};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to create all perms: {0}
    CreatePerms(diesel::result::Error),
    /// Failed to create all roles: {0}
    CreateRoles(diesel::result::Error),
    /// Failed to find org owners for org `{0}`: {1}
    FindOrgOwners(OrgId, diesel::result::Error),
    /// Failed to find roles for user `{0}` and org `{1}`: {2}
    FindOrgRoles(UserId, OrgId, diesel::result::Error),
    /// Failed to find permissions for Role `{0}`: {1}
    FindPermsForRole(Role, diesel::result::Error),
    /// Failed to find permissions for roles: {0}
    FindPermsForRoles(diesel::result::Error),
    /// Failed to find user roles for org ids `{0:?}`: `{1}`
    FindUserRolesForOrgIds(HashSet<OrgId>, diesel::result::Error),
    /// Failed to check if User `{0}` has Role `{1}`: {2}
    HasRole(UserId, Role, diesel::result::Error),
    /// Failed to link Role `{0}` to Perm `{1}`: {2}
    LinkRolePerm(Role, Perm, diesel::result::Error),
    /// Failed to link User `{0}` and Org `{1}` to Role `{2}`: {3}
    LinkUserRole(UserId, OrgId, Role, diesel::result::Error),
    /// Failed to parse Perm: {0}
    ParsePerm(String),
    /// Failed to parse Role: {0}
    ParseRole(String),
    /// Nothing was deleted.
    NothingDeleted,
    /// Nothing was inserted.
    NothingInserted,
    /// Failed to check if Role `{0}` has Perm `{1}`: {2}
    RoleHasPerm(Role, Perm, diesel::result::Error),
    /// Failed to unlink Role `{0}` from Perm `{1}`: {2}
    UnlinkRolePerm(Role, Perm, diesel::result::Error),
    /// Failed to unlink User `{0}` and Org `{1}` from Role `{2:?}`: {3}
    UnlinkUserRole(UserId, OrgId, Option<Role>, diesel::result::Error),
    /// Unexpected deleted count of `{0}`. This should not happen.
    UnexpectedDeleted(usize),
    /// Unexpected inserted count of `{0}`. This should not happen.
    UnexpectedInserted(usize),
    /// User `{0}` does not belong to org `{1}`.
    UserNotInOrg(UserId, OrgId),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            FindOrgRoles(_, _, NotFound) => Status::not_found("Org roles not found"),
            FindPermsForRole(_, NotFound)
            | FindPermsForRoles(NotFound)
            | FindUserRolesForOrgIds(_, NotFound)
            | NothingDeleted
            | NothingInserted => Status::not_found("Not found."),
            UserNotInOrg(..) => Status::forbidden("Permission denied."),
            _ => Status::internal("Internal error."),
        }
    }
}

pub struct RbacRole;

impl RbacRole {
    pub async fn create_all(conn: &mut Conn<'_>) -> Result<(), Error> {
        let roles: Vec<_> = Role::iter()
            .map(|role| (roles::name.eq(role.to_string())))
            .collect();

        let _ = diesel::insert_into(roles::table)
            .values(&roles)
            .on_conflict(roles::name)
            .do_nothing()
            .execute(conn)
            .await
            .map_err(Error::CreateRoles);

        Ok(())
    }

    #[allow(unused)]
    async fn has_perm<P, R>(role: R, perm: P, conn: &mut Conn<'_>) -> Result<bool, Error>
    where
        P: Into<Perm> + Send,
        R: Into<Role> + Send,
    {
        let (role, perm) = (role.into(), perm.into());
        let query = role_permissions::table
            .filter(role_permissions::role.eq(role.to_string()))
            .filter(role_permissions::permission.eq(perm.to_string()));

        diesel::select(dsl::exists(query))
            .get_result(conn)
            .await
            .map_err(|err| Error::RoleHasPerm(role, perm, err))
    }

    #[allow(unused)]
    async fn link_perm<P, R>(role: R, perm: P, conn: &mut Conn<'_>) -> Result<(), Error>
    where
        P: Into<Perm> + Send,
        R: Into<Role> + Send,
    {
        let (role, perm) = (role.into(), perm.into());
        diesel::insert_into(role_permissions::table)
            .values((
                role_permissions::role.eq(role.to_string()),
                role_permissions::permission.eq(perm.to_string()),
            ))
            .execute(conn)
            .await
            .map_err(|err| Error::LinkRolePerm(role, perm, err))
            .and_then(|inserted| match inserted {
                0 => Err(Error::NothingInserted),
                1 => Ok(()),
                n => Err(Error::UnexpectedInserted(n)),
            })
    }

    #[allow(unused)]
    async fn unlink_perm<P, R>(role: R, perm: P, conn: &mut Conn<'_>) -> Result<(), Error>
    where
        P: Into<Perm> + Send,
        R: Into<Role> + Send,
    {
        let (role, perm) = (role.into(), perm.into());
        diesel::delete(role_permissions::table)
            .filter(role_permissions::role.eq(role.to_string()))
            .filter(role_permissions::permission.eq(perm.to_string()))
            .execute(conn)
            .await
            .map_err(|err| Error::UnlinkRolePerm(role, perm, err))
            .and_then(|deleted| match deleted {
                0 => Err(Error::NothingDeleted),
                1 => Ok(()),
                n => Err(Error::UnexpectedDeleted(n)),
            })
    }
}

pub struct RbacPerm;

impl RbacPerm {
    pub async fn create_all(conn: &mut Conn<'_>) -> Result<(), Error> {
        let perms: Vec<_> = Perm::iter()
            .map(|perm| (permissions::name.eq(perm.to_string())))
            .collect();

        let _ = diesel::insert_into(permissions::table)
            .values(&perms)
            .on_conflict(permissions::name)
            .do_nothing()
            .execute(conn)
            .await
            .map_err(Error::CreatePerms);

        Ok(())
    }

    pub async fn for_role<R>(role: R, conn: &mut Conn<'_>) -> Result<HashSet<Perm>, Error>
    where
        R: Into<Role> + Send,
    {
        let role = role.into();
        role_permissions::table
            .filter(role_permissions::role.eq(role.to_string()))
            .select(role_permissions::permission)
            .get_results(conn)
            .await
            .map_err(|err| Error::FindPermsForRole(role, err))?
            .into_iter()
            .map(|perm: String| perm.parse().map_err(Error::ParsePerm))
            .collect()
    }

    pub async fn for_roles(
        roles: &HashSet<Role>,
        conn: &mut Conn<'_>,
    ) -> Result<HashSet<Perm>, Error> {
        role_permissions::table
            .filter(role_permissions::role.eq_any(roles.iter().map(ToString::to_string)))
            .select(role_permissions::permission)
            .get_results(conn)
            .await
            .map_err(Error::FindPermsForRoles)?
            .into_iter()
            .map(|perm: String| perm.parse().map_err(Error::ParsePerm))
            .collect()
    }

    /// Find all role permissions for a user and org.
    ///
    /// Also includes non org-specific role permissions.
    pub async fn for_org(
        user_id: UserId,
        org_id: OrgId,
        ensure_member: bool,
        conn: &mut Conn<'_>,
    ) -> Result<HashSet<Perm>, Error> {
        let roles = RbacUser::org_roles(user_id, org_id, ensure_member, conn).await?;
        let mut perms = RbacPerm::for_roles(&roles, conn).await?;

        perms.extend(RbacUser::perms_for_non_org_roles(user_id, conn).await?);
        Ok(perms)
    }
}

pub struct RbacUser;

impl RbacUser {
    pub async fn org_roles(
        user_id: UserId,
        org_id: OrgId,
        ensure_member: bool,
        conn: &mut Conn<'_>,
    ) -> Result<HashSet<Role>, Error> {
        let roles: Vec<_> = user_roles::table
            .filter(user_roles::user_id.eq(user_id))
            .filter(user_roles::org_id.eq(org_id))
            .select(user_roles::role)
            .get_results(conn)
            .await
            .map_err(|err| Error::FindOrgRoles(user_id, org_id, err))?;

        if ensure_member && roles.is_empty() {
            return Err(Error::UserNotInOrg(user_id, org_id));
        }

        roles
            .into_iter()
            .map(|role: String| role.parse().map_err(Error::ParseRole))
            .collect()
    }

    pub async fn org_owners(org_id: OrgId, conn: &mut Conn<'_>) -> Result<Vec<UserId>, Error> {
        user_roles::table
            .filter(user_roles::org_id.eq(org_id))
            .filter(user_roles::role.eq(OrgRole::Owner.to_string()))
            .select(user_roles::user_id)
            .get_results(conn)
            .await
            .map_err(|err| Error::FindOrgOwners(org_id, err))
    }

    /// The set of user permissions for roles that are not org-specific.
    pub async fn perms_for_non_org_roles(
        user_id: UserId,
        conn: &mut Conn<'_>,
    ) -> Result<HashSet<Perm>, Error> {
        let mut perms = hashset! {};

        if Self::has_non_org_role(user_id, BlockjoyRole::Admin, conn).await? {
            perms.extend(&RbacPerm::for_role(BlockjoyRole::Admin, conn).await?);
        }

        if Self::has_non_org_role(user_id, ViewRole::DeveloperPreview, conn).await? {
            perms.extend(&RbacPerm::for_role(ViewRole::DeveloperPreview, conn).await?);
        }

        Ok(perms)
    }

    /// Predicate to determine whether the user has some non org-specific role.
    pub async fn has_non_org_role<R>(
        user_id: UserId,
        role: R,
        conn: &mut Conn<'_>,
    ) -> Result<bool, Error>
    where
        R: Into<Role> + Send,
    {
        let role = role.into();
        let query = user_roles::table
            .filter(user_roles::user_id.eq(user_id))
            .filter(user_roles::role.eq(role.to_string()));

        diesel::select(dsl::exists(query))
            .get_result(conn)
            .await
            .map_err(|err| Error::HasRole(user_id, role, err))
    }

    pub async fn link_role<R>(
        user_id: UserId,
        org_id: OrgId,
        role: R,
        conn: &mut Conn<'_>,
    ) -> Result<(), Error>
    where
        R: Into<Role> + Send,
    {
        let role = role.into();
        diesel::insert_into(user_roles::table)
            .values((
                user_roles::user_id.eq(user_id),
                user_roles::org_id.eq(org_id),
                user_roles::role.eq(role.to_string()),
            ))
            .execute(conn)
            .await
            .map_err(|err| Error::LinkUserRole(user_id, org_id, role, err))
            .and_then(|inserted| match inserted {
                0 => Err(Error::NothingInserted),
                1 => Ok(()),
                n => Err(Error::UnexpectedInserted(n)),
            })
    }

    pub async fn link_roles<I, R>(
        user_id: UserId,
        org_id: OrgId,
        roles: I,
        conn: &mut Conn<'_>,
    ) -> Result<(), Error>
    where
        I: Iterator<Item = R> + Send,
        R: Into<Role> + Send,
    {
        for role in roles {
            Self::link_role(user_id, org_id, role, conn).await?;
        }

        Ok(())
    }

    /// Unlinks the user from a role within an org.
    ///
    /// If `role` is None then the user is unlinked from all roles within that org.
    pub async fn unlink_role<R>(
        user_id: UserId,
        org_id: OrgId,
        role: Option<R>,
        conn: &mut Conn<'_>,
    ) -> Result<(), Error>
    where
        R: Into<Role> + Send,
    {
        let role = role.map(Into::into);
        let mut delete = diesel::delete(user_roles::table)
            .filter(user_roles::user_id.eq(user_id))
            .filter(user_roles::org_id.eq(org_id))
            .into_boxed();

        if let Some(role) = role {
            delete = delete.filter(user_roles::role.eq(role.to_string()));
        }

        delete
            .execute(conn)
            .await
            .map_err(|err| Error::UnlinkUserRole(user_id, org_id, role, err))
            .and_then(|deleted| match deleted {
                0 => Err(Error::NothingDeleted),
                _ => Ok(()),
            })
    }
}

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = user_roles)]
pub struct UserRole {
    pub user_id: UserId,
    pub org_id: OrgId,
    pub role: String,
    pub created_at: DateTime<Utc>,
}

/// Provides a mapping of `UserId` to their roles within some `OrgId`.
#[derive(Debug)]
pub struct OrgUsers {
    pub org_id: OrgId,
    pub user_roles: HashMap<UserId, Vec<Role>>,
}

impl OrgUsers {
    pub async fn for_org_ids(
        org_ids: &HashSet<OrgId>,
        conn: &mut Conn<'_>,
    ) -> Result<HashMap<OrgId, OrgUsers>, Error> {
        let rows: Vec<UserRole> = user_roles::table
            .filter(user_roles::org_id.eq_any(org_ids))
            .get_results(conn)
            .await
            .map_err(|err| Error::FindUserRolesForOrgIds(org_ids.clone(), err))?;

        let mut orgs_users: HashMap<OrgId, OrgUsers> = HashMap::with_capacity(rows.len());

        for row in rows {
            let org_users = orgs_users
                .entry(row.org_id)
                .or_insert_with(|| OrgUsers::empty(row.org_id));

            let role = row.role.parse().map_err(Error::ParseRole)?;
            org_users
                .user_roles
                .entry(row.user_id)
                .or_default()
                .push(role);
        }

        Ok(orgs_users)
    }

    pub fn empty(org_id: OrgId) -> Self {
        Self {
            org_id,
            user_roles: HashMap::new(),
        }
    }
}
