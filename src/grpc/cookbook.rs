use super::{
    api::{self, cookbook_service_server},
    helpers::required,
};
use crate::{
    auth::{self, token::Endpoint},
    cookbook,
    models::Conn,
};

#[tonic::async_trait]
impl cookbook_service_server::CookbookService for super::GrpcImpl {
    // Retrieve plugin for specific version and state.
    async fn retrieve_plugin(
        &self,
        req: tonic::Request<api::CookbookServiceRetrievePluginRequest>,
    ) -> super::Resp<api::CookbookServiceRetrievePluginResponse> {
        let mut conn = self.conn().await?;
        let resp = retrieve_plugin(self, req, &mut conn).await?;
        Ok(resp)
    }

    // Retrieve image for specific version and state.
    async fn retrieve_image(
        &self,
        req: tonic::Request<api::CookbookServiceRetrieveImageRequest>,
    ) -> super::Resp<api::CookbookServiceRetrieveImageResponse> {
        let mut conn = self.conn().await?;
        let resp = retrieve_image(self, req, &mut conn).await?;
        Ok(resp)
    }

    // Retrieve kernel file for specific version and state.
    async fn retrieve_kernel(
        &self,
        req: tonic::Request<api::CookbookServiceRetrieveKernelRequest>,
    ) -> super::Resp<api::CookbookServiceRetrieveKernelResponse> {
        let mut conn = self.conn().await?;
        let resp = retrieve_kernel(self, req, &mut conn).await?;
        Ok(resp)
    }

    // Retrieve hardware requirements for given identifier.
    async fn requirements(
        &self,
        req: tonic::Request<api::CookbookServiceRequirementsRequest>,
    ) -> super::Resp<api::CookbookServiceRequirementsResponse> {
        let mut conn = self.conn().await?;
        let resp = requirements(self, req, &mut conn).await?;
        Ok(resp)
    }

    // Retrieve net configurations for given chain.
    async fn net_configurations(
        &self,
        req: tonic::Request<api::CookbookServiceNetConfigurationsRequest>,
    ) -> super::Resp<api::CookbookServiceNetConfigurationsResponse> {
        let mut conn = self.conn().await?;
        let resp = net_configurations(self, req, &mut conn).await?;
        Ok(resp)
    }

    // List all available babel versions.
    async fn list_babel_versions(
        &self,
        req: tonic::Request<api::CookbookServiceListBabelVersionsRequest>,
    ) -> super::Resp<api::CookbookServiceListBabelVersionsResponse> {
        let mut conn = self.conn().await?;
        let resp = list_babel_versions(self, req, &mut conn).await?;
        Ok(resp)
    }
}

async fn retrieve_plugin(
    grpc: &super::GrpcImpl,
    req: tonic::Request<api::CookbookServiceRetrievePluginRequest>,
    conn: &mut Conn,
) -> super::Result<api::CookbookServiceRetrievePluginResponse> {
    auth::get_claims(&req, Endpoint::CookbookRetrievePlugin, conn).await?;
    let req = req.into_inner();
    let id = req.id.ok_or_else(required("id"))?;
    let rhai_content = grpc
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
    grpc: &super::GrpcImpl,
    req: tonic::Request<api::CookbookServiceRetrieveImageRequest>,
    conn: &mut Conn,
) -> super::Result<api::CookbookServiceRetrieveImageResponse> {
    auth::get_claims(&req, Endpoint::CookbookRetrieveImage, conn).await?;
    let req = req.into_inner();
    let id = req.id.ok_or_else(required("id"))?;
    let url = grpc
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
    grpc: &super::GrpcImpl,
    req: tonic::Request<api::CookbookServiceRetrieveKernelRequest>,
    conn: &mut Conn,
) -> super::Result<api::CookbookServiceRetrieveKernelResponse> {
    auth::get_claims(&req, Endpoint::CookbookRetrieveKernel, conn).await?;
    let req = req.into_inner();
    let id = req.id.ok_or_else(required("id"))?;
    let url = grpc
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
    grpc: &super::GrpcImpl,
    req: tonic::Request<api::CookbookServiceRequirementsRequest>,
    conn: &mut Conn,
) -> super::Result<api::CookbookServiceRequirementsResponse> {
    auth::get_claims(&req, Endpoint::CookbookRequirements, conn).await?;
    let req = req.into_inner();
    let id = req.id.ok_or_else(required("id"))?;
    let requirements = grpc
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
    grpc: &super::GrpcImpl,
    req: tonic::Request<api::CookbookServiceNetConfigurationsRequest>,
    conn: &mut Conn,
) -> super::Result<api::CookbookServiceNetConfigurationsResponse> {
    auth::get_claims(&req, Endpoint::CookbookNetConfigurations, conn).await?;
    let req = req.into_inner();
    let id = req.id.ok_or_else(required("id"))?;
    let networks = grpc
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
    grpc: &super::GrpcImpl,
    req: tonic::Request<api::CookbookServiceListBabelVersionsRequest>,
    conn: &mut Conn,
) -> super::Result<api::CookbookServiceListBabelVersionsResponse> {
    auth::get_claims(&req, Endpoint::CookbookListBabelVersions, conn).await?;
    let req = req.into_inner();
    let identifiers = grpc.cookbook.list(&req.protocol, &req.node_type).await?;
    let resp = api::CookbookServiceListBabelVersionsResponse { identifiers };
    Ok(tonic::Response::new(resp))
}
