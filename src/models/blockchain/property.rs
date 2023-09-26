use derive_more::{Deref, Display, From, FromStr};
use diesel::prelude::*;
use diesel::result::Error::NotFound;
use diesel_async::RunQueryDsl;
use diesel_derive_enum::DbEnum;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display as DisplayDoc;
use std::collections::{HashMap, HashSet};
use thiserror::Error;
use tonic::Status;
use uuid::Uuid;

use crate::database::Conn;
use crate::models::schema::{blockchain_properties, sql_types};

use super::{BlockchainId, BlockchainNodeTypeId, BlockchainVersion, BlockchainVersionId};

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Failed to bulk create blockchain properties: {0}
    BulkCreate(diesel::result::Error),
    /// Failed to find blockchain property by blockchain ids `{0:?}`: {1}
    ByBlockchainIds(HashSet<BlockchainId>, diesel::result::Error),
    /// Failed to find blockchain property by property ids `{0:?}`: {1}
    ByPropertyIds(HashSet<BlockchainPropertyId>, diesel::result::Error),
    /// Failed to find blockchain property for version id `{0}`: {1}
    ByVersionId(BlockchainVersionId, diesel::result::Error),
    /// Failed to find blockchain property by version ids `{0:?}`: {1}
    ByVersionIds(HashSet<BlockchainVersionId>, diesel::result::Error),
    /// Failed to create map from blockchain property id to name: {0}
    IdToName(diesel::result::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            ByBlockchainIds(_, NotFound)
            | ByPropertyIds(_, NotFound)
            | ByVersionId(_, NotFound)
            | ByVersionIds(_, NotFound) => Status::not_found("Not found."),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Copy, Debug, Display, Hash, PartialEq, Eq, DieselNewType, Deref, From, FromStr)]
pub struct BlockchainPropertyId(Uuid);

#[derive(Debug, Clone, Insertable, Queryable)]
#[diesel(table_name = blockchain_properties)]
pub struct BlockchainProperty {
    pub id: BlockchainPropertyId,
    pub blockchain_id: BlockchainId,
    pub name: String,
    pub default: Option<String>,
    pub ui_type: BlockchainPropertyUiType,
    pub disabled: bool,
    pub required: bool,
    pub blockchain_node_type_id: BlockchainNodeTypeId,
    pub blockchain_version_id: BlockchainVersionId,
    pub display_name: String,
}

impl BlockchainProperty {
    pub async fn bulk_create(
        properties: Vec<Self>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        diesel::insert_into(blockchain_properties::table)
            .values(properties)
            .get_results(conn)
            .await
            .map_err(Error::BulkCreate)
    }

    pub async fn by_blockchain_ids(
        blockchain_ids: HashSet<BlockchainId>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        blockchain_properties::table
            .filter(blockchain_properties::blockchain_id.eq_any(blockchain_ids.iter()))
            .get_results(conn)
            .await
            .map_err(|err| Error::ByBlockchainIds(blockchain_ids, err))
    }

    pub async fn by_property_ids(
        property_ids: HashSet<BlockchainPropertyId>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        blockchain_properties::table
            .filter(blockchain_properties::id.eq_any(property_ids.iter()))
            .get_results(conn)
            .await
            .map_err(|err| Error::ByPropertyIds(property_ids, err))
    }

    pub async fn by_version_id(
        version_id: BlockchainVersionId,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        blockchain_properties::table
            .filter(blockchain_properties::blockchain_version_id.eq(version_id))
            .get_results(conn)
            .await
            .map_err(|err| Error::ByVersionId(version_id, err))
    }

    pub async fn by_version_ids(
        version_ids: HashSet<BlockchainVersionId>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        blockchain_properties::table
            .filter(blockchain_properties::blockchain_version_id.eq_any(version_ids.iter()))
            .get_results(conn)
            .await
            .map_err(|err| Error::ByVersionIds(version_ids, err))
    }

    /// Returns a map from `BlockchainPropertyId` to the `name` field of that blockchain property.
    pub async fn id_to_name_map(
        version: &BlockchainVersion,
        conn: &mut Conn<'_>,
    ) -> Result<HashMap<BlockchainPropertyId, String>, Error> {
        let props: Vec<Self> = blockchain_properties::table
            .filter(blockchain_properties::blockchain_version_id.eq(version.id))
            .get_results(conn)
            .await
            .map_err(Error::IdToName)?;

        Ok(props.into_iter().map(|b| (b.id, b.name)).collect())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::BlockchainPropertyUiType"]
pub enum BlockchainPropertyUiType {
    Switch,
    Password,
    Text,
    FileUpload,
}
