use std::collections::HashMap;

use diesel_async::scoped_futures::ScopedFutureExt;
use tonic::{Request, Status};

use crate::auth::endpoint::Endpoint;
use crate::auth::resource::{Resource, ResourceEntry, UserId};
use crate::database::{Conn, ReadConn, Transaction, WriteConn};
use crate::mail;
use crate::models::invitation::{Invitation, InvitationFilter, NewInvitation};
use crate::models::org::{Org, OrgRole};
use crate::models::user::User;
use crate::timestamp::NanosUtc;

use super::api::{self, invitation_service_server};
use super::helpers::required;

#[tonic::async_trait]
impl invitation_service_server::InvitationService for super::Grpc {
    async fn create(
        &self,
        req: Request<api::InvitationServiceCreateRequest>,
    ) -> super::Resp<api::InvitationServiceCreateResponse> {
        self.write(|write| create(req, write).scope_boxed()).await
    }

    async fn list(
        &self,
        req: Request<api::InvitationServiceListRequest>,
    ) -> super::Resp<api::InvitationServiceListResponse> {
        self.read(|read| list(req, read).scope_boxed()).await
    }

    async fn accept(
        &self,
        req: Request<api::InvitationServiceAcceptRequest>,
    ) -> super::Resp<api::InvitationServiceAcceptResponse> {
        self.write(|write| accept(req, write).scope_boxed()).await
    }

    async fn decline(
        &self,
        req: Request<api::InvitationServiceDeclineRequest>,
    ) -> super::Resp<api::InvitationServiceDeclineResponse> {
        self.write(|write| decline(req, write).scope_boxed()).await
    }

    async fn revoke(
        &self,
        req: Request<api::InvitationServiceRevokeRequest>,
    ) -> super::Resp<api::InvitationServiceRevokeResponse> {
        self.write(|write| revoke(req, write).scope_boxed()).await
    }
}

async fn create(
    req: Request<api::InvitationServiceCreateRequest>,
    write: WriteConn<'_, '_>,
) -> super::Result<api::InvitationServiceCreateResponse> {
    let WriteConn { conn, ctx, mqtt_tx } = write;
    let claims = ctx.claims(&req, Endpoint::InvitationCreate, conn).await?;
    let req = req.into_inner();
    // In principle, it is allowed for resources other than a user to create an invite, but we
    // currently include a field `created_by_user` with a created invite.
    let (is_allowed, caller) = match claims.resource() {
        Resource::User(user_id) => {
            let caller = User::find_by_id(user_id, conn).await?;
            let is_allowed = Org::is_admin(caller.id, req.org_id.parse()?, conn).await?;
            (is_allowed, caller)
        }
        Resource::Org(_) => todo!(),
        Resource::Host(_) => todo!(),
        Resource::Node(_) => todo!(),
    };
    if !is_allowed {
        super::forbidden!("Access denied for invitations create");
    }

    let org_id = req.org_id.parse()?;
    if Invitation::has_open_invite(org_id, &req.invitee_email, conn).await? {
        super::forbidden!("User is already invited");
    }

    // Check if the user-to-invite is not already a member of the organization
    let invited_user = User::find_by_email(&req.invitee_email, conn).await;
    if let Ok(invited_user) = &invited_user {
        if Org::is_member(invited_user.id, org_id, conn).await? {
            super::forbidden!("Already a member");
        }
    }

    let invitation = req.into_new(caller.id)?.create(conn).await?;

    match invited_user {
        Ok(user) => {
            // Note that here we abort the transaction if sending the email failed. This way we do
            // not get invites in the db that we cannot send emails to. The existence of such an
            // invite would prevent them from trying to recreate again at a later point.
            ctx.mail
                .invitation_for_registered(&caller, &user, "1 week")
                .await?;
        }
        Err(_) => {
            let invitee = mail::Recipient {
                email: &invitation.invitee_email,
                first_name: "",
                last_name: "",
                preferred_language: None,
            };

            ctx.mail
                .invitation(&invitation, &caller, invitee, "1 week")
                .await?;
        }
    }

    let org = Org::find_by_id(invitation.org_id, conn).await?;
    let msg = api::OrgMessage::invitation_created(org, invitation.clone(), conn).await?;
    let resp = api::InvitationServiceCreateResponse {
        invitation: Some(api::Invitation::from_model(invitation, conn).await?),
    };

    mqtt_tx.send(msg.into()).expect("mqtt_rx");

    Ok(tonic::Response::new(resp))
}

async fn list(
    req: Request<api::InvitationServiceListRequest>,
    read: ReadConn<'_, '_>,
) -> super::Result<api::InvitationServiceListResponse> {
    let ReadConn { conn, ctx } = read;
    let claims = ctx.claims(&req, Endpoint::InvitationList, conn).await?;
    let req = req.into_inner();

    let entry = if let Some(org_id) = &req.org_id {
        ResourceEntry::new_org(org_id.parse()?)
    } else if let Some(user_id) = &req.created_by {
        ResourceEntry::new_user(user_id.parse()?)
    } else if let Some(invitee_email) = &req.invitee_email {
        let user = User::find_by_email(invitee_email, conn).await?;
        ResourceEntry::new_user(user.id)
    } else {
        super::forbidden!("Access denied for invitations list");
    };

    let _ = claims.ensure(entry.into(), conn).await?;

    let filter = req.as_filter()?;
    let invitations = Invitation::filter(filter, conn).await?;
    let invitations = api::Invitation::from_models(invitations, conn).await?;
    let resp = api::InvitationServiceListResponse { invitations };
    Ok(tonic::Response::new(resp))
}

async fn accept(
    req: Request<api::InvitationServiceAcceptRequest>,
    write: WriteConn<'_, '_>,
) -> super::Result<api::InvitationServiceAcceptResponse> {
    let WriteConn { conn, ctx, mqtt_tx } = write;
    let claims = ctx.claims(&req, Endpoint::InvitationAccept, conn).await?;
    let req = req.into_inner();
    let invitation_id = req.invitation_id.parse()?;
    let invitation = Invitation::find_by_id(invitation_id, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => {
            let user = User::find_by_id(user_id, conn).await?;
            invitation.invitee_email == user.email
        }
        Resource::Org(org_id) => {
            let email = claims.data.get("email").ok_or_else(required("email"))?;
            let user = User::find_by_email(email, conn).await?;
            invitation.org_id == org_id && invitation.invitee_email == user.email
        }
        Resource::Host(_) => false,
        Resource::Node(_) => false,
    };
    if !is_allowed {
        super::forbidden!("Access denied for invitations accept of {invitation_id}");
    }
    if invitation.accepted_at.is_some() {
        return Err(Status::failed_precondition("Invitation already accepted").into());
    }
    if invitation.declined_at.is_some() {
        return Err(Status::failed_precondition("Invitation is declined").into());
    }

    let invitation = invitation.accept(conn).await?;
    let org = Org::find_by_id(invitation.org_id, conn).await?;
    // Only registered users can accept an invitation
    let new_member = User::find_by_email(&invitation.invitee_email, conn).await?;
    let org_user = org.add_member(new_member.id, OrgRole::Member, conn).await?;
    let org = Org::find_by_id(org_user.org_id, conn).await?;
    let user = User::find_by_id(org_user.user_id, conn).await?;
    let msg = api::OrgMessage::invitation_accepted(org, invitation, user, conn).await?;
    let resp = api::InvitationServiceAcceptResponse {};

    mqtt_tx.send(msg.into()).expect("mqtt_rx");

    Ok(tonic::Response::new(resp))
}

async fn decline(
    req: Request<api::InvitationServiceDeclineRequest>,
    write: WriteConn<'_, '_>,
) -> super::Result<api::InvitationServiceDeclineResponse> {
    let WriteConn { conn, ctx, mqtt_tx } = write;
    let claims = ctx.claims(&req, Endpoint::InvitationDecline, conn).await?;
    let req = req.into_inner();
    let invitation_id = req.invitation_id.parse()?;
    let invitation = Invitation::find_by_id(invitation_id, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => {
            let user = User::find_by_id(user_id, conn).await?;
            invitation.invitee_email == user.email
        }
        Resource::Org(org_id) => {
            let email = claims.data.get("email").ok_or_else(required("email"))?;
            invitation.org_id == org_id && invitation.invitee_email == *email
        }
        Resource::Host(_) => false,
        Resource::Node(_) => false,
    };
    if !is_allowed {
        super::forbidden!("Access denied for invitations decline of {invitation_id}");
    }
    if invitation.accepted_at.is_some() {
        return Err(Status::failed_precondition("Invite is accepted").into());
    }
    if invitation.declined_at.is_some() {
        return Err(Status::failed_precondition("Invite already declined").into());
    }

    invitation.decline(conn).await?;
    let org = Org::find_by_id(invitation.org_id, conn).await?;
    let msg = api::OrgMessage::invitation_declined(org, invitation, conn).await?;
    let resp = api::InvitationServiceDeclineResponse {};

    mqtt_tx.send(msg.into()).expect("mqtt_rx");

    Ok(tonic::Response::new(resp))
}

async fn revoke(
    req: Request<api::InvitationServiceRevokeRequest>,
    write: WriteConn<'_, '_>,
) -> super::Result<api::InvitationServiceRevokeResponse> {
    let WriteConn { conn, ctx, mqtt_tx } = write;
    let claims = ctx.claims(&req, Endpoint::InvitationRevoke, conn).await?;
    let req = req.into_inner();
    let invitation_id = req.invitation_id.parse()?;
    let invitation = Invitation::find_by_id(invitation_id, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => Org::is_member(user_id, invitation.org_id, conn).await?,
        Resource::Org(_) => false,
        Resource::Host(_) => false,
        Resource::Node(_) => false,
    };
    if !is_allowed {
        super::forbidden!("Access denied for invitations revoke of {invitation_id}");
    }
    if invitation.accepted_at.is_some() {
        return Err(Status::failed_precondition("Invite is accepted").into());
    }
    if invitation.declined_at.is_some() {
        return Err(Status::failed_precondition("Invite is declined").into());
    }
    invitation.revoke(conn).await?;
    let org = Org::find_by_id(invitation.org_id, conn).await?;
    let msg = api::OrgMessage::invitation_declined(org, invitation, conn).await?;
    let resp = api::InvitationServiceRevokeResponse {};

    mqtt_tx.send(msg.into()).expect("mqtt_rx");

    Ok(tonic::Response::new(resp))
}

impl api::InvitationServiceCreateRequest {
    pub fn into_new(self, user_id: UserId) -> crate::Result<NewInvitation> {
        Ok(NewInvitation {
            created_by: user_id,
            org_id: self.org_id.parse()?,
            invitee_email: self.invitee_email,
        })
    }
}

impl api::Invitation {
    async fn from_models(models: Vec<Invitation>, conn: &mut Conn<'_>) -> crate::Result<Vec<Self>> {
        let creator_ids = models.iter().map(|i| i.created_by).collect();
        let creators: HashMap<_, _> = User::find_by_ids(creator_ids, conn)
            .await?
            .into_iter()
            .map(|u| (u.id, u))
            .collect();

        let org_ids = models.iter().map(|i| i.org_id).collect();
        let orgs: HashMap<_, _> = Org::find_by_ids(org_ids, conn)
            .await?
            .into_iter()
            .map(|o| (o.id, o))
            .collect();

        models
            .into_iter()
            .filter_map(|i| orgs.get(&i.org_id).map(|o| (i, o)))
            .map(|(i, org)| {
                let creator = &creators[&i.created_by];
                Self::new(i, creator, org)
            })
            .collect()
    }

    pub async fn from_model(model: Invitation, conn: &mut Conn<'_>) -> crate::Result<Self> {
        let creator = User::find_by_id(model.created_by, conn).await?;
        let org = Org::find_by_id(model.org_id, conn).await?;
        Self::new(model, &creator, &org)
    }

    fn new(model: Invitation, creator: &User, org: &Org) -> crate::Result<Self> {
        let mut invitation = Self {
            id: model.id.to_string(),
            created_by: model.created_by.to_string(),
            created_by_name: creator.name(),
            org_id: model.org_id.to_string(),
            org_name: org.name.clone(),
            invitee_email: model.invitee_email,
            created_at: Some(NanosUtc::from(model.created_at).into()),
            status: 0, // We use the setter to set this field for type-safety
            accepted_at: model.accepted_at.map(NanosUtc::from).map(Into::into),
            declined_at: model.declined_at.map(NanosUtc::from).map(Into::into),
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

impl api::InvitationServiceListRequest {
    fn as_filter(&self) -> crate::Result<InvitationFilter<'_>> {
        let status = self.status();
        let status = (status != api::InvitationStatus::Unspecified).then_some(status);
        Ok(InvitationFilter {
            org_id: self.org_id.as_ref().map(|id| id.parse()).transpose()?,
            invitee_email: self.invitee_email.as_deref(),
            created_by: self.created_by.as_ref().map(|id| id.parse()).transpose()?,
            accepted: status.map(|s| s == api::InvitationStatus::Accepted),
            declined: status.map(|s| s == api::InvitationStatus::Declined),
        })
    }
}
