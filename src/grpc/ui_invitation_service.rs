use crate::auth::{FindableById, InvitationToken, JwtToken, UserAuthToken};
use crate::errors::{ApiError, Result};
use crate::grpc::blockjoy_ui::invitation_service_server::InvitationService;
use crate::grpc::blockjoy_ui::{
    CreateInvitationRequest, CreateInvitationResponse, Invitation as GrpcInvitation,
    InvitationRequest, InvitationsResponse, ListPendingInvitationRequest,
    ListReceivedInvitationRequest, ResponseMeta,
};
use crate::grpc::helpers::try_get_token;
use crate::grpc::{get_refresh_token, response_with_refresh_token};
use crate::mail::MailClient;
use crate::models;
use crate::models::{Invitation, Org, OrgRole, User};
use tonic::{Request, Response, Status};
use uuid::Uuid;

pub struct InvitationServiceImpl {
    db: models::DbPool,
}

impl InvitationServiceImpl {
    pub fn new(db: models::DbPool) -> Self {
        Self { db }
    }

    fn get_refresh_token_invitation_id_from_request(
        request: Request<InvitationRequest>,
    ) -> Result<(Option<String>, Uuid), Status> {
        let refresh_token = get_refresh_token(&request);
        let invitation_id = match try_get_token::<_, InvitationToken>(&request) {
            Ok(token) => {
                tracing::debug!("Found invitation token");

                token.id
            }
            Err(_) => {
                tracing::debug!("No invitation token available, trying user auth token");

                let inner = request.into_inner();
                let invitation_id = inner
                    .invitation
                    .ok_or_else(|| Status::permission_denied("No valid invitation found"))?
                    .id
                    .ok_or_else(|| Status::permission_denied("No valid invitation ID found"))?;

                Uuid::parse_str(invitation_id.as_str()).map_err(ApiError::from)?
            }
        };

        Ok((refresh_token, invitation_id))
    }
}

#[tonic::async_trait]
impl InvitationService for InvitationServiceImpl {
    async fn create(
        &self,
        request: Request<CreateInvitationRequest>,
    ) -> Result<Response<CreateInvitationResponse>, Status> {
        let token = try_get_token::<_, UserAuthToken>(&request)?.try_into()?;
        let refresh_token = get_refresh_token(&request);
        let creator_id = try_get_token::<_, UserAuthToken>(&request)?.get_id();
        let mut tx = self.db.begin().await?;
        let creator = User::find_by_id(creator_id, &mut tx).await?;
        let inner = request.into_inner();
        let invitation = GrpcInvitation {
            id: None,
            created_by_id: Some(creator_id.to_string()),
            created_for_org_id: Some(inner.created_for_org_id),
            invitee_email: Some(inner.invitee_email),
            created_at: None,
            accepted_at: None,
            declined_at: None,
            created_by_user_name: None,
            created_for_org_name: None,
        };

        let db_invitation = Invitation::create(&invitation, &mut tx).await?;

        let response_meta = ResponseMeta::from_meta(inner.meta, Some(token));
        let response = CreateInvitationResponse {
            meta: Some(response_meta),
        };

        match User::find_by_email(&db_invitation.invitee_email, &mut tx).await {
            Ok(user) => {
                // Note that here we abort the transaction if sending the email failed. This way we
                // do not get users in the db that we cannot send emails to. The existence of such
                // a user would prevent them from trying to recreate again at a later point.
                MailClient::new()
                    .invitation_for_registered(&creator, &user, "1 week".to_string())
                    .await?
            }
            Err(_) => {
                let invitee = User {
                    id: Default::default(),
                    email: db_invitation.invitee_email.clone(),
                    first_name: "".to_string(),
                    last_name: "".to_string(),
                    hashword: "".to_string(),
                    salt: "".to_string(),
                    refresh: None,
                    fee_bps: 0,
                    staking_quota: 0,
                    created_at: Default::default(),
                    confirmed_at: None,
                };

                MailClient::new()
                    .invitation(&db_invitation, &creator, &invitee, "1 week".to_string())
                    .await?
            }
        }
        tx.commit().await?;

        Ok(response_with_refresh_token(refresh_token, response)?)
    }

    /// TODO: Role should be checked by policies
    async fn list_pending(
        &self,
        request: Request<ListPendingInvitationRequest>,
    ) -> Result<Response<InvitationsResponse>, Status> {
        let token = try_get_token::<_, UserAuthToken>(&request)?.try_into()?;
        let refresh_token = get_refresh_token(&request);
        let user_id = try_get_token::<_, UserAuthToken>(&request)?.get_id();
        let inner = request.into_inner();
        let org_id = inner.org_id.parse().map_err(ApiError::from)?;
        let mut conn = self.db.conn().await?;
        let org_user = Org::find_org_user(user_id, org_id, &mut conn).await?;

        match org_user.role {
            OrgRole::Member => Err(Status::permission_denied(format!(
                "User {user_id} is not allowed to list pending invitations on org {org_id}"
            ))),
            OrgRole::Admin | OrgRole::Owner => {
                let invitations = Invitation::pending(org_id, &mut conn)
                    .await?
                    .into_iter()
                    .map(|i| i.try_into())
                    .collect::<Result<_>>()?;

                let response_meta = ResponseMeta::from_meta(inner.meta, Some(token));
                let response = InvitationsResponse {
                    meta: Some(response_meta),
                    invitations,
                };

                Ok(response_with_refresh_token(refresh_token, response)?)
            }
        }
    }

    async fn list_received(
        &self,
        request: Request<ListReceivedInvitationRequest>,
    ) -> Result<Response<InvitationsResponse>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?.clone();
        let mut conn = self.db.conn().await?;
        let user = User::find_by_id(token.get_id(), &mut conn).await?;
        let inner = request.into_inner();
        let invitations = Invitation::received(&user.email, &mut conn)
            .await?
            .into_iter()
            .map(|i| i.try_into())
            .collect::<Result<_>>()?;
        let response_meta = ResponseMeta::from_meta(inner.meta, Some(token.try_into()?));
        let response = InvitationsResponse {
            meta: Some(response_meta),
            invitations,
        };

        Ok(response_with_refresh_token(refresh_token, response)?)
    }

    async fn accept(&self, request: Request<InvitationRequest>) -> Result<Response<()>, Status> {
        let (refresh_token, invitation_id) =
            InvitationServiceImpl::get_refresh_token_invitation_id_from_request(request)?;
        let mut tx = self.db.begin().await?;
        let invitation = Invitation::accept(invitation_id, &mut tx).await?;
        // Only registered users can accept an invitation
        let new_member = User::find_by_email(&invitation.invitee_email, &mut tx).await?;

        Org::add_member(
            new_member.id,
            invitation.created_for_org,
            OrgRole::Member,
            &mut tx,
        )
        .await?;
        tx.commit().await?;

        Ok(response_with_refresh_token::<()>(refresh_token, ())?)
    }

    async fn decline(&self, request: Request<InvitationRequest>) -> Result<Response<()>, Status> {
        let (refresh_token, invitation_id) =
            InvitationServiceImpl::get_refresh_token_invitation_id_from_request(request)?;

        let mut tx = self.db.begin().await?;
        Invitation::decline(invitation_id, &mut tx).await?;
        tx.commit().await?;

        Ok(response_with_refresh_token::<()>(refresh_token, ())?)
    }

    async fn revoke(&self, request: Request<InvitationRequest>) -> Result<Response<()>, Status> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?;
        let user_id = token.id;
        let grpc_invitation = request
            .into_inner()
            .invitation
            .ok_or_else(|| Status::invalid_argument("invitation missing"))?;
        let invitee_email = grpc_invitation
            .invitee_email
            .ok_or_else(|| Status::invalid_argument("invitee email missing"))?;
        let mut tx = self.db.begin().await?;
        let invitation =
            Invitation::find_by_creator_for_email(user_id, invitee_email, &mut tx).await?;

        // Check if user belongs to org, the role is already checked by the auth middleware
        Org::find_org_user(
            invitation.created_by_user,
            invitation.created_for_org,
            &mut tx,
        )
        .await?;

        Invitation::revoke(invitation.id, &mut tx).await?;
        tx.commit().await?;

        Ok(response_with_refresh_token::<()>(refresh_token, ())?)
    }
}
