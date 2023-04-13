use super::api_ui::dashboard_service_server::DashboardService;
use super::api_ui::{metric, DashboardMetricsRequest, DashboardMetricsResponse, Metric};
use super::blockjoy_ui::ResponseMeta;
use crate::auth::UserAuthToken;
use crate::grpc::helpers::try_get_token;
use crate::grpc::{get_refresh_token, response_with_refresh_token};
use crate::models;
use tonic::{Request, Response, Status};

#[tonic::async_trait]
impl DashboardService for super::GrpcImpl {
    async fn metrics(
        &self,
        request: Request<DashboardMetricsRequest>,
    ) -> Result<Response<DashboardMetricsResponse>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?.clone();
        let inner = request.into_inner();
        let org_id = inner.org_id.parse().map_err(crate::Error::from)?;
        if token.try_org_id()? != org_id {
            super::bail_unauthorized!("Can't get metrics for this org");
        }

        let mut conn = self.conn().await?;
        let mut metrics: Vec<Metric> = Vec::with_capacity(2);
        if let Ok(running_nodes) = models::Node::running_nodes_count(org_id, &mut conn).await {
            let running = Metric {
                name: metric::Name::Online.into(),
                value: running_nodes.to_string(),
            };
            metrics.push(running);
        }
        if let Ok(stopped_nodes) = models::Node::halted_nodes_count(&org_id, &mut conn).await {
            let stopped = Metric {
                name: metric::Name::Offline.into(),
                value: stopped_nodes.to_string(),
            };
            metrics.push(stopped);
        }
        let response = DashboardMetricsResponse {
            meta: Some(ResponseMeta::from_meta(inner.meta, Some(token.try_into()?))),
            metrics,
        };

        response_with_refresh_token(refresh_token, response)
    }
}
