use std::collections::HashSet;

use chrono::{DateTime, Utc};
use derive_more::{Deref, Display, From, FromStr};
use diesel::prelude::*;
use diesel::result::Error::NotFound;
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
    /// Failed to find blockchain node type by id `{0}`: {1}
    FindById(BlockchainId, diesel::result::Error),
    /// Failed to find blockchain node types by ids `{0:?}`: {1}
    FindByIds(HashSet<BlockchainId>, diesel::result::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            FindById(_, NotFound) | FindByIds(_, NotFound) => Status::not_found("Not found."),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Copy, Debug, Display, Hash, PartialEq, Eq, DieselNewType, Deref, From, FromStr)]
pub struct BlockchainNodeTypeId(Uuid);

#[derive(Clone, Debug, Insertable, Queryable)]
#[diesel(table_name = blockchain_node_types)]
pub struct BlockchainNodeType {
    pub id: BlockchainNodeTypeId,
    pub blockchain_id: BlockchainId,
    pub node_type: NodeType,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl BlockchainNodeType {
    pub async fn bulk_create(props: Vec<Self>, conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
        diesel::insert_into(blockchain_node_types::table)
            .values(props)
            .get_results(conn)
            .await
            .map_err(Error::BulkCreate)
    }

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
}
