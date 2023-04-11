use super::helpers::{required, try_get_token};
use super::{blockjoy_ui, convert};
use crate::auth::{FindableById, UserAuthToken};
use crate::grpc::blockjoy_ui::organization_service_server::OrganizationService;
use crate::grpc::blockjoy_ui::{
    CreateOrganizationRequest, CreateOrganizationResponse, DeleteOrganizationRequest,
    DeleteOrganizationResponse, GetOrganizationsRequest, GetOrganizationsResponse,
    LeaveOrganizationRequest, Organization, OrganizationMemberRequest, OrganizationMemberResponse,
    RemoveMemberRequest, ResponseMeta, RestoreOrganizationRequest, RestoreOrganizationResponse,
    UpdateOrganizationRequest, UpdateOrganizationResponse,
};
use crate::grpc::helpers::pagination_parameters;
use crate::grpc::{get_refresh_token, response_with_refresh_token};
use crate::models;
use crate::Error;
use diesel_async::scoped_futures::ScopedFutureExt;
use tonic::{Request, Response, Status};
use uuid::Uuid;

impl blockjoy_ui::Organization {
    pub fn from_model(model: models::Org) -> crate::Result<Self> {
        let (model, member_count) = (model.org, model.members);
        let org = Self {
            id: model.id.to_string(),
            name: model.name,
            personal: model.is_personal,
            member_count: member_count.try_into()?,
            created_at: Some(convert::try_dt_to_ts(model.created_at)?),
            updated_at: Some(convert::try_dt_to_ts(model.updated_at)?),
            current_user: None,
        };
        Ok(org)
    }
}

#[tonic::async_trait]
impl OrganizationService for super::GrpcImpl {
    async fn get(
        &self,
        request: Request<GetOrganizationsRequest>,
    ) -> Result<Response<GetOrganizationsResponse>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?.clone();
        let user_id = token.id;
        let inner = request.into_inner();
        let org_id = inner.org_id;

        let mut conn = self.conn().await?;
        let organizations: Vec<models::Org> = match org_id {
            Some(org_id) => {
                let org_id = org_id.parse().map_err(Error::UuidParseError)?;
                vec![models::Org::find_by_id(org_id, &mut conn).await?]
            }
            None => models::Org::find_all_by_user(user_id, &mut conn).await?,
        };
        let mut organizations = organizations
            .into_iter()
            .map(Organization::from_model)
            .collect::<crate::Result<Vec<_>>>()?;

        for org in &mut organizations {
            let org_id: Uuid = org.id.parse().map_err(Error::UuidParseError)?;
            let user = models::Org::find_org_user(user_id, org_id, &mut conn)
                .await?
                .into();
            org.current_user = Some(user);
        }

        let inner = GetOrganizationsResponse {
            meta: Some(ResponseMeta::from_meta(inner.meta, Some(token.try_into()?))),
            organizations,
        };

        response_with_refresh_token(refresh_token, inner)
    }

    async fn create(
        &self,
        request: Request<CreateOrganizationRequest>,
    ) -> Result<Response<CreateOrganizationResponse>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?.clone();
        let user_id = token.id;
        let inner = request.into_inner();
        let new_org = models::NewOrg {
            name: &inner.name,
            is_personal: false,
        };
        let (org, msg) = self
            .trx(|c| {
                async move {
                    let user = models::User::find_by_id(user_id, c).await?;
                    let org = new_org.create(user.id, c).await?;
                    let ui_org = blockjoy_ui::Organization::from_model(org.clone())?;
                    Ok((ui_org, blockjoy_ui::OrgMessage::created(org, user)?))
                }
                .scope_boxed()
            })
            .await?;
        self.notifier.ui_orgs_sender()?.send(&msg).await?;
        let response_meta =
            ResponseMeta::from_meta(inner.meta, Some(token.try_into()?)).with_message(&org.id);
        let inner = CreateOrganizationResponse {
            meta: Some(response_meta),
            organization: Some(org),
        };
        response_with_refresh_token(refresh_token, inner)
    }

    async fn update(
        &self,
        request: Request<UpdateOrganizationRequest>,
    ) -> Result<Response<UpdateOrganizationResponse>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?;
        let user_id = token.id;
        let token = token.try_into()?;
        let inner = request.into_inner();
        let org_id = inner.id.parse().map_err(crate::Error::from)?;
        let update = models::UpdateOrg {
            id: org_id,
            name: inner.name.as_deref(),
        };

        let msg = self
            .trx(|c| {
                async move {
                    let org = update.update(c).await?;
                    let user = models::User::find_by_id(user_id, c).await?;
                    blockjoy_ui::OrgMessage::updated(org, user)
                }
                .scope_boxed()
            })
            .await?;
        self.notifier.ui_orgs_sender()?.send(&msg).await?;
        let meta = ResponseMeta::from_meta(inner.meta, Some(token));
        let inner = UpdateOrganizationResponse { meta: Some(meta) };
        response_with_refresh_token(refresh_token, inner)
    }

    async fn delete(
        &self,
        request: Request<DeleteOrganizationRequest>,
    ) -> Result<Response<DeleteOrganizationResponse>, Status> {
        use models::OrgRole::*;

        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?.clone();
        let user_id = token.id;
        let inner = request.into_inner();
        let org_id = inner.id.parse().map_err(crate::Error::from)?;
        let msg = self
            .trx(|c| {
                async move {
                    let org = models::Org::find_by_id(org_id, c).await?;
                    if org.is_personal {
                        super::bail_unauthorized!("Can't deleted personal org");
                    }
                    let member = models::Org::find_org_user(user_id, org_id, c).await?;

                    // Only owner or admins may delete orgs
                    let is_allowed = match member.role {
                        Member => false,
                        Owner | Admin => true,
                    };
                    if !is_allowed {
                        super::bail_unauthorized!(
                            "User {user_id} has insufficient privileges to delete org {org_id}"
                        );
                    }
                    tracing::debug!("Deleting org: {}", org_id);
                    models::Org::delete(org_id, c).await?;
                    let user = models::User::find_by_id(user_id, c).await?;
                    Ok(blockjoy_ui::OrgMessage::deleted(org, user))
                }
                .scope_boxed()
            })
            .await?;
        self.notifier.ui_orgs_sender()?.send(&msg).await?;
        let meta = ResponseMeta::from_meta(inner.meta, Some(token.try_into()?));
        let resp = DeleteOrganizationResponse { meta: Some(meta) };
        response_with_refresh_token(refresh_token, resp)
    }

    async fn restore(
        &self,
        request: Request<RestoreOrganizationRequest>,
    ) -> Result<Response<RestoreOrganizationResponse>, Status> {
        use models::OrgRole::*;

        let token = try_get_token::<_, UserAuthToken>(&request)?.clone();
        let user_id = token.id;
        let inner = request.into_inner();
        let org_id = inner.id.parse().map_err(crate::Error::from)?;
        let resp = self
            .trx(|c| {
                async move {
                    let member = models::Org::find_org_user(user_id, org_id, c).await?;
                    let is_allowed = match member.role {
                        Member => false,
                        // Only owner or admins may restore orgs
                        Owner | Admin => true,
                    };
                    if !is_allowed {
                        super::bail_unauthorized!(
                            "User {user_id} has no sufficient privileges to restore org {org_id}"
                        );
                    }
                    let org = models::Org::restore(org_id, c).await?;
                    let meta = ResponseMeta::from_meta(inner.meta, Some(token.try_into()?));
                    let inner = RestoreOrganizationResponse {
                        meta: Some(meta),
                        organization: Some(blockjoy_ui::Organization::from_model(org)?),
                    };
                    Ok(inner)
                }
                .scope_boxed()
            })
            .await?;
        Ok(Response::new(resp))
    }

    async fn members(
        &self,
        request: Request<OrganizationMemberRequest>,
    ) -> Result<Response<OrganizationMemberResponse>, Status> {
        let token = try_get_token::<_, UserAuthToken>(&request)?.try_into()?;
        let refresh_token = get_refresh_token(&request);
        let inner = request.into_inner();
        let meta = inner.meta.ok_or_else(required("meta"))?;
        let org_id = inner.id.parse().map_err(crate::Error::from)?;

        let (limit, offset) = pagination_parameters(meta.pagination.clone())?;
        let mut conn = self.conn().await?;
        let users =
            models::Org::find_all_member_users_paginated(org_id, limit, offset, &mut conn).await?;
        let users: Result<_, Error> = users
            .into_iter()
            .map(blockjoy_ui::User::from_model)
            .collect();
        let inner = OrganizationMemberResponse {
            meta: Some(ResponseMeta::from_meta(meta, Some(token)).with_pagination()),
            users: users?,
        };

        response_with_refresh_token(refresh_token, inner)
    }

    async fn remove_member(
        &self,
        request: Request<RemoveMemberRequest>,
    ) -> Result<Response<()>, Status> {
        use models::OrgRole::*;

        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?;
        let caller_id = token.id;
        let inner = request.into_inner();
        let user_id = inner.user_id.parse().map_err(crate::Error::from)?;
        let org_id = inner.org_id.parse().map_err(crate::Error::from)?;
        let msg = self
            .trx(|c| {
                async move {
                    let member = models::Org::find_org_user(caller_id, org_id, c).await?;
                    let is_allowed = match member.role {
                        Member => false,
                        Owner | Admin => true,
                    };
                    if !is_allowed {
                        super::bail_unauthorized!(
                            "User {caller_id} has insufficient privileges to remove other user \
                            {user_id} from org {org_id}"
                        )
                    }
                    let user_to_remove = models::User::find_by_id(user_id, c).await?;
                    models::Org::remove_org_user(user_id, org_id, c).await?;
                    // In case a user needs to be re-invited later, we also remove the (already
                    // accepted) invites from the database. This is to prevent them from running
                    // into a unique constraint when they are invited again.
                    models::Invitation::remove_by_org_user(&user_to_remove.email, org_id, c)
                        .await?;
                    let org = models::Org::find_by_id(org_id, c).await?;
                    let user = models::User::find_by_id(caller_id, c).await?;
                    blockjoy_ui::OrgMessage::updated(org, user)
                }
                .scope_boxed()
            })
            .await?;
        self.notifier.ui_orgs_sender()?.send(&msg).await?;
        response_with_refresh_token(refresh_token, ())
    }

    async fn leave(
        &self,
        request: Request<LeaveOrganizationRequest>,
    ) -> Result<Response<()>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?;
        let user_id = token.id;
        let inner = request.into_inner();
        let org_id = inner.org_id.parse().map_err(crate::Error::from)?;
        let msg = self
            .trx(|c| {
                async move {
                    models::Org::remove_org_user(user_id, org_id, c).await?;
                    let org = models::Org::find_by_id(org_id, c).await?;
                    let user = models::User::find_by_id(user_id, c).await?;
                    blockjoy_ui::OrgMessage::updated(org, user)
                }
                .scope_boxed()
            })
            .await?;
        self.notifier.ui_orgs_sender()?.send(&msg).await?;

        response_with_refresh_token(refresh_token, ())
    }
}
