use super::schema::node_key_files;
use crate::auth::FindableById;
use crate::Result;
use diesel::prelude::*;
use diesel_async::{AsyncPgConnection, RunQueryDsl};

#[derive(Debug, Queryable)]
pub struct NodeKeyFile {
    pub id: uuid::Uuid,
    pub name: String,
    pub content: String,
    pub node_id: uuid::Uuid,
}

impl NodeKeyFile {
    pub async fn find_by_node(
        node_id: uuid::Uuid,
        conn: &mut AsyncPgConnection,
    ) -> Result<Vec<Self>> {
        let files = node_key_files::table
            .filter(node_key_files::node_id.eq(node_id))
            .get_results(conn)
            .await?;
        Ok(files)
    }

    pub async fn delete(node_id: uuid::Uuid, conn: &mut AsyncPgConnection) -> Result<()> {
        diesel::delete(node_key_files::table.find(node_id))
            .execute(conn)
            .await?;
        Ok(())
    }
}

#[tonic::async_trait]
impl FindableById for NodeKeyFile {
    async fn find_by_id(id: uuid::Uuid, conn: &mut AsyncPgConnection) -> Result<Self> {
        let file = node_key_files::table.find(id).get_result(conn).await?;
        Ok(file)
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = node_key_files)]
pub struct NewNodeKeyFile<'a> {
    pub name: &'a str,
    pub content: &'a str,
    pub node_id: uuid::Uuid,
}

impl NewNodeKeyFile<'_> {
    pub async fn create(self, conn: &mut AsyncPgConnection) -> Result<NodeKeyFile> {
        let file = diesel::insert_into(node_key_files::table)
            .values(self)
            .get_result(conn)
            .await?;
        Ok(file)
    }

    pub async fn bulk_create(
        key_files: Vec<Self>,
        conn: &mut AsyncPgConnection,
    ) -> Result<Vec<NodeKeyFile>> {
        let files = diesel::insert_into(node_key_files::table)
            .values(key_files)
            .get_results(conn)
            .await?;
        Ok(files)
    }
}
