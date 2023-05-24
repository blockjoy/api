use super::schema::invitations;
use crate::Result;
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel_async::{AsyncPgConnection, RunQueryDsl};

#[derive(Debug, Queryable)]
pub struct Invitation {
    pub id: uuid::Uuid,
    pub created_by_user: uuid::Uuid,
    pub created_for_org: uuid::Uuid,
    pub invitee_email: String,
    pub created_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub declined_at: Option<DateTime<Utc>>,
    pub created_by_user_name: String,
    pub created_for_org_name: String,
}

impl Invitation {
    pub async fn find_by_id(id: uuid::Uuid, conn: &mut AsyncPgConnection) -> Result<Self> {
        let invitation = invitations::table.find(id).get_result(conn).await?;
        Ok(invitation)
    }

    pub async fn received(email: &str, conn: &mut AsyncPgConnection) -> Result<Vec<Self>> {
        let pending = invitations::table
            .filter(invitations::invitee_email.eq(email))
            .filter(invitations::accepted_at.is_null())
            .filter(invitations::declined_at.is_null())
            .order_by(invitations::created_at.desc())
            .get_results(conn)
            .await?;
        Ok(pending)
    }

    pub async fn filter(
        filter: InvitationFilter<'_>,
        conn: &mut AsyncPgConnection,
    ) -> Result<Vec<Self>> {
        let mut query = invitations::table.into_boxed();

        if let Some(org_id) = filter.org_id {
            query = query.filter(invitations::created_for_org.eq(org_id));
        }
        if let Some(invitee_email) = filter.invitee_email {
            query = query.filter(invitations::invitee_email.eq(invitee_email));
        }
        if let Some(created_by) = filter.created_by {
            query = query.filter(invitations::created_by_user.eq(created_by));
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

    pub async fn accept(self, conn: &mut AsyncPgConnection) -> Result<Self> {
        let invitation = diesel::update(invitations::table.find(self.id))
            .set(invitations::accepted_at.eq(chrono::Utc::now()))
            .get_result(conn)
            .await?;
        Ok(invitation)
    }

    pub async fn decline(&self, conn: &mut AsyncPgConnection) -> Result<Self> {
        let invitation = diesel::update(invitations::table.find(self.id))
            .set(invitations::declined_at.eq(chrono::Utc::now()))
            .get_result(conn)
            .await?;
        Ok(invitation)
    }

    pub async fn revoke(&self, conn: &mut AsyncPgConnection) -> Result<()> {
        diesel::delete(invitations::table.find(self.id))
            .execute(conn)
            .await?;
        Ok(())
    }

    pub async fn remove_by_org_user(
        user_email: &str,
        org_id: uuid::Uuid,
        conn: &mut AsyncPgConnection,
    ) -> Result<()> {
        let to_delete = invitations::table
            .filter(invitations::invitee_email.eq(user_email))
            .filter(invitations::created_for_org.eq(org_id));
        diesel::delete(to_delete).execute(conn).await?;
        Ok(())
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = invitations)]
pub struct NewInvitation<'a> {
    pub created_by_user: uuid::Uuid,
    pub created_by_user_name: String,
    pub created_for_org: uuid::Uuid,
    pub created_for_org_name: String,
    pub invitee_email: &'a str,
}

impl<'a> NewInvitation<'a> {
    pub async fn create(self, conn: &mut AsyncPgConnection) -> Result<Invitation> {
        let invitation = diesel::insert_into(invitations::table)
            .values(self)
            .get_result(conn)
            .await?;
        Ok(invitation)
    }
}

pub struct InvitationFilter<'a> {
    pub org_id: Option<uuid::Uuid>,
    pub invitee_email: Option<&'a str>,
    pub created_by: Option<uuid::Uuid>,
    pub accepted: Option<bool>,
    pub declined: Option<bool>,
}
