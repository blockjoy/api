use std::collections::HashMap;
use std::collections::HashSet;

use chrono::{DateTime, Utc};
use derive_more::{Deref, Display, From, FromStr};
use diesel::dsl;
use diesel::prelude::*;
use diesel::result::DatabaseErrorKind::UniqueViolation;
use diesel::result::Error::{DatabaseError, NotFound};
use diesel_async::RunQueryDsl;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display as DisplayDoc;
use thiserror::Error;
use uuid::Uuid;

use crate::auth::resource::{OrgId, Resource, ResourceId, ResourceType};
use crate::database::Conn;
use crate::grpc::Status;

use super::schema::invitations;

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Failed to accept invitation: {0}
    Accept(diesel::result::Error),
    /// Failed to bulk delete invitations `{0:?}`: {1}
    BulkDelete(HashSet<InvitationId>, diesel::result::Error),
    /// Failed to create new invitation: {0}
    Create(diesel::result::Error),
    /// Failed to decline invitation: {0}
    Decline(diesel::result::Error),
    /// Failed to filter invite emails: {0}
    FilterInvites(diesel::result::Error),
    /// Failed to find invitation id `{0}`: {1}
    FindById(InvitationId, diesel::result::Error),
    /// Failed to find invitation by org id `{0}`: {1}
    FindByOrgId(OrgId, diesel::result::Error),
    /// Failed to find invitations for org ids `{0:?}`: {1}
    FindForOrgIds(HashSet<OrgId>, diesel::result::Error),
    /// Failed to check for open invitations: {0}
    OpenInvite(diesel::result::Error),
    /// Failed to find received invitations for email `{0}`: {1}
    Received(String, diesel::result::Error),
    /// Failed to remove invitation for org email `{0}`: {1}
    RemoveOrgUser(String, diesel::result::Error),
    /// Failed to revoke invitation: {0}
    Revoke(diesel::result::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            Create(DatabaseError(UniqueViolation, _)) => Status::already_exists("Already exists."),
            Accept(NotFound)
            | Decline(NotFound)
            | FindById(_, NotFound)
            | FindByOrgId(_, NotFound)
            | Received(_, NotFound)
            | RemoveOrgUser(_, NotFound) => Status::not_found("Not found."),
            _ => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Copy, Debug, Display, Hash, PartialEq, Eq, DieselNewType, Deref, From, FromStr)]
pub struct InvitationId(Uuid);

#[derive(Clone, Debug, Queryable)]
pub struct Invitation {
    pub id: InvitationId,
    pub invited_by: ResourceId,
    pub org_id: OrgId,
    pub invitee_email: String,
    pub created_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub declined_at: Option<DateTime<Utc>>,
    pub invited_by_resource: ResourceType,
}

impl Invitation {
    pub async fn by_id(id: InvitationId, conn: &mut Conn<'_>) -> Result<Self, Error> {
        invitations::table
            .find(id)
            .get_result(conn)
            .await
            .map_err(|err| Error::FindById(id, err))
    }

    pub async fn by_org_id(org_id: OrgId, conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
        invitations::table
            .filter(invitations::org_id.eq(org_id))
            .get_results(conn)
            .await
            .map_err(|err| Error::FindByOrgId(org_id, err))
    }

    pub async fn for_org_ids(
        org_ids: &HashSet<OrgId>,
        conn: &mut Conn<'_>,
    ) -> Result<HashMap<OrgId, Vec<Self>>, Error> {
        let invitations: Vec<Self> = invitations::table
            .filter(invitations::org_id.eq_any(org_ids))
            .get_results(conn)
            .await
            .map_err(|err| Error::FindForOrgIds(org_ids.clone(), err))?;

        let mut org_invitations: HashMap<OrgId, Vec<_>> = HashMap::with_capacity(invitations.len());

        for invitation in invitations {
            org_invitations
                .entry(invitation.org_id)
                .or_default()
                .push(invitation);
        }

        Ok(org_invitations)
    }

    pub async fn received(email: &str, conn: &mut Conn<'_>) -> Result<Vec<Self>, Error> {
        invitations::table
            .filter(invitations::invitee_email.eq(email))
            .filter(invitations::accepted_at.is_null())
            .filter(invitations::declined_at.is_null())
            .order_by(invitations::created_at.desc())
            .get_results(conn)
            .await
            .map_err(|err| Error::Received(email.into(), err))
    }

    pub async fn filter(
        filter: InvitationFilter<'_>,
        conn: &mut Conn<'_>,
    ) -> Result<Vec<Self>, Error> {
        let mut query = invitations::table.into_boxed();

        if let Some(org_id) = filter.org_id {
            query = query.filter(invitations::org_id.eq(org_id));
        }
        if let Some(invitee_email) = filter.invitee_email {
            query = query.filter(invitations::invitee_email.eq(invitee_email));
        }
        if let Some(resource) = filter.invited_by {
            query = query.filter(invitations::invited_by_resource.eq(resource.typ()));
            query = query.filter(invitations::invited_by.eq(resource.id()));
        }

        let query = match filter.accepted {
            Some(true) => query.filter(invitations::accepted_at.is_not_null()),
            Some(false) => query.filter(invitations::accepted_at.is_null()),
            None => query,
        };

        let query = match filter.declined {
            Some(true) => query.filter(invitations::declined_at.is_not_null()),
            Some(false) => query.filter(invitations::declined_at.is_null()),
            None => query,
        };

        query
            .select(invitations::all_columns)
            .get_results(conn)
            .await
            .map_err(Error::FilterInvites)
    }

    pub async fn has_open_invite(
        org_id: OrgId,
        email: &str,
        conn: &mut Conn<'_>,
    ) -> Result<bool, Error> {
        let invitation = invitations::table
            .filter(invitations::org_id.eq(org_id))
            .filter(invitations::invitee_email.eq(email))
            .filter(invitations::accepted_at.is_null())
            .filter(invitations::declined_at.is_null());

        diesel::select(dsl::exists(invitation))
            .get_result(conn)
            .await
            .map_err(Error::OpenInvite)
    }

    pub async fn accept(self, conn: &mut Conn<'_>) -> Result<Self, Error> {
        diesel::update(invitations::table.find(self.id))
            .set(invitations::accepted_at.eq(chrono::Utc::now()))
            .get_result(conn)
            .await
            .map_err(Error::Accept)
    }

    pub async fn decline(&self, conn: &mut Conn<'_>) -> Result<Self, Error> {
        diesel::update(invitations::table.find(self.id))
            .set(invitations::declined_at.eq(chrono::Utc::now()))
            .get_result(conn)
            .await
            .map_err(Error::Decline)
    }

    pub async fn revoke(&self, conn: &mut Conn<'_>) -> Result<(), Error> {
        diesel::delete(invitations::table.find(self.id))
            .execute(conn)
            .await
            .map(|_| ())
            .map_err(Error::Revoke)
    }

    pub async fn remove_by_org_user(
        user_email: &str,
        org_id: OrgId,
        conn: &mut Conn<'_>,
    ) -> Result<(), Error> {
        let to_delete = invitations::table
            .filter(invitations::invitee_email.eq(user_email))
            .filter(invitations::org_id.eq(org_id));

        diesel::delete(to_delete)
            .execute(conn)
            .await
            .map(|_| ())
            .map_err(|err| Error::RemoveOrgUser(user_email.into(), err))
    }

    pub async fn bulk_delete(
        ids: &HashSet<InvitationId>,
        conn: &mut Conn<'_>,
    ) -> Result<(), Error> {
        let to_delete = invitations::table.filter(invitations::id.eq_any(ids));
        diesel::delete(to_delete)
            .execute(conn)
            .await
            .map(|_| ())
            .map_err(|err| Error::BulkDelete(ids.clone(), err))
    }

    pub fn invited_by(&self) -> Resource {
        Resource::new(self.invited_by_resource, self.invited_by)
    }
}

pub struct InvitationFilter<'a> {
    pub org_id: Option<OrgId>,
    pub invitee_email: Option<&'a str>,
    pub invited_by: Option<Resource>,
    pub accepted: Option<bool>,
    pub declined: Option<bool>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = invitations)]
pub struct NewInvitation {
    pub org_id: OrgId,
    pub invitee_email: String,
    pub invited_by_resource: ResourceType,
    pub invited_by: ResourceId,
}

impl NewInvitation {
    pub fn new<R>(org_id: OrgId, invitee_email: &str, invited_by: R) -> Self
    where
        R: Into<Resource>,
    {
        let invited_by = invited_by.into();
        NewInvitation {
            org_id,
            invitee_email: invitee_email.trim().to_lowercase(),
            invited_by_resource: invited_by.typ(),
            invited_by: invited_by.id(),
        }
    }

    pub async fn create(self, conn: &mut Conn<'_>) -> Result<Invitation, Error> {
        diesel::insert_into(invitations::table)
            .values(self)
            .get_result(conn)
            .await
            .map_err(Error::Create)
    }
}
