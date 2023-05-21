use super::api::{self, key_file_service_server};
use crate::auth::FindableById;
use crate::models;
use anyhow::Context;
use diesel_async::scoped_futures::ScopedFutureExt;
use tonic::{Request, Response};

#[tonic::async_trait]
impl key_file_service_server::KeyFileService for super::GrpcImpl {
    async fn create(
        &self,
        request: Request<api::KeyFileServiceCreateRequest>,
    ) -> super::Result<api::KeyFileServiceCreateResponse> {
        let request = request.into_inner();

        self.trx(|c| {
            async move {
                let node_id = request.node_id.parse()?;
                // Explicitly check, if node exists
                models::Node::find_by_id(node_id, c).await?;
                let key_files = request
                    .key_files
                    .iter()
                    .map(|key_file| {
                        Ok(models::NewNodeKeyFile {
                            name: &key_file.name,
                            content: std::str::from_utf8(&key_file.content)
                                .with_context(|| "File is not valid utf8")?,
                            node_id,
                        })
                    })
                    .collect::<crate::Result<_>>()?;
                models::NewNodeKeyFile::bulk_create(key_files, c).await
            }
            .scope_boxed()
        })
        .await?;
        let response = api::KeyFileServiceCreateResponse {};
        Ok(Response::new(response))
    }

    async fn list(
        &self,
        request: Request<api::KeyFileServiceListRequest>,
    ) -> super::Result<api::KeyFileServiceListResponse> {
        let inner = request.into_inner();
        let node_id = inner.node_id.parse().map_err(crate::Error::from)?;
        let mut conn = self.conn().await?;
        let key_files = models::NodeKeyFile::find_by_node(node_id, &mut conn).await?;

        // Ensure we return "Not found" if no key files could be found
        if key_files.is_empty() {
            tracing::debug!("No key files found");
        }

        let key_files = api::Keyfile::from_models(key_files);
        let response = api::KeyFileServiceListResponse { key_files };

        Ok(Response::new(response))
    }
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
