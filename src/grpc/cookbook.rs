use diesel_async::scoped_futures::ScopedFutureExt;

use crate::auth::endpoint::Endpoint;
use crate::config::Context;
use crate::cookbook;
use crate::database::{Conn, Transaction};

use super::api::{self, bundle_service_server, cookbook_service_server, manifest_service_server};
use super::helpers::required;

// --------------------------------------- Cookbook Service ---------------------------------------

#[tonic::async_trait]
impl cookbook_service_server::CookbookService for super::Grpc {
    // Retrieve plugin for specific version and state.
    async fn retrieve_plugin(
        &self,
        req: tonic::Request<api::CookbookServiceRetrievePluginRequest>,
    ) -> super::Resp<api::CookbookServiceRetrievePluginResponse> {
        self.context
            .read(|conn, ctx| retrieve_plugin(req, conn, ctx).scope_boxed())
            .await
    }

    // Retrieve image for specific version and state.
    async fn retrieve_image(
        &self,
        req: tonic::Request<api::CookbookServiceRetrieveImageRequest>,
    ) -> super::Resp<api::CookbookServiceRetrieveImageResponse> {
        self.context
            .read(|conn, ctx| retrieve_image(req, conn, ctx).scope_boxed())
            .await
    }

    // Retrieve kernel file for specific version and state.
    async fn retrieve_kernel(
        &self,
        req: tonic::Request<api::CookbookServiceRetrieveKernelRequest>,
    ) -> super::Resp<api::CookbookServiceRetrieveKernelResponse> {
        self.context
            .read(|conn, ctx| retrieve_kernel(req, conn, ctx).scope_boxed())
            .await
    }

    // Retrieve hardware requirements for given identifier.
    async fn requirements(
        &self,
        req: tonic::Request<api::CookbookServiceRequirementsRequest>,
    ) -> super::Resp<api::CookbookServiceRequirementsResponse> {
        self.context
            .read(|conn, ctx| requirements(req, conn, ctx).scope_boxed())
            .await
    }

    // Retrieve net configurations for given chain.
    async fn net_configurations(
        &self,
        req: tonic::Request<api::CookbookServiceNetConfigurationsRequest>,
    ) -> super::Resp<api::CookbookServiceNetConfigurationsResponse> {
        self.context
            .read(|conn, ctx| net_configurations(req, conn, ctx).scope_boxed())
            .await
    }

    // List all available babel versions.
    async fn list_babel_versions(
        &self,
        req: tonic::Request<api::CookbookServiceListBabelVersionsRequest>,
    ) -> super::Resp<api::CookbookServiceListBabelVersionsResponse> {
        self.context
            .read(|conn, ctx| list_babel_versions(req, conn, ctx).scope_boxed())
            .await
    }
}

async fn retrieve_plugin(
    req: tonic::Request<api::CookbookServiceRetrievePluginRequest>,
    _conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::CookbookServiceRetrievePluginResponse> {
    let _claims = ctx.claims(&req, Endpoint::CookbookRetrievePlugin).await?;
    let req = req.into_inner();
    let id = req.id.ok_or_else(required("id"))?;
    let rhai_content = ctx
        .cookbook
        .read_file(
            &id.protocol,
            &id.node_type,
            &id.node_version,
            cookbook::RHAI_FILE_NAME,
        )
        .await?;
    let plugin = api::Plugin {
        identifier: Some(id),
        rhai_content,
    };
    let resp = api::CookbookServiceRetrievePluginResponse {
        plugin: Some(plugin),
    };
    Ok(tonic::Response::new(resp))
}

async fn retrieve_image(
    req: tonic::Request<api::CookbookServiceRetrieveImageRequest>,
    _conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::CookbookServiceRetrieveImageResponse> {
    let _claims = ctx.claims(&req, Endpoint::CookbookRetrieveImage).await?;
    let req = req.into_inner();
    let id = req.id.ok_or_else(required("id"))?;
    let url = ctx
        .cookbook
        .download_url(
            &id.protocol,
            &id.node_type,
            &id.node_version,
            cookbook::BABEL_IMAGE_NAME,
        )
        .await?;
    let location = api::ArchiveLocation { url };
    let resp = api::CookbookServiceRetrieveImageResponse {
        location: Some(location),
    };
    Ok(tonic::Response::new(resp))
}

async fn retrieve_kernel(
    req: tonic::Request<api::CookbookServiceRetrieveKernelRequest>,
    _conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::CookbookServiceRetrieveKernelResponse> {
    let _claims = ctx.claims(&req, Endpoint::CookbookRetrieveKernel).await?;
    let req = req.into_inner();
    let id = req.id.ok_or_else(required("id"))?;
    let url = ctx
        .cookbook
        .download_url(
            &id.protocol,
            &id.node_type,
            &id.node_version,
            cookbook::KERNEL_NAME,
        )
        .await?;
    let location = api::ArchiveLocation { url };
    let resp = api::CookbookServiceRetrieveKernelResponse {
        location: Some(location),
    };
    Ok(tonic::Response::new(resp))
}

async fn requirements(
    req: tonic::Request<api::CookbookServiceRequirementsRequest>,
    _conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::CookbookServiceRequirementsResponse> {
    let _claims = ctx.claims(&req, Endpoint::CookbookRequirements).await?;
    let req = req.into_inner();
    let id = req.id.ok_or_else(required("id"))?;
    let requirements = ctx
        .cookbook
        .rhai_metadata(&id.protocol, &id.node_type, &id.node_version)
        .await?
        .requirements;
    let resp = api::CookbookServiceRequirementsResponse {
        cpu_count: requirements.vcpu_count,
        mem_size_bytes: requirements.mem_size_mb * 1000 * 1000,
        disk_size_bytes: requirements.disk_size_gb * 1000 * 1000 * 1000,
    };
    Ok(tonic::Response::new(resp))
}

async fn net_configurations(
    req: tonic::Request<api::CookbookServiceNetConfigurationsRequest>,
    _conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::CookbookServiceNetConfigurationsResponse> {
    let _claims = ctx
        .claims(&req, Endpoint::CookbookNetConfigurations)
        .await?;
    let req = req.into_inner();
    let id = req.id.ok_or_else(required("id"))?;
    let networks = ctx
        .cookbook
        .rhai_metadata(&id.protocol, &id.node_type, &id.node_version)
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
    let resp = api::CookbookServiceNetConfigurationsResponse { networks };
    Ok(tonic::Response::new(resp))
}

async fn list_babel_versions(
    req: tonic::Request<api::CookbookServiceListBabelVersionsRequest>,
    _conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::CookbookServiceListBabelVersionsResponse> {
    let _claims = ctx
        .claims(&req, Endpoint::CookbookListBabelVersions)
        .await?;
    let req = req.into_inner();
    let identifiers = ctx.cookbook.list(&req.protocol, &req.node_type).await?;
    let resp = api::CookbookServiceListBabelVersionsResponse { identifiers };
    Ok(tonic::Response::new(resp))
}

// ---------------------------------------- Bundle Service ----------------------------------------
#[tonic::async_trait]
impl bundle_service_server::BundleService for super::Grpc {
    /// Retrieve image for specific version and state.
    async fn retrieve(
        &self,
        req: tonic::Request<api::BundleServiceRetrieveRequest>,
    ) -> super::Resp<api::BundleServiceRetrieveResponse> {
        self.context
            .read(|conn, ctx| retrieve(req, conn, ctx).scope_boxed())
            .await
    }

    /// List all available bundle versions.
    async fn list_bundle_versions(
        &self,
        req: tonic::Request<api::BundleServiceListBundleVersionsRequest>,
    ) -> super::Resp<api::BundleServiceListBundleVersionsResponse> {
        self.context
            .read(|conn, ctx| list_bundle_versions(req, conn, ctx).scope_boxed())
            .await
    }

    /// Delete bundle from storage.
    async fn delete(
        &self,
        req: tonic::Request<api::BundleServiceDeleteRequest>,
    ) -> super::Resp<api::BundleServiceDeleteResponse> {
        self.context
            .read(|conn, ctx| delete(req, conn, ctx).scope_boxed())
            .await
    }
}

async fn retrieve(
    req: tonic::Request<api::BundleServiceRetrieveRequest>,
    _conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::BundleServiceRetrieveResponse> {
    let _claims = ctx.claims(&req, Endpoint::BundleRetrieve).await?;
    let req = req.into_inner();
    let id = req.id.ok_or_else(required("id"))?;
    let url = ctx.cookbook.bundle_download_url(&id.version).await?;
    let resp = api::BundleServiceRetrieveResponse {
        location: Some(api::ArchiveLocation { url }),
    };
    Ok(tonic::Response::new(resp))
}

/// List all available bundle versions.
async fn list_bundle_versions(
    req: tonic::Request<api::BundleServiceListBundleVersionsRequest>,
    _conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::BundleServiceListBundleVersionsResponse> {
    let _claims = ctx.claims(&req, Endpoint::BundleListBundleVersions).await?;
    let identifiers = ctx.cookbook.list_bundles().await?;
    let resp = api::BundleServiceListBundleVersionsResponse { identifiers };
    Ok(tonic::Response::new(resp))
}

/// Delete bundle from storage.
async fn delete(
    req: tonic::Request<api::BundleServiceDeleteRequest>,
    _conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::BundleServiceDeleteResponse> {
    let _claims = ctx.claims(&req, Endpoint::BundleDelete).await?;
    // This endpoint is not currently used.
    Err(anyhow::anyhow!("Sod off").into())
}

// ---------------------------------------- Manifest Service ----------------------------------------
#[tonic::async_trait]
impl manifest_service_server::ManifestService for super::Grpc {
    /// Retrieve image for specific version and state.
    async fn retrieve_download_manifest(
        &self,
        req: tonic::Request<api::ManifestServiceRetrieveDownloadManifestRequest>,
    ) -> super::Resp<api::ManifestServiceRetrieveDownloadManifestResponse> {
        self.context
            .read(|conn, ctx| retrieve_download_manifest(req, conn, ctx).scope_boxed())
            .await
    }
}

async fn retrieve_download_manifest(
    req: tonic::Request<api::ManifestServiceRetrieveDownloadManifestRequest>,
    _conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::ManifestServiceRetrieveDownloadManifestResponse> {
    let _claims = ctx.claims(&req, Endpoint::ManifestRetrieveDownload).await?;
    let req = req.into_inner();
    let id = req.id.ok_or_else(required("id"))?;
    let manifest = ctx.cookbook.get_download_manifest(id, req.network).await?;
    let resp = api::ManifestServiceRetrieveDownloadManifestResponse {
        manifest: Some(manifest),
    };
    Ok(tonic::Response::new(resp))
}
