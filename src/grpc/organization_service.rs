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
use diesel_async::AsyncPgConnection;
use std::collections::HashMap;
use tonic::{Request, Response, Status};

impl blockjoy_ui::Organization {
    /// Converts a list of `Org` models into a list of `Organization` DTO's. We take care to perform
    /// O(1) queries, no matter the length of `models`. For this we need to find all users belonging
    /// to this each org.
    pub async fn from_models(
        models: Vec<models::Org>,
        current_user: Option<models::User>,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<Vec<Self>> {
        // We find all OrgUsers belonging to each model. This gives us a map from `org_id` to
        // `Vec<OrgUser>`.
        let org_users = models::OrgUser::by_orgs(&models, conn).await?;

        // Now we get the actual users for each `OrgUser`, because we also need to provide the name
        // and email of each user.
        let user_ids: Vec<uuid::Uuid> = org_users.values().flatten().map(|ou| ou.user_id).collect();
        let users: HashMap<uuid::Uuid, models::User> = models::User::find_by_ids(&user_ids, conn)
            .await?
            .into_iter()
            .map(|u| (u.id, u))
            .collect();

        // Finally we can loop over the models to construct the final list of messages we set out to
        // create.
        models
            .into_iter()
            .map(|model| {
                let org_users = &org_users[&model.id];
                let current_user = current_user.as_ref().and_then(|u| {
                    org_users
                        .iter()
                        .find(|ou| ou.user_id == u.id)
                        .map(|ou| blockjoy_ui::OrgUser {
                            user_id: ou.user_id.to_string(),
                            org_id: ou.org_id.to_string(),
                            role: ou.role as i32,
                            name: format!("{} {}", u.first_name, u.last_name),
                            email: u.email.clone(),
                        })
                });
                Ok(Self {
                    id: model.id.to_string(),
                    name: model.name.clone(),
                    personal: model.is_personal,
                    member_count: org_users.len().try_into()?,
                    created_at: Some(convert::try_dt_to_ts(model.created_at)?),
                    updated_at: Some(convert::try_dt_to_ts(model.updated_at)?),
                    members: org_users
                        .iter()
                        .map(|ou| {
                            let user = &users[&ou.user_id];
                            blockjoy_ui::OrgUser {
                                user_id: ou.user_id.to_string(),
                                org_id: ou.org_id.to_string(),
                                role: ou.role as i32,
                                name: format!("{} {}", user.first_name, user.last_name),
                                email: user.email.clone(),
                            }
                        })
                        .collect(),
                    current_user,
                })
            })
            .collect()
    }

    pub async fn from_model(
        model: models::Org,
        current_user: Option<models::User>,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<Self> {
        Ok(Self::from_models(vec![model], current_user, conn).await?[0].clone())
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
        let user = models::User::find_by_id(user_id, &mut conn).await?;
        let organizations = Organization::from_models(organizations, Some(user), &mut conn).await?;
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
        self.trx(|c| {
            async move {
                let user = models::User::find_by_id(user_id, c).await?;
                let org = new_org.create(user.id, c).await?;
                let msg = blockjoy_ui::OrgMessage::created(org.clone(), user, c).await?;
                let user = models::User::find_by_id(user_id, c).await?;
                let org = blockjoy_ui::Organization::from_model(org, Some(user), c).await?;
                self.notifier.ui_orgs_sender()?.send(&msg).await?;
                let response_meta = ResponseMeta::from_meta(inner.meta, Some(token.try_into()?))
                    .with_message(&org.id);
                let resp = CreateOrganizationResponse {
                    meta: Some(response_meta),
                    organization: Some(org),
                };
                Ok(response_with_refresh_token(refresh_token, resp)?)
            }
            .scope_boxed()
        })
        .await
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

        self.trx(|c| {
            async move {
                let org = update.update(c).await?;
                let user = models::User::find_by_id(user_id, c).await?;
                let msg = blockjoy_ui::OrgMessage::updated(org, user, c).await?;
                self.notifier.ui_orgs_sender()?.send(&msg).await?;
                let meta = ResponseMeta::from_meta(inner.meta, Some(token));
                let resp = UpdateOrganizationResponse { meta: Some(meta) };
                Ok(response_with_refresh_token(refresh_token, resp)?)
            }
            .scope_boxed()
        })
        .await
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
        self.trx(|c| {
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
                let msg = blockjoy_ui::OrgMessage::deleted(org, user);
                self.notifier.ui_orgs_sender()?.send(&msg).await?;
                let meta = ResponseMeta::from_meta(inner.meta, Some(token.try_into()?));
                let resp = DeleteOrganizationResponse { meta: Some(meta) };
                Ok(response_with_refresh_token(refresh_token, resp)?)
            }
            .scope_boxed()
        })
        .await
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
        self.trx(|c| {
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
                let user = models::User::find_by_id(user_id, c).await?;
                let meta = ResponseMeta::from_meta(inner.meta, Some(token.try_into()?));
                let resp = RestoreOrganizationResponse {
                    meta: Some(meta),
                    organization: Some(
                        blockjoy_ui::Organization::from_model(org, Some(user), c).await?,
                    ),
                };
                Ok(Response::new(resp))
            }
            .scope_boxed()
        })
        .await
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
        self.trx(|c| {
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
                models::Invitation::remove_by_org_user(&user_to_remove.email, org_id, c).await?;
                let org = models::Org::find_by_id(org_id, c).await?;
                let user = models::User::find_by_id(caller_id, c).await?;
                let msg = blockjoy_ui::OrgMessage::updated(org, user, c).await?;
                self.notifier.ui_orgs_sender()?.send(&msg).await?;
                Ok(response_with_refresh_token(refresh_token, ())?)
            }
            .scope_boxed()
        })
        .await
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
        self.trx(|c| {
            async move {
                models::Org::remove_org_user(user_id, org_id, c).await?;
                let org = models::Org::find_by_id(org_id, c).await?;
                let user = models::User::find_by_id(user_id, c).await?;
                let msg = blockjoy_ui::OrgMessage::updated(org, user, c).await?;
                self.notifier.ui_orgs_sender()?.send(&msg).await?;
                Ok(response_with_refresh_token(refresh_token, ())?)
            }
            .scope_boxed()
        })
        .await
    }
}
