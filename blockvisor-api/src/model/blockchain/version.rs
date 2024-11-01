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
use uuid::Uuid;

use crate::auth::AuthZ;
use crate::database::Conn;
use crate::grpc::Status;
use crate::model::node::{NodeType, NodeVersion};
use crate::model::schema::{blockchain_node_types, blockchain_versions};

use super::{BlockchainId, BlockchainNodeType, BlockchainNodeTypeId};

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Failed to create blockchain version: {0}
    Create(diesel::result::Error),
    /// Failed to find blockchain versions by id `{0}`: {1}
    FindById(BlockchainId, diesel::result::Error),
    /// Failed to find blockchain versions by ids `{0:?}`: {1}
    FindByIds(HashSet<BlockchainId>, diesel::result::Error),
    /// Failed to find blockchain version by node type `{0}` and version string `{1}`: {2}
    FindByNodeTypeVersion(BlockchainNodeTypeId, String, diesel::result::Error),
    /// Failed to find blockchain id `{0}`, node_type `{1}`, version `{2}`: {3}
    FindVersion(BlockchainId, NodeType, NodeVersion, diesel::result::Error),
    /// Blockchain version node type: {0}
    NodeType(#[from] super::node_type::Error),
    /// The requested new version already exists.
    VersionExists,
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            Create(DatabaseError(UniqueViolation, _)) | VersionExists => {
                Status::already_exists("Already exists.")
            }
            FindVersion(_, _, _, NotFound) | FindById(_, NotFound) | FindByIds(_, NotFound) => {
                Status::not_found("Not found.")
            }
            NodeType(err) => err.into(),
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
        blockchain_id: BlockchainId,
        node_type: NodeType,
        version: &NodeVersion,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        blockchain_versions::table
            .inner_join(blockchain_node_types::table)
            .filter(blockchain_versions::blockchain_id.eq(blockchain_id))
            .filter(blockchain_versions::version.eq(version))
            .filter(blockchain_node_types::node_type.eq(node_type))
            .select(BlockchainVersion::as_select())
            .get_result(conn)
            .await
            .map_err(|err| Error::FindVersion(blockchain_id, node_type, version.clone(), err))
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

    pub async fn by_node_type_version(
        node_type_id: BlockchainNodeTypeId,
        version: &str,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        blockchain_versions::table
            .filter(blockchain_versions::blockchain_node_type_id.eq(node_type_id))
            .filter(blockchain_versions::version.eq(version))
            .get_result(conn)
            .await
            .map_err(|err| Error::FindByNodeTypeVersion(node_type_id, version.to_string(), err))
    }
}

#[derive(Clone, Debug, Insertable, Queryable)]
#[diesel(table_name = blockchain_versions)]
pub struct NewVersion {
    pub blockchain_id: BlockchainId,
    pub blockchain_node_type_id: BlockchainNodeTypeId,
    pub version: String,
    pub description: Option<String>,
}

impl NewVersion {
    pub async fn new(
        blockchain_id: BlockchainId,
        node_type: NodeType,
        version: &NodeVersion,
        description: Option<String>,
        authz: &AuthZ,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        match BlockchainVersion::find(blockchain_id, node_type, version, conn).await {
            Ok(_) => Err(Error::VersionExists),
            Err(Error::FindVersion(_, _, _, NotFound)) => Ok(()),
            Err(err) => Err(err),
        }?;

        let blockchain_node_type =
            BlockchainNodeType::by_node_type(blockchain_id, node_type, authz, conn).await?;
        let blockchain_node_type_id = blockchain_node_type.id;

        Ok(NewVersion {
            blockchain_id,
            blockchain_node_type_id,
            version: version.to_string(),
            description,
        })
    }

    pub async fn create(self, conn: &mut Conn<'_>) -> Result<BlockchainVersion, Error> {
        diesel::insert_into(blockchain_versions::table)
            .values(self)
            .get_result(conn)
            .await
            .map_err(Error::Create)
    }
}
