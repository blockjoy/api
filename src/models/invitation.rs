use chrono::{DateTime, Utc};
use diesel::dsl;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;

use crate::auth::resource::{OrgId, UserId};
use crate::database::Conn;
use crate::Result;

use super::schema::invitations;

#[derive(Debug, Clone, Queryable)]
pub struct Invitation {
    pub id: uuid::Uuid,
    pub created_by: UserId,
    pub org_id: OrgId,
    pub invitee_email: String,
    pub created_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub declined_at: Option<DateTime<Utc>>,
}

impl Invitation {
    pub async fn find_by_id(id: uuid::Uuid, conn: &mut Conn<'_>) -> Result<Self> {
        let invitation = invitations::table.find(id).get_result(conn).await?;
        Ok(invitation)
    }

    pub async fn received(email: &str, conn: &mut Conn<'_>) -> Result<Vec<Self>> {
        let pending = invitations::table
            .filter(invitations::invitee_email.eq(email))
            .filter(invitations::accepted_at.is_null())
            .filter(invitations::declined_at.is_null())
            .order_by(invitations::created_at.desc())
            .get_results(conn)
            .await?;
        Ok(pending)
    }

    pub async fn filter(filter: InvitationFilter<'_>, conn: &mut Conn<'_>) -> Result<Vec<Self>> {
        let mut query = invitations::table.into_boxed();

        if let Some(org_id) = filter.org_id {
            query = query.filter(invitations::org_id.eq(org_id));
        }
        if let Some(invitee_email) = filter.invitee_email {
            query = query.filter(invitations::invitee_email.eq(invitee_email));
        }
        if let Some(created_by) = filter.created_by {
            query = query.filter(invitations::created_by.eq(created_by));
        }
        if let Some(true) = filter.accepted {
            query = query.filter(invitations::accepted_at.is_not_null());
        }
        if let Some(false) = filter.accepted {
            query = query.filter(invitations::accepted_at.is_null());
        }
        if let Some(true) = filter.declined {
            query = query.filter(invitations::declined_at.is_not_null());
        }
        if let Some(false) = filter.declined {
            query = query.filter(invitations::declined_at.is_null());
        }

        let invites = query
            .select(invitations::all_columns)
            .get_results(conn)
            .await?;
        Ok(invites)
    }

    pub async fn has_open_invite(org_id: OrgId, email: &str, conn: &mut Conn<'_>) -> Result<bool> {
        let invitation = invitations::table
            .filter(invitations::org_id.eq(org_id))
            .filter(invitations::invitee_email.eq(email))
            .filter(invitations::accepted_at.is_null())
            .filter(invitations::declined_at.is_null());
        Ok(diesel::select(dsl::exists(invitation))
            .get_result(conn)
            .await?)
    }

    pub async fn accept(self, conn: &mut Conn<'_>) -> Result<Self> {
        let invitation = diesel::update(invitations::table.find(self.id))
            .set(invitations::accepted_at.eq(chrono::Utc::now()))
            .get_result(conn)
            .await?;
        Ok(invitation)
    }

    pub async fn decline(&self, conn: &mut Conn<'_>) -> Result<Self> {
        let invitation = diesel::update(invitations::table.find(self.id))
            .set(invitations::declined_at.eq(chrono::Utc::now()))
            .get_result(conn)
            .await?;
        Ok(invitation)
    }

    pub async fn revoke(&self, conn: &mut Conn<'_>) -> Result<()> {
        diesel::delete(invitations::table.find(self.id))
            .execute(conn)
            .await?;
        Ok(())
    }

    pub async fn remove_by_org_user(
        user_email: &str,
        org_id: OrgId,
        conn: &mut Conn<'_>,
    ) -> Result<()> {
        let to_delete = invitations::table
            .filter(invitations::invitee_email.eq(user_email))
            .filter(invitations::org_id.eq(org_id));
        diesel::delete(to_delete).execute(conn).await?;
        Ok(())
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = invitations)]
pub struct NewInvitation {
    pub created_by: UserId,
    pub org_id: OrgId,
    pub invitee_email: String,
}

impl NewInvitation {
    pub async fn create(mut self, conn: &mut Conn<'_>) -> Result<Invitation> {
        self.invitee_email = self.invitee_email.trim().to_lowercase();
        let invitation = diesel::insert_into(invitations::table)
            .values(self)
            .get_result(conn)
            .await?;
        Ok(invitation)
    }
}

pub struct InvitationFilter<'a> {
    pub org_id: Option<OrgId>,
    pub invitee_email: Option<&'a str>,
    pub created_by: Option<UserId>,
    pub accepted: Option<bool>,
    pub declined: Option<bool>,
}
