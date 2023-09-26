use diesel::prelude::*;
use diesel::result::DatabaseErrorKind::UniqueViolation;
use diesel::result::Error::{DatabaseError, NotFound};
use diesel_async::RunQueryDsl;
use displaydoc::Display;
use thiserror::Error;
use tonic::Status;

use crate::auth::resource::NodeId;
use crate::database::Conn;
use crate::models::schema::node_key_files;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to bulk create node key files: {0}
    BulkCreate(diesel::result::Error),
    /// Failed to find node key file for node id `{0}`: {1}
    FindById(NodeId, diesel::result::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            BulkCreate(DatabaseError(UniqueViolation, _)) => {
                Status::already_exists("Already exists.")
            }
            FindById(_, NotFound) => Status::not_found("Not found."),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Debug, Queryable)]
pub struct NodeKeyFile {
    pub id: uuid::Uuid,
    pub name: String,
    pub content: String,
    pub node_id: NodeId,
}

impl NodeKeyFile {
    pub async fn find_by_id(node_id: NodeId, conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
        node_key_files::table
            .filter(node_key_files::node_id.eq(node_id))
            .get_results(conn)
            .await
            .map_err(|err| Error::FindById(node_id, err))
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = node_key_files)]
pub struct NewNodeKeyFile<'a> {
    pub name: &'a str,
    pub content: &'a str,
    pub node_id: NodeId,
}

impl NewNodeKeyFile<'_> {
    pub async fn bulk_create(
        key_files: Vec<Self>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<NodeKeyFile>, Error> {
        diesel::insert_into(node_key_files::table)
            .values(key_files)
            .get_results(conn)
            .await
            .map_err(Error::BulkCreate)
    }
}
