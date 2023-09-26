pub mod bundle;
pub mod kernel;
pub mod manifest;

use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::error;

use crate::auth::rbac::CookbookPerm;
use crate::auth::Authorize;
use crate::cookbook;
use crate::database::{ReadConn, Transaction};
use crate::grpc::api::cookbook_service_server::CookbookService;
use crate::grpc::{api, Grpc};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Cookbook failed: {0}
    Cookbook(#[from] crate::cookbook::Error),
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Missing cookbook id.
    MissingId,
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        error!("{err}");
        use Error::*;
        match err {
            Cookbook(_) | Diesel(_) => Status::internal("Internal error."),
            MissingId => Status::invalid_argument("id"),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl CookbookService for Grpc {
    // Retrieve plugin for specific version and state.
    async fn retrieve_plugin(
        &self,
        req: Request<api::CookbookServiceRetrievePluginRequest>,
    ) -> Result<Response<api::CookbookServiceRetrievePluginResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| retrieve_plugin(req, meta, read).scope_boxed())
            .await
    }

    // Retrieve image for specific version and state.
    async fn retrieve_image(
        &self,
        req: Request<api::CookbookServiceRetrieveImageRequest>,
    ) -> Result<Response<api::CookbookServiceRetrieveImageResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| retrieve_image(req, meta, read).scope_boxed())
            .await
    }

    // Retrieve hardware requirements for given identifier.
    async fn requirements(
        &self,
        req: Request<api::CookbookServiceRequirementsRequest>,
    ) -> Result<Response<api::CookbookServiceRequirementsResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| requirements(req, meta, read).scope_boxed())
            .await
    }

    // Retrieve net configurations for given chain.
    async fn net_configurations(
        &self,
        req: Request<api::CookbookServiceNetConfigurationsRequest>,
    ) -> Result<Response<api::CookbookServiceNetConfigurationsResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| net_configurations(req, meta, read).scope_boxed())
            .await
    }

    // List all available babel versions.
    async fn list_babel_versions(
        &self,
        req: Request<api::CookbookServiceListBabelVersionsRequest>,
    ) -> Result<Response<api::CookbookServiceListBabelVersionsResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list_babel_versions(req, meta, read).scope_boxed())
            .await
    }
}

async fn retrieve_plugin(
    req: api::CookbookServiceRetrievePluginRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::CookbookServiceRetrievePluginResponse, Error> {
    let _ = read.auth_all(&meta, CookbookPerm::RetrievePlugin).await?;

    let id = req.id.ok_or(Error::MissingId)?;
    let rhai_content = read
        .ctx
        .cookbook
        .read_file(
            &id.protocol,
            id.node_type().into_model(),
            &id.node_version,
            cookbook::RHAI_FILE_NAME,
        )
        .await?;
    let plugin = api::Plugin {
        identifier: Some(id),
        rhai_content,
    };

    Ok(api::CookbookServiceRetrievePluginResponse {
        plugin: Some(plugin),
    })
}

async fn retrieve_image(
    req: api::CookbookServiceRetrieveImageRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::CookbookServiceRetrieveImageResponse, Error> {
    let _ = read.auth_all(&meta, CookbookPerm::RetrieveImage).await?;

    let id = req.id.ok_or(Error::MissingId)?;
    let url = read
        .ctx
        .cookbook
        .download_url(
            &id.protocol,
            id.node_type().into_model(),
            &id.node_version,
            cookbook::BABEL_IMAGE_NAME,
        )
        .await?;
    let location = api::ArchiveLocation { url };

    Ok(api::CookbookServiceRetrieveImageResponse {
        location: Some(location),
    })
}

async fn requirements(
    req: api::CookbookServiceRequirementsRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::CookbookServiceRequirementsResponse, Error> {
    let _ = read.auth_all(&meta, CookbookPerm::Requirements).await?;

    let id = req.id.ok_or(Error::MissingId)?;
    let node_type = id.node_type().into_model();
    let requirements = read
        .ctx
        .cookbook
        .rhai_metadata(&id.protocol, node_type, &id.node_version)
        .await?
        .requirements;

    Ok(api::CookbookServiceRequirementsResponse {
        cpu_count: requirements.vcpu_count,
        mem_size_bytes: requirements.mem_size_mb * 1000 * 1000,
        disk_size_bytes: requirements.disk_size_gb * 1000 * 1000 * 1000,
    })
}

async fn net_configurations(
    req: api::CookbookServiceNetConfigurationsRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::CookbookServiceNetConfigurationsResponse, Error> {
    let _ = read
        .auth_all(&meta, CookbookPerm::NetConfigurations)
        .await?;

    let id = req.id.ok_or(Error::MissingId)?;
    let node_type = id.node_type().into_model();
    let networks = read
        .ctx
        .cookbook
        .rhai_metadata(&id.protocol, node_type, &id.node_version)
        .await?
        .nets
        .into_iter()
        .map(|(name, network)| {
            let mut net = api::NetworkConfiguration {
                name,
                url: network.url,
                net_type: 0, // we use a setter
                meta: network.meta,
            };
            net.set_net_type(network.net_type.into());
            net
        })
        .collect();

    Ok(api::CookbookServiceNetConfigurationsResponse { networks })
}

async fn list_babel_versions(
    req: api::CookbookServiceListBabelVersionsRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::CookbookServiceListBabelVersionsResponse, Error> {
    let _ = read
        .auth_all(&meta, CookbookPerm::ListBabelVersions)
        .await?;

    let node_type = req.node_type().into_model();
    let identifiers = read.ctx.cookbook.list(&req.protocol, node_type).await?;

    Ok(api::CookbookServiceListBabelVersionsResponse { identifiers })
}
