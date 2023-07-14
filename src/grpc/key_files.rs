use anyhow::Context;
use diesel_async::scoped_futures::ScopedFutureExt;
use tonic::{Request, Response};

use crate::auth::endpoint::Endpoint;
use crate::auth::resource::Resource;
use crate::models;

use super::api::{self, key_file_service_server};

#[tonic::async_trait]
impl key_file_service_server::KeyFileService for super::Grpc {
    async fn create(
        &self,
        req: Request<api::KeyFileServiceCreateRequest>,
    ) -> super::Resp<api::KeyFileServiceCreateResponse> {
        self.trx(|c| create(req, c).scope_boxed()).await
    }

    async fn list(
        &self,
        req: Request<api::KeyFileServiceListRequest>,
    ) -> super::Resp<api::KeyFileServiceListResponse> {
        self.run(|c| list(req, c).scope_boxed()).await
    }
}

async fn create(
    req: Request<api::KeyFileServiceCreateRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::KeyFileServiceCreateResponse> {
    let claims = conn.claims(&req, Endpoint::KeyFileCreate).await?;
    let req = req.into_inner();
    let node = models::Node::find_by_id(req.node_id.parse()?, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => models::Org::is_member(user_id, node.org_id, conn).await?,
        Resource::Org(org_id) => org_id == node.org_id,
        Resource::Host(host_id) => host_id == node.host_id,
        Resource::Node(node_id) => node_id == node.id,
    };
    if !is_allowed {
        super::forbidden!("Access denied for key files create");
    }
    let key_files = req
        .key_files
        .iter()
        .map(|key_file| {
            Ok(models::NewNodeKeyFile {
                name: &key_file.name,
                content: std::str::from_utf8(&key_file.content)
                    .with_context(|| "File is not valid utf8")?,
                node_id: node.id,
            })
        })
        .collect::<crate::Result<_>>()?;
    models::NewNodeKeyFile::bulk_create(key_files, conn).await?;
    let response = api::KeyFileServiceCreateResponse {};
    Ok(Response::new(response))
}

async fn list(
    req: Request<api::KeyFileServiceListRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::KeyFileServiceListResponse> {
    let claims = conn.claims(&req, Endpoint::KeyFileList).await?;
    let req = req.into_inner();
    let node = models::Node::find_by_id(req.node_id.parse()?, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => models::Org::is_member(user_id, node.org_id, conn).await?,
        Resource::Org(org_id) => org_id == node.org_id,
        Resource::Host(host_id) => host_id == node.host_id,
        Resource::Node(node_id) => node_id == node.id,
    };
    if !is_allowed {
        super::forbidden!("Access denied for key files list");
    }
    let key_files = models::NodeKeyFile::find_by_node(&node, conn).await?;
    let key_files = api::Keyfile::from_models(key_files);
    let response = api::KeyFileServiceListResponse { key_files };
    Ok(Response::new(response))
}

impl api::Keyfile {
    fn from_models(models: Vec<models::NodeKeyFile>) -> Vec<Self> {
        models
            .into_iter()
            .map(|key_file| Self {
                name: key_file.name,
                content: key_file.content.into_bytes(),
            })
            .collect()
    }
}
