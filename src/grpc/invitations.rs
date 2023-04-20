use super::api::{self, invitations_server};
use super::helpers::try_get_token;
use crate::auth::{FindableById, InvitationToken, JwtToken, UserAuthToken};
use crate::mail;
use crate::models;
use diesel_async::scoped_futures::ScopedFutureExt;
use diesel_async::AsyncPgConnection;
use tonic::{Request, Status};

#[tonic::async_trait]
impl invitations_server::Invitations for super::GrpcImpl {
    async fn create(
        &self,
        request: Request<api::CreateInvitationRequest>,
    ) -> super::Result<api::CreateInvitationResponse> {
        let token = try_get_token::<_, UserAuthToken>(&request)?;
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
        let token = try_get_token::<_, UserAuthToken>(&request)?;
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

    async fn accept(
        &self,
        request: Request<api::AcceptInvitationRequest>,
    ) -> super::Result<api::AcceptInvitationResponse> {
        let refresh_token = super::get_refresh_token(&request);
        self.trx(|c| {
            async move {
                let auth_token = try_get_token::<_, UserAuthToken>(&request).ok().cloned();
                let inv_token = try_get_token::<_, InvitationToken>(&request).ok().cloned();
                let invitation_id = request.into_inner().invitation_id.parse()?;
                let invitation = authorized_invite(invitation_id, inv_token, auth_token, c).await?;
                if invitation.accepted_at.is_some() {
                    return Err(Status::failed_precondition("Invitation already accepted").into());
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
                let resp = api::AcceptInvitationResponse {};
                Ok(super::response_with_refresh_token(refresh_token, resp)?)
            }
            .scope_boxed()
        })
        .await
    }

    async fn decline(
        &self,
        request: Request<api::DeclineInvitationRequest>,
    ) -> super::Result<api::DeclineInvitationResponse> {
        let refresh_token = super::get_refresh_token(&request);
        let auth_token = try_get_token::<_, UserAuthToken>(&request).ok().cloned();
        let inv_token = try_get_token::<_, InvitationToken>(&request).ok().cloned();
        self.trx(|c| {
            async move {
                let invitation_id = request.into_inner().invitation_id.parse()?;
                let invitation = authorized_invite(invitation_id, inv_token, auth_token, c).await?;
                if invitation.accepted_at.is_some() {
                    return Err(Status::failed_precondition("Invite is accepted").into());
                }
                if invitation.declined_at.is_some() {
                    return Err(Status::failed_precondition("Invite already declined").into());
                }

                invitation.decline(c).await?;

                let resp = api::DeclineInvitationResponse {};
                Ok(super::response_with_refresh_token(refresh_token, resp)?)
            }
            .scope_boxed()
        })
        .await
    }

    async fn revoke(
        &self,
        request: Request<api::RevokeInvitationRequest>,
    ) -> super::Result<api::RevokeInvitationResponse> {
        let refresh_token = super::get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?;
        let user_id = token.id;
        let request = request.into_inner();
        self.trx(|c| {
            async move {
                let invitation_id = request.invitation_id.parse()?;
                let invitation = models::Invitation::find_by_id(invitation_id, c).await?;

                // Our checks. We check that the invite hasn't already been used and that the user
                // is actually in the organization that the invite is for.
                if invitation.accepted_at.is_some() {
                    return Err(Status::failed_precondition("Invite is accepted").into());
                }
                if invitation.declined_at.is_some() {
                    return Err(Status::failed_precondition("Invite is declined").into());
                }
                if !models::Org::is_member(user_id, invitation.created_for_org, c).await? {
                    super::bail_unauthorized!("User not in org");
                }

                invitation.revoke(c).await
            }
            .scope_boxed()
        })
        .await?;

        let resp = api::RevokeInvitationResponse {};
        super::response_with_refresh_token(refresh_token, resp)
    }
}

/// Given an invite id and the two possible auth tokens, return either the invite from the database
/// if the user is allowed to use this invite, or an error if access is denied.
async fn authorized_invite(
    invitation_id: uuid::Uuid,
    inv_token: Option<InvitationToken>,
    auth_token: Option<UserAuthToken>,
    conn: &mut AsyncPgConnection,
) -> crate::Result<models::Invitation> {
    let invite = models::Invitation::find_by_id(invitation_id, conn).await?;
    let is_allowed = match (inv_token, auth_token) {
        // If we get an invite token, we just need to check that it was created for the current
        // invite.
        (Some(inv_token), _) => inv_token.id == invite.id,
        // We are taking the invitation id from the request. That means that we need to validate
        // that the currently logged in user is actually the user that was invited.
        (_, Some(auth_token)) => {
            let invitee = models::User::find_by_id(auth_token.id, conn).await?;
            invitee.email != invite.invitee_email
        }
        _ => return Err(crate::Error::unexpected("need auth or invite token")),
    };
    if !is_allowed {
        super::bail_unauthorized!("Not the invited user");
    }
    Ok(invite)
}

impl api::CreateInvitationRequest {
    pub async fn as_new(
        &self,
        created_by_user: uuid::Uuid,
        conn: &mut diesel_async::AsyncPgConnection,
    ) -> crate::Result<models::NewInvitation<'_>> {
        let creator = models::User::find_by_id(created_by_user, conn).await?;
        let org_id = self.org_id.parse()?;
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
        let mut invitation = Self {
            id: model.id.to_string(),
            created_by: model.created_by_user.to_string(),
            created_by_name: model.created_by_user_name,
            org_id: model.created_for_org.to_string(),
            org_name: model.created_for_org_name,
            invitee_email: model.invitee_email,
            created_at: Some(super::try_dt_to_ts(model.created_at)?),
            status: 0, // We use the setter to set this field for type-safety
            accepted_at: model.accepted_at.map(super::try_dt_to_ts).transpose()?,
            declined_at: model.declined_at.map(super::try_dt_to_ts).transpose()?,
        };
        let status = match (model.accepted_at, model.declined_at) {
            (None, None) => api::InvitationStatus::Open,
            (Some(_), None) => api::InvitationStatus::Accepted,
            (None, Some(_)) => api::InvitationStatus::Declined,
            (Some(_), Some(_)) => api::InvitationStatus::Unspecified,
        };
        invitation.set_status(status);
        Ok(invitation)
    }
}
