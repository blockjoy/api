use crate::auth::{FindableById, UserAuthToken};
use crate::errors::ApiError;
use crate::grpc::blockjoy_ui::organization_service_server::OrganizationService;
use crate::grpc::blockjoy_ui::{
    CreateOrganizationRequest, CreateOrganizationResponse, DeleteOrganizationRequest,
    DeleteOrganizationResponse, GetOrganizationsRequest, GetOrganizationsResponse,
    LeaveOrganizationRequest, Organization, OrganizationMemberRequest, OrganizationMemberResponse,
    RemoveMemberRequest, ResponseMeta, RestoreOrganizationRequest, RestoreOrganizationResponse,
    UpdateOrganizationRequest, UpdateOrganizationResponse, User as GrpcUiUser,
};
use crate::grpc::helpers::pagination_parameters;
use crate::grpc::{get_refresh_token, response_with_refresh_token};
use crate::models::{self, Org, OrgRequest, OrgRole};
use tonic::{Request, Response, Status};
use uuid::Uuid;

use super::helpers::{required, try_get_token};

pub struct OrganizationServiceImpl {
    db: models::DbPool,
}

impl OrganizationServiceImpl {
    pub fn new(db: models::DbPool) -> Self {
        Self { db }
    }
}

#[tonic::async_trait]
impl OrganizationService for OrganizationServiceImpl {
    async fn get(
        &self,
        request: Request<GetOrganizationsRequest>,
    ) -> Result<Response<GetOrganizationsResponse>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?.clone();
        let user_id = token.id;
        let inner = request.into_inner();
        let org_id = inner.org_id.clone();

        let mut conn = self.db.conn().await?;
        let organizations: Vec<Org> = match org_id {
            Some(org_id) => vec![
                Org::find_by_id(org_id.parse().map_err(ApiError::UuidParseError)?, &mut conn)
                    .await?,
            ],
            None => Org::find_all_by_user(user_id, &mut conn).await?,
        };
        let organizations: Result<Vec<Organization>, ApiError> =
            organizations.iter().map(Organization::try_from).collect();

        match organizations {
            Ok(mut organizations) => {
                for mut org in &mut organizations {
                    let org_id: Uuid = org
                        .id
                        .as_ref()
                        .unwrap_or(&"".to_string())
                        .parse()
                        .map_err(ApiError::UuidParseError)?;
                    org.current_user =
                        Some(Org::find_org_user(user_id, org_id, &mut conn).await?.into());
                }

                let inner = GetOrganizationsResponse {
                    meta: Some(ResponseMeta::from_meta(inner.meta, Some(token.try_into()?))),
                    organizations,
                };

                Ok(response_with_refresh_token(refresh_token, inner)?)
            }
            Err(e) => Err(Status::from(e)),
        }
    }

    async fn create(
        &self,
        request: Request<CreateOrganizationRequest>,
    ) -> Result<Response<CreateOrganizationResponse>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?.clone();
        let user_id = token.id;
        let inner = request.into_inner();
        let org = inner.organization.ok_or_else(required("organization"))?;
        let name = org.name.ok_or_else(required("organization.name"))?;
        let org_request = OrgRequest { name };
        let mut tx = self.db.begin().await?;
        let org = Org::create(&org_request, user_id, &mut tx).await?;
        tx.commit().await?;
        let response_meta =
            ResponseMeta::from_meta(inner.meta, Some(token.try_into()?)).with_message(org.id);
        let inner = CreateOrganizationResponse {
            meta: Some(response_meta),
        };
        Ok(response_with_refresh_token(refresh_token, inner)?)
    }

    async fn update(
        &self,
        request: Request<UpdateOrganizationRequest>,
    ) -> Result<Response<UpdateOrganizationResponse>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?.try_into()?;
        let inner = request.into_inner();
        let org = inner.organization.ok_or_else(required("organization"))?;
        let org_id = Uuid::parse_str(org.id.ok_or_else(required("organization.id"))?.as_str())
            .map_err(ApiError::from)?;
        let update = OrgRequest {
            name: org.name.ok_or_else(required("organization.name"))?,
        };

        let mut tx = self.db.begin().await?;
        Org::update(org_id, update, &mut tx).await?;
        tx.commit().await?;
        let meta = ResponseMeta::from_meta(inner.meta, Some(token));
        let inner = UpdateOrganizationResponse { meta: Some(meta) };
        Ok(response_with_refresh_token(refresh_token, inner)?)
    }

    async fn delete(
        &self,
        request: Request<DeleteOrganizationRequest>,
    ) -> Result<Response<DeleteOrganizationResponse>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?.clone();
        let user_id = token.id;
        let inner = request.into_inner();
        let org_id = Uuid::parse_str(inner.id.as_str()).map_err(ApiError::from)?;
        let mut tx = self.db.begin().await?;
        let member = Org::find_org_user(user_id, org_id, &mut tx).await?;

        // Only owner or admins may delete orgs
        match member.role {
            OrgRole::Member => Err(Status::permission_denied(format!(
                "User {user_id} has no sufficient privileges to delete org {org_id}"
            ))),
            OrgRole::Owner | OrgRole::Admin => {
                tracing::debug!("Deleting org: {}", org_id);
                Org::delete(org_id, &mut tx).await?;
                tx.commit().await?;

                let meta = ResponseMeta::from_meta(inner.meta, Some(token.try_into()?));
                let inner = DeleteOrganizationResponse { meta: Some(meta) };

                Ok(response_with_refresh_token(refresh_token, inner)?)
            }
        }
    }

    async fn restore(
        &self,
        request: Request<RestoreOrganizationRequest>,
    ) -> Result<Response<RestoreOrganizationResponse>, Status> {
        let token = try_get_token::<_, UserAuthToken>(&request)?.clone();
        let user_id = token.id;
        let inner = request.into_inner();
        let org_id = Uuid::parse_str(inner.id.as_str()).map_err(ApiError::from)?;
        let mut tx = self.db.begin().await?;
        let member = Org::find_org_user(user_id, org_id, &mut tx).await?;

        match member.role {
            OrgRole::Member => Err(Status::permission_denied(format!(
                "User {user_id} has no sufficient privileges to restore org {org_id}"
            ))),
            // Only owner or admins may restore orgs
            OrgRole::Owner | OrgRole::Admin => {
                let org = Org::restore(org_id, &mut tx).await?;
                tx.commit().await?;
                let meta = ResponseMeta::from_meta(inner.meta, Some(token.try_into()?));
                let inner = RestoreOrganizationResponse {
                    meta: Some(meta),
                    organization: Some(org.try_into()?),
                };

                Ok(Response::new(inner))
            }
        }
    }

    async fn members(
        &self,
        request: Request<OrganizationMemberRequest>,
    ) -> Result<Response<OrganizationMemberResponse>, Status> {
        let token = try_get_token::<_, UserAuthToken>(&request)?.try_into()?;
        let refresh_token = get_refresh_token(&request);
        let inner = request.into_inner();
        let meta = inner.meta.ok_or_else(required("meta"))?;
        let org_id = Uuid::parse_str(inner.id.as_str()).map_err(ApiError::from)?;

        let (limit, offset) = pagination_parameters(meta.pagination.clone())?;
        let mut conn = self.db.conn().await?;
        let users = Org::find_all_member_users_paginated(org_id, limit, offset, &mut conn).await?;
        let users: Result<_, ApiError> = users.iter().map(GrpcUiUser::try_from).collect();
        let inner = OrganizationMemberResponse {
            meta: Some(ResponseMeta::from_meta(meta, Some(token)).with_pagination()),
            users: users?,
        };

        Ok(response_with_refresh_token(refresh_token, inner)?)
    }

    async fn remove_member(
        &self,
        request: Request<RemoveMemberRequest>,
    ) -> Result<Response<()>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?;
        let caller_id = token.id;
        let inner = request.into_inner();
        let user_id = Uuid::parse_str(inner.user_id.as_str()).map_err(ApiError::from)?;
        let org_id = Uuid::parse_str(inner.org_id.as_str()).map_err(ApiError::from)?;
        let mut tx = self.db.begin().await?;
        let member = Org::find_org_user(caller_id, org_id, &mut tx).await?;

        match member.role {
            OrgRole::Member => Err(Status::permission_denied(format!(
                "User {caller_id} has no sufficient privileges to remove other user {user_id} from org {org_id}"
            ))),
            OrgRole::Owner | OrgRole::Admin => {
                Org::remove_org_user(user_id, org_id, &mut tx).await?;
                tx.commit().await?;

                Ok(response_with_refresh_token::<()>(refresh_token, ())?)
            }
        }
    }

    async fn leave(
        &self,
        request: Request<LeaveOrganizationRequest>,
    ) -> Result<Response<()>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?;
        let user_id = token.id;
        let inner = request.into_inner();
        let org_id = Uuid::parse_str(inner.org_id.as_str()).map_err(ApiError::from)?;

        let mut tx = self.db.begin().await?;
        Org::remove_org_user(user_id, org_id, &mut tx).await?;
        tx.commit().await?;

        Ok(response_with_refresh_token::<()>(refresh_token, ())?)
    }
}
