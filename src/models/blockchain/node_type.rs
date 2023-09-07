use diesel::prelude::*;
use diesel_async::RunQueryDsl;

use crate::database::Conn;
use crate::models::schema::blockchain_node_types;
use crate::models::NodeType;

#[derive(Debug, Clone, Insertable, Queryable)]
#[diesel(table_name = blockchain_node_types)]
pub struct BlockchainNodeType {
    pub id: uuid::Uuid,
    pub blockchain_id: super::BlockchainId,
    pub node_type: NodeType,
    pub description: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl BlockchainNodeType {
    pub async fn bulk_create(props: Vec<Self>, conn: &mut Conn<'_>) -> crate::Result<Vec<Self>> {
        let props = diesel::insert_into(blockchain_node_types::table)
            .values(props)
            .get_results(conn)
            .await?;
        Ok(props)
    }

    pub async fn by_blockchains(
        blockchains: &[super::Blockchain],
        conn: &mut Conn<'_>,
    ) -> crate::Result<Vec<Self>> {
        let blockchain_ids: Vec<_> = blockchains.iter().map(|b| b.id).collect();
        let versions = blockchain_node_types::table
            .filter(blockchain_node_types::blockchain_id.eq_any(blockchain_ids))
            .get_results(conn)
            .await?;
        Ok(versions)
    }

    pub async fn by_blockchain(
        blockchain: &super::Blockchain,
        conn: &mut Conn<'_>,
    ) -> crate::Result<Vec<Self>> {
        let versions = blockchain_node_types::table
            .filter(blockchain_node_types::blockchain_id.eq(blockchain.id))
            .get_results(conn)
            .await?;
        Ok(versions)
    }
}
