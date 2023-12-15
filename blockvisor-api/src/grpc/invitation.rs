use diesel::result::Error::NotFound;
use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::error;

use crate::auth::rbac::InvitationPerm;
use crate::auth::resource::{OrgId, Resource, ResourceType};
use crate::auth::Authorize;
use crate::database::{Conn, ReadConn, Transaction, WriteConn};
use crate::email::Recipient;
use crate::models::invitation::{Invitation, InvitationFilter, NewInvitation};
use crate::models::org::Org;
use crate::models::user::User;
use crate::util::{HashVec, NanosUtc};

use super::api::invitation_service_server::InvitationService;
use super::{api, common, Grpc};

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
    /// Claims Resource is not a user or org.
    ClaimsNotUserOrOrg,
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Invitation email error: {0}
    Email(#[from] crate::email::Error),
    /// Host token not valid for invitation.
    HostClaims,
    /// List invitations is missing a Resource.
    ListResource,
    /// Invitation MQTT message error: {0}
    Message(#[from] crate::mqtt::message::Error),
    /// Claims data is missing email address.
    MissingEmail,
    /// Invitation model error: {0}
    Model(#[from] crate::models::invitation::Error),
    /// Node token not valid for invitation.
    NodeClaims,
    /// Invitation org error: {0}
    Org(#[from] crate::models::org::Error),
    /// Failed to parse InvitationId: {0}
    ParseId(uuid::Error),
    /// Failed to parse `invited_by`: {0}
    ParseInvitedBy(crate::auth::resource::Error),
    /// Failed to parse OrgId: {0}
    ParseOrgId(uuid::Error),
    /// Invitation resource error: {0}
    Resource(#[from] crate::auth::resource::Error),
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
            Diesel(_) | Email(_) | Message(_) => Status::internal("Internal error."),
            ClaimsNotUser | ClaimsNotUserOrOrg | HostClaims | ListResource | MissingEmail
            | NodeClaims | WrongEmail | WrongOrg => Status::permission_denied("Access denied."),
            AlreadyAccepted => Status::failed_precondition("Already accepted."),
            AlreadyDeclined => Status::failed_precondition("Already declined."),
            AlreadyInvited => Status::failed_precondition("Already invited."),
            AlreadyMember => Status::failed_precondition("Already member."),
            ParseId(_) => Status::invalid_argument("invitation_id"),
            ParseInvitedBy(_) => Status::invalid_argument("invited_by"),
            ParseOrgId(_) => Status::invalid_argument("org_id"),
            Auth(err) => err.into(),
            Claims(err) => err.into(),
            Model(err) => err.into(),
            Org(err) => err.into(),
            Resource(err) => err.into(),
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

    let new_invitation = NewInvitation::new(org_id, &req.invitee_email, &authz);
    let invitation = new_invitation.create(&mut write).await?;

    let invitor = match authz.resource() {
        Resource::User(user_id) => {
            let user = User::by_id(user_id, &mut write).await?;
            Ok(format!("{} ({})", user.name(), user.email))
        }
        Resource::Org(org_id) => {
            let org = Org::by_id(org_id, &mut write).await?;
            Ok(format!("Org: {}", org.name))
        }
        _ => Err(Error::ClaimsNotUserOrOrg),
    }?;

    match User::by_email(&req.invitee_email, &mut write).await {
        Ok(invitee) => {
            if Org::has_user(org_id, invitee.id, &mut write).await? {
                return Err(Error::AlreadyMember);
            }

            write
                .ctx
                .email
                .invitation_for_registered(&invitation, invitor, &invitee, "1 week")
                .await?;
        }

        Err(crate::models::user::Error::FindByEmail(_, NotFound)) => {
            let recipient = Recipient {
                email: &invitation.invitee_email,
                first_name: "",
                last_name: "",
                preferred_language: None,
            };

            write
                .ctx
                .email
                .invitation(&invitation, invitor, recipient, "1 week")
                .await?;
        }

        Err(err) => return Err(err.into()),
    }

    let org = Org::by_id(invitation.org_id, &mut write).await?;
    let invitation = api::Invitation::from_model(invitation, &org, &mut write).await?;

    let created = api::OrgMessage::invitation_created(invitation.clone(), &org);
    write.mqtt(created);

    Ok(api::InvitationServiceCreateResponse {
        invitation: Some(invitation),
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
    } else if let Some(invited_by) = &req.invited_by {
        invited_by.try_into().map_err(Error::ParseInvitedBy)?
    } else if let Some(email) = &req.invitee_email {
        User::by_email(email, &mut read)
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
    let invitation = Invitation::by_id(id, &mut write).await?;

    // First validate claims for all resources, then apply additional constraints.
    let authz = write.auth_all(&meta, InvitationPerm::Accept).await?;
    let user = match authz.resource() {
        Resource::User(user_id) => Ok(User::by_id(user_id, &mut write).await?),

        Resource::Org(org_id) if org_id != invitation.org_id => Err(Error::WrongOrg),
        Resource::Org(_) => {
            let email = authz.get_data("email").ok_or(Error::MissingEmail)?;
            Ok(User::by_email(email, &mut write).await?)
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
    let user = User::by_email(&invitation.invitee_email, &mut write).await?;
    let org = Org::by_id(invitation.org_id, &mut write).await?;
    org.add_member(user.id, &mut write).await?;

    let invitation = api::Invitation::from_model(invitation, &org, &mut write).await?;
    let accepted = api::OrgMessage::invitation_accepted(invitation, &org, user);
    write.mqtt(accepted);

    Ok(api::InvitationServiceAcceptResponse {})
}

async fn decline(
    req: api::InvitationServiceDeclineRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::InvitationServiceDeclineResponse, Error> {
    let id = req.invitation_id.parse().map_err(Error::ParseId)?;
    let invitation = Invitation::by_id(id, &mut write).await?;

    // First validate claims for all resources, then apply additional constraints.
    let authz = write.auth_all(&meta, InvitationPerm::Decline).await?;
    let email = match authz.resource() {
        Resource::User(user_id) => User::by_id(user_id, &mut write)
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

    let org = Org::by_id(invitation.org_id, &mut write).await?;
    let invitation = invitation.decline(&mut write).await?;
    let invitation = api::Invitation::from_model(invitation, &org, &mut write).await?;

    let declined = api::OrgMessage::invitation_declined(invitation, &org);
    write.mqtt(declined);

    Ok(api::InvitationServiceDeclineResponse {})
}

async fn revoke(
    req: api::InvitationServiceRevokeRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::InvitationServiceRevokeResponse, Error> {
    let id = req.invitation_id.parse().map_err(Error::ParseId)?;
    let invitation = Invitation::by_id(id, &mut write).await?;

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

    let org = Org::by_id(invitation.org_id, &mut write).await?;
    let invitation = api::Invitation::from_model(invitation, &org, &mut write).await?;
    let declined = api::OrgMessage::invitation_declined(invitation, &org);
    write.mqtt(declined);

    Ok(api::InvitationServiceRevokeResponse {})
}

impl api::Invitation {
    async fn from_models(models: Vec<Invitation>, conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
        let org_ids = models.iter().map(|i| i.org_id).collect();
        let orgs = Org::by_ids(org_ids, conn)
            .await?
            .to_map_keep_last(|o| (o.id, o));

        let mut invitations = Vec::with_capacity(models.len());
        for model in models {
            if let Some(org) = orgs.get(&model.org_id) {
                invitations.push(Self::from_model(model, org, conn).await?);
            }
        }

        Ok(invitations)
    }

    async fn from_model(
        invitation: Invitation,
        org: &Org,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        let status = match (invitation.accepted_at, invitation.declined_at) {
            (None, None) => api::InvitationStatus::Open,
            (Some(_), None) => api::InvitationStatus::Accepted,
            (None, Some(_)) => api::InvitationStatus::Declined,
            (Some(_), Some(_)) => api::InvitationStatus::Unspecified,
        };

        let invited_by = match invitation.invited_by_resource {
            ResourceType::User => {
                let user = User::by_id((*invitation.invited_by).into(), conn).await?;
                Some(common::EntityUpdate::from_user(&user))
            }
            ResourceType::Org => Some(common::EntityUpdate::from_org(
                (*invitation.invited_by).into(),
            )),
            _ => None,
        };

        Ok(api::Invitation {
            id: invitation.id.to_string(),
            org_id: invitation.org_id.to_string(),
            org_name: org.name.clone(),
            invitee_email: invitation.invitee_email,
            invited_by,
            created_at: Some(NanosUtc::from(invitation.created_at).into()),
            status: status.into(),
            accepted_at: invitation.accepted_at.map(NanosUtc::from).map(Into::into),
            declined_at: invitation.declined_at.map(NanosUtc::from).map(Into::into),
        })
    }
}

impl api::InvitationServiceListRequest {
    fn as_filter(&self) -> Result<InvitationFilter<'_>, Error> {
        let status = self.status();
        let status = (status != api::InvitationStatus::Unspecified).then_some(status);

        let invited_by = self
            .invited_by
            .as_ref()
            .map(|entity| entity.try_into().map_err(Error::ParseInvitedBy))
            .transpose()?;

        Ok(InvitationFilter {
            org_id: self
                .org_id
                .as_ref()
                .map(|id| id.parse().map_err(Error::ParseOrgId))
                .transpose()?,
            invitee_email: self.invitee_email.as_deref(),
            invited_by,
            accepted: status.map(|s| s == api::InvitationStatus::Accepted),
            declined: status.map(|s| s == api::InvitationStatus::Declined),
        })
    }
}
