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
use crate::models::node::{NodeType, NodeVersion};
use crate::models::schema::{blockchain_node_types, blockchain_versions};

use super::{Blockchain, BlockchainId, BlockchainNodeTypeId};

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Failed to find blockchain version by id `{0}`: {1}
    FindById(BlockchainId, diesel::result::Error),
    /// Failed to find blockchain version by ids `{0:?}`: {1}
    FindByIds(HashSet<BlockchainId>, diesel::result::Error),
    /// Failed to find blockchain version `{0}`: {1}
    FindVersion(String, diesel::result::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            FindVersion(_, NotFound) | FindById(_, NotFound) | FindByIds(_, NotFound) => {
                Status::not_found("Not found.")
            }
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Copy, Debug, Display, Hash, PartialEq, Eq, DieselNewType, Deref, From, FromStr)]
pub struct BlockchainVersionId(Uuid);

#[derive(Clone, Debug, Insertable, Queryable, Selectable)]
#[diesel(table_name = blockchain_versions)]
pub struct BlockchainVersion {
    pub id: BlockchainVersionId,
    pub blockchain_id: BlockchainId,
    pub blockchain_node_type_id: BlockchainNodeTypeId,
    pub version: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl BlockchainVersion {
    pub async fn find(
        blockchain: &Blockchain,
        version: &NodeVersion,
        node_type: NodeType,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        let version = version.as_ref().to_lowercase();
        let id = format!("{}/{version}/{node_type}", blockchain.name);

        blockchain_versions::table
            .inner_join(blockchain_node_types::table)
            .filter(blockchain_versions::blockchain_id.eq(blockchain.id))
            .filter(blockchain_versions::version.eq(&version))
            .filter(blockchain_node_types::node_type.eq(node_type))
            .select(BlockchainVersion::as_select())
            .get_result(conn)
            .await
            .map_err(|err| Error::FindVersion(id, err))
    }

    pub async fn by_blockchain_id(
        blockchain_id: BlockchainId,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        blockchain_versions::table
            .filter(blockchain_versions::blockchain_id.eq(blockchain_id))
            .get_results(conn)
            .await
            .map_err(|err| Error::FindById(blockchain_id, err))
    }

    pub async fn by_blockchain_ids(
        blockchain_ids: HashSet<BlockchainId>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        blockchain_versions::table
            .filter(blockchain_versions::blockchain_id.eq_any(blockchain_ids.iter()))
            .get_results(conn)
            .await
            .map_err(|err| Error::FindByIds(blockchain_ids, err))
    }
}
