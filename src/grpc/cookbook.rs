use diesel_async::scoped_futures::ScopedFutureExt;

use crate::auth::endpoint::Endpoint;
use crate::cookbook;
use crate::models::Conn;

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
        self.run(|c| retrieve_plugin(req, c).scope_boxed()).await
    }

    // Retrieve image for specific version and state.
    async fn retrieve_image(
        &self,
        req: tonic::Request<api::CookbookServiceRetrieveImageRequest>,
    ) -> super::Resp<api::CookbookServiceRetrieveImageResponse> {
        self.run(|c| retrieve_image(req, c).scope_boxed()).await
    }

    // Retrieve kernel file for specific version and state.
    async fn retrieve_kernel(
        &self,
        req: tonic::Request<api::CookbookServiceRetrieveKernelRequest>,
    ) -> super::Resp<api::CookbookServiceRetrieveKernelResponse> {
        self.run(|c| retrieve_kernel(req, c).scope_boxed()).await
    }

    // Retrieve hardware requirements for given identifier.
    async fn requirements(
        &self,
        req: tonic::Request<api::CookbookServiceRequirementsRequest>,
    ) -> super::Resp<api::CookbookServiceRequirementsResponse> {
        self.run(|c| requirements(req, c).scope_boxed()).await
    }

    // Retrieve net configurations for given chain.
    async fn net_configurations(
        &self,
        req: tonic::Request<api::CookbookServiceNetConfigurationsRequest>,
    ) -> super::Resp<api::CookbookServiceNetConfigurationsResponse> {
        self.run(|c| net_configurations(req, c).scope_boxed()).await
    }

    // List all available babel versions.
    async fn list_babel_versions(
        &self,
        req: tonic::Request<api::CookbookServiceListBabelVersionsRequest>,
    ) -> super::Resp<api::CookbookServiceListBabelVersionsResponse> {
        self.run(|c| list_babel_versions(req, c).scope_boxed())
            .await
    }
}

async fn retrieve_plugin(
    req: tonic::Request<api::CookbookServiceRetrievePluginRequest>,
    conn: &mut Conn,
) -> super::Result<api::CookbookServiceRetrievePluginResponse> {
    let _claims = conn.claims(&req, Endpoint::CookbookRetrievePlugin).await?;
    let req = req.into_inner();
    let id = req.id.ok_or_else(required("id"))?;
    let rhai_content = conn
        .context
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
    conn: &mut Conn,
) -> super::Result<api::CookbookServiceRetrieveImageResponse> {
    let _claims = conn.claims(&req, Endpoint::CookbookRetrieveImage).await?;
    let req = req.into_inner();
    let id = req.id.ok_or_else(required("id"))?;
    let url = conn
        .context
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
    conn: &mut Conn,
) -> super::Result<api::CookbookServiceRetrieveKernelResponse> {
    let _claims = conn.claims(&req, Endpoint::CookbookRetrieveKernel).await?;
    let req = req.into_inner();
    let id = req.id.ok_or_else(required("id"))?;
    let url = conn
        .context
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
    conn: &mut Conn,
) -> super::Result<api::CookbookServiceRequirementsResponse> {
    let _claims = conn.claims(&req, Endpoint::CookbookRequirements).await?;
    let req = req.into_inner();
    let id = req.id.ok_or_else(required("id"))?;
    let requirements = conn
        .context
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
    conn: &mut Conn,
) -> super::Result<api::CookbookServiceNetConfigurationsResponse> {
    let _claims = conn
        .claims(&req, Endpoint::CookbookNetConfigurations)
        .await?;
    let req = req.into_inner();
    let id = req.id.ok_or_else(required("id"))?;
    let networks = conn
        .context
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
    conn: &mut Conn,
) -> super::Result<api::CookbookServiceListBabelVersionsResponse> {
    let _claims = conn
        .claims(&req, Endpoint::CookbookListBabelVersions)
        .await?;
    let req = req.into_inner();
    let identifiers = conn
        .context
        .cookbook
        .list(&req.protocol, &req.node_type)
        .await?;
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
        self.run(|c| retrieve(req, c).scope_boxed()).await
    }

    /// List all available bundle versions.
    async fn list_bundle_versions(
        &self,
        req: tonic::Request<api::BundleServiceListBundleVersionsRequest>,
    ) -> super::Resp<api::BundleServiceListBundleVersionsResponse> {
        self.run(|c| list_bundle_versions(req, c).scope_boxed())
            .await
    }

    /// Delete bundle from storage.
    async fn delete(
        &self,
        req: tonic::Request<api::BundleServiceDeleteRequest>,
    ) -> super::Resp<api::BundleServiceDeleteResponse> {
        self.run(|c| delete(req, c).scope_boxed()).await
    }
}

async fn retrieve(
    req: tonic::Request<api::BundleServiceRetrieveRequest>,
    conn: &mut Conn,
) -> super::Result<api::BundleServiceRetrieveResponse> {
    let _claims = conn.claims(&req, Endpoint::BundleRetrieve).await?;
    let req = req.into_inner();
    let id = req.id.ok_or_else(required("id"))?;
    let url = conn
        .context
        .cookbook
        .bundle_download_url(&id.version)
        .await?;
    let resp = api::BundleServiceRetrieveResponse {
        location: Some(api::ArchiveLocation { url }),
    };
    Ok(tonic::Response::new(resp))
}

/// List all available bundle versions.
async fn list_bundle_versions(
    req: tonic::Request<api::BundleServiceListBundleVersionsRequest>,
    conn: &mut Conn,
) -> super::Result<api::BundleServiceListBundleVersionsResponse> {
    let _claims = conn
        .claims(&req, Endpoint::BundleListBundleVersions)
        .await?;
    let identifiers = conn.context.cookbook.list_bundles().await?;
    let resp = api::BundleServiceListBundleVersionsResponse { identifiers };
    Ok(tonic::Response::new(resp))
}

/// Delete bundle from storage.
async fn delete(
    req: tonic::Request<api::BundleServiceDeleteRequest>,
    conn: &mut Conn,
) -> super::Result<api::BundleServiceDeleteResponse> {
    let _claims = conn.claims(&req, Endpoint::BundleDelete).await?;
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
        self.run(|c| retrieve_download_manifest(req, c).scope_boxed())
            .await
    }
}

async fn retrieve_download_manifest(
    req: tonic::Request<api::ManifestServiceRetrieveDownloadManifestRequest>,
    conn: &mut Conn,
) -> super::Result<api::ManifestServiceRetrieveDownloadManifestResponse> {
    let _claims = conn
        .claims(&req, Endpoint::ManifestRetrieveDownload)
        .await?;
    let req = req.into_inner();
    let id = req.id.ok_or_else(required("id"))?;
    let manifest = conn
        .context
        .cookbook
        .get_download_manifest(id, req.network)
        .await?;
    let resp = api::ManifestServiceRetrieveDownloadManifestResponse {
        manifest: Some(manifest),
    };
    Ok(tonic::Response::new(resp))
}
