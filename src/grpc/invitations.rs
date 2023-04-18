use super::api::{self, invitations_server};
use super::helpers;
use crate::auth::{self, FindableById, JwtToken};
use crate::mail;
use crate::models;
use diesel_async::scoped_futures::ScopedFutureExt;
use tonic::{Request, Status};
use uuid::Uuid;

#[tonic::async_trait]
impl invitations_server::Invitations for super::GrpcImpl {
    async fn create(
        &self,
        request: Request<api::CreateInvitationRequest>,
    ) -> super::Result<api::CreateInvitationResponse> {
        let token = helpers::try_get_token::<_, auth::UserAuthToken>(&request)?;
        let refresh_token = super::get_refresh_token(&request);
        let creator_id = token.get_id();

        self.trx(|c| {
            async move {
                let creator = models::User::find_by_id(creator_id, c).await?;
                let request = request.into_inner();
                let invitation = request.as_new(creator_id, c).await?.create(c).await?;

                match models::User::find_by_email(&invitation.invitee_email, c).await {
                    Ok(user) => {
                        // Note that here we abort the transaction if sending the email failed.
                        // This way we do not get users in the db that we cannot send emails to.
                        // The existence of such a user would prevent them from trying to recreate
                        // again at a later point.
                        mail::MailClient::new()
                            .invitation_for_registered(&creator, &user, "1 week")
                            .await?;
                    }
                    Err(_) => {
                        let invitee = mail::Recipient {
                            email: &invitation.invitee_email,
                            first_name: "",
                            last_name: "",
                            preferred_language: None,
                        };

                        mail::MailClient::new()
                            .invitation(&invitation, &creator, invitee, "1 week")
                            .await?;
                    }
                }

                let response = api::CreateInvitationResponse {};
                Ok(super::response_with_refresh_token(refresh_token, response)?)
            }
            .scope_boxed()
        })
        .await
    }

    /// TODO: Role should be checked by policies
    async fn list_pending(
        &self,
        request: Request<api::ListPendingInvitationRequest>,
    ) -> super::Result<api::InvitationsResponse> {
        let token = helpers::try_get_token::<_, auth::UserAuthToken>(&request)?;
        let refresh_token = super::get_refresh_token(&request);
        let user_id = token.get_id();
        let inner = request.into_inner();
        let org_id = inner.org_id.parse().map_err(crate::Error::from)?;
        let mut conn = self.conn().await?;
        let org_user = models::Org::find_org_user(user_id, org_id, &mut conn).await?;

        let is_allowed = match org_user.role {
            models::OrgRole::Member => false,
            models::OrgRole::Admin | models::OrgRole::Owner => true,
        };
        if !is_allowed {
            super::bail_unauthorized!(
                "User {user_id} is not allowed to list pending invitations on org {org_id}"
            );
        }
        let invitations = models::Invitation::pending(org_id, &mut conn)
            .await?
            .into_iter()
            .map(api::Invitation::from_model)
            .collect::<crate::Result<_>>()?;

        let response = api::InvitationsResponse { invitations };

        super::response_with_refresh_token(refresh_token, response)
    }

    async fn list_received(
        &self,
        request: Request<api::ListReceivedInvitationRequest>,
    ) -> super::Result<api::InvitationsResponse> {
        let refresh_token = super::get_refresh_token(&request);
        let request = request.into_inner();
        let user_id = request.user_id.parse().map_err(crate::Error::from)?;
        let mut conn = self.conn().await?;
        let user = models::User::find_by_id(user_id, &mut conn).await?;
        let invitations = models::Invitation::received(&user.email, &mut conn)
            .await?
            .into_iter()
            .map(api::Invitation::from_model)
            .collect::<crate::Result<_>>()?;
        let response = api::InvitationsResponse { invitations };

        super::response_with_refresh_token(refresh_token, response)
    }

    async fn accept(&self, request: Request<api::InvitationRequest>) -> super::Result<()> {
        let (refresh_token, invitation_id) = get_refresh_token_invitation_id_from_request(request)?;
        self.trx(|c| {
            async move {
                let invitation = models::Invitation::find_by_id(invitation_id, c).await?;
                if invitation.accepted_at.is_some() {
                    return Err(
                        Status::failed_precondition("Invitation is already accepted").into(),
                    );
                }
                if invitation.declined_at.is_some() {
                    return Err(Status::failed_precondition("Invitation is declined").into());
                }

                let invitation = invitation.accept(c).await?;
                // Only registered users can accept an invitation
                let new_member = models::User::find_by_email(&invitation.invitee_email, c).await?;
                let org_user = models::Org::add_member(
                    new_member.id,
                    invitation.created_for_org,
                    models::OrgRole::Member,
                    c,
                )
                .await?;
                let org = models::Org::find_by_id(org_user.org_id, c).await?;
                let user = models::User::find_by_id(org_user.user_id, c).await?;
                let msg = api::OrgMessage::updated(org, user, c).await?;
                self.notifier.orgs_sender().send(&msg).await?;
                Ok(super::response_with_refresh_token(refresh_token, ())?)
            }
            .scope_boxed()
        })
        .await
    }

    async fn decline(&self, request: Request<api::InvitationRequest>) -> super::Result<()> {
        let (refresh_token, invitation_id) = get_refresh_token_invitation_id_from_request(request)?;
        self.trx(|c| {
            async move {
                let invitation = models::Invitation::find_by_id(invitation_id, c).await?;
                if invitation.accepted_at.is_some() {
                    return Err(Status::failed_precondition("Invitation is accepted").into());
                }
                if invitation.declined_at.is_some() {
                    return Err(
                        Status::failed_precondition("Invitation is already declined").into(),
                    );
                }
                invitation.decline(c).await
            }
            .scope_boxed()
        })
        .await?;

        super::response_with_refresh_token(refresh_token, ())
    }

    async fn revoke(&self, request: Request<api::InvitationRequest>) -> super::Result<()> {
        let refresh_token = super::get_refresh_token(&request);
        let token = helpers::try_get_token::<_, auth::UserAuthToken>(&request)?;
        let user_id = token.id;
        let grpc_invitation = request
            .into_inner()
            .invitation
            .ok_or_else(|| Status::invalid_argument("invitation missing"))?;
        self.trx(|c| {
            async move {
                let invitation = models::Invitation::find_by_creator_for_email(
                    user_id,
                    &grpc_invitation.invitee_email,
                    c,
                )
                .await?;

                if invitation.accepted_at.is_some() {
                    return Err(Status::failed_precondition("Invitation is accepted").into());
                }
                if invitation.declined_at.is_some() {
                    return Err(Status::failed_precondition("Invitation is declined").into());
                }

                // Check if user belongs to org, the role is already checked by the auth middleware
                models::Org::find_org_user(
                    invitation.created_by_user,
                    invitation.created_for_org,
                    c,
                )
                .await?;

                invitation.revoke(c).await
            }
            .scope_boxed()
        })
        .await?;

        super::response_with_refresh_token(refresh_token, ())
    }
}

fn get_refresh_token_invitation_id_from_request(
    request: Request<api::InvitationRequest>,
) -> Result<(Option<String>, Uuid), tonic::Status> {
    let refresh_token = super::get_refresh_token(&request);
    let invitation_id = match helpers::try_get_token::<_, auth::InvitationToken>(&request) {
        Ok(token) => token.id,
        Err(_) => request
            .into_inner()
            .invitation
            .ok_or_else(helpers::required("invitation"))?
            .id
            .parse()
            .map_err(crate::Error::from)?,
    };

    Ok((refresh_token, invitation_id))
}

impl api::CreateInvitationRequest {
    pub async fn as_new(
        &self,
        created_by_user: uuid::Uuid,
        conn: &mut diesel_async::AsyncPgConnection,
    ) -> crate::Result<models::NewInvitation<'_>> {
        let creator = models::User::find_by_id(created_by_user, conn).await?;
        let org_id = self.created_for_org_id.parse()?;
        let for_org = models::Org::find_by_id(org_id, conn).await?;

        let name = format!(
            "{} {} ({})",
            creator.first_name, creator.last_name, creator.email
        );
        Ok(models::NewInvitation {
            created_by_user,
            created_by_user_name: name,
            created_for_org: for_org.id,
            created_for_org_name: for_org.name,
            invitee_email: &self.invitee_email,
        })
    }
}

impl api::Invitation {
    fn from_model(model: models::Invitation) -> crate::Result<Self> {
        Ok(Self {
            id: model.id.to_string(),
            created_by_id: model.created_by_user.to_string(),
            created_by_user_name: model.created_by_user_name,
            created_for_org_id: model.created_for_org.to_string(),
            created_for_org_name: model.created_for_org_name,
            invitee_email: model.invitee_email,
            created_at: Some(super::try_dt_to_ts(model.created_at)?),
            accepted_at: model.accepted_at.map(super::try_dt_to_ts).transpose()?,
            declined_at: model.declined_at.map(super::try_dt_to_ts).transpose()?,
        })
    }
}
