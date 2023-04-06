use super::schema::invitations;
use crate::auth::FindableById;
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

#[tonic::async_trait]
impl FindableById for Invitation {
    async fn find_by_id(id: uuid::Uuid, conn: &mut AsyncPgConnection) -> Result<Self> {
        let invitation = invitations::table.find(id).get_result(conn).await?;
        Ok(invitation)
    }
}

impl Invitation {
    pub async fn find_by_creator_for_email(
        creator_id: uuid::Uuid,
        invitee_email: &str,
        conn: &mut AsyncPgConnection,
    ) -> Result<Self> {
        let invitation = invitations::table
            .filter(invitations::created_by_user.eq(creator_id))
            .filter(invitations::invitee_email.eq(invitee_email))
            .get_result(conn)
            .await?;
        Ok(invitation)
    }

    pub async fn pending(org_id: uuid::Uuid, conn: &mut AsyncPgConnection) -> Result<Vec<Self>> {
        let pending = invitations::table
            .filter(invitations::created_for_org.eq(org_id))
            .filter(invitations::accepted_at.is_null())
            .filter(invitations::declined_at.is_null())
            .order_by(invitations::created_at.desc())
            .get_results(conn)
            .await?;
        Ok(pending)
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

    pub async fn accept(self, conn: &mut AsyncPgConnection) -> Result<Self> {
        let invitation = diesel::update(invitations::table.find(self.id))
            .set(invitations::accepted_at.eq(chrono::Utc::now()))
            .get_result(conn)
            .await?;
        Ok(invitation)
    }

    pub async fn decline(self, conn: &mut AsyncPgConnection) -> Result<Self> {
        let invitation = diesel::update(invitations::table.find(self.id))
            .set(invitations::declined_at.eq(chrono::Utc::now()))
            .get_result(conn)
            .await?;
        Ok(invitation)
    }

    pub async fn revoke(self, conn: &mut AsyncPgConnection) -> Result<()> {
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
