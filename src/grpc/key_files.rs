use super::api::{self, key_files_server};
use crate::auth::FindableById;
use crate::models;
use diesel_async::scoped_futures::ScopedFutureExt;
use tonic::{Request, Response, Status};

#[tonic::async_trait]
impl key_files_server::KeyFiles for super::GrpcImpl {
    async fn get(
        &self,
        request: Request<api::KeyFilesGetRequest>,
    ) -> super::Result<api::KeyFilesGetResponse> {
        let inner = request.into_inner();
        let node_id = inner.node_id.parse().map_err(crate::Error::from)?;
        let mut conn = self.conn().await?;
        let key_files = models::NodeKeyFile::find_by_node(node_id, &mut conn).await?;

        // Ensure we return "Not found" if no key files could be found
        if key_files.is_empty() {
            tracing::debug!("No key files found");
        }

        let key_files = api::Keyfile::from_models(key_files);
        let response = api::KeyFilesGetResponse { key_files };

        Ok(Response::new(response))
    }

    async fn save(
        &self,
        request: Request<api::KeyFilesSaveRequest>,
    ) -> super::Result<api::KeyFilesSaveResponse> {
        let inner = request.into_inner();
        let node_id = inner.node_id.parse().map_err(crate::Error::from)?;

        self.trx(|c| {
            async move {
                // Explicitly check, if node exists
                models::Node::find_by_id(node_id, c).await?;

                for file in inner.key_files {
                    let new_node_key_file = models::NewNodeKeyFile {
                        name: &file.name,
                        content: std::str::from_utf8(&file.content).map_err(|e| {
                            Status::invalid_argument(format!(
                                "Couldn't read key file contents: {e}"
                            ))
                        })?,
                        node_id,
                    };

                    new_node_key_file.create(c).await?;
                }
                Ok(())
            }
            .scope_boxed()
        })
        .await?;

        let response = api::KeyFilesSaveResponse {};

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
