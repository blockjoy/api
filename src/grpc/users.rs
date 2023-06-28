use diesel_async::scoped_futures::ScopedFutureExt;

use super::api::{self, user_service_server};
use crate::auth::token::{Endpoint, Resource};
// use crate::mail::MailClient;
use crate::{auth, models};

#[tonic::async_trait]
impl user_service_server::UserService for super::GrpcImpl {
    async fn create(
        &self,
        req: tonic::Request<api::UserServiceCreateRequest>,
    ) -> super::Resp<api::UserServiceCreateResponse> {
        self.trx(|c| create(req, c).scope_boxed()).await
    }

    async fn get(
        &self,
        req: tonic::Request<api::UserServiceGetRequest>,
    ) -> super::Resp<api::UserServiceGetResponse> {
        let mut conn = self.conn().await?;
        let resp = get(req, &mut conn).await?;
        Ok(resp)
    }

    async fn update(
        &self,
        req: tonic::Request<api::UserServiceUpdateRequest>,
    ) -> super::Resp<api::UserServiceUpdateResponse> {
        self.trx(|c| update(req, c).scope_boxed()).await
    }

    async fn delete(
        &self,
        req: tonic::Request<api::UserServiceDeleteRequest>,
    ) -> super::Resp<api::UserServiceDeleteResponse> {
        self.trx(|c| delete(req, c).scope_boxed()).await
    }
}

async fn create(
    req: tonic::Request<api::UserServiceCreateRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::UserServiceCreateResponse> {
    // Temporary: we require authentication to create a new user. This means that somebody needs to
    // either be logged in, or have an email with an invitation token in there.
    auth::get_claims(&req, Endpoint::UserCreate, conn).await?;
    let inner = req.into_inner();
    let new_user = inner.as_new()?;
    let new_user = new_user.create(conn).await?;

    // Since new users can't create accounts on their own initiative anymore, we don't need to
    // confirm email addresses anymore.
    // MailClient::new(&conn.context.config)
    //     .registration_confirmation(&new_user, &conn.context.cipher)
    //     .await?;

    // Instead we immediately mark the user as confirmed immediately
    models::User::confirm(new_user.id, conn).await?;

    let resp = api::UserServiceCreateResponse {
        user: Some(api::User::from_model(new_user)?),
    };
    Ok(tonic::Response::new(resp))
}

async fn get(
    req: tonic::Request<api::UserServiceGetRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::UserServiceGetResponse> {
    let claims = auth::get_claims(&req, Endpoint::UserGet, conn).await?;
    let req = req.into_inner();
    let user = models::User::find_by_id(req.id.parse()?, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => user_id == user.id,
        Resource::Org(_) => false,
        Resource::Host(_) => false,
        Resource::Node(_) => false,
    };
    if !is_allowed {
        super::forbidden!("Access not allowed")
    }
    let resp = api::UserServiceGetResponse {
        user: Some(api::User::from_model(user)?),
    };
    Ok(tonic::Response::new(resp))
}

async fn update(
    req: tonic::Request<api::UserServiceUpdateRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::UserServiceUpdateResponse> {
    let claims = auth::get_claims(&req, Endpoint::UserUpdate, conn).await?;
    let req = req.into_inner();
    let user = models::User::find_by_id(req.id.parse()?, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => user_id == user.id,
        Resource::Org(_) => false,
        Resource::Host(_) => false,
        Resource::Node(_) => false,
    };
    if !is_allowed {
        super::forbidden!("Access not allowed")
    }
    let user = req.as_update()?.update(conn).await?;
    let resp = api::UserServiceUpdateResponse {
        user: Some(api::User::from_model(user)?),
    };
    Ok(tonic::Response::new(resp))
}

async fn delete(
    req: tonic::Request<api::UserServiceDeleteRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::UserServiceDeleteResponse> {
    let claims = auth::get_claims(&req, Endpoint::UserUpdate, conn).await?;
    let req = req.into_inner();
    let user = models::User::find_by_id(req.id.parse()?, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => user_id == user.id,
        Resource::Org(_) => false,
        Resource::Host(_) => false,
        Resource::Node(_) => false,
    };
    if !is_allowed {
        super::forbidden!("Access not allowed")
    }
    models::User::delete(user.id, conn).await?;
    let resp = api::UserServiceDeleteResponse {};
    Ok(tonic::Response::new(resp))
}

impl api::User {
    pub fn from_model(model: models::User) -> crate::Result<Self> {
        let user = Self {
            id: model.id.to_string(),
            email: model.email,
            first_name: model.first_name,
            last_name: model.last_name,
            created_at: Some(super::try_dt_to_ts(model.created_at)?),
            updated_at: None,
        };
        Ok(user)
    }
}

impl api::UserServiceCreateRequest {
    fn as_new(&self) -> crate::Result<models::NewUser> {
        models::NewUser::new(
            &self.email,
            &self.first_name,
            &self.last_name,
            &self.password,
        )
    }
}

impl api::UserServiceUpdateRequest {
    pub fn as_update(&self) -> crate::Result<models::UpdateUser<'_>> {
        Ok(models::UpdateUser {
            id: self.id.parse()?,
            first_name: self.first_name.as_deref(),
            last_name: self.last_name.as_deref(),
        })
    }
}
