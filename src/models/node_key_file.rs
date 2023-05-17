use super::schema::node_key_files;
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
        node: &super::Node,
        conn: &mut AsyncPgConnection,
    ) -> Result<Vec<Self>> {
        let files = node_key_files::table
            .filter(node_key_files::node_id.eq(node.id))
            .get_results(conn)
            .await?;
        Ok(files)
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
