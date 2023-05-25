use super::api::{self, auth_service_server};
use crate::auth::Endpoint::AuthUpdatePassword;
use crate::auth::{self, expiration_provider};
use crate::mail::MailClient;
use crate::models;
use diesel_async::scoped_futures::ScopedFutureExt;

/// This is a list of all the endpoints that a user is allowed to access with the jwt that they
/// generate on login. It does not contain endpoints like confirm, because those are accessed by a
/// token.
const USER_ENDPOINTS: [auth::Endpoint; 14] = [
    auth::Endpoint::AuthRefresh,
    auth::Endpoint::AuthUpdateUiPassword,
    auth::Endpoint::BabelAll,
    auth::Endpoint::BlockchainAll,
    auth::Endpoint::CommandAll,
    auth::Endpoint::DiscoveryAll,
    auth::Endpoint::HostAll,
    auth::Endpoint::HostProvisionAll,
    auth::Endpoint::InvitationAll,
    auth::Endpoint::KeyFileAll,
    auth::Endpoint::MetricsAll,
    auth::Endpoint::NodeAll,
    auth::Endpoint::OrgAll,
    auth::Endpoint::UserAll,
];

#[tonic::async_trait]
impl auth_service_server::AuthService for super::GrpcImpl {
    async fn login(
        &self,
        req: tonic::Request<api::AuthServiceLoginRequest>,
    ) -> super::Resp<api::AuthServiceLoginResponse> {
        self.trx(|c| login(req, c).scope_boxed()).await
    }

    async fn confirm(
        &self,
        req: tonic::Request<api::AuthServiceConfirmRequest>,
    ) -> super::Resp<api::AuthServiceConfirmResponse> {
        self.trx(|c| confirm(req, c).scope_boxed()).await
    }

    async fn refresh(
        &self,
        req: tonic::Request<api::AuthServiceRefreshRequest>,
    ) -> super::Resp<api::AuthServiceRefreshResponse> {
        let mut conn = self.conn().await?;
        let resp = refresh(req, &mut conn).await?;
        Ok(resp)
    }

    /// This endpoint triggers the sending of the reset-password email. The actual resetting is
    /// then done through the `update` function.
    async fn reset_password(
        &self,
        req: tonic::Request<api::AuthServiceResetPasswordRequest>,
    ) -> super::Resp<api::AuthServiceResetPasswordResponse> {
        self.trx(|c| reset_password(req, c).scope_boxed()).await
    }

    async fn update_password(
        &self,
        req: tonic::Request<api::AuthServiceUpdatePasswordRequest>,
    ) -> super::Resp<api::AuthServiceUpdatePasswordResponse> {
        self.trx(|c| update_password(req, c).scope_boxed()).await
    }

    async fn update_ui_password(
        &self,
        req: tonic::Request<api::AuthServiceUpdateUiPasswordRequest>,
    ) -> super::Resp<api::AuthServiceUpdateUiPasswordResponse> {
        self.trx(|c| update_ui_password(req, c).scope_boxed()).await
    }
}

async fn login(
    req: tonic::Request<api::AuthServiceLoginRequest>,
    conn: &mut diesel_async::AsyncPgConnection,
) -> super::Result<api::AuthServiceLoginResponse> {
    // This endpoint requires no auth, it is where you get your token from.
    let inner = req.into_inner();
    let user = models::User::login(&inner.email, &inner.password, conn).await?;
    let iat = chrono::Utc::now();
    let exp = expiration_provider::ExpirationProvider::expiration(auth::TOKEN_EXPIRATION_MINS)?;
    let claims = auth::Claims::new_user(user.id, iat, exp, USER_ENDPOINTS);
    let token = auth::Jwt { claims };
    let refresh_exp =
        expiration_provider::ExpirationProvider::expiration(auth::REFRESH_EXPIRATION_USER_MINS)?;
    let refresh = auth::Refresh::new(user.id, iat, refresh_exp)?;
    let resp = api::AuthServiceLoginResponse {
        token: token.encode()?,
        refresh: refresh.encode()?,
    };
    let mut resp = tonic::Response::new(resp);
    let refresh = refresh.as_set_cookie()?;
    resp.metadata_mut().insert("set-cookie", refresh.parse()?);
    Ok(resp)
}

async fn confirm(
    req: tonic::Request<api::AuthServiceConfirmRequest>,
    conn: &mut diesel_async::AsyncPgConnection,
) -> super::Result<api::AuthServiceConfirmResponse> {
    let claims = auth::get_claims(&req, auth::Endpoint::AuthConfirm, conn).await?;
    let auth::Resource::User(user_id) = claims.resource() else { super::forbidden!("Must be user") };
    let iat = chrono::Utc::now();
    let exp = expiration_provider::ExpirationProvider::expiration(auth::TOKEN_EXPIRATION_MINS)?;
    let claims = auth::Claims::new_user(user_id, iat, exp, USER_ENDPOINTS);
    let token = auth::Jwt { claims };
    let refresh_exp =
        expiration_provider::ExpirationProvider::expiration(auth::REFRESH_EXPIRATION_USER_MINS)?;
    let refresh = auth::Refresh::new(user_id, iat, refresh_exp)?;
    models::User::confirm(user_id, conn).await?;
    let resp = api::AuthServiceConfirmResponse {
        token: token.encode()?,
        refresh: refresh.encode()?,
    };
    let mut resp = tonic::Response::new(resp);
    let refresh = refresh.as_set_cookie()?;
    resp.metadata_mut().insert("set-cookie", refresh.parse()?);
    Ok(resp)
}

async fn refresh(
    req: tonic::Request<api::AuthServiceRefreshRequest>,
    conn: &mut diesel_async::AsyncPgConnection,
) -> super::Result<api::AuthServiceRefreshResponse> {
    let refresh = auth::get_refresh(&req)?;
    let req = req.into_inner();
    let token = auth::Jwt::decode_expired(&req.token)?;
    let refresh = req
        .refresh
        .map(|refresh| auth::Refresh::decode(&refresh))
        .transpose()?
        .or(refresh)
        .ok_or_else(|| {
            crate::Error::validation("Need refresh token from cookies or request body")
        })?;
    // For each type of resource, we perform some queries down below to verify that the resource
    // still exists.
    let (resource_type, resource_id) = match token.claims.resource() {
        auth::Resource::User(user_id) => {
            models::User::find_by_id(user_id, conn).await?;
            (auth::ResourceType::User, user_id)
        }
        auth::Resource::Org(org_id) => {
            models::Org::find_by_id(org_id, conn).await?;
            (auth::ResourceType::Org, org_id)
        }
        auth::Resource::Host(host_id) => {
            models::Host::find_by_id(host_id, conn).await?;
            (auth::ResourceType::Host, host_id)
        }
        auth::Resource::Node(node_id) => {
            models::Node::find_by_id(node_id, conn).await?;
            (auth::ResourceType::Node, node_id)
        }
    };
    if refresh.resource_id != resource_id {
        super::forbidden!("Jwt and refresh grantee don't match");
    }

    let iat = chrono::Utc::now();
    let exp = expiration_provider::ExpirationProvider::expiration(auth::TOKEN_EXPIRATION_MINS)?;
    let claims = auth::Claims {
        resource_type,
        resource_id,
        iat,
        exp: iat + exp,
        endpoints: token.claims.endpoints,
        data: token.claims.data,
    };
    let token = auth::Jwt { claims };
    let refresh_exp = refresh.duration();
    let refresh = auth::Refresh::new(resource_id, iat, refresh_exp)?;
    let resp = api::AuthServiceRefreshResponse {
        token: token.encode()?,
        refresh: refresh.encode()?,
    };
    let mut resp = tonic::Response::new(resp);
    let val = refresh.as_set_cookie()?;
    resp.metadata_mut().insert("set-cookie", val.parse()?);
    Ok(resp)
}

/// This endpoint triggers the sending of the reset-password email. The actual resetting is
/// then done through the `update` function.
async fn reset_password(
    req: tonic::Request<api::AuthServiceResetPasswordRequest>,
    conn: &mut diesel_async::AsyncPgConnection,
) -> super::Result<api::AuthServiceResetPasswordResponse> {
    let req = req.into_inner();
    // We are going to query the user and send them an email, but when something goes wrong we
    // are not going to return an error. This hides whether or not a user is registered with
    // us to the caller of the api, because this info may be sensitive and this endpoint is not
    // protected by any authentication.
    let user = models::User::find_by_email(&req.email, conn).await;
    if let Ok(user) = user {
        let _ = user.email_reset_password(conn).await;
    }

    let resp = api::AuthServiceResetPasswordResponse {};
    Ok(tonic::Response::new(resp))
}

async fn update_password(
    req: tonic::Request<api::AuthServiceUpdatePasswordRequest>,
    conn: &mut diesel_async::AsyncPgConnection,
) -> super::Result<api::AuthServiceUpdatePasswordResponse> {
    let claims = auth::get_claims(&req, AuthUpdatePassword, conn).await?;
    let req = req.into_inner();
    // Only users have passwords; orgs, hosts and nodes do not.
    let auth::Resource::User(user_id) = claims.resource() else { super::forbidden!("Need user_id") };
    let cur_user = models::User::find_by_id(user_id, conn)
        .await?
        .update_password(&req.password, conn)
        .await?;
    let resp = api::AuthServiceUpdatePasswordResponse {};

    // Send notification mail
    MailClient::new().update_password(&cur_user).await?;
    Ok(tonic::Response::new(resp))
}

async fn update_ui_password(
    req: tonic::Request<api::AuthServiceUpdateUiPasswordRequest>,
    conn: &mut diesel_async::AsyncPgConnection,
) -> super::Result<api::AuthServiceUpdateUiPasswordResponse> {
    let claims = auth::get_claims(&req, auth::Endpoint::AuthUpdateUiPassword, conn).await?;
    let auth::Resource::User(user_id_) = claims.resource() else { super::forbidden!("Must be user") };
    let req = req.into_inner();
    let user_id = req.user_id.parse()?;
    let is_allowed = user_id == user_id_;
    if !is_allowed {
        super::forbidden!("Can only update your own password");
    }
    let user = models::User::find_by_id(user_id, conn).await?;
    user.verify_password(&req.old_password)?;
    user.update_password(&req.new_password, conn).await?;

    let resp = api::AuthServiceUpdateUiPasswordResponse {};

    // Send notification mail
    MailClient::new().update_password(&user).await?;
    Ok(tonic::Response::new(resp))
}
