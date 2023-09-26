use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use diesel::dsl;
use diesel::prelude::*;
use diesel::result::DatabaseErrorKind::UniqueViolation;
use diesel::result::Error::{DatabaseError, NotFound};
use diesel_async::RunQueryDsl;
use displaydoc::Display;
use rand::distributions::Alphanumeric;
use rand::Rng;
use thiserror::Error;
use tonic::Status;

use crate::auth::rbac::OrgRole;
use crate::auth::rbac::Role;
use crate::auth::resource::{OrgId, UserId};
use crate::database::Conn;

use super::rbac::RbacUser;
use super::schema::{nodes, orgs, orgs_users, user_roles};

const PERSONAL_ORG_NAME: &str = "Personal";

type NotDeleted = dsl::Filter<orgs::table, dsl::IsNull<orgs::deleted_at>>;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to create org: {0}
    Create(diesel::result::Error),
    /// Failed to create org user: {0}
    CreateOrgUser(diesel::result::Error),
    /// Failed to delete org `{0}`: {1}
    Delete(OrgId, diesel::result::Error),
    /// Failed to filter orgs: {0}
    Filter(diesel::result::Error),
    /// Failed to find org by id `{0}`: {1}
    FindById(OrgId, diesel::result::Error),
    /// Failed to find org by ids `{0:?}`: {1}
    FindByIds(HashSet<OrgId>, diesel::result::Error),
    /// Failed to find org user: {0}
    FindOrgUser(diesel::result::Error),
    /// Failed to find org user by token: {0}
    FindOrgUserByToken(diesel::result::Error),
    /// Failed to find personal org for user `{0}`: {1}
    FindPersonal(UserId, diesel::result::Error),
    /// Failed to check if org `{0}` has user `{1}`: {2}
    HasUser(OrgId, UserId, diesel::result::Error),
    /// Failed to find org memberships for user `{0}`: {1}
    Memberships(UserId, diesel::result::Error),
    /// Failed to parse node count for org: {0}
    NodeCount(std::num::TryFromIntError),
    /// Failed to get node counts for org: {0}
    NodeCounts(diesel::result::Error),
    /// Org model RBAC error: {0}
    Rbac(#[from] crate::models::rbac::Error),
    /// Failed to remove org user: {0}
    RemoveUser(diesel::result::Error),
    /// Failed to reset token: {0}
    ResetToken(diesel::result::Error),
    /// Failed to update org: {0}
    Update(diesel::result::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            Create(DatabaseError(UniqueViolation, _))
            | CreateOrgUser(DatabaseError(UniqueViolation, _)) => {
                Status::already_exists("Already exists.")
            }
            Delete(_, NotFound)
            | FindById(_, NotFound)
            | FindByIds(_, NotFound)
            | FindOrgUser(NotFound)
            | FindPersonal(_, NotFound)
            | RemoveUser(NotFound) => Status::not_found("Not found."),
            FindOrgUserByToken(_) => Status::permission_denied("Invalid token."),
            Rbac(err) => err.into(),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Debug, Clone, Queryable, Selectable)]
pub struct Org {
    pub id: OrgId,
    pub name: String,
    pub is_personal: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl Org {
    pub async fn find_by_id(id: OrgId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        Org::not_deleted()
            .find(id)
            .get_result(conn)
            .await
            .map_err(|err| Error::FindById(id, err))
    }

    pub async fn find_by_ids(
        org_ids: HashSet<OrgId>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        orgs::table
            .filter(orgs::id.eq_any(org_ids.iter()))
            .get_results(conn)
            .await
            .map_err(|err| Error::FindByIds(org_ids, err))
    }

    pub async fn filter(
        member_id: Option<UserId>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        let mut query = Self::not_deleted()
            .left_join(user_roles::table)
            .into_boxed();

        if let Some(member_id) = member_id {
            query = query.filter(user_roles::user_id.eq(member_id));
        }

        query
            .select(Self::as_select())
            .distinct()
            .get_results(conn)
            .await
            .map_err(Error::Filter)
    }

    pub async fn find_personal(user_id: UserId, conn: &mut Conn<'_>) -> Result<Org, Error> {
        Self::not_deleted()
            .inner_join(user_roles::table)
            .filter(user_roles::user_id.eq(user_id))
            .filter(orgs::is_personal)
            .select(Org::as_select())
            .get_result(conn)
            .await
            .map_err(|err| Error::FindPersonal(user_id, err))
    }

    pub async fn has_user(
        org_id: OrgId,
        user_id: UserId,
        conn: &mut Conn<'_>,
    ) -> Result<bool, Error> {
        let target_user = user_roles::table
            .filter(user_roles::user_id.eq(user_id))
            .filter(user_roles::org_id.eq(org_id));

        diesel::select(dsl::exists(target_user))
            .get_result(conn)
            .await
            .map_err(|err| Error::HasUser(org_id, user_id, err))
    }

    pub async fn add_admin(&self, user_id: UserId, conn: &mut Conn<'_>) -> Result<OrgUser, Error> {
        NewOrgUser::new(self.id, user_id, OrgRole::Admin)
            .create(conn)
            .await
    }

    pub async fn add_member(&self, user_id: UserId, conn: &mut Conn<'_>) -> Result<OrgUser, Error> {
        NewOrgUser::new(self.id, user_id, OrgRole::Member)
            .create(conn)
            .await
    }

    pub async fn remove_user(&self, user_id: UserId, conn: &mut Conn<'_>) -> Result<(), Error> {
        let org_user = orgs_users::table
            .filter(orgs_users::user_id.eq(user_id))
            .filter(orgs_users::org_id.eq(self.id));

        diesel::delete(org_user)
            .execute(conn)
            .await
            .map(|_| ())
            .map_err(Error::RemoveUser)?;

        RbacUser::unlink_role(user_id, self.id, None::<Role>, conn)
            .await
            .map_err(Into::into)
    }

    /// Marks the the given organization as deleted
    pub async fn delete(&self, conn: &mut Conn<'_>) -> Result<(), Error> {
        let org_id = self.id;
        let to_delete = orgs::table
            .filter(orgs::id.eq(org_id))
            .filter(orgs::is_personal.eq(false));

        diesel::update(to_delete)
            .set(orgs::deleted_at.eq(Utc::now()))
            .execute(conn)
            .await
            .map(|_| ())
            .map_err(|err| Error::Delete(org_id, err))
    }

    pub async fn node_counts(
        org_ids: &HashSet<OrgId>,
        conn: &mut Conn<'_>,
    ) -> Result<HashMap<OrgId, u64>, Error> {
        let counts: Vec<(OrgId, i64)> = nodes::table
            .filter(nodes::org_id.eq_any(org_ids))
            .group_by(nodes::org_id)
            .select((nodes::org_id, dsl::count(nodes::id)))
            .get_results(conn)
            .await
            .map_err(Error::NodeCounts)?;

        counts
            .into_iter()
            .map(|(id, count)| Ok((id, count.try_into().map_err(Error::NodeCount)?)))
            .collect()
    }

    fn not_deleted() -> NotDeleted {
        orgs::table.filter(orgs::deleted_at.is_null())
    }
}

impl AsRef<Org> for Org {
    fn as_ref(&self) -> &Org {
        self
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = orgs)]
pub struct NewOrg<'a> {
    pub name: &'a str,
    pub is_personal: bool,
}

impl<'a> NewOrg<'a> {
    pub fn personal() -> Self {
        NewOrg {
            name: PERSONAL_ORG_NAME,
            is_personal: true,
        }
    }

    pub async fn create(self, user_id: UserId, conn: &mut Conn<'_>) -> Result<Org, Error> {
        let role = if self.is_personal {
            OrgRole::Personal
        } else {
            OrgRole::Owner
        };

        let org: Org = diesel::insert_into(orgs::table)
            .values(self)
            .get_result(conn)
            .await
            .map_err(Error::Create)?;

        NewOrgUser::new(org.id, user_id, role).create(conn).await?;

        Ok(org)
    }
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = orgs)]
pub struct UpdateOrg<'a> {
    pub id: OrgId,
    pub name: Option<&'a str>,
}

impl<'a> UpdateOrg<'a> {
    pub async fn update(self, conn: &mut Conn<'_>) -> Result<Org, Error> {
        diesel::update(orgs::table.find(self.id))
            .set((self, orgs::updated_at.eq(Utc::now())))
            .get_result(conn)
            .await
            .map_err(Error::Update)
    }
}

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = orgs_users)]
pub struct OrgUser {
    pub org_id: OrgId,
    pub user_id: UserId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub host_provision_token: String,
}

impl OrgUser {
    pub async fn by_user_org(
        user_id: UserId,
        org_id: OrgId,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        orgs_users::table
            .filter(orgs_users::user_id.eq(user_id))
            .filter(orgs_users::org_id.eq(org_id))
            .get_result(conn)
            .await
            .map_err(Error::FindOrgUser)
    }

    pub async fn by_token(token: &str, conn: &mut Conn<'_>) -> Result<Self, Error> {
        orgs_users::table
            .filter(orgs_users::host_provision_token.eq(token))
            .get_result(conn)
            .await
            .map_err(Error::FindOrgUserByToken)
    }

    pub async fn reset_token(&self, conn: &mut Conn<'_>) -> Result<String, Error> {
        let token = Self::token();
        let to_update = orgs_users::table
            .filter(orgs_users::user_id.eq(self.user_id))
            .filter(orgs_users::org_id.eq(self.org_id));

        diesel::update(to_update)
            .set(orgs_users::host_provision_token.eq(&token))
            .execute(conn)
            .await
            .map(|_| token)
            .map_err(Error::ResetToken)
    }

    fn token() -> String {
        rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(12)
            .map(char::from)
            .collect()
    }
}

pub struct NewOrgUser {
    org_id: OrgId,
    user_id: UserId,
    role: OrgRole,
}

impl NewOrgUser {
    pub fn new(org_id: OrgId, user_id: UserId, role: OrgRole) -> Self {
        Self {
            org_id,
            user_id,
            role,
        }
    }

    /// Create a new org user.
    ///
    /// Also links the user to roles within the org based on the `OrgRole`.
    pub async fn create(self, conn: &mut Conn<'_>) -> Result<OrgUser, Error> {
        let org_user = diesel::insert_into(orgs_users::table)
            .values((
                orgs_users::org_id.eq(self.org_id),
                orgs_users::user_id.eq(self.user_id),
                orgs_users::host_provision_token.eq(OrgUser::token()),
            ))
            .get_result(conn)
            .await
            .map_err(Error::CreateOrgUser)?;

        let roles = match self.role {
            OrgRole::Owner => [OrgRole::Owner, OrgRole::Admin, OrgRole::Member].iter(),
            OrgRole::Admin => [OrgRole::Admin, OrgRole::Member].iter(),
            OrgRole::Member => [OrgRole::Member].iter(),
            OrgRole::Personal => [OrgRole::Personal].iter(),
        };
        RbacUser::link_roles(self.user_id, self.org_id, roles.copied(), conn).await?;

        Ok(org_user)
    }
}
