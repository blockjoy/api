use std::collections::HashSet;

use chrono::{DateTime, Utc};
use derive_more::{Deref, Display, From, FromStr};
use diesel::prelude::*;
use diesel::result::DatabaseErrorKind::UniqueViolation;
use diesel::result::Error::{DatabaseError, NotFound};
use diesel_async::RunQueryDsl;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display as DisplayDoc;
use thiserror::Error;
use tonic::Status;
use uuid::Uuid;

use crate::database::Conn;
use crate::models::schema::blockchain_node_types;
use crate::models::NodeType;

use super::BlockchainId;

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Failed to bulk create blockchain node types: {0}
    BulkCreate(diesel::result::Error),
    /// Failed to create blockchain node type: {0}
    Create(diesel::result::Error),
    /// Failed to find blockchain node type by id `{0}`: {1}
    FindById(BlockchainId, diesel::result::Error),
    /// Failed to find blockchain node types by ids `{0:?}`: {1}
    FindByIds(HashSet<BlockchainId>, diesel::result::Error),
    /// Failed to find blockchain node type by id `{0}` and node_type `{1}`: {2}
    FindByNodeType(BlockchainId, NodeType, diesel::result::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            BulkCreate(DatabaseError(UniqueViolation, _))
            | Create(DatabaseError(UniqueViolation, _)) => {
                Status::already_exists("Already exists.")
            }
            FindById(_, NotFound) | FindByIds(_, NotFound) | FindByNodeType(_, _, NotFound) => {
                Status::not_found("Not found.")
            }
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Copy, Debug, Display, Hash, PartialEq, Eq, DieselNewType, Deref, From, FromStr)]
pub struct BlockchainNodeTypeId(Uuid);

#[derive(Clone, Debug, Queryable)]
#[diesel(table_name = blockchain_node_types)]
pub struct BlockchainNodeType {
    pub id: BlockchainNodeTypeId,
    pub blockchain_id: BlockchainId,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub node_type: NodeType,
}

impl BlockchainNodeType {
    pub async fn by_blockchain_id(
        blockchain_id: BlockchainId,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        blockchain_node_types::table
            .filter(blockchain_node_types::blockchain_id.eq(blockchain_id))
            .get_results(conn)
            .await
            .map_err(|err| Error::FindById(blockchain_id, err))
    }

    pub async fn by_blockchain_ids(
        blockchain_ids: HashSet<BlockchainId>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        blockchain_node_types::table
            .filter(blockchain_node_types::blockchain_id.eq_any(blockchain_ids.iter()))
            .get_results(conn)
            .await
            .map_err(|err| Error::FindByIds(blockchain_ids, err))
    }

    pub async fn by_node_type(
        blockchain_id: BlockchainId,
        node_type: NodeType,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        blockchain_node_types::table
            .filter(blockchain_node_types::blockchain_id.eq(blockchain_id))
            .filter(blockchain_node_types::node_type.eq(node_type))
            .get_result(conn)
            .await
            .map_err(|err| Error::FindByNodeType(blockchain_id, node_type, err))
    }

    pub async fn node_types(
        blockchain_id: BlockchainId,
        conn: &mut Conn<'_>,
    ) -> Result<HashSet<NodeType>, Error> {
        let rows = Self::by_blockchain_id(blockchain_id, conn).await?;
        Ok(rows.into_iter().map(|row| row.node_type).collect())
    }
}

#[derive(Clone, Debug, Insertable)]
#[diesel(table_name = blockchain_node_types)]
pub struct NewBlockchainNodeType {
    pub blockchain_id: BlockchainId,
    pub node_type: NodeType,
    pub description: Option<String>,
}

impl NewBlockchainNodeType {
    pub const fn new(
        blockchain_id: BlockchainId,
        node_type: NodeType,
        description: Option<String>,
    ) -> Self {
        NewBlockchainNodeType {
            blockchain_id,
            node_type,
            description,
        }
    }

    pub async fn create(self, conn: &mut Conn<'_>) -> Result<BlockchainNodeType, Error> {
        diesel::insert_into(blockchain_node_types::table)
            .values(self)
            .get_result(conn)
            .await
            .map_err(Error::Create)
    }
}
