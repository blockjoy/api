use diesel_async::scoped_futures::ScopedFutureExt;

use crate::auth::endpoint::Endpoint;
use crate::auth::resource::Resource;
use crate::config::Context;
use crate::database::{Conn, Transaction};
use crate::models::user::{NewUser, UpdateUser, User};
use crate::timestamp::NanosUtc;

use super::api::{self, user_service_server};

#[tonic::async_trait]
impl user_service_server::UserService for super::Grpc {
    async fn create(
        &self,
        req: tonic::Request<api::UserServiceCreateRequest>,
    ) -> super::Resp<api::UserServiceCreateResponse> {
        self.context
            .write(|conn, ctx| create(req, conn, ctx).scope_boxed())
            .await
    }

    async fn get(
        &self,
        req: tonic::Request<api::UserServiceGetRequest>,
    ) -> super::Resp<api::UserServiceGetResponse> {
        self.context
            .write(|conn, ctx| get(req, conn, ctx).scope_boxed())
            .await
    }

    async fn update(
        &self,
        req: tonic::Request<api::UserServiceUpdateRequest>,
    ) -> super::Resp<api::UserServiceUpdateResponse> {
        self.context
            .write(|conn, ctx| update(req, conn, ctx).scope_boxed())
            .await
    }

    async fn delete(
        &self,
        req: tonic::Request<api::UserServiceDeleteRequest>,
    ) -> super::Resp<api::UserServiceDeleteResponse> {
        self.context
            .write(|conn, ctx| delete(req, conn, ctx).scope_boxed())
            .await
    }

    async fn get_billing(
        &self,
        req: tonic::Request<api::UserServiceGetBillingRequest>,
    ) -> super::Resp<api::UserServiceGetBillingResponse> {
        self.context
            .read(|conn, ctx| get_billing(req, conn, ctx).scope_boxed())
            .await
    }

    async fn update_billing(
        &self,
        req: tonic::Request<api::UserServiceUpdateBillingRequest>,
    ) -> super::Resp<api::UserServiceUpdateBillingResponse> {
        self.context
            .write(|conn, ctx| update_billing(req, conn, ctx).scope_boxed())
            .await
    }

    async fn delete_billing(
        &self,
        req: tonic::Request<api::UserServiceDeleteBillingRequest>,
    ) -> super::Resp<api::UserServiceDeleteBillingResponse> {
        self.context
            .write(|conn, ctx| delete_billing(req, conn, ctx).scope_boxed())
            .await
    }
}

async fn create(
    req: tonic::Request<api::UserServiceCreateRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::UserServiceCreateResponse> {
    // Temporary: we require authentication to create a new user. This means that somebody needs to
    // either be logged in, or have an email with an invitation token in there.
    let _claims = ctx.claims(&req, Endpoint::UserCreate).await?;

    let inner = req.into_inner();
    let new_user = inner.as_new()?;
    let new_user = new_user.create(conn).await?;

    ctx.mail.registration_confirmation(&new_user).await?;

    let resp = api::UserServiceCreateResponse {
        user: Some(api::User::from_model(new_user)?),
    };
    Ok(tonic::Response::new(resp))
}

async fn get(
    req: tonic::Request<api::UserServiceGetRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::UserServiceGetResponse> {
    let claims = ctx.claims(&req, Endpoint::UserGet).await?;
    let req = req.into_inner();
    let user = User::find_by_id(req.id.parse()?, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => user_id == user.id,
        Resource::Org(_) => false,
        Resource::Host(_) => false,
        Resource::Node(_) => false,
    };
    if !is_allowed {
        super::forbidden!("Access denied for users get")
    }
    let resp = api::UserServiceGetResponse {
        user: Some(api::User::from_model(user)?),
    };
    Ok(tonic::Response::new(resp))
}

async fn update(
    req: tonic::Request<api::UserServiceUpdateRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::UserServiceUpdateResponse> {
    let claims = ctx.claims(&req, Endpoint::UserUpdate).await?;
    let req = req.into_inner();
    let user = User::find_by_id(req.id.parse()?, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => user_id == user.id,
        Resource::Org(_) => false,
        Resource::Host(_) => false,
        Resource::Node(_) => false,
    };
    if !is_allowed {
        super::forbidden!("Access denied for users update")
    }
    let user = req.as_update()?.update(conn).await?;
    let resp = api::UserServiceUpdateResponse {
        user: Some(api::User::from_model(user)?),
    };
    Ok(tonic::Response::new(resp))
}

async fn delete(
    req: tonic::Request<api::UserServiceDeleteRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::UserServiceDeleteResponse> {
    let claims = ctx.claims(&req, Endpoint::UserDelete).await?;
    let req = req.into_inner();
    let user = User::find_by_id(req.id.parse()?, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => user_id == user.id,
        Resource::Org(_) => false,
        Resource::Host(_) => false,
        Resource::Node(_) => false,
    };
    if !is_allowed {
        super::forbidden!("Access denied for users delete")
    }
    User::delete(user.id, conn).await?;
    let resp = api::UserServiceDeleteResponse {};
    Ok(tonic::Response::new(resp))
}

async fn get_billing(
    req: tonic::Request<api::UserServiceGetBillingRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::UserServiceGetBillingResponse> {
    let claims = ctx.claims(&req, Endpoint::UserGetBilling).await?;
    let req = req.into_inner();
    let user_id = req.user_id.parse()?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id_) => user_id == user_id_,
        Resource::Org(_) => false,
        Resource::Host(_) => false,
        Resource::Node(_) => false,
    };
    if !is_allowed {
        super::forbidden!("Access denied for users get billing");
    }
    let user = User::find_by_id(user_id, conn).await?;
    let resp = api::UserServiceGetBillingResponse {
        billing_id: user.billing_id,
    };
    Ok(tonic::Response::new(resp))
}

async fn update_billing(
    req: tonic::Request<api::UserServiceUpdateBillingRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::UserServiceUpdateBillingResponse> {
    let claims = ctx.claims(&req, Endpoint::UserUpdateBilling).await?;
    let req = req.into_inner();
    let user_id = req.user_id.parse()?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id_) => user_id == user_id_,
        Resource::Org(_) => false,
        Resource::Host(_) => false,
        Resource::Node(_) => false,
    };
    if !is_allowed {
        super::forbidden!("Access denied for users update billing");
    }
    let mut user = User::find_by_id(user_id, conn).await?;
    user.billing_id = req.billing_id;
    user.update(conn).await?;
    let resp = api::UserServiceUpdateBillingResponse {
        billing_id: user.billing_id,
    };
    Ok(tonic::Response::new(resp))
}

async fn delete_billing(
    req: tonic::Request<api::UserServiceDeleteBillingRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::UserServiceDeleteBillingResponse> {
    let claims = ctx.claims(&req, Endpoint::UserDeleteBilling).await?;
    let req = req.into_inner();
    let user_id = req.user_id.parse()?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id_) => user_id == user_id_,
        Resource::Org(_) => false,
        Resource::Host(_) => false,
        Resource::Node(_) => false,
    };
    if !is_allowed {
        super::forbidden!("Access denied for users delete billing");
    }
    let user = User::find_by_id(user_id, conn).await?;
    user.delete_billing(conn).await?;
    let resp = api::UserServiceDeleteBillingResponse {};
    Ok(tonic::Response::new(resp))
}

impl api::User {
    pub fn from_model(model: User) -> crate::Result<Self> {
        let user = Self {
            id: model.id.to_string(),
            email: model.email,
            first_name: model.first_name,
            last_name: model.last_name,
            created_at: Some(NanosUtc::from(model.created_at).into()),
            updated_at: None,
        };
        Ok(user)
    }
}

impl api::UserServiceCreateRequest {
    fn as_new(&self) -> crate::Result<NewUser<'_>> {
        NewUser::new(
            &self.email,
            &self.first_name,
            &self.last_name,
            &self.password,
        )
    }
}

impl api::UserServiceUpdateRequest {
    pub fn as_update(&self) -> crate::Result<UpdateUser<'_>> {
        Ok(UpdateUser {
            id: self.id.parse()?,
            first_name: self.first_name.as_deref(),
            last_name: self.last_name.as_deref(),
        })
    }
}
