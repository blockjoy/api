pub mod node_type;
use diesel::expression::expression_types::NotSelectable;
use diesel::pg::Pg;
use diesel::sql_types::Bool;
pub use node_type::{BlockchainNodeType, BlockchainNodeTypeId, NewBlockchainNodeType};

pub mod property;
pub use property::{BlockchainProperty, BlockchainPropertyId, NewProperty};

pub mod version;
pub use version::{BlockchainVersion, BlockchainVersionId, NewVersion};

use std::collections::{HashSet, VecDeque};

use chrono::{DateTime, Utc};
use derive_more::{Deref, Display, From, FromStr};
use diesel::dsl::{count, not};
use diesel::prelude::*;
use diesel::result::Error::NotFound;
use diesel_async::RunQueryDsl;
use diesel_derive_enum::DbEnum;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display as DisplayDoc;
use thiserror::Error;
use tonic::Status;
use uuid::Uuid;

use crate::auth::rbac::{BlockchainAdminPerm, BlockchainPerm};
use crate::auth::resource::OrgId;
use crate::auth::AuthZ;
use crate::database::Conn;
use crate::grpc::api;
use crate::models::node::{ContainerStatus, NodeStatus, SyncStatus};
use crate::models::schema::sql_types;
use crate::util::{SearchOperator, SortOrder};

use super::schema::{blockchains, nodes};
use super::Node;
use super::Paginate;

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Failed to find all blockchains: {0}
    FindAll(diesel::result::Error),
    /// Failed to find blockchain by name `{0}`: {1}
    FindByName(String, diesel::result::Error),
    /// Failed to find blockchain id `{0:?}`: {1}
    FindId(BlockchainId, diesel::result::Error),
    /// Failed to find blockchain ids `{0:?}`: {1}
    FindIds(HashSet<BlockchainId>, diesel::result::Error),
    /// Failed to get all node stats: {0}
    NodeStatsForAll(diesel::result::Error),
    /// Failed to get node stats for orgs `{0:?}`: {1}
    NodeStatsForOrgs(Vec<OrgId>, diesel::result::Error),
    /// Node pagination: {0}
    Paginate(#[from] crate::models::paginate::Error),
    /// Blockchain Property model error: {0}
    Property(#[from] property::Error),
    /// Failed to update blockchain id `{0}`: {1}
    Update(BlockchainId, diesel::result::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            FindAll(NotFound)
            | FindByName(_, NotFound)
            | FindId(_, NotFound)
            | FindIds(_, NotFound)
            | NodeStatsForAll(NotFound)
            | NodeStatsForOrgs(_, NotFound) => Status::not_found("Not found."),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Copy, Debug, Display, Hash, PartialEq, Eq, DieselNewType, Deref, From, FromStr)]
pub struct BlockchainId(Uuid);

#[derive(Clone, Debug, Queryable, Identifiable, AsChangeset)]
pub struct Blockchain {
    pub id: BlockchainId,
    pub name: String,
    pub description: Option<String>,
    pub project_url: Option<String>,
    pub repo_url: Option<String>,
    pub version: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub visibility: Visibility,
    pub ticker: String,
    pub display_name: String,
}

impl Blockchain {
    pub async fn by_id(
        id: BlockchainId,
        authz: &AuthZ,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        blockchains::table
            .filter(blockchains::visibility.eq_any(Visibility::from(authz).iter()))
            .find(id)
            .get_result(conn)
            .await
            .map_err(|err| Error::FindId(id, err))
    }

    pub async fn by_ids(
        ids: HashSet<BlockchainId>,
        authz: &AuthZ,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        blockchains::table
            .filter(blockchains::id.eq_any(ids.iter()))
            .filter(blockchains::visibility.eq_any(Visibility::from(authz).iter()))
            .order_by(super::lower(blockchains::name))
            .get_results(conn)
            .await
            .map_err(|err| Error::FindIds(ids, err))
    }
}

#[derive(Debug)]
pub struct BlockchainFilter {
    pub org_ids: Vec<OrgId>,
    pub offset: u64,
    pub limit: u64,
    pub search: Option<BlockchainSearch>,
    pub sort: VecDeque<BlockchainSort>,
}

impl BlockchainFilter {
    pub async fn query(
        mut self,
        authz: &AuthZ,
        conn: &mut Conn<'_>,
    ) -> Result<(Vec<Blockchain>, u64), Error> {
        let mut query = blockchains::table
            .filter(blockchains::visibility.eq_any(Visibility::from(authz).into_iter()))
            .into_boxed();

        if let Some(search) = self.search {
            query = query.filter(search.into_expression());
        }

        if let Some(sort) = self.sort.pop_front() {
            query = query.order_by(sort.into_expr());
        } else {
            query = query.order_by(blockchains::created_at.desc());
        }

        while let Some(sort) = self.sort.pop_front() {
            query = query.then_order_by(sort.into_expr());
        }

        query
            .select(blockchains::all_columns)
            .paginate(self.limit, self.offset)?
            .count_results(conn)
            .await
            .map_err(Into::into)
    }
}
#[derive(Debug)]
pub struct BlockchainSearch {
    pub operator: SearchOperator,
    pub id: Option<String>,
    pub name: Option<String>,
    pub display_name: Option<String>,
}

impl BlockchainSearch {
    fn into_expression(self) -> Box<dyn BoxableExpression<blockchains::table, Pg, SqlType = Bool>> {
        match self.operator {
            SearchOperator::Or => {
                let mut predicate: Box<
                    dyn BoxableExpression<blockchains::table, Pg, SqlType = Bool>,
                > = Box::new(false.into_sql::<Bool>());
                if let Some(id) = self.id {
                    predicate = Box::new(predicate.or(super::text(blockchains::id).like(id)));
                }
                if let Some(name) = self.name {
                    predicate = Box::new(predicate.or(super::lower(blockchains::name).like(name)));
                }
                if let Some(display_name) = self.display_name {
                    predicate = Box::new(
                        predicate.or(super::lower(blockchains::display_name).like(display_name)),
                    );
                }
                predicate
            }
            SearchOperator::And => {
                let mut predicate: Box<
                    dyn BoxableExpression<blockchains::table, Pg, SqlType = Bool>,
                > = Box::new(true.into_sql::<Bool>());
                if let Some(id) = self.id {
                    predicate = Box::new(predicate.and(super::text(blockchains::id).like(id)));
                }
                if let Some(name) = self.name {
                    predicate = Box::new(predicate.and(super::lower(blockchains::name).like(name)));
                }
                if let Some(display_name) = self.display_name {
                    predicate = Box::new(
                        predicate.and(super::lower(blockchains::display_name).like(display_name)),
                    );
                }
                predicate
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum BlockchainSort {
    Name(SortOrder),
    DisplayName(SortOrder),
}

impl BlockchainSort {
    fn into_expr<T>(self) -> Box<dyn BoxableExpression<T, Pg, SqlType = NotSelectable>>
    where
        blockchains::name: SelectableExpression<T>,
        blockchains::display_name: SelectableExpression<T>,
    {
        use BlockchainSort::*;
        use SortOrder::*;

        match self {
            Name(Asc) => Box::new(blockchains::name.asc()),
            Name(Desc) => Box::new(blockchains::name.desc()),

            DisplayName(Asc) => Box::new(blockchains::display_name.asc()),
            DisplayName(Desc) => Box::new(blockchains::display_name.desc()),
        }
    }
}

#[derive(Queryable)]
pub struct NodeStats {
    pub blockchain_id: BlockchainId,
    pub node_count: i64,
    pub node_count_active: i64,
    pub node_count_syncing: i64,
    pub node_count_provisioning: i64,
    pub node_count_failed: i64,
}

impl NodeStats {
    const ACTIVE_STATES: [ContainerStatus; 1] = [ContainerStatus::Running];
    const SYNCING_STATES: [SyncStatus; 1] = [SyncStatus::Syncing];
    const PROVISIONING_STATES: [NodeStatus; 1] = [NodeStatus::Provisioning];

    /// Compute stats about nodes across all orgs and their blockchain states.
    pub async fn for_all(
        authz: &AuthZ,
        conn: &mut Conn<'_>,
    ) -> Result<Option<Vec<NodeStats>>, Error> {
        if !authz.has_any_perm([BlockchainAdminPerm::Get, BlockchainAdminPerm::List]) {
            return Ok(None);
        }

        Node::not_deleted()
            .group_by(nodes::blockchain_id)
            .select((
                nodes::blockchain_id,
                count(nodes::id),
                count(nodes::container_status.eq_any(Self::ACTIVE_STATES)),
                count(nodes::sync_status.eq_any(Self::SYNCING_STATES)),
                count(nodes::node_status.eq_any(Self::PROVISIONING_STATES)),
                count(not((nodes::container_status.eq_any(Self::ACTIVE_STATES))
                    .or(nodes::sync_status.eq_any(Self::SYNCING_STATES))
                    .or(nodes::node_status.eq_any(Self::PROVISIONING_STATES)))),
            ))
            .get_results(conn)
            .await
            .map(Some)
            .map_err(Error::NodeStatsForAll)
    }

    /// Compute stats about nodes within an org and their blockchain states.
    pub async fn for_orgs(
        org_ids: &[OrgId],
        authz: &AuthZ,
        conn: &mut Conn<'_>,
    ) -> Result<Option<Vec<NodeStats>>, Error> {
        if !authz.has_any_perm([BlockchainPerm::Get, BlockchainPerm::List]) {
            return Ok(None);
        }

        Node::not_deleted()
            .filter(nodes::org_id.eq_any(org_ids.iter()))
            .group_by(nodes::blockchain_id)
            .select((
                nodes::blockchain_id,
                count(nodes::id),
                count(nodes::container_status.eq_any(Self::ACTIVE_STATES)),
                count(nodes::sync_status.eq_any(Self::SYNCING_STATES)),
                count(nodes::node_status.eq_any(Self::PROVISIONING_STATES)),
                count(not((nodes::container_status.eq_any(Self::ACTIVE_STATES))
                    .or(nodes::sync_status.eq_any(Self::SYNCING_STATES))
                    .or(nodes::node_status.eq_any(Self::PROVISIONING_STATES)))),
            ))
            .get_results(conn)
            .await
            .map(Some)
            .map_err(|err| Error::NodeStatsForOrgs(org_ids.to_vec(), err))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumBlockchainVisibility"]
pub enum Visibility {
    Private,
    Public,
    Development,
}

impl Visibility {
    fn from(authz: &AuthZ) -> Vec<Self> {
        let mut visibility = vec![];
        authz
            .has_perm(BlockchainAdminPerm::ViewPrivate)
            .then(|| visibility.push(Visibility::Private));
        authz
            .has_perm(BlockchainPerm::ViewPublic)
            .then(|| visibility.push(Visibility::Public));
        authz
            .has_perm(BlockchainPerm::ViewDevelopment)
            .then(|| visibility.push(Visibility::Development));
        visibility
    }
}

impl From<api::BlockchainVisibility> for Option<Visibility> {
    fn from(visibility: api::BlockchainVisibility) -> Self {
        match visibility {
            api::BlockchainVisibility::Unspecified => None,
            api::BlockchainVisibility::Private => Some(Visibility::Private),
            api::BlockchainVisibility::Public => Some(Visibility::Public),
            api::BlockchainVisibility::Development => Some(Visibility::Development),
        }
    }
}

impl From<Visibility> for api::BlockchainVisibility {
    fn from(visibility: Visibility) -> Self {
        match visibility {
            Visibility::Private => api::BlockchainVisibility::Private,
            Visibility::Public => api::BlockchainVisibility::Public,
            Visibility::Development => api::BlockchainVisibility::Development,
        }
    }
}
