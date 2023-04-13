use super::api::{self, orgs_server};
use super::convert;
use super::helpers;
use crate::auth::{FindableById, UserAuthToken};
use crate::models;
use diesel_async::scoped_futures::ScopedFutureExt;
use tonic::{Request, Response, Status};

impl api::Org {
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
impl orgs_server::Orgs for super::GrpcImpl {
    async fn get(
        &self,
        request: Request<api::GetOrgRequest>,
    ) -> super::Result<api::GetOrgResponse> {
        let refresh_token = super::get_refresh_token(&request);
        let token = helpers::try_get_token::<_, UserAuthToken>(&request)?;
        let request = request.into_inner();
        let mut conn = self.db.conn().await?;
        let org_id = request.org_id.parse().map_err(crate::Error::from)?;
        let org = models::Org::find_by_id(org_id, &mut conn).await?;
        let org = api::Org::from_model(org)?;
        let resp = api::GetOrgResponse { org: Some(org) };
        super::response_with_refresh_token(refresh_token, resp)
    }

    async fn list(
        &self,
        request: Request<api::ListOrgsRequest>,
    ) -> super::Result<api::ListOrgsResponse> {
        let refresh_token = super::get_refresh_token(&request);
        let token = helpers::try_get_token::<_, UserAuthToken>(&request)?;
        let request = request.into_inner();

        let mut conn = self.db.conn().await?;
        let member_id = request
            .member_id
            .map(|id| id.parse())
            .transpose()
            .map_err(crate::Error::from)?;
        let orgs = models::Org::filter(member_id, &mut conn).await?;
        let orgs = orgs
            .into_iter()
            .map(api::Org::from_model)
            .collect::<crate::Result<Vec<_>>>()?;
        let resp = api::ListOrgsResponse { orgs };
        super::response_with_refresh_token(refresh_token, resp)
    }

    async fn create(
        &self,
        request: Request<api::CreateOrgRequest>,
    ) -> super::Result<api::CreateOrgResponse> {
        let refresh_token = super::get_refresh_token(&request);
        let token = helpers::try_get_token::<_, UserAuthToken>(&request)?;
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
                let msg = api::OrgMessage::created(org.clone(), user)?;
                let org = api::Org::from_model(org)?;
                self.notifier.orgs_sender().send(&msg).await?;
                let resp = api::CreateOrgResponse { org: Some(org) };
                Ok(super::response_with_refresh_token(refresh_token, resp)?)
            }
            .scope_boxed()
        })
        .await
    }

    async fn update(
        &self,
        request: Request<api::UpdateOrgRequest>,
    ) -> super::Result<api::UpdateOrgResponse> {
        let refresh_token = super::get_refresh_token(&request);
        let token = helpers::try_get_token::<_, UserAuthToken>(&request)?;
        let user_id = token.id;
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
                let msg = api::OrgMessage::updated(org, user)?;
                self.notifier.orgs_sender().send(&msg).await?;
                let resp = api::UpdateOrgResponse {};
                Ok(super::response_with_refresh_token(refresh_token, resp)?)
            }
            .scope_boxed()
        })
        .await
    }

    async fn delete(
        &self,
        request: Request<api::DeleteOrgRequest>,
    ) -> super::Result<api::DeleteOrgResponse> {
        use models::OrgRole::*;

        let refresh_token = super::get_refresh_token(&request);
        let token = helpers::try_get_token::<_, UserAuthToken>(&request)?;
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
                let msg = api::OrgMessage::deleted(org, user);
                self.notifier.orgs_sender().send(&msg).await?;
                let resp = api::DeleteOrgResponse {};
                Ok(super::response_with_refresh_token(refresh_token, resp)?)
            }
            .scope_boxed()
        })
        .await
    }

    async fn restore(
        &self,
        request: Request<api::RestoreOrgRequest>,
    ) -> super::Result<api::RestoreOrgResponse> {
        use models::OrgRole::*;

        let token = helpers::try_get_token::<_, UserAuthToken>(&request)?;
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
                let resp = api::RestoreOrgResponse {
                    org: Some(api::Org::from_model(org)?),
                };
                Ok(Response::new(resp))
            }
            .scope_boxed()
        })
        .await
    }

    async fn members(
        &self,
        request: Request<api::OrgMemberRequest>,
    ) -> super::Result<api::OrgMemberResponse> {
        let token = helpers::try_get_token::<_, UserAuthToken>(&request)?;
        let refresh_token = super::get_refresh_token(&request);
        let request = request.into_inner();
        let org_id = request.id.parse().map_err(crate::Error::from)?;
        let mut conn = self.conn().await?;
        let users = models::Org::find_all_member_users(org_id, &mut conn).await?;
        let users: crate::Result<_> = users.into_iter().map(api::User::from_model).collect();
        let response = api::OrgMemberResponse { users: users? };
        super::response_with_refresh_token(refresh_token, response)
    }

    async fn remove_member(
        &self,
        request: Request<api::RemoveMemberRequest>,
    ) -> Result<Response<()>, Status> {
        use models::OrgRole::*;

        let refresh_token = super::get_refresh_token(&request);
        let token = helpers::try_get_token::<_, UserAuthToken>(&request)?;
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
                let msg = api::OrgMessage::updated(org, user)?;
                self.notifier.orgs_sender().send(&msg).await?;
                Ok(super::response_with_refresh_token(refresh_token, ())?)
            }
            .scope_boxed()
        })
        .await
    }

    async fn leave(&self, request: Request<api::LeaveOrgRequest>) -> Result<Response<()>, Status> {
        let refresh_token = super::get_refresh_token(&request);
        let token = helpers::try_get_token::<_, UserAuthToken>(&request)?;
        let user_id = token.id;
        let inner = request.into_inner();
        let org_id = inner.org_id.parse().map_err(crate::Error::from)?;
        self.trx(|c| {
            async move {
                models::Org::remove_org_user(user_id, org_id, c).await?;
                let org = models::Org::find_by_id(org_id, c).await?;
                let user = models::User::find_by_id(user_id, c).await?;
                let msg = api::OrgMessage::updated(org, user)?;
                self.notifier.orgs_sender().send(&msg).await?;
                Ok(super::response_with_refresh_token(refresh_token, ())?)
            }
            .scope_boxed()
        })
        .await
    }
}
