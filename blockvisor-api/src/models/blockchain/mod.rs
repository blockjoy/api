pub mod node_type;
pub use node_type::{BlockchainNodeType, BlockchainNodeTypeId, NewBlockchainNodeType};

pub mod property;
pub use property::{BlockchainProperty, BlockchainPropertyId, NewProperty, UiType};

pub mod version;
pub use version::{BlockchainVersion, BlockchainVersionId, NewVersion};

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use derive_more::{Deref, Display, From, FromStr};
use diesel::dsl::{count, not};
use diesel::prelude::*;
use diesel::result::Error::NotFound;
use diesel_async::RunQueryDsl;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display as DisplayDoc;
use thiserror::Error;
use tonic::Status;
use uuid::Uuid;

use crate::auth::rbac::{BlockchainAdminPerm, BlockchainPerm};
use crate::auth::resource::OrgId;
use crate::auth::AuthZ;
use crate::database::Conn;
use crate::models::node::{ContainerStatus, NodeStatus, SyncStatus};

use super::schema::{blockchains, nodes};
use super::Node;

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
    /// Failed to get node stats for org `{0}`: {1}
    NodeStatsForOrg(OrgId, diesel::result::Error),
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
            | NodeStatsForOrg(_, NotFound) => Status::not_found("Not found."),
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
}

impl Blockchain {
    pub async fn find_all(conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
        blockchains::table
            .order_by(super::lower(blockchains::name))
            .get_results(conn)
            .await
            .map_err(Error::FindAll)
    }

    pub async fn find_by_id(id: BlockchainId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        blockchains::table
            .find(id)
            .get_result(conn)
            .await
            .map_err(|err| Error::FindId(id, err))
    }

    pub async fn find_by_ids(
        ids: HashSet<BlockchainId>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        blockchains::table
            .filter(blockchains::id.eq_any(ids.iter()))
            .order_by(super::lower(blockchains::name))
            .get_results(conn)
            .await
            .map_err(|err| Error::FindIds(ids, err))
    }

    pub async fn find_by_name(blockchain: &str, conn: &mut Conn<'_>) -> Result<Self, Error> {
        blockchains::table
            .filter(super::lower(blockchains::name).eq(super::lower(blockchain)))
            .first(conn)
            .await
            .map_err(|err| Error::FindByName(blockchain.to_lowercase(), err))
    }

    pub async fn update(&self, conn: &mut Conn<'_>) -> Result<Self, Error> {
        let mut updated = self.clone();
        updated.updated_at = Utc::now();

        diesel::update(blockchains::table.find(updated.id))
            .set(updated)
            .get_result(conn)
            .await
            .map_err(|err| Error::Update(self.id, err))
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
    pub async fn for_org(
        org_id: OrgId,
        authz: &AuthZ,
        conn: &mut Conn<'_>,
    ) -> Result<Option<Vec<NodeStats>>, Error> {
        if !authz.has_any_perm([BlockchainPerm::Get, BlockchainPerm::List]) {
            return Ok(None);
        }

        Node::not_deleted()
            .filter(nodes::org_id.eq(org_id))
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
            .map_err(|err| Error::NodeStatsForOrg(org_id, err))
    }
}
