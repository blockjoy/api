use std::collections::{HashSet, VecDeque};

use chrono::{DateTime, Utc};
use diesel::dsl::{self, LeftJoinQuerySource};
use diesel::expression::expression_types::NotSelectable;
use diesel::pg::Pg;
use diesel::prelude::*;
use diesel::result::DatabaseErrorKind::UniqueViolation;
use diesel::result::Error::{DatabaseError, NotFound};
use diesel::sql_types::Bool;
use diesel_async::RunQueryDsl;
use displaydoc::Display;
use thiserror::Error;

use crate::auth::rbac::OrgRole;
use crate::auth::rbac::Role;
use crate::auth::resource::{OrgId, UserId};
use crate::database::Conn;
use crate::grpc::Status;
use crate::stripe::api::customer::CustomerId;
use crate::util::{sql, SearchOperator, SortOrder};

use super::address::AddressId;
use super::rbac::RbacUser;
use super::schema::{orgs, user_roles};
use super::{Paginate, Token};

const PERSONAL_ORG_NAME: &str = "Personal";

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to increment host count for org `{0}`: {1}
    AddHost(OrgId, diesel::result::Error),
    /// Failed to increment member count for org `{0}`: {1}
    AddMember(OrgId, diesel::result::Error),
    /// Failed to increment node count for org `{0}`: {1}
    AddNode(OrgId, diesel::result::Error),
    /// Failed to create org: {0}
    Create(diesel::result::Error),
    /// Failed to delete org `{0}`: {1}
    Delete(OrgId, diesel::result::Error),
    /// Failed to find org by id `{0}`: {1}
    FindById(OrgId, diesel::result::Error),
    /// Failed to find org by ids `{0:?}`: {1}
    FindByIds(HashSet<OrgId>, diesel::result::Error),
    /// Failed to find personal org for user `{0}`: {1}
    FindPersonal(UserId, diesel::result::Error),
    /// Failed to check if org `{0}` has user `{1}`: {2}
    HasUser(OrgId, UserId, diesel::result::Error),
    /// Failed to get host counts for org: {0}
    HostCounts(diesel::result::Error),
    /// Failed to find org memberships for user `{0}`: {1}
    Memberships(UserId, diesel::result::Error),
    /// Org pagination: {0}
    Paginate(#[from] crate::model::paginate::Error),
    /// Org model RBAC error: {0}
    Rbac(#[from] crate::model::rbac::Error),
    /// Failed to decrement host count for org `{0}`: {1}
    RemoveHost(OrgId, diesel::result::Error),
    /// Failed to decrement member count for org `{0}`: {1}
    RemoveMember(OrgId, diesel::result::Error),
    /// Failed to decrement node count for org `{0}`: {1}
    RemoveNode(OrgId, diesel::result::Error),
    /// Failed update customer_id for org: {0}
    SetCustomerId(diesel::result::Error),
    /// Org model token error: {0}
    Token(#[from] crate::model::token::Error),
    /// Failed to update org: {0}
    Update(diesel::result::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            Create(DatabaseError(UniqueViolation, _)) => Status::already_exists("Already exists."),
            Delete(_, NotFound)
            | FindById(_, NotFound)
            | FindByIds(_, NotFound)
            | FindPersonal(_, NotFound) => Status::not_found("Not found."),
            Paginate(err) => err.into(),
            Rbac(err) => err.into(),
            Token(err) => err.into(),
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
    pub host_count: i32,
    pub node_count: i32,
    pub member_count: i32,
    pub stripe_customer_id: Option<CustomerId>,
    pub address_id: Option<AddressId>,
}

impl Org {
    pub async fn by_id(id: OrgId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        orgs::table
            .find(id)
            .filter(orgs::deleted_at.is_null())
            .get_result(conn)
            .await
            .map_err(|err| Error::FindById(id, err))
    }

    pub async fn by_ids(org_ids: &HashSet<OrgId>, conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
        orgs::table
            .filter(orgs::id.eq_any(org_ids))
            .filter(orgs::deleted_at.is_null())
            .get_results(conn)
            .await
            .map_err(|err| Error::FindByIds(org_ids.clone(), err))
    }

    pub async fn find_personal(user_id: UserId, conn: &mut Conn<'_>) -> Result<Org, Error> {
        orgs::table
            .inner_join(user_roles::table)
            .filter(user_roles::user_id.eq(user_id))
            .filter(orgs::is_personal)
            .filter(orgs::deleted_at.is_null())
            .select(Org::as_select())
            .get_result(conn)
            .await
            .map_err(|err| Error::FindPersonal(user_id, err))
    }

    pub async fn set_customer_id(
        self,
        customer_id: &str,
        conn: &mut Conn<'_>,
    ) -> Result<Org, Error> {
        diesel::update(orgs::table.filter(orgs::id.eq(self.id)))
            .set(orgs::stripe_customer_id.eq(customer_id))
            .get_result(conn)
            .await
            .map_err(Error::SetCustomerId)
    }

    pub async fn add_user(
        user_id: UserId,
        org_id: OrgId,
        role: OrgRole,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        let roles = match role {
            OrgRole::Owner => [OrgRole::Owner, OrgRole::Admin, OrgRole::Member].iter(),
            OrgRole::Admin => [OrgRole::Admin, OrgRole::Member].iter(),
            OrgRole::Member => [OrgRole::Member].iter(),
            OrgRole::Personal => [OrgRole::Personal].iter(),
        };

        Token::new_host_provision(user_id, org_id, conn).await?;
        RbacUser::link_roles(user_id, org_id, roles.copied(), conn).await?;
        Org::add_member(org_id, conn).await
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

    pub async fn remove_user(
        user_id: UserId,
        org_id: OrgId,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        Token::delete_host_provision(user_id, org_id, conn).await?;
        RbacUser::unlink_role(user_id, org_id, None::<Role>, conn).await?;
        Org::remove_member(org_id, conn).await
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

    pub async fn add_host(org_id: OrgId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        diesel::update(orgs::table.filter(orgs::id.eq(org_id)))
            .set((
                orgs::host_count.eq(orgs::host_count + 1),
                orgs::updated_at.eq(Utc::now()),
            ))
            .get_result(conn)
            .await
            .map_err(|err| Error::AddHost(org_id, err))
    }

    pub async fn remove_host(org_id: OrgId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        diesel::update(orgs::table.filter(orgs::id.eq(org_id)))
            .set((
                orgs::host_count.eq(orgs::host_count - 1),
                orgs::updated_at.eq(Utc::now()),
            ))
            .get_result(conn)
            .await
            .map_err(|err| Error::RemoveHost(org_id, err))
    }

    pub async fn add_node(org_id: OrgId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        diesel::update(orgs::table.filter(orgs::id.eq(org_id)))
            .set((
                orgs::node_count.eq(orgs::node_count + 1),
                orgs::updated_at.eq(Utc::now()),
            ))
            .get_result(conn)
            .await
            .map_err(|err| Error::AddNode(org_id, err))
    }

    pub async fn remove_node(org_id: OrgId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        diesel::update(orgs::table.filter(orgs::id.eq(org_id)))
            .set((
                orgs::node_count.eq(orgs::node_count - 1),
                orgs::updated_at.eq(Utc::now()),
            ))
            .get_result(conn)
            .await
            .map_err(|err| Error::RemoveNode(org_id, err))
    }

    pub async fn add_member(org_id: OrgId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        diesel::update(orgs::table.filter(orgs::id.eq(org_id)))
            .set((
                orgs::member_count.eq(orgs::member_count + 1),
                orgs::updated_at.eq(Utc::now()),
            ))
            .get_result(conn)
            .await
            .map_err(|err| Error::AddMember(org_id, err))
    }

    pub async fn remove_member(org_id: OrgId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        diesel::update(orgs::table.filter(orgs::id.eq(org_id)))
            .set((
                orgs::member_count.eq(orgs::member_count - 1),
                orgs::updated_at.eq(Utc::now()),
            ))
            .get_result(conn)
            .await
            .map_err(|err| Error::RemoveMember(org_id, err))
    }
}

impl AsRef<Org> for Org {
    fn as_ref(&self) -> &Org {
        self
    }
}

pub struct OrgSearch {
    pub operator: SearchOperator,
    pub id: Option<String>,
    pub name: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub enum OrgSort {
    Name(SortOrder),
    CreatedAt(SortOrder),
    UpdatedAt(SortOrder),
    HostCount(SortOrder),
    NodeCount(SortOrder),
    MemberCount(SortOrder),
}

impl OrgSort {
    fn into_expr<T>(self) -> Box<dyn BoxableExpression<T, Pg, SqlType = NotSelectable>>
    where
        orgs::name: SelectableExpression<T>,
        orgs::created_at: SelectableExpression<T>,
        orgs::updated_at: SelectableExpression<T>,
        orgs::host_count: SelectableExpression<T>,
        orgs::node_count: SelectableExpression<T>,
        orgs::member_count: SelectableExpression<T>,
    {
        use OrgSort::*;
        use SortOrder::*;

        match self {
            Name(Asc) => Box::new(orgs::name.asc()),
            Name(Desc) => Box::new(orgs::name.desc()),

            CreatedAt(Asc) => Box::new(orgs::created_at.asc()),
            CreatedAt(Desc) => Box::new(orgs::created_at.desc()),

            UpdatedAt(Asc) => Box::new(orgs::updated_at.asc()),
            UpdatedAt(Desc) => Box::new(orgs::updated_at.desc()),

            HostCount(Asc) => Box::new(orgs::host_count.asc()),
            HostCount(Desc) => Box::new(orgs::host_count.desc()),

            NodeCount(Asc) => Box::new(orgs::node_count.asc()),
            NodeCount(Desc) => Box::new(orgs::node_count.desc()),

            MemberCount(Asc) => Box::new(orgs::member_count.asc()),
            MemberCount(Desc) => Box::new(orgs::member_count.desc()),
        }
    }
}

pub struct OrgFilter {
    pub member_id: Option<UserId>,
    pub personal: Option<bool>,
    pub offset: u64,
    pub limit: u64,
    pub search: Option<OrgSearch>,
    pub sort: VecDeque<OrgSort>,
}

impl OrgFilter {
    pub async fn query(mut self, conn: &mut Conn<'_>) -> Result<(Vec<Org>, u64), Error> {
        let mut query = orgs::table.left_join(user_roles::table).into_boxed();

        if let Some(search) = self.search {
            query = query.filter(search.into_expression());
        }

        if let Some(member_id) = self.member_id {
            query = query.filter(user_roles::user_id.eq(member_id));
        }

        if let Some(personal) = self.personal {
            query = query.filter(orgs::is_personal.eq(personal));
        }

        if let Some(sort) = self.sort.pop_front() {
            query = query.order_by(sort.into_expr());
        } else {
            query = query.order_by(orgs::created_at.desc());
        }

        while let Some(sort) = self.sort.pop_front() {
            query = query.then_order_by(sort.into_expr());
        }

        query
            .filter(orgs::deleted_at.is_null())
            .select(Org::as_select())
            .distinct()
            .paginate(self.limit, self.offset)?
            .count_results(conn)
            .await
            .map_err(Into::into)
    }
}

type OrgsAndUsers = LeftJoinQuerySource<orgs::table, user_roles::table>;

impl OrgSearch {
    fn into_expression(self) -> Box<dyn BoxableExpression<OrgsAndUsers, Pg, SqlType = Bool>> {
        match self.operator {
            SearchOperator::Or => {
                let mut predicate: Box<dyn BoxableExpression<OrgsAndUsers, Pg, SqlType = Bool>> =
                    Box::new(false.into_sql::<Bool>());
                if let Some(id) = self.id {
                    predicate = Box::new(predicate.or(sql::text(orgs::id).like(id)));
                }
                if let Some(name) = self.name {
                    predicate = Box::new(predicate.or(sql::lower(orgs::name).like(name)));
                }
                predicate
            }
            SearchOperator::And => {
                let mut predicate: Box<dyn BoxableExpression<OrgsAndUsers, Pg, SqlType = Bool>> =
                    Box::new(true.into_sql::<Bool>());
                if let Some(id) = self.id {
                    predicate = Box::new(predicate.and(sql::text(orgs::id).like(id)));
                }
                if let Some(name) = self.name {
                    predicate = Box::new(predicate.and(sql::lower(orgs::name).like(name)));
                }
                predicate
            }
        }
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = orgs)]
pub struct NewOrg<'a> {
    pub name: &'a str,
    pub is_personal: bool,
}

impl<'a> NewOrg<'a> {
    pub const fn personal() -> Self {
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

        Org::add_user(user_id, org.id, role, conn).await
    }
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = orgs)]
pub struct UpdateOrg<'a> {
    pub id: OrgId,
    pub name: Option<&'a str>,
    pub address_id: Option<AddressId>,
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
