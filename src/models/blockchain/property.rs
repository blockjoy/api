use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use std::collections::HashMap;

use crate::database::Conn;
use crate::models::schema::blockchain_properties;
use crate::models::NodeProperty;

#[derive(Debug, Clone, Insertable, Queryable)]
#[diesel(table_name = blockchain_properties)]
pub struct BlockchainProperty {
    pub id: uuid::Uuid,
    pub blockchain_id: super::BlockchainId,
    pub name: String,
    pub default: Option<String>,
    pub ui_type: BlockchainPropertyUiType,
    pub disabled: bool,
    pub required: bool,
    pub blockchain_node_type_id: uuid::Uuid,
    pub blockchain_version_id: uuid::Uuid,
    pub display_name: String,
}

impl BlockchainProperty {
    pub async fn bulk_create(props: Vec<Self>, conn: &mut Conn<'_>) -> crate::Result<Vec<Self>> {
        let props = diesel::insert_into(blockchain_properties::table)
            .values(props)
            .get_results(conn)
            .await?;
        Ok(props)
    }

    pub async fn by_blockchain_version(
        version: &super::BlockchainVersion,
        conn: &mut Conn<'_>,
    ) -> crate::Result<Vec<Self>> {
        let props = blockchain_properties::table
            .filter(blockchain_properties::blockchain_version_id.eq(version.id))
            .get_results(conn)
            .await?;
        Ok(props)
    }

    pub async fn by_blockchain_versions(
        versions: &[super::BlockchainVersion],
        conn: &mut Conn<'_>,
    ) -> crate::Result<Vec<Self>> {
        let mut ids: Vec<_> = versions.iter().map(|b| b.id).collect();
        ids.sort();
        ids.dedup();
        let props = blockchain_properties::table
            .filter(blockchain_properties::blockchain_version_id.eq_any(ids))
            .get_results(conn)
            .await?;
        Ok(props)
    }

    /// Returns a map from blockchain_property_id to the `name` field of that blockchain property.
    pub async fn by_node_props(
        nprops: &[NodeProperty],
        conn: &mut Conn<'_>,
    ) -> crate::Result<Vec<Self>> {
        let ids: Vec<_> = nprops
            .iter()
            .map(|nprop| nprop.blockchain_property_id)
            .collect();
        let props = blockchain_properties::table
            .filter(blockchain_properties::id.eq_any(ids))
            .get_results(conn)
            .await?;
        Ok(props)
    }

    /// Returns a map from blockchain_property_id to the `name` field of that blockchain property.
    pub async fn id_to_name_map(
        version: &super::BlockchainVersion,
        conn: &mut Conn<'_>,
    ) -> crate::Result<HashMap<uuid::Uuid, String>> {
        let props: Vec<Self> = blockchain_properties::table
            .filter(blockchain_properties::blockchain_version_id.eq(version.id))
            .get_results(conn)
            .await?;
        props.into_iter().map(|b| Ok((b.id, b.name))).collect()
    }

    pub async fn by_blockchains(
        blockchains: &[super::Blockchain],
        conn: &mut Conn<'_>,
    ) -> crate::Result<Vec<Self>> {
        let mut blockchain_ids: Vec<_> = blockchains.iter().map(|b| b.id).collect();
        blockchain_ids.sort();
        blockchain_ids.dedup();
        let versions = blockchain_properties::table
            .filter(blockchain_properties::blockchain_id.eq_any(blockchain_ids))
            .get_results(conn)
            .await?;
        Ok(versions)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, diesel_derive_enum::DbEnum)]
#[ExistingTypePath = "crate::models::schema::sql_types::BlockchainPropertyUiType"]
pub enum BlockchainPropertyUiType {
    Switch,
    Password,
    Text,
    FileUpload,
}
