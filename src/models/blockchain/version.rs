use diesel::prelude::*;
use diesel_async::RunQueryDsl;

use crate::database::Conn;
use crate::error::QueryError;
use crate::models;
use crate::models::schema::blockchain_versions;

#[derive(Debug, Clone, Insertable, Queryable, Selectable)]
#[diesel(table_name = blockchain_versions)]
pub struct BlockchainVersion {
    pub id: uuid::Uuid,
    pub blockchain_id: super::BlockchainId,
    pub blockchain_node_type_id: uuid::Uuid,
    pub version: String,
    pub description: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl BlockchainVersion {
    pub async fn find(
        blockchain: &super::Blockchain,
        version: &str,
        node_type: models::NodeType,
        conn: &mut Conn<'_>,
    ) -> crate::Result<Self> {
        use crate::models::schema::blockchain_node_types;
        let id = format!("{}/{version}/{node_type}", blockchain.name);
        blockchain_versions::table
            .inner_join(blockchain_node_types::table)
            .filter(blockchain_versions::blockchain_id.eq(blockchain.id))
            .filter(blockchain_versions::version.eq(version.to_lowercase()))
            .filter(blockchain_node_types::node_type.eq(node_type))
            .select(BlockchainVersion::as_select())
            .get_result(conn)
            .await
            .for_table_id("blockchain_versions", id)
    }

    pub async fn by_blockchains(
        blockchains: &[super::Blockchain],
        conn: &mut Conn<'_>,
    ) -> crate::Result<Vec<Self>> {
        let mut blockchain_ids: Vec<_> = blockchains.iter().map(|b| b.id).collect();
        blockchain_ids.sort();
        blockchain_ids.dedup();
        let versions = blockchain_versions::table
            .filter(blockchain_versions::blockchain_id.eq_any(blockchain_ids))
            .get_results(conn)
            .await?;
        Ok(versions)
    }

    pub async fn by_blockchain(
        blockchain: &super::Blockchain,
        conn: &mut Conn<'_>,
    ) -> crate::Result<Vec<Self>> {
        let versions = blockchain_versions::table
            .filter(blockchain_versions::blockchain_id.eq(blockchain.id))
            .get_results(conn)
            .await?;
        Ok(versions)
    }
}
