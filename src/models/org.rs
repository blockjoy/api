use chrono::{DateTime, Utc};
use diesel::{dsl, prelude::*};
use diesel_async::RunQueryDsl;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};

use crate::auth::resource::{OrgId, UserId};
use crate::Result;

use super::schema::{orgs, orgs_users};
use super::Conn;

#[derive(Debug, Clone, Queryable, Selectable)]
pub struct Org {
    pub id: OrgId,
    pub name: String,
    pub is_personal: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

type NotDeleted = dsl::Filter<orgs::table, dsl::IsNull<orgs::deleted_at>>;

impl Org {
    pub async fn find_by_id(org_id: OrgId, conn: &mut Conn) -> crate::Result<Self> {
        let org = Org::not_deleted().find(org_id).get_result(conn).await?;
        Ok(org)
    }

    pub async fn find_by_ids(mut org_ids: Vec<OrgId>, conn: &mut super::Conn) -> Result<Vec<Self>> {
        org_ids.sort();
        org_ids.dedup();
        let orgs = Org::not_deleted()
            .filter(orgs::id.eq_any(org_ids))
            .get_results(conn)
            .await?;
        Ok(orgs)
    }

    pub async fn filter(member_id: Option<UserId>, conn: &mut Conn) -> Result<Vec<Self>> {
        let mut query = Self::not_deleted()
            .left_join(orgs_users::table)
            .into_boxed();

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

    pub async fn memberships(user_id: UserId, conn: &mut Conn) -> Result<Vec<OrgUser>> {
        let orgs = orgs_users::table
            .filter(orgs_users::user_id.eq(user_id))
            .select(OrgUser::as_select())
            .get_results(conn)
            .await?;

        Ok(orgs)
    }

    pub async fn find_personal_org(user: &super::User, conn: &mut Conn) -> Result<Org> {
        let org = Self::not_deleted()
            .filter(orgs::is_personal)
            .filter(orgs_users::user_id.eq(user.id))
            .filter(orgs_users::role.eq(OrgRole::Owner))
            .inner_join(orgs_users::table)
            .select(Org::as_select())
            .get_result(conn)
            .await?;
        Ok(org)
    }

    /// Checks if the user is a member.
    pub async fn is_member(user_id: UserId, org_id: OrgId, conn: &mut Conn) -> Result<bool> {
        let target_user = orgs_users::table
            .filter(orgs_users::user_id.eq(user_id))
            .filter(orgs_users::org_id.eq(org_id));
        let is_member = diesel::select(dsl::exists(target_user))
            .get_result(conn)
            .await?;
        Ok(is_member)
    }

    /// Checks if the user is a member with the role `Admin` or above (the other option being
    /// `Owner`).
    pub async fn is_admin(user_id: UserId, org_id: OrgId, conn: &mut Conn) -> Result<bool> {
        let target_user = orgs_users::table
            .filter(orgs_users::user_id.eq(user_id))
            .filter(orgs_users::org_id.eq(org_id))
            .filter(orgs_users::role.eq_any([OrgRole::Admin, OrgRole::Owner]));
        let is_member = diesel::select(dsl::exists(target_user))
            .get_result(conn)
            .await?;
        Ok(is_member)
    }

    pub async fn add_member(
        &self,
        user_id: UserId,
        role: OrgRole,
        conn: &mut Conn,
    ) -> Result<OrgUser> {
        NewOrgUser::new(self.id, user_id, role).create(conn).await
    }

    pub async fn remove_member(&self, user: &super::User, conn: &mut Conn) -> Result<()> {
        let org_user = orgs_users::table
            .filter(orgs_users::user_id.eq(user.id))
            .filter(orgs_users::org_id.eq(self.id));
        diesel::delete(org_user).execute(conn).await?;
        Ok(())
    }

    /// Marks the the given organization as deleted
    pub async fn delete(&self, conn: &mut Conn) -> Result<()> {
        let to_delete = orgs::table
            .filter(orgs::id.eq(self.id))
            .filter(orgs::is_personal.eq(false));
        diesel::update(to_delete)
            .set(orgs::deleted_at.eq(chrono::Utc::now()))
            .execute(conn)
            .await?;
        Ok(())
    }

    pub async fn node_counts(orgs: &[Self], conn: &mut Conn) -> Result<HashMap<OrgId, u64>> {
        use super::schema::nodes;

        let org_ids: Vec<_> = orgs.iter().map(|o| o.id).collect();
        let counts: Vec<(OrgId, i64)> = nodes::table
            .filter(nodes::org_id.eq_any(org_ids))
            .group_by(nodes::org_id)
            .select((nodes::org_id, dsl::count(nodes::id)))
            .get_results(conn)
            .await?;
        counts
            .into_iter()
            .map(|(id, count)| Ok((id, count.try_into()?)))
            .collect()
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
    pub async fn create(self, user_id: UserId, conn: &mut Conn) -> Result<Org> {
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
    pub id: OrgId,
    pub name: Option<&'a str>,
}

impl<'a> UpdateOrg<'a> {
    /// Updates an organization
    pub async fn update(self, conn: &mut Conn) -> Result<Org> {
        let org = diesel::update(orgs::table.find(self.id))
            .set((self, orgs::updated_at.eq(chrono::Utc::now())))
            .get_result(conn)
            .await?;

        Ok(org)
    }
}

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = orgs_users)]
pub struct OrgUser {
    pub org_id: OrgId,
    pub user_id: UserId,
    pub role: OrgRole,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub host_provision_token: String,
}

impl OrgUser {
    /// For a given list of orgs, returns a map from org id to all the orgs_users entries belonging
    /// to that org.
    pub async fn by_orgs(
        orgs: &[super::Org],
        conn: &mut Conn,
    ) -> Result<HashMap<OrgId, Vec<Self>>> {
        let org_ids: Vec<OrgId> = orgs.iter().map(|o| o.id).collect();
        let org_users: Vec<Self> = orgs_users::table
            .filter(orgs_users::org_id.eq_any(org_ids))
            .get_results(conn)
            .await?;
        let mut res: HashMap<OrgId, Vec<Self>> = HashMap::new();
        for org_user in org_users {
            res.entry(org_user.org_id).or_default().push(org_user)
        }
        Ok(res)
    }

    pub async fn by_user_org(user_id: UserId, org_id: OrgId, conn: &mut Conn) -> Result<Self> {
        let org_user = orgs_users::table
            .filter(orgs_users::user_id.eq(user_id))
            .filter(orgs_users::org_id.eq(org_id))
            .get_result(conn)
            .await?;
        Ok(org_user)
    }

    pub async fn by_token(token: &str, conn: &mut Conn) -> Result<Self> {
        let org_user = orgs_users::table
            .filter(orgs_users::host_provision_token.eq(token))
            .get_result(conn)
            .await?;
        Ok(org_user)
    }

    pub async fn reset_token(&self, conn: &mut Conn) -> Result<String> {
        let token = Self::token();
        let to_update = orgs_users::table
            .filter(orgs_users::user_id.eq(self.user_id))
            .filter(orgs_users::org_id.eq(self.org_id));
        diesel::update(to_update)
            .set(orgs_users::host_provision_token.eq(&token))
            .execute(conn)
            .await?;
        Ok(token)
    }

    fn token() -> String {
        use rand::Rng;
        rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(12)
            .map(char::from)
            .collect()
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = orgs_users)]
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

    pub async fn create(self, conn: &mut Conn) -> Result<OrgUser> {
        let org_user = diesel::insert_into(orgs_users::table)
            .values((self, orgs_users::host_provision_token.eq(OrgUser::token())))
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
