use diesel_async::scoped_futures::ScopedFutureExt;

use crate::auth::endpoint::Endpoint;
use crate::auth::resource::Resource;
use crate::models;
use crate::timestamp::NanosUtc;

use super::api::{self, user_service_server};

#[tonic::async_trait]
impl user_service_server::UserService for super::Grpc {
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
        self.run(|c| get(req, c).scope_boxed()).await
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

    async fn get_billing(
        &self,
        req: tonic::Request<api::UserServiceGetBillingRequest>,
    ) -> super::Resp<api::UserServiceGetBillingResponse> {
        self.trx(|c| get_billing(req, c).scope_boxed()).await
    }

    async fn update_billing(
        &self,
        req: tonic::Request<api::UserServiceUpdateBillingRequest>,
    ) -> super::Resp<api::UserServiceUpdateBillingResponse> {
        self.trx(|c| update_billing(req, c).scope_boxed()).await
    }

    async fn delete_billing(
        &self,
        req: tonic::Request<api::UserServiceDeleteBillingRequest>,
    ) -> super::Resp<api::UserServiceDeleteBillingResponse> {
        self.trx(|c| delete_billing(req, c).scope_boxed()).await
    }
}

async fn create(
    req: tonic::Request<api::UserServiceCreateRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::UserServiceCreateResponse> {
    // Temporary: we require authentication to create a new user. This means that somebody needs to
    // either be logged in, or have an email with an invitation token in there.
    let _claims = conn.claims(&req, Endpoint::UserCreate).await?;

    let inner = req.into_inner();
    let new_user = inner.as_new()?;
    let new_user = new_user.create(conn).await?;

    conn.context
        .mail
        .registration_confirmation(&new_user)
        .await?;

    let resp = api::UserServiceCreateResponse {
        user: Some(api::User::from_model(new_user)?),
    };
    Ok(tonic::Response::new(resp))
}

async fn get(
    req: tonic::Request<api::UserServiceGetRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::UserServiceGetResponse> {
    let claims = conn.claims(&req, Endpoint::UserGet).await?;
    let req = req.into_inner();
    let user = models::User::find_by_id(req.id.parse()?, conn).await?;
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
    conn: &mut models::Conn,
) -> super::Result<api::UserServiceUpdateResponse> {
    let claims = conn.claims(&req, Endpoint::UserUpdate).await?;
    let req = req.into_inner();
    let user = models::User::find_by_id(req.id.parse()?, conn).await?;
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
    conn: &mut models::Conn,
) -> super::Result<api::UserServiceDeleteResponse> {
    let claims = conn.claims(&req, Endpoint::UserDelete).await?;
    let req = req.into_inner();
    let user = models::User::find_by_id(req.id.parse()?, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => user_id == user.id,
        Resource::Org(_) => false,
        Resource::Host(_) => false,
        Resource::Node(_) => false,
    };
    if !is_allowed {
        super::forbidden!("Access denied for users delete")
    }
    models::User::delete(user.id, conn).await?;
    let resp = api::UserServiceDeleteResponse {};
    Ok(tonic::Response::new(resp))
}

async fn get_billing(
    req: tonic::Request<api::UserServiceGetBillingRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::UserServiceGetBillingResponse> {
    let claims = conn.claims(&req, Endpoint::UserGetBilling).await?;
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
    let user = models::User::find_by_id(user_id, conn).await?;
    let resp = api::UserServiceGetBillingResponse {
        billing_id: user.billing_id,
    };
    Ok(tonic::Response::new(resp))
}

async fn update_billing(
    req: tonic::Request<api::UserServiceUpdateBillingRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::UserServiceUpdateBillingResponse> {
    let claims = conn.claims(&req, Endpoint::UserUpdateBilling).await?;
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
    let mut user = models::User::find_by_id(user_id, conn).await?;
    user.billing_id = req.billing_id;
    user.update(conn).await?;
    let resp = api::UserServiceUpdateBillingResponse {
        billing_id: user.billing_id,
    };
    Ok(tonic::Response::new(resp))
}

async fn delete_billing(
    req: tonic::Request<api::UserServiceDeleteBillingRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::UserServiceDeleteBillingResponse> {
    let claims = conn.claims(&req, Endpoint::UserDeleteBilling).await?;
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
    let user = models::User::find_by_id(user_id, conn).await?;
    user.delete_billing(conn).await?;
    let resp = api::UserServiceDeleteBillingResponse {};
    Ok(tonic::Response::new(resp))
}

impl api::User {
    pub fn from_model(model: models::User) -> crate::Result<Self> {
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
    fn as_new(&self) -> crate::Result<models::NewUser<'_>> {
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
