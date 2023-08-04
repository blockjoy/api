use anyhow::anyhow;
use diesel_async::scoped_futures::ScopedFutureExt;

use crate::auth::endpoint::Endpoint;
use crate::auth::resource::Resource;
use crate::database::{ReadConn, Transaction, WriteConn};
use crate::models::user::{NewUser, UpdateUser, User};
use crate::timestamp::NanosUtc;

use super::api::{self, user_service_server};

#[tonic::async_trait]
impl user_service_server::UserService for super::Grpc {
    async fn create(
        &self,
        req: tonic::Request<api::UserServiceCreateRequest>,
    ) -> super::Resp<api::UserServiceCreateResponse> {
        self.write(|write| create(req, write).scope_boxed()).await
    }

    async fn get(
        &self,
        req: tonic::Request<api::UserServiceGetRequest>,
    ) -> super::Resp<api::UserServiceGetResponse> {
        self.read(|read| get(req, read).scope_boxed()).await
    }

    async fn update(
        &self,
        req: tonic::Request<api::UserServiceUpdateRequest>,
    ) -> super::Resp<api::UserServiceUpdateResponse> {
        self.write(|write| update(req, write).scope_boxed()).await
    }

    async fn delete(
        &self,
        req: tonic::Request<api::UserServiceDeleteRequest>,
    ) -> super::Resp<api::UserServiceDeleteResponse> {
        self.write(|write| delete(req, write).scope_boxed()).await
    }

    async fn get_billing(
        &self,
        req: tonic::Request<api::UserServiceGetBillingRequest>,
    ) -> super::Resp<api::UserServiceGetBillingResponse> {
        self.read(|read| get_billing(req, read).scope_boxed()).await
    }

    async fn update_billing(
        &self,
        req: tonic::Request<api::UserServiceUpdateBillingRequest>,
    ) -> super::Resp<api::UserServiceUpdateBillingResponse> {
        self.write(|write| update_billing(req, write).scope_boxed())
            .await
    }

    async fn delete_billing(
        &self,
        req: tonic::Request<api::UserServiceDeleteBillingRequest>,
    ) -> super::Resp<api::UserServiceDeleteBillingResponse> {
        self.write(|write| delete_billing(req, write).scope_boxed())
            .await
    }
}

async fn create(
    req: tonic::Request<api::UserServiceCreateRequest>,
    write: WriteConn<'_, '_>,
) -> super::Result<api::UserServiceCreateResponse> {
    let WriteConn { conn, ctx, .. } = write;
    // Temporary: we require authentication to create a new user. This means that somebody needs to
    // either be logged in, or have an email with an invitation token in there.
    let _claims = ctx.claims(&req, Endpoint::UserCreate, conn).await?;

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
    read: ReadConn<'_, '_>,
) -> super::Result<api::UserServiceGetResponse> {
    let ReadConn { conn, ctx } = read;
    let claims = ctx.claims(&req, Endpoint::UserGet, conn).await?;
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
    write: WriteConn<'_, '_>,
) -> super::Result<api::UserServiceUpdateResponse> {
    let WriteConn { conn, ctx, .. } = write;
    let claims = ctx.claims(&req, Endpoint::UserUpdate, conn).await?;
    let req = req.into_inner();
    let user = User::find_by_id(req.id.parse()?, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => user_id == user.id,
        Resource::Org(_) => false,
        Resource::Host(_) => false,
        Resource::Node(_) => false,
    };
    let role_ok = req.role.map(|_| user.is_blockjoy_admin).unwrap_or(true);
    if !is_allowed || !role_ok {
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
    write: WriteConn<'_, '_>,
) -> super::Result<api::UserServiceDeleteResponse> {
    let WriteConn { conn, ctx, .. } = write;
    let claims = ctx.claims(&req, Endpoint::UserDelete, conn).await?;
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
    read: ReadConn<'_, '_>,
) -> super::Result<api::UserServiceGetBillingResponse> {
    let ReadConn { conn, ctx } = read;
    let claims = ctx.claims(&req, Endpoint::UserGetBilling, conn).await?;
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
    write: WriteConn<'_, '_>,
) -> super::Result<api::UserServiceUpdateBillingResponse> {
    let WriteConn { conn, ctx, .. } = write;
    let claims = ctx.claims(&req, Endpoint::UserUpdateBilling, conn).await?;
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
    write: WriteConn<'_, '_>,
) -> super::Result<api::UserServiceDeleteBillingResponse> {
    let WriteConn { conn, ctx, .. } = write;
    let claims = ctx.claims(&req, Endpoint::UserDeleteBilling, conn).await?;
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
        let mut user = Self {
            id: model.id.to_string(),
            email: model.email,
            first_name: model.first_name,
            last_name: model.last_name,
            role: 0,
            created_at: Some(NanosUtc::from(model.created_at).into()),
            updated_at: None,
        };
        user.set_role(api::UserRole::from_model(model.is_blockjoy_admin));
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
            is_blockjoy_admin: self
                .role
                .map(|r| api::UserRole::from_i32(r).ok_or_else(|| anyhow!("Invalid org role: {r}")))
                .transpose()?
                .map(api::UserRole::into_model),
        })
    }
}

impl api::UserRole {
    pub fn from_model(is_admin: bool) -> Self {
        if is_admin {
            Self::BlockjoyStaff
        } else {
            Self::Unprivileged
        }
    }

    pub fn into_model(self) -> bool {
        self == api::UserRole::BlockjoyStaff
    }
}
