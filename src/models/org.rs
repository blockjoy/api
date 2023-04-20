use super::schema::{orgs, orgs_users, users};
use crate::auth::{FindableById, Identifiable};
use crate::models::User;
use crate::Result;
use chrono::{DateTime, Utc};
use diesel::{dsl, prelude::*};
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use uuid::Uuid;

#[derive(Debug, Clone, Queryable, Selectable)]
pub struct Org {
    pub id: Uuid,
    pub name: String,
    pub is_personal: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[tonic::async_trait]
impl FindableById for Org {
    async fn find_by_id(org_id: Uuid, conn: &mut AsyncPgConnection) -> Result<Self> {
        let org = Org::not_deleted().find(org_id).get_result(conn).await?;
        Ok(org)
    }
}

type NotDeleted = dsl::Filter<orgs::table, dsl::IsNull<orgs::deleted_at>>;

impl Org {
    pub async fn find_by_user(
        org_id: Uuid,
        user_id: Uuid,
        conn: &mut AsyncPgConnection,
    ) -> Result<Org> {
        let org = Self::not_deleted()
            .find(org_id)
            .inner_join(orgs_users::table)
            .filter(orgs_users::user_id.eq(user_id))
            .select(Org::as_select())
            .get_result(conn)
            .await?;
        Ok(org)
    }

    pub async fn filter(
        member_id: Option<uuid::Uuid>,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<Vec<Self>> {
        let mut query = orgs::table.left_join(orgs_users::table).into_boxed();

        if let Some(member_id) = member_id {
            query = query.filter(orgs_users::user_id.eq(member_id));
        }

        let orgs = query
            .select(Self::as_select())
            .distinct()
            .get_results(conn)
            .await?;
        Ok(orgs)
    }

    pub async fn find_all_by_user(user_id: Uuid, conn: &mut AsyncPgConnection) -> Result<Vec<Org>> {
        let orgs = Self::not_deleted()
            .inner_join(orgs_users::table)
            .filter(orgs_users::user_id.eq(user_id))
            .select(Org::as_select())
            .get_results(conn)
            .await?;

        Ok(orgs)
    }

    /// Returns the users of an organization
    pub async fn find_all_members(
        org_id: Uuid,
        conn: &mut AsyncPgConnection,
    ) -> Result<Vec<OrgUser>> {
        let org_users = orgs_users::table
            .filter(orgs_users::org_id.eq(org_id))
            .get_results(conn)
            .await?;
        Ok(org_users)
    }

    /// Returns the users of an organization
    pub async fn find_all_member_users(
        org_id: Uuid,
        conn: &mut AsyncPgConnection,
    ) -> Result<Vec<User>> {
        let users = orgs_users::table
            .inner_join(users::table)
            .filter(orgs_users::org_id.eq(org_id))
            .select(users::all_columns)
            .get_results(conn)
            .await?;
        Ok(users)
    }

    pub async fn find_personal_org(user_id: Uuid, conn: &mut AsyncPgConnection) -> Result<Org> {
        let org = Self::not_deleted()
            .filter(orgs::is_personal)
            .filter(orgs_users::user_id.eq(user_id))
            .filter(orgs_users::role.eq(OrgRole::Owner))
            .inner_join(orgs_users::table)
            .select(Org::as_select())
            .get_result(conn)
            .await?;
        Ok(org)
    }

    /// Returns the users of an organization
    pub async fn find_all_member_users_paginated(
        org_id: Uuid,
        limit: i64,
        offset: i64,
        conn: &mut AsyncPgConnection,
    ) -> Result<Vec<User>> {
        let users = users::table
            .inner_join(orgs_users::table)
            .filter(orgs_users::org_id.eq(org_id))
            .order_by(users::email)
            .limit(limit)
            .offset(offset)
            .select(users::all_columns)
            .get_results(conn)
            .await?;
        Ok(users)
    }

    /// Checks if the user is a member
    pub async fn is_member(
        user_id: Uuid,
        org_id: Uuid,
        conn: &mut AsyncPgConnection,
    ) -> Result<bool> {
        let target_user = orgs_users::table
            .filter(orgs_users::user_id.eq(user_id))
            .filter(orgs_users::org_id.eq(org_id));
        let is_member = diesel::select(dsl::exists(target_user))
            .get_result(conn)
            .await?;
        Ok(is_member)
    }

    pub async fn add_member(
        user_id: Uuid,
        org_id: Uuid,
        role: OrgRole,
        conn: &mut AsyncPgConnection,
    ) -> Result<OrgUser> {
        NewOrgUser::new(org_id, user_id, role).create(conn).await
    }

    /// Returns the user role in the organization
    pub async fn find_org_user(
        user_id: Uuid,
        org_id: Uuid,
        conn: &mut AsyncPgConnection,
    ) -> Result<OrgUser> {
        let org_user = orgs_users::table
            .filter(orgs_users::user_id.eq(user_id))
            .filter(orgs_users::org_id.eq(org_id))
            .get_result(conn)
            .await?;

        Ok(org_user)
    }

    pub async fn remove_org_user(
        user_id: Uuid,
        org_id: Uuid,
        conn: &mut AsyncPgConnection,
    ) -> Result<()> {
        let org_user = orgs_users::table
            .filter(orgs_users::user_id.eq(user_id))
            .filter(orgs_users::org_id.eq(org_id));
        diesel::delete(org_user).execute(conn).await?;
        Ok(())
    }

    /// Marks the the given organization as deleted
    pub async fn delete(org_id: Uuid, conn: &mut AsyncPgConnection) -> Result<()> {
        let to_delete = orgs::table
            .filter(orgs::id.eq(org_id))
            .filter(orgs::is_personal.eq(false));
        diesel::update(to_delete)
            .set(orgs::deleted_at.eq(chrono::Utc::now()))
            .execute(conn)
            .await?;
        Ok(())
    }

    /// Unmarks the the given organization as deleted
    pub async fn restore(org_id: Uuid, conn: &mut AsyncPgConnection) -> Result<Self> {
        let to_restore = orgs::table
            .filter(orgs::id.eq(org_id))
            .filter(orgs::is_personal.eq(false));
        let none: Option<chrono::DateTime<chrono::Utc>> = None;
        let org = diesel::update(to_restore)
            .set(orgs::deleted_at.eq(none))
            .get_result(conn)
            .await?;
        Ok(org)
    }

    fn not_deleted() -> NotDeleted {
        orgs::table.filter(orgs::deleted_at.is_null())
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = orgs)]
pub struct NewOrg<'a> {
    pub name: &'a str,
    pub is_personal: bool,
}

impl<'a> NewOrg<'a> {
    /// Creates a new organization
    pub async fn create(self, user_id: Uuid, conn: &mut AsyncPgConnection) -> Result<Org> {
        let org: Org = diesel::insert_into(orgs::table)
            .values(self)
            .get_result(conn)
            .await?;
        NewOrgUser::new(org.id, user_id, OrgRole::Owner)
            .create(conn)
            .await?;

        Ok(org)
    }
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = orgs)]
pub struct UpdateOrg<'a> {
    pub id: uuid::Uuid,
    pub name: Option<&'a str>,
}

impl<'a> UpdateOrg<'a> {
    /// Updates an organization
    pub async fn update(self, conn: &mut AsyncPgConnection) -> Result<Org> {
        let org = diesel::update(orgs::table.find(self.id))
            .set((self, orgs::updated_at.eq(chrono::Utc::now())))
            .get_result(conn)
            .await?;

        Ok(org)
    }
}

#[derive(Debug, Queryable)]
#[diesel(table_name = orgs_users)]
pub struct OrgUser {
    pub org_id: Uuid,
    pub user_id: Uuid,
    pub role: OrgRole,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl OrgUser {
    /// For a given list of orgs, returns a map from org id to all the orgs_users entries belonging
    /// to that org.
    pub async fn by_orgs(
        orgs: &[super::Org],
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<HashMap<uuid::Uuid, Vec<Self>>> {
        let org_ids: Vec<uuid::Uuid> = orgs.iter().map(|o| o.id).collect();
        let org_users: Vec<Self> = orgs_users::table
            .filter(orgs_users::org_id.eq_any(org_ids))
            .get_results(conn)
            .await?;
        let mut res: HashMap<Uuid, Vec<Self>> = HashMap::new();
        for org_user in org_users {
            res.entry(org_user.org_id).or_default().push(org_user)
        }
        Ok(res)
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = orgs_users)]
pub struct NewOrgUser {
    org_id: Uuid,
    user_id: Uuid,
    role: OrgRole,
}

impl NewOrgUser {
    pub fn new(org_id: Uuid, user_id: Uuid, role: OrgRole) -> Self {
        Self {
            org_id,
            user_id,
            role,
        }
    }

    pub async fn create(self, conn: &mut AsyncPgConnection) -> Result<OrgUser> {
        let org_user = diesel::insert_into(orgs_users::table)
            .values(self)
            .get_result(conn)
            .await?;
        Ok(org_user)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, diesel_derive_enum::DbEnum)]
#[ExistingTypePath = "crate::models::schema::sql_types::EnumOrgRole"]
pub enum OrgRole {
    Admin,
    Owner,
    Member,
}

impl Display for OrgRole {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            OrgRole::Admin => write!(f, "admin"),
            OrgRole::Owner => write!(f, "owner"),
            OrgRole::Member => write!(f, "member"),
        }
    }
}

impl Identifiable for OrgUser {
    fn get_id(&self) -> Uuid {
        self.user_id
    }
}
