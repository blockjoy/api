use diesel::result::Error::NotFound;
use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::{Request, Response};
use tracing::error;

use crate::auth::rbac::{InvitationAdminPerm, InvitationPerm, OrgRole};
use crate::auth::resource::{OrgId, Resource};
use crate::auth::Authorize;
use crate::database::{Conn, ReadConn, Transaction, WriteConn};
use crate::email::Recipient;
use crate::model::invitation::{Invitation, InvitationFilter, NewInvitation};
use crate::model::org::Org;
use crate::model::user::User;
use crate::util::{HashVec, NanosUtc};

use super::api::invitation_service_server::InvitationService;
use super::{api, common, Grpc, Metadata, Status};

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
    Model(#[from] crate::model::invitation::Error),
    /// Node token not valid for invitation.
    NodeClaims,
    /// Invitation org error: {0}
    Org(#[from] crate::model::org::Error),
    /// Failed to parse InvitationId: {0}
    ParseId(uuid::Error),
    /// Failed to parse `invited_by`: {0}
    ParseInvitedBy(crate::auth::resource::Error),
    /// Failed to parse OrgId: {0}
    ParseOrgId(uuid::Error),
    /// Invitation resource error: {0}
    Resource(#[from] crate::auth::resource::Error),
    /// Invitation user error: {0}
    User(#[from] crate::model::user::Error),
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
            | NodeClaims | WrongEmail | WrongOrg => Status::forbidden("Access denied."),
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
    ) -> Result<Response<api::InvitationServiceCreateResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| create(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn list(
        &self,
        req: Request<api::InvitationServiceListRequest>,
    ) -> Result<Response<api::InvitationServiceListResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn accept(
        &self,
        req: Request<api::InvitationServiceAcceptRequest>,
    ) -> Result<Response<api::InvitationServiceAcceptResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| accept(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn decline(
        &self,
        req: Request<api::InvitationServiceDeclineRequest>,
    ) -> Result<Response<api::InvitationServiceDeclineResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| decline(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn revoke(
        &self,
        req: Request<api::InvitationServiceRevokeRequest>,
    ) -> Result<Response<api::InvitationServiceRevokeResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| revoke(req, meta.into(), write).scope_boxed())
            .await
    }
}

pub async fn create(
    req: api::InvitationServiceCreateRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::InvitationServiceCreateResponse, Error> {
    let org_id: OrgId = req.org_id.parse().map_err(Error::ParseOrgId)?;
    let authz = write
        .auth_or_for(
            &meta,
            InvitationAdminPerm::Create,
            InvitationPerm::Create,
            org_id,
        )
        .await?;

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

        Err(crate::model::user::Error::FindByEmail(_, NotFound)) => {
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
    let invitation = api::Invitation::from(invitation, &org);

    let created = api::OrgMessage::invitation_created(invitation.clone(), &org);
    write.mqtt(created);

    Ok(api::InvitationServiceCreateResponse {
        invitation: Some(invitation),
    })
}

pub async fn list(
    req: api::InvitationServiceListRequest,
    meta: Metadata,
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

    read.auth_or_for(
        &meta,
        InvitationAdminPerm::List,
        InvitationPerm::List,
        resource,
    )
    .await?;

    let filter = req.as_filter()?;
    let invitations = Invitation::filter(filter, &mut read).await?;
    let invitations = api::Invitation::from_models(invitations, &mut read).await?;

    Ok(api::InvitationServiceListResponse { invitations })
}

pub async fn accept(
    req: api::InvitationServiceAcceptRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::InvitationServiceAcceptResponse, Error> {
    let id = req.invitation_id.parse().map_err(Error::ParseId)?;
    let invitation = Invitation::by_id(id, &mut write).await?;

    // First validate claims for all resources, then apply additional constraints.
    let authz = write.auth(&meta, InvitationPerm::Accept).await?;
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
    let org = Org::add_user(user.id, invitation.org_id, OrgRole::Member, &mut write).await?;

    let invitation = api::Invitation::from(invitation, &org);
    let accepted = api::OrgMessage::invitation_accepted(invitation, &org, user);
    write.mqtt(accepted);

    Ok(api::InvitationServiceAcceptResponse {})
}

pub async fn decline(
    req: api::InvitationServiceDeclineRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::InvitationServiceDeclineResponse, Error> {
    let id = req.invitation_id.parse().map_err(Error::ParseId)?;
    let invitation = Invitation::by_id(id, &mut write).await?;

    // First validate claims for all resources, then apply additional constraints.
    let authz = write.auth(&meta, InvitationPerm::Decline).await?;
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
    let invitation = api::Invitation::from(invitation, &org);

    let declined = api::OrgMessage::invitation_declined(invitation, &org);
    write.mqtt(declined);

    Ok(api::InvitationServiceDeclineResponse {})
}

pub async fn revoke(
    req: api::InvitationServiceRevokeRequest,
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::InvitationServiceRevokeResponse, Error> {
    let id = req.invitation_id.parse().map_err(Error::ParseId)?;
    let invitation = Invitation::by_id(id, &mut write).await?;

    let authz = write
        .auth_or_for(
            &meta,
            InvitationAdminPerm::Revoke,
            InvitationPerm::Revoke,
            invitation.org_id,
        )
        .await?;
    authz.resource().user().ok_or(Error::ClaimsNotUser)?;

    if invitation.accepted_at.is_some() {
        return Err(Error::AlreadyAccepted);
    } else if invitation.declined_at.is_some() {
        return Err(Error::AlreadyDeclined);
    }

    invitation.revoke(&mut write).await?;

    let org = Org::by_id(invitation.org_id, &mut write).await?;
    let invitation = api::Invitation::from(invitation, &org);
    let declined = api::OrgMessage::invitation_declined(invitation, &org);
    write.mqtt(declined);

    Ok(api::InvitationServiceRevokeResponse {})
}

impl api::Invitation {
    async fn from_models(
        invitations: Vec<Invitation>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        let org_ids = invitations.iter().map(|inv| inv.org_id).collect();
        let orgs = Org::by_ids(&org_ids, conn)
            .await?
            .to_map_keep_last(|org| (org.id, org));

        Ok(invitations
            .into_iter()
            .filter_map(|invitation| {
                orgs.get(&invitation.org_id)
                    .map(|org| api::Invitation::from(invitation, org))
            })
            .collect())
    }

    fn from(invitation: Invitation, org: &Org) -> Self {
        let invited_by = invitation.invited_by();
        let status = match (invitation.accepted_at, invitation.declined_at) {
            (None, None) => api::InvitationStatus::Open,
            (Some(_), None) => api::InvitationStatus::Accepted,
            (None, Some(_)) => api::InvitationStatus::Declined,
            (Some(_), Some(_)) => api::InvitationStatus::Unspecified,
        };

        api::Invitation {
            invitation_id: invitation.id.to_string(),
            org_id: invitation.org_id.to_string(),
            org_name: org.name.clone(),
            invitee_email: invitation.invitee_email,
            invited_by: Some(common::Resource::from(invited_by)),
            created_at: Some(NanosUtc::from(invitation.created_at).into()),
            status: status.into(),
            accepted_at: invitation.accepted_at.map(NanosUtc::from).map(Into::into),
            declined_at: invitation.declined_at.map(NanosUtc::from).map(Into::into),
        }
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
