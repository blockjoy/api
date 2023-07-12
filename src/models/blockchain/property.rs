use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use std::collections::HashMap;

use crate::database::Conn;
use crate::models::schema::blockchain_properties;
use crate::models::{NodeProperty, NodeType};

#[derive(Debug, Clone, Insertable, Queryable)]
#[diesel(table_name = blockchain_properties)]
pub struct BlockchainProperty {
    pub id: uuid::Uuid,
    pub blockchain_id: uuid::Uuid,
    pub version: String,
    pub node_type: NodeType,
    pub name: String,
    pub default: Option<String>,
    pub ui_type: BlockchainPropertyUiType,
    pub disabled: bool,
    pub required: bool,
}

impl BlockchainProperty {
    pub async fn bulk_create(props: Vec<Self>, conn: &mut Conn<'_>) -> crate::Result<Vec<Self>> {
        let props = diesel::insert_into(blockchain_properties::table)
            .values(props)
            .get_results(conn)
            .await?;
        Ok(props)
    }

    pub async fn by_blockchain(
        blockchain: &super::Blockchain,
        conn: &mut Conn<'_>,
    ) -> crate::Result<Vec<Self>> {
        let props = blockchain_properties::table
            .filter(blockchain_properties::blockchain_id.eq(blockchain.id))
            .get_results(conn)
            .await?;
        Ok(props)
    }

    pub async fn by_blockchains(
        blockchains: &[super::Blockchain],
        conn: &mut Conn<'_>,
    ) -> crate::Result<Vec<Self>> {
        let ids: Vec<_> = blockchains.iter().map(|b| b.id).collect();
        let props = blockchain_properties::table
            .filter(blockchain_properties::blockchain_id.eq_any(ids))
            .get_results(conn)
            .await?;
        Ok(props)
    }

    pub async fn by_blockchain_node_type(
        blockchain: &super::Blockchain,
        node_type: NodeType,
        conn: &mut Conn<'_>,
    ) -> crate::Result<Vec<Self>> {
        let props = blockchain_properties::table
            .filter(blockchain_properties::blockchain_id.eq(blockchain.id))
            .filter(blockchain_properties::node_type.eq(node_type))
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
        blockchain: &super::Blockchain,
        node_type: NodeType,
        version: &str,
        conn: &mut Conn<'_>,
    ) -> crate::Result<HashMap<uuid::Uuid, String>> {
        let props: Vec<Self> = blockchain_properties::table
            .filter(blockchain_properties::blockchain_id.eq(blockchain.id))
            .filter(blockchain_properties::node_type.eq(node_type))
            .filter(blockchain_properties::version.eq(version))
            .get_results(conn)
            .await?;
        props.into_iter().map(|b| Ok((b.id, b.name))).collect()
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
