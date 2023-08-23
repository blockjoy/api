use diesel_async::scoped_futures::ScopedFutureExt;

use crate::auth::endpoint::Endpoint;
use crate::cookbook;
use crate::database::{ReadConn, Transaction};

use super::api::{
    self, bundle_service_server, cookbook_service_server, kernel_service_server,
    manifest_service_server,
};
use super::helpers::required;

// --------------------------------------- Cookbook Service ---------------------------------------

#[tonic::async_trait]
impl cookbook_service_server::CookbookService for super::Grpc {
    // Retrieve plugin for specific version and state.
    async fn retrieve_plugin(
        &self,
        req: tonic::Request<api::CookbookServiceRetrievePluginRequest>,
    ) -> super::Resp<api::CookbookServiceRetrievePluginResponse> {
        self.read(|read| retrieve_plugin(req, read).scope_boxed())
            .await
    }

    // Retrieve image for specific version and state.
    async fn retrieve_image(
        &self,
        req: tonic::Request<api::CookbookServiceRetrieveImageRequest>,
    ) -> super::Resp<api::CookbookServiceRetrieveImageResponse> {
        self.read(|read| retrieve_image(req, read).scope_boxed())
            .await
    }

    // Retrieve hardware requirements for given identifier.
    async fn requirements(
        &self,
        req: tonic::Request<api::CookbookServiceRequirementsRequest>,
    ) -> super::Resp<api::CookbookServiceRequirementsResponse> {
        self.read(|read| requirements(req, read).scope_boxed())
            .await
    }

    // Retrieve net configurations for given chain.
    async fn net_configurations(
        &self,
        req: tonic::Request<api::CookbookServiceNetConfigurationsRequest>,
    ) -> super::Resp<api::CookbookServiceNetConfigurationsResponse> {
        self.read(|read| net_configurations(req, read).scope_boxed())
            .await
    }

    // List all available babel versions.
    async fn list_babel_versions(
        &self,
        req: tonic::Request<api::CookbookServiceListBabelVersionsRequest>,
    ) -> super::Resp<api::CookbookServiceListBabelVersionsResponse> {
        self.read(|read| list_babel_versions(req, read).scope_boxed())
            .await
    }
}

async fn retrieve_plugin(
    req: tonic::Request<api::CookbookServiceRetrievePluginRequest>,
    read: ReadConn<'_, '_>,
) -> super::Result<api::CookbookServiceRetrievePluginResponse> {
    let ReadConn { conn, ctx } = read;
    let _claims = ctx
        .claims(&req, Endpoint::CookbookRetrievePlugin, conn)
        .await?;

    let req = req.into_inner();
    let id = req.id.ok_or_else(required("id"))?;
    let rhai_content = ctx
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
    let resp = api::CookbookServiceRetrievePluginResponse {
        plugin: Some(plugin),
    };
    Ok(tonic::Response::new(resp))
}

async fn retrieve_image(
    req: tonic::Request<api::CookbookServiceRetrieveImageRequest>,
    read: ReadConn<'_, '_>,
) -> super::Result<api::CookbookServiceRetrieveImageResponse> {
    let ReadConn { conn, ctx } = read;
    let _claims = ctx
        .claims(&req, Endpoint::CookbookRetrieveImage, conn)
        .await?;

    let req = req.into_inner();
    let id = req.id.ok_or_else(required("id"))?;
    let url = ctx
        .cookbook
        .download_url(
            &id.protocol,
            id.node_type().into_model(),
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

async fn requirements(
    req: tonic::Request<api::CookbookServiceRequirementsRequest>,
    read: ReadConn<'_, '_>,
) -> super::Result<api::CookbookServiceRequirementsResponse> {
    let ReadConn { conn, ctx } = read;
    let _claims = ctx
        .claims(&req, Endpoint::CookbookRequirements, conn)
        .await?;

    let req = req.into_inner();
    let id = req.id.ok_or_else(required("id"))?;
    let node_type = id.node_type().into_model();
    let requirements = ctx
        .cookbook
        .rhai_metadata(&id.protocol, node_type, &id.node_version)
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
    read: ReadConn<'_, '_>,
) -> super::Result<api::CookbookServiceNetConfigurationsResponse> {
    let ReadConn { conn, ctx } = read;
    let _claims = ctx
        .claims(&req, Endpoint::CookbookNetConfigurations, conn)
        .await?;

    let req = req.into_inner();
    let id = req.id.ok_or_else(required("id"))?;
    let node_type = id.node_type().into_model();
    let networks = ctx
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
    let resp = api::CookbookServiceNetConfigurationsResponse { networks };
    Ok(tonic::Response::new(resp))
}

async fn list_babel_versions(
    req: tonic::Request<api::CookbookServiceListBabelVersionsRequest>,
    read: ReadConn<'_, '_>,
) -> super::Result<api::CookbookServiceListBabelVersionsResponse> {
    let ReadConn { conn, ctx } = read;
    let _claims = ctx
        .claims(&req, Endpoint::CookbookListBabelVersions, conn)
        .await?;

    let req = req.into_inner();
    let node_type = req.node_type().into_model();
    let identifiers = ctx.cookbook.list(&req.protocol, node_type).await?;
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
        self.read(|read| retrieve(req, read).scope_boxed()).await
    }

    /// List all available bundle versions.
    async fn list_bundle_versions(
        &self,
        req: tonic::Request<api::BundleServiceListBundleVersionsRequest>,
    ) -> super::Resp<api::BundleServiceListBundleVersionsResponse> {
        self.read(|read| list_bundle_versions(req, read).scope_boxed())
            .await
    }

    /// Delete bundle from storage.
    async fn delete(
        &self,
        req: tonic::Request<api::BundleServiceDeleteRequest>,
    ) -> super::Resp<api::BundleServiceDeleteResponse> {
        self.read(|read| delete(req, read).scope_boxed()).await
    }
}

async fn retrieve(
    req: tonic::Request<api::BundleServiceRetrieveRequest>,
    read: ReadConn<'_, '_>,
) -> super::Result<api::BundleServiceRetrieveResponse> {
    let ReadConn { conn, ctx } = read;
    let _claims = ctx.claims(&req, Endpoint::BundleRetrieve, conn).await?;
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
    read: ReadConn<'_, '_>,
) -> super::Result<api::BundleServiceListBundleVersionsResponse> {
    let ReadConn { conn, ctx } = read;
    let _claims = ctx
        .claims(&req, Endpoint::BundleListBundleVersions, conn)
        .await?;
    let identifiers = ctx.cookbook.list_bundles().await?;
    let resp = api::BundleServiceListBundleVersionsResponse { identifiers };
    Ok(tonic::Response::new(resp))
}

/// Delete bundle from storage.
async fn delete(
    req: tonic::Request<api::BundleServiceDeleteRequest>,
    read: ReadConn<'_, '_>,
) -> super::Result<api::BundleServiceDeleteResponse> {
    let ReadConn { conn, ctx } = read;
    let _claims = ctx.claims(&req, Endpoint::BundleDelete, conn).await?;
    // This endpoint is not currently used.
    Err(anyhow::anyhow!("Sod off").into())
}

// ---------------------------------------- Kernel Service ----------------------------------------

#[tonic::async_trait]
impl kernel_service_server::KernelService for super::Grpc {
    async fn retrieve(
        &self,
        req: tonic::Request<api::KernelServiceRetrieveRequest>,
    ) -> super::Resp<api::KernelServiceRetrieveResponse> {
        self.read(|read| retrieve_kernel_(req, read).scope_boxed())
            .await
    }

    async fn list_kernel_versions(
        &self,
        req: tonic::Request<api::KernelServiceListKernelVersionsRequest>,
    ) -> super::Resp<api::KernelServiceListKernelVersionsResponse> {
        self.read(|read| list_kernel_versions(req, read).scope_boxed())
            .await
    }
}

async fn retrieve_kernel_(
    req: tonic::Request<api::KernelServiceRetrieveRequest>,
    read: ReadConn<'_, '_>,
) -> super::Result<api::KernelServiceRetrieveResponse> {
    let ReadConn { conn, ctx } = read;
    let _claims = ctx
        .claims(&req, Endpoint::CookbookRetrieveKernel, conn)
        .await?;

    let req = req.into_inner();
    let id = req.id.ok_or_else(required("id"))?;
    let url = ctx.cookbook.download_url_kernel(&id.version).await?;
    let location = api::ArchiveLocation { url };
    let resp = api::KernelServiceRetrieveResponse {
        location: Some(location),
    };
    Ok(tonic::Response::new(resp))
}

async fn list_kernel_versions(
    _: tonic::Request<api::KernelServiceListKernelVersionsRequest>,
    read: ReadConn<'_, '_>,
) -> super::Result<api::KernelServiceListKernelVersionsResponse> {
    let ReadConn { ctx, .. } = read;
    let identifiers = ctx.cookbook.list_kernels().await?;
    let resp = api::KernelServiceListKernelVersionsResponse { identifiers };
    Ok(tonic::Response::new(resp))
}

// --------------------------------------- Manifest Service ---------------------------------------
#[tonic::async_trait]
impl manifest_service_server::ManifestService for super::Grpc {
    /// Retrieve image for specific version and state.
    async fn retrieve_download_manifest(
        &self,
        req: tonic::Request<api::ManifestServiceRetrieveDownloadManifestRequest>,
    ) -> super::Resp<api::ManifestServiceRetrieveDownloadManifestResponse> {
        self.read(|read| retrieve_download_manifest(req, read).scope_boxed())
            .await
    }
}

async fn retrieve_download_manifest(
    req: tonic::Request<api::ManifestServiceRetrieveDownloadManifestRequest>,
    read: ReadConn<'_, '_>,
) -> super::Result<api::ManifestServiceRetrieveDownloadManifestResponse> {
    let ReadConn { conn, ctx } = read;
    let _claims = ctx
        .claims(&req, Endpoint::ManifestRetrieveDownload, conn)
        .await?;
    let req = req.into_inner();
    let id = req.id.ok_or_else(required("id"))?;
    let manifest = ctx.cookbook.get_download_manifest(id, req.network).await?;
    let resp = api::ManifestServiceRetrieveDownloadManifestResponse {
        manifest: Some(manifest),
    };
    Ok(tonic::Response::new(resp))
}
