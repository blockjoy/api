use crate::models::{self, schema::blockchain_properties, string_to_array};
use diesel::{dsl, prelude::*};
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use std::collections::HashMap;

#[derive(Debug, Clone, Insertable, Queryable)]
#[diesel(table_name = blockchain_properties)]
pub struct BlockchainProperty {
    pub id: uuid::Uuid,
    pub blockchain_id: uuid::Uuid,
    pub version: String,
    pub node_type: models::NodeType,
    pub name: String,
    pub default: Option<String>,
    pub ui_type: BlockchainPropertyUiType,
    pub disabled: bool,
    pub required: bool,
}

impl BlockchainProperty {
    pub async fn bulk_create(
        props: Vec<Self>,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<Vec<Self>> {
        let props = diesel::insert_into(blockchain_properties::table)
            .values(props)
            .get_results(conn)
            .await?;
        Ok(props)
    }

    pub async fn by_blockchain(
        blockchain: &super::Blockchain,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<Vec<Self>> {
        let props = blockchain_properties::table
            .filter(blockchain_properties::blockchain_id.eq(blockchain.id))
            .get_results(conn)
            .await?;
        Ok(props)
    }

    pub async fn by_blockchains(
        blockchains: &[super::Blockchain],
        conn: &mut AsyncPgConnection,
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
        node_type: models::NodeType,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<Vec<Self>> {
        let props = blockchain_properties::table
            .filter(blockchain_properties::blockchain_id.eq(blockchain.id))
            .filter(blockchain_properties::node_type.eq(node_type))
            .get_results(conn)
            .await?;
        Ok(props)
    }

    /// Returns the same values as `by_blockchain_node_type`, but takes only the values for the most
    /// recent version.
    pub async fn by_blockchain_node_type_recent(
        blockchain: &super::Blockchain,
        node_type: models::NodeType,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<Vec<Self>> {
        let max_version = dsl::max(string_to_array(blockchain_properties::version, "."));
        let max_version: Option<Vec<String>> = blockchain_properties::table
            .filter(blockchain_properties::blockchain_id.eq(blockchain.id))
            .filter(blockchain_properties::node_type.eq(node_type))
            .select(max_version)
            .get_result(conn)
            .await?;
        let current_version = string_to_array(blockchain_properties::version, ".");
        let chains = blockchain_properties::table
            .filter(blockchain_properties::blockchain_id.eq(blockchain.id))
            .filter(blockchain_properties::node_type.eq(node_type))
            .filter(current_version.eq(max_version.as_deref().unwrap_or(&[])))
            .get_results(conn)
            .await?;
        Ok(chains)
    }

    /// Returns a map from blockchain_property_id to the `name` field of that blockchain property.
    pub async fn by_node_props(
        nprops: &[models::NodeProperty],
        conn: &mut AsyncPgConnection,
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
        node_type: models::NodeType,
        version: &str,
        conn: &mut AsyncPgConnection,
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
