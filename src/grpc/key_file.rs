use std::str;

use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::error;

use crate::auth::rbac::KeyFilePerm;
use crate::auth::resource::NodeId;
use crate::auth::Authorize;
use crate::database::{ReadConn, Transaction, WriteConn};
use crate::models::node::key_file::{NewNodeKeyFile, NodeKeyFile};
use crate::models::Node;

use super::api::key_file_service_server::KeyFileService;
use super::{api, Grpc};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Key file content is not valid UTF8: {0}
    Content(std::str::Utf8Error),
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Key file model error: {0}
    Model(#[from] crate::models::node::key_file::Error),
    /// Failed to parse NodeId: {0}
    ParseNodeId(uuid::Error),
    /// Key File node error: {0}
    Node(#[from] crate::models::node::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        error!("{err}");
        use Error::*;
        match err {
            Diesel(_) => Status::internal("Internal error."),
            Content(_) => Status::invalid_argument("key_file.content"),
            ParseNodeId(_) => Status::invalid_argument("node_id"),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
            Model(err) => err.into(),
            Node(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl KeyFileService for Grpc {
    async fn create(
        &self,
        req: Request<api::KeyFileServiceCreateRequest>,
    ) -> Result<Response<api::KeyFileServiceCreateResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| create(req, meta, write).scope_boxed())
            .await
    }

    async fn list(
        &self,
        req: Request<api::KeyFileServiceListRequest>,
    ) -> Result<Response<api::KeyFileServiceListResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list(req, meta, read).scope_boxed()).await
    }
}

async fn create(
    req: api::KeyFileServiceCreateRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::KeyFileServiceCreateResponse, Error> {
    let node_id: NodeId = req.node_id.parse().map_err(Error::ParseNodeId)?;
    let _ = Node::find_by_id(node_id, &mut write).await?;

    let _ = write.auth(&meta, KeyFilePerm::Create, node_id).await?;

    let key_files = req
        .key_files
        .iter()
        .map(|key_file| {
            Ok(NewNodeKeyFile {
                name: &key_file.name,
                content: str::from_utf8(&key_file.content).map_err(Error::Content)?,
                node_id,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;

    NewNodeKeyFile::bulk_create(key_files, &mut write).await?;

    Ok(api::KeyFileServiceCreateResponse {})
}

async fn list(
    req: api::KeyFileServiceListRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::KeyFileServiceListResponse, Error> {
    let node_id: NodeId = req.node_id.parse().map_err(Error::ParseNodeId)?;
    let _ = Node::find_by_id(node_id, &mut read).await?;

    let _ = read.auth(&meta, KeyFilePerm::List, node_id).await?;

    let key_files = NodeKeyFile::find_by_id(node_id, &mut read)
        .await
        .map(api::Keyfile::from_models)?;

    Ok(api::KeyFileServiceListResponse { key_files })
}

impl api::Keyfile {
    fn from_models(models: Vec<NodeKeyFile>) -> Vec<Self> {
        models
            .into_iter()
            .map(|key_file| Self {
                name: key_file.name,
                content: key_file.content.into_bytes(),
            })
            .collect()
    }
}
