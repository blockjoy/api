use std::collections::HashMap;

use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::error;

use crate::auth::rbac::InvitationPerm;
use crate::auth::resource::{OrgId, Resource, UserId};
use crate::auth::Authorize;
use crate::database::{Conn, ReadConn, Transaction, WriteConn};
use crate::email::Recipient;
use crate::models::invitation::{Invitation, InvitationFilter, NewInvitation};
use crate::models::org::Org;
use crate::models::user::User;
use crate::timestamp::NanosUtc;

use super::api::invitation_service_server::InvitationService;
use super::{api, Grpc};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Invitation already accepted.
    AlreadyAccepted,
    /// Invitation already declined.
    AlreadyDeclined,
    /// User is already invited.
    AlreadyInvited,
    /// User is already a member of the org.
    AlreadyMember,
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Claims Resource is not a user.
    ClaimsNotUser,
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Invitation email error: {0}
    Email(#[from] crate::email::Error),
    /// Host token not valid for invitation.
    HostClaims,
    /// List invitations is missing a Resource.
    ListResource,
    /// Invitation MQTT message error: {0}
    Message(Box<crate::mqtt::message::Error>),
    /// Claims data is missing email address.
    MissingEmail,
    /// Invitation model error: {0}
    Model(#[from] crate::models::invitation::Error),
    /// Node token not valid for invitation.
    NodeClaims,
    /// Invitation org error: {0}
    Org(#[from] crate::models::org::Error),
    /// Failed to parse created_by user: {0}
    ParseCreatedBy(uuid::Error),
    /// Failed to parse InvitationId: {0}
    ParseId(uuid::Error),
    /// Failed to parse OrgId: {0}
    ParseOrgId(uuid::Error),
    /// Invitation user error: {0}
    User(#[from] crate::models::user::Error),
    /// Wrong email for invitation.
    WrongEmail,
    /// Wrong org for invitation.
    WrongOrg,
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            ClaimsNotUser | HostClaims | ListResource | MissingEmail | NodeClaims | WrongEmail
            | WrongOrg => Status::permission_denied("Access denied."),
            AlreadyAccepted => Status::failed_precondition("Already accepted."),
            AlreadyDeclined => Status::failed_precondition("Already declined."),
            AlreadyInvited => Status::failed_precondition("Already invited."),
            AlreadyMember => Status::failed_precondition("Already member."),
            Diesel(_) | Email(_) | Message(_) => Status::internal("Internal error."),
            ParseCreatedBy(_) => Status::invalid_argument("created_by"),
            ParseId(_) => Status::invalid_argument("invitation_id"),
            ParseOrgId(_) => Status::invalid_argument("org_id"),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
            Model(err) => err.into(),
            Org(err) => err.into(),
            User(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl InvitationService for Grpc {
    async fn create(
        &self,
        req: Request<api::InvitationServiceCreateRequest>,
    ) -> Result<Response<api::InvitationServiceCreateResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| create(req, meta, write).scope_boxed())
            .await
    }

    async fn list(
        &self,
        req: Request<api::InvitationServiceListRequest>,
    ) -> Result<Response<api::InvitationServiceListResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list(req, meta, read).scope_boxed()).await
    }

    async fn accept(
        &self,
        req: Request<api::InvitationServiceAcceptRequest>,
    ) -> Result<Response<api::InvitationServiceAcceptResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| accept(req, meta, write).scope_boxed())
            .await
    }

    async fn decline(
        &self,
        req: Request<api::InvitationServiceDeclineRequest>,
    ) -> Result<Response<api::InvitationServiceDeclineResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| decline(req, meta, write).scope_boxed())
            .await
    }

    async fn revoke(
        &self,
        req: Request<api::InvitationServiceRevokeRequest>,
    ) -> Result<Response<api::InvitationServiceRevokeResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| revoke(req, meta, write).scope_boxed())
            .await
    }
}

async fn create(
    req: api::InvitationServiceCreateRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::InvitationServiceCreateResponse, Error> {
    let org_id: OrgId = req.org_id.parse().map_err(Error::ParseOrgId)?;
    let authz = write.auth(&meta, InvitationPerm::Create, org_id).await?;

    if Invitation::has_open_invite(org_id, &req.invitee_email, &mut write).await? {
        return Err(Error::AlreadyInvited);
    }

    let user_id = authz.resource().user().ok_or(Error::ClaimsNotUser)?;
    let invitor = User::find_by_id(user_id, &mut write).await?;
    let invitee = User::find_by_email(&req.invitee_email, &mut write).await;

    let invitation = req.into_new(invitor.id)?.create(&mut write).await?;

    if let Ok(invitee) = invitee {
        if Org::has_user(org_id, invitee.id, &mut write).await? {
            return Err(Error::AlreadyMember);
        }

        write
            .ctx
            .email
            .invitation_for_registered(&invitation, &invitor, &invitee, "1 week")
            .await?;
    } else {
        let invitee = Recipient {
            email: &invitation.invitee_email,
            first_name: "",
            last_name: "",
            preferred_language: None,
        };

        write
            .ctx
            .email
            .invitation(&invitation, &invitor, invitee, "1 week")
            .await?;
    }

    let org = Org::find_by_id(invitation.org_id, &mut write).await?;
    let msg = api::OrgMessage::invitation_created(invitation.clone(), org, &mut write)
        .await
        .map_err(|err| Error::Message(Box::new(err)))?;
    write.mqtt(msg);

    Ok(api::InvitationServiceCreateResponse {
        invitation: Some(api::Invitation::from_model(invitation, &mut write).await?),
    })
}

async fn list(
    req: api::InvitationServiceListRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::InvitationServiceListResponse, Error> {
    let resource: Resource = if let Some(org_id) = &req.org_id {
        org_id
            .parse::<OrgId>()
            .map(Into::into)
            .map_err(Error::ParseOrgId)?
    } else if let Some(created_by) = &req.created_by {
        created_by
            .parse::<UserId>()
            .map(Into::into)
            .map_err(Error::ParseCreatedBy)?
    } else if let Some(email) = &req.invitee_email {
        User::find_by_email(email, &mut read)
            .await
            .map(|user| user.id.into())?
    } else {
        return Err(Error::ListResource);
    };

    read.auth(&meta, InvitationPerm::List, resource).await?;

    let filter = req.as_filter()?;
    let invitations = Invitation::filter(filter, &mut read).await?;
    let invitations = api::Invitation::from_models(invitations, &mut read).await?;

    Ok(api::InvitationServiceListResponse { invitations })
}

async fn accept(
    req: api::InvitationServiceAcceptRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::InvitationServiceAcceptResponse, Error> {
    let id = req.invitation_id.parse().map_err(Error::ParseId)?;
    let invitation = Invitation::find_by_id(id, &mut write).await?;

    // First validate claims for all resources, then apply additional constraints.
    let authz = write.auth_all(&meta, InvitationPerm::Accept).await?;
    let user = match authz.resource() {
        Resource::User(user_id) => Ok(User::find_by_id(user_id, &mut write).await?),

        Resource::Org(org_id) if org_id != invitation.org_id => Err(Error::WrongOrg),
        Resource::Org(_) => {
            let email = authz.get_data("email").ok_or(Error::MissingEmail)?;
            Ok(User::find_by_email(email, &mut write).await?)
        }

        Resource::Host(_) => Err(Error::HostClaims),
        Resource::Node(_) => Err(Error::NodeClaims),
    }?;

    if user.email != invitation.invitee_email {
        return Err(Error::WrongEmail);
    } else if invitation.accepted_at.is_some() {
        return Err(Error::AlreadyAccepted);
    } else if invitation.declined_at.is_some() {
        return Err(Error::AlreadyDeclined);
    }

    let invitation = invitation.accept(&mut write).await?;

    // Only registered users can accept an invitation
    let user = User::find_by_email(&invitation.invitee_email, &mut write).await?;
    let org = Org::find_by_id(invitation.org_id, &mut write).await?;
    org.add_member(user.id, &mut write).await?;

    let msg = api::OrgMessage::invitation_accepted(invitation, org, user, &mut write)
        .await
        .map_err(|err| Error::Message(Box::new(err)))?;
    write.mqtt(msg);

    Ok(api::InvitationServiceAcceptResponse {})
}

async fn decline(
    req: api::InvitationServiceDeclineRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::InvitationServiceDeclineResponse, Error> {
    let id = req.invitation_id.parse().map_err(Error::ParseId)?;
    let invitation = Invitation::find_by_id(id, &mut write).await?;

    // First validate claims for all resources, then apply additional constraints.
    let authz = write.auth_all(&meta, InvitationPerm::Decline).await?;
    let email = match authz.resource() {
        Resource::User(user_id) => User::find_by_id(user_id, &mut write)
            .await
            .map(|user| user.email)
            .map_err(Into::into),

        Resource::Org(org_id) if org_id != invitation.org_id => Err(Error::WrongOrg),
        Resource::Org(_) => authz
            .get_data("email")
            .map(ToString::to_string)
            .ok_or(Error::MissingEmail),

        Resource::Host(_) => Err(Error::HostClaims),
        Resource::Node(_) => Err(Error::NodeClaims),
    }?;

    if email != invitation.invitee_email {
        return Err(Error::WrongEmail);
    } else if invitation.accepted_at.is_some() {
        return Err(Error::AlreadyAccepted);
    } else if invitation.declined_at.is_some() {
        return Err(Error::AlreadyDeclined);
    }

    let invitation = invitation.decline(&mut write).await?;

    let org = Org::find_by_id(invitation.org_id, &mut write).await?;
    let msg = api::OrgMessage::invitation_declined(invitation, org, &mut write)
        .await
        .map_err(|err| Error::Message(Box::new(err)))?;
    write.mqtt(msg);

    Ok(api::InvitationServiceDeclineResponse {})
}

async fn revoke(
    req: api::InvitationServiceRevokeRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::InvitationServiceRevokeResponse, Error> {
    let id = req.invitation_id.parse().map_err(Error::ParseId)?;
    let invitation = Invitation::find_by_id(id, &mut write).await?;

    let authz = write
        .auth(&meta, InvitationPerm::Revoke, invitation.org_id)
        .await?;
    authz.resource().user().ok_or(Error::ClaimsNotUser)?;

    if invitation.accepted_at.is_some() {
        return Err(Error::AlreadyAccepted);
    } else if invitation.declined_at.is_some() {
        return Err(Error::AlreadyDeclined);
    }

    invitation.revoke(&mut write).await?;

    let org = Org::find_by_id(invitation.org_id, &mut write).await?;
    let msg = api::OrgMessage::invitation_declined(invitation, org, &mut write)
        .await
        .map_err(|err| Error::Message(Box::new(err)))?;
    write.mqtt(msg);

    Ok(api::InvitationServiceRevokeResponse {})
}

impl api::InvitationServiceCreateRequest {
    pub fn into_new(self, created_by: UserId) -> Result<NewInvitation, Error> {
        Ok(NewInvitation {
            created_by,
            org_id: self.org_id.parse().map_err(Error::ParseOrgId)?,
            invitee_email: self.invitee_email,
        })
    }
}

impl api::Invitation {
    async fn from_models(models: Vec<Invitation>, conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
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

        let invitations = models
            .into_iter()
            .filter_map(|invitation| {
                orgs.get(&invitation.org_id).and_then(|org| {
                    let creator = creators.get(&invitation.created_by)?;
                    Some(Self::new(invitation, creator, org))
                })
            })
            .collect();

        Ok(invitations)
    }

    pub async fn from_model(model: Invitation, conn: &mut Conn<'_>) -> Result<Self, Error> {
        let creator = User::find_by_id(model.created_by, conn).await?;
        let org = Org::find_by_id(model.org_id, conn).await?;

        Ok(Self::new(model, &creator, &org))
    }

    fn new(model: Invitation, creator: &User, org: &Org) -> Self {
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
        invitation
    }
}

impl api::InvitationServiceListRequest {
    fn as_filter(&self) -> Result<InvitationFilter<'_>, Error> {
        let status = self.status();
        let status = (status != api::InvitationStatus::Unspecified).then_some(status);
        Ok(InvitationFilter {
            org_id: self
                .org_id
                .as_ref()
                .map(|id| id.parse().map_err(Error::ParseOrgId))
                .transpose()?,
            invitee_email: self.invitee_email.as_deref(),
            created_by: self
                .created_by
                .as_ref()
                .map(|id| id.parse().map_err(Error::ParseCreatedBy))
                .transpose()?,
            accepted: status.map(|s| s == api::InvitationStatus::Accepted),
            declined: status.map(|s| s == api::InvitationStatus::Declined),
        })
    }
}
