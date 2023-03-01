use super::blockjoy_ui::ResponseMeta;
use crate::auth::UserAuthToken;
use crate::errors::ApiError;
use crate::grpc::blockjoy_ui::dashboard_service_server::DashboardService;
use crate::grpc::blockjoy_ui::{metric, DashboardMetricsRequest, DashboardMetricsResponse, Metric};
use crate::grpc::helpers::try_get_token;
use crate::grpc::{get_refresh_token, response_with_refresh_token};
use crate::models;
use crate::models::{Node, Org};
use std::str::FromStr;
use tonic::{Request, Response, Status};

pub struct DashboardServiceImpl {
    db: models::DbPool,
}

impl DashboardServiceImpl {
    pub fn new(db: models::DbPool) -> Self {
        Self { db }
    }
}

#[tonic::async_trait]
impl DashboardService for DashboardServiceImpl {
    async fn metrics(
        &self,
        request: Request<DashboardMetricsRequest>,
    ) -> Result<Response<DashboardMetricsResponse>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?.clone();
        let user_id = token.id;
        let inner = request.into_inner();
        let org_id = uuid::Uuid::from_str(inner.org_id.as_str()).map_err(ApiError::from)?;

        let mut conn = self.db.conn().await?;
        // Ensure user is of member of the org
        Org::find_org_user(user_id, org_id, &mut conn).await?;

        let mut metrics: Vec<Metric> = Vec::with_capacity(2);

        if let Ok(running_nodes) = Node::running_nodes_count(org_id, &mut conn).await {
            let running = Metric {
                name: metric::Name::Online.into(),
                value: running_nodes.to_string(),
            };
            metrics.insert(0, running);
        }

        if let Ok(stopped_nodes) = Node::halted_nodes_count(&org_id, &mut conn).await {
            let stopped = Metric {
                name: metric::Name::Offline.into(),
                value: stopped_nodes.to_string(),
            };
            metrics.insert(1, stopped);
        }

        let response = DashboardMetricsResponse {
            meta: Some(ResponseMeta::from_meta(inner.meta, Some(token.try_into()?))),
            metrics,
        };

        response_with_refresh_token(refresh_token, response)
    }
}
