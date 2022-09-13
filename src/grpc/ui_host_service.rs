use crate::auth::{FindableById, TokenType};
use crate::grpc::blockjoy_ui::host_service_server::HostService;
use crate::grpc::blockjoy_ui::{
    get_hosts_request, response_meta, CreateHostRequest, CreateHostResponse, DeleteHostRequest,
    DeleteHostResponse, GetHostsRequest, GetHostsResponse, Host as GrpcHost, UpdateHostRequest,
    UpdateHostResponse,
};
use crate::grpc::helpers::success_response_meta;
use crate::models::{Host, HostRequest, HostSelectiveUpdate, Token};
use crate::server::DbPool;
use tonic::{Request, Response, Status};
use uuid::Uuid;

pub struct HostServiceImpl {
    db: DbPool,
}

impl HostServiceImpl {
    pub fn new(db: DbPool) -> Self {
        Self { db }
    }
}

#[tonic::async_trait]
impl HostService for HostServiceImpl {
    /// Get host(s) by one of:
    /// - ID
    /// - Organization ID
    /// - Token
    async fn get(
        &self,
        request: Request<GetHostsRequest>,
    ) -> Result<Response<GetHostsResponse>, Status> {
        let inner = request.into_inner();

        if inner.param.is_none() {
            return Err(Status::not_found("None of ID, OrgID, Token was provided"));
        }

        let hosts = match inner.param.unwrap() {
            get_hosts_request::Param::Id(id) => vec![GrpcHost::from(
                Host::find_by_id(Uuid::from(id), &self.db).await?,
            )],
            get_hosts_request::Param::OrgId(org_id) => {
                Host::find_by_org(Uuid::from(org_id), &self.db)
                    .await?
                    .iter()
                    .map(GrpcHost::from)
                    .collect()
            }
            get_hosts_request::Param::Token(ref token) => vec![GrpcHost::from(
                Token::get_host_for_token(token, TokenType::Login, &self.db).await?,
            )],
        };

        if !hosts.is_empty() {
            let response_meta = success_response_meta(
                response_meta::Status::Success as i32,
                inner.meta.unwrap().id,
            );
            let response = GetHostsResponse {
                meta: Some(response_meta),
                hosts,
            };

            Ok(Response::new(response))
        } else {
            Err(Status::not_found("No hosts found"))
        }
    }

    async fn create(
        &self,
        request: Request<CreateHostRequest>,
    ) -> Result<Response<CreateHostResponse>, Status> {
        let inner = request.into_inner();
        let host = inner.host.unwrap();
        let fields: HostRequest = host.into();

        match Host::create(fields, &self.db).await {
            Ok(_) => {
                let response_meta = success_response_meta(
                    response_meta::Status::Success as i32,
                    inner.meta.unwrap().id,
                );
                let response = CreateHostResponse {
                    meta: Some(response_meta),
                };

                Ok(Response::new(response))
            }
            Err(e) => Err(Status::from(e)),
        }
    }

    async fn update(
        &self,
        request: Request<UpdateHostRequest>,
    ) -> Result<Response<UpdateHostResponse>, Status> {
        let inner = request.into_inner();
        let host = inner.host.unwrap();
        let host_id = host.id.clone().unwrap();
        let fields: HostSelectiveUpdate = host.into();

        match Host::update_all(Uuid::from(host_id), fields, &self.db).await {
            Ok(_) => {
                let response_meta = success_response_meta(
                    response_meta::Status::Success as i32,
                    inner.meta.unwrap().id,
                );
                let response = UpdateHostResponse {
                    meta: Some(response_meta),
                };

                Ok(Response::new(response))
            }
            Err(e) => Err(Status::from(e)),
        }
    }

    async fn delete(
        &self,
        request: Request<DeleteHostRequest>,
    ) -> Result<Response<DeleteHostResponse>, Status> {
        let inner = request.into_inner();
        let host_id = inner.id.unwrap();

        match Host::delete(Uuid::from(host_id), &self.db).await {
            Ok(_) => {
                let response_meta = success_response_meta(
                    response_meta::Status::Success as i32,
                    inner.meta.unwrap().id,
                );
                let response = DeleteHostResponse {
                    meta: Some(response_meta),
                };

                Ok(Response::new(response))
            }
            Err(e) => Err(Status::from(e)),
        }
    }
}
