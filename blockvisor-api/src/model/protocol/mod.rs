pub mod stats;

pub mod version;
pub use version::{ProtocolVersion, VersionId, VersionKey, VersionMetadata};

use std::collections::{HashSet, VecDeque};

use chrono::{DateTime, Utc};
use derive_more::{Deref, Display, From, FromStr};
use diesel::expression::expression_types::NotSelectable;
use diesel::pg::Pg;
use diesel::prelude::*;
use diesel::result::DatabaseErrorKind::UniqueViolation;
use diesel::result::Error::{DatabaseError, NotFound};
use diesel::sql_types::{Bool, Nullable};
use diesel_async::RunQueryDsl;
use diesel_derive_enum::DbEnum;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display as DisplayDoc;
use thiserror::Error;
use uuid::Uuid;

use crate::auth::rbac::{ProtocolAdminPerm, ProtocolPerm};
use crate::auth::resource::OrgId;
use crate::auth::AuthZ;
use crate::database::{Conn, WriteConn};
use crate::grpc::{common, Status};
use crate::model::sql;
use crate::util::{SearchOperator, SortOrder};

use super::schema::{protocols, sql_types};
use super::Paginate;

use self::version::ProtocolKey;

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Failed to find protocol id `{0:?}`: {1}
    ById(ProtocolId, diesel::result::Error),
    /// Failed to find protocol ids `{0:?}`: {1}
    ByIds(HashSet<ProtocolId>, diesel::result::Error),
    /// Failed to find protocol key `{0}`: {1}
    ByKey(ProtocolKey, diesel::result::Error),
    /// Failed to create new protocol: {0}
    Create(diesel::result::Error),
    /// Protocol pagination: {0}
    Paginate(#[from] crate::model::paginate::Error),
    /// Protocol Region: {0}
    Region(#[from] crate::model::region::Error),
    /// Unknown Visibility.
    UnknownVisibility,
    /// Failed to update protocol id `{0}`: {1}
    Update(ProtocolId, diesel::result::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            ById(_, NotFound) | ByIds(_, NotFound) | ByKey(_, NotFound) => {
                Status::not_found("Not found.")
            }
            Create(DatabaseError(UniqueViolation, _)) => {
                Status::already_exists("Protocol already exists.")
            }
            UnknownVisibility => Status::invalid_argument("visibility"),
            Paginate(err) => err.into(),
            Region(err) => err.into(),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Copy, Debug, Display, Hash, PartialEq, Eq, DieselNewType, Deref, From, FromStr)]
pub struct ProtocolId(Uuid);

#[derive(Clone, Debug, Queryable)]
pub struct Protocol {
    pub id: ProtocolId,
    pub org_id: Option<OrgId>,
    pub key: ProtocolKey,
    pub name: String,
    pub description: Option<String>,
    pub ticker: Option<String>,
    pub visibility: Visibility,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl Protocol {
    pub async fn by_id(
        id: ProtocolId,
        org_id: Option<OrgId>,
        authz: &AuthZ,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        protocols::table
            .find(id)
            .filter(protocols::visibility.eq_any(<&[Visibility]>::from(authz)))
            .filter(protocols::org_id.eq(org_id).or(protocols::org_id.is_null()))
            .get_result(conn)
            .await
            .map_err(|err| Error::ById(id, err))
    }

    pub async fn by_ids(
        ids: &HashSet<ProtocolId>,
        org_ids: &HashSet<OrgId>,
        authz: &AuthZ,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        protocols::table
            .filter(protocols::id.eq_any(ids))
            .filter(protocols::visibility.eq_any(<&[Visibility]>::from(authz)))
            .filter(
                protocols::org_id
                    .eq_any(org_ids)
                    .or(protocols::org_id.is_null()),
            )
            .get_results(conn)
            .await
            .map_err(|err| Error::ByIds(ids.clone(), err))
    }

    pub async fn by_key(
        key: &ProtocolKey,
        org_id: Option<OrgId>,
        authz: &AuthZ,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        protocols::table
            .filter(protocols::key.eq(key))
            .filter(protocols::visibility.eq_any(<&[Visibility]>::from(authz)))
            .filter(protocols::org_id.eq(org_id).or(protocols::org_id.is_null()))
            .get_result(conn)
            .await
            .map_err(|err| Error::ByKey(key.clone(), err))
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = protocols)]
pub struct NewProtocol {
    pub org_id: Option<OrgId>,
    pub key: ProtocolKey,
    pub name: String,
    pub description: Option<String>,
    pub ticker: Option<String>,
}

impl NewProtocol {
    pub async fn create(self, mut write: &mut WriteConn<'_, '_>) -> Result<Protocol, Error> {
        diesel::insert_into(protocols::table)
            .values(self)
            .get_result(&mut write)
            .await
            .map_err(Error::Create)
    }
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = protocols)]
pub struct UpdateProtocol<'u> {
    pub id: ProtocolId,
    pub name: Option<&'u str>,
    pub description: Option<&'u str>,
    pub visibility: Option<Visibility>,
}

impl UpdateProtocol<'_> {
    pub async fn apply(self, conn: &mut Conn<'_>) -> Result<Protocol, Error> {
        let id = self.id;
        diesel::update(protocols::table.find(id))
            .set((self, protocols::updated_at.eq(Utc::now())))
            .get_result(conn)
            .await
            .map_err(|err| Error::Update(id, err))
    }
}

#[derive(Debug)]
pub struct ProtocolFilter {
    pub org_ids: HashSet<OrgId>,
    pub search: Option<ProtocolSearch>,
    pub sort: VecDeque<ProtocolSort>,
    pub limit: i64,
    pub offset: i64,
}

impl ProtocolFilter {
    pub async fn query(
        mut self,
        authz: &AuthZ,
        conn: &mut Conn<'_>,
    ) -> Result<(Vec<Protocol>, u64), Error> {
        let mut query = protocols::table
            .filter(protocols::visibility.eq_any(<&[Visibility]>::from(authz)))
            .filter(
                protocols::org_id
                    .eq_any(self.org_ids)
                    .or(protocols::org_id.is_null()),
            )
            .into_boxed();

        if let Some(search) = self.search {
            query = query.filter(search.into_expression());
        }

        if let Some(sort) = self.sort.pop_front() {
            query = query.order_by(sort.into_expr());
        } else {
            query = query.order_by(protocols::created_at.desc());
        }

        while let Some(sort) = self.sort.pop_front() {
            query = query.then_order_by(sort.into_expr());
        }

        query
            .select(protocols::all_columns)
            .paginate(self.limit, self.offset)?
            .count_results(conn)
            .await
            .map_err(Into::into)
    }
}

#[derive(Debug)]
pub struct ProtocolSearch {
    pub operator: SearchOperator,
    pub id: Option<String>,
    pub name: Option<String>,
}

impl ProtocolSearch {
    fn into_expression(self) -> Box<dyn BoxableExpression<protocols::table, Pg, SqlType = Bool>> {
        match self.operator {
            SearchOperator::Or => {
                let mut predicate: Box<
                    dyn BoxableExpression<protocols::table, Pg, SqlType = Bool>,
                > = Box::new(false.into_sql::<Bool>());
                if let Some(id) = self.id {
                    predicate = Box::new(predicate.or(sql::text(protocols::id).like(id)));
                }
                if let Some(name) = self.name {
                    predicate = Box::new(predicate.or(sql::lower(protocols::name).like(name)));
                }
                predicate
            }
            SearchOperator::And => {
                let mut predicate: Box<
                    dyn BoxableExpression<protocols::table, Pg, SqlType = Bool>,
                > = Box::new(true.into_sql::<Bool>());
                if let Some(id) = self.id {
                    predicate = Box::new(predicate.and(sql::text(protocols::id).like(id)));
                }
                if let Some(name) = self.name {
                    predicate = Box::new(predicate.and(sql::lower(protocols::name).like(name)));
                }
                predicate
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ProtocolSort {
    Key(SortOrder),
    Name(SortOrder),
}

impl ProtocolSort {
    fn into_expr<T>(self) -> Box<dyn BoxableExpression<T, Pg, SqlType = NotSelectable>>
    where
        protocols::key: SelectableExpression<T>,
        protocols::name: SelectableExpression<T>,
    {
        use ProtocolSort::*;
        use SortOrder::*;

        match self {
            Key(Asc) => Box::new(protocols::key.asc()),
            Key(Desc) => Box::new(protocols::key.desc()),

            Name(Asc) => Box::new(protocols::name.asc()),
            Name(Desc) => Box::new(protocols::name.desc()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumVisibility"]
pub enum Visibility {
    Public,
    Private,
    Development,
}

impl From<&AuthZ> for &[Visibility] {
    fn from(authz: &AuthZ) -> Self {
        use Visibility::*;
        let view_public = authz.has_perm(ProtocolPerm::ViewPublic);
        let view_private = authz.has_perm(ProtocolAdminPerm::ViewPrivate);
        let view_development = authz.has_perm(ProtocolPerm::ViewDevelopment);

        match (view_public, view_private, view_development) {
            (false, false, false) => &[],
            (false, false, true) => &[Development],
            (false, true, false) => &[Private],
            (false, true, true) => &[Private, Development],
            (true, false, false) => &[Public],
            (true, false, true) => &[Public, Development],
            (true, true, false) => &[Public, Private],
            (true, true, true) => &[Public, Private, Development],
        }
    }
}

impl From<Visibility> for common::Visibility {
    fn from(visibility: Visibility) -> Self {
        match visibility {
            Visibility::Public => common::Visibility::Public,
            Visibility::Private => common::Visibility::Private,
            Visibility::Development => common::Visibility::Development,
        }
    }
}

impl TryFrom<common::Visibility> for Visibility {
    type Error = Error;

    fn try_from(visibility: common::Visibility) -> Result<Self, Self::Error> {
        match visibility {
            common::Visibility::Unspecified => Err(Error::UnknownVisibility),
            common::Visibility::Public => Ok(Visibility::Public),
            common::Visibility::Private => Ok(Visibility::Private),
            common::Visibility::Development => Ok(Visibility::Development),
        }
    }
}
