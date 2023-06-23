use diesel_async::scoped_futures::ScopedFutureExt;

use super::api::{self, auth_service_server};
use crate::auth::token::refresh::Refresh;
use crate::auth::token::{Claims, Endpoint, Resource, ResourceType};
use crate::mail::MailClient;
use crate::{auth, models};

/// This is a list of all the endpoints that a user is allowed to access with the jwt that they
/// generate on login. It does not contain endpoints like confirm, because those are accessed by a
/// token.
const USER_ENDPOINTS: [Endpoint; 14] = [
    Endpoint::AuthRefresh,
    Endpoint::AuthUpdateUiPassword,
    Endpoint::BabelAll,
    Endpoint::BlockchainAll,
    Endpoint::CommandAll,
    Endpoint::DiscoveryAll,
    Endpoint::HostAll,
    Endpoint::HostProvisionAll,
    Endpoint::InvitationAll,
    Endpoint::KeyFileAll,
    Endpoint::MetricsAll,
    Endpoint::NodeAll,
    Endpoint::OrgAll,
    Endpoint::UserAll,
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
    conn: &mut models::Conn,
) -> super::Result<api::AuthServiceLoginResponse> {
    // This endpoint requires no auth, it is where you get your token from.
    let inner = req.into_inner();
    let user = models::User::login(&inner.email, &inner.password, conn).await?;

    let iat = chrono::Utc::now();
    let exp = conn.context.config.token.expire.token.try_into()?;
    let claims = Claims::new_user(user.id, iat, exp, USER_ENDPOINTS)?;

    let refresh_exp = conn.context.config.token.expire.refresh_user.try_into()?;
    let refresh = Refresh::new(user.id, iat, refresh_exp)?;

    let resp = api::AuthServiceLoginResponse {
        token: conn.context.cipher.jwt.encode(&claims)?.into(),
        refresh: conn.context.cipher.refresh.encode(&refresh)?.into(),
    };

    let mut resp = tonic::Response::new(resp);
    resp.metadata_mut()
        .insert("set-cookie", conn.context.cipher.refresh.cookie(&refresh)?);

    Ok(resp)
}

async fn confirm(
    req: tonic::Request<api::AuthServiceConfirmRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::AuthServiceConfirmResponse> {
    let claims = auth::get_claims(&req, Endpoint::AuthConfirm, conn).await?;
    let Resource::User(user_id) = claims.resource() else { super::forbidden!("Must be user") };

    let iat = chrono::Utc::now();
    let exp = conn.context.config.token.expire.token.try_into()?;
    let claims = Claims::new_user(user_id, iat, exp, USER_ENDPOINTS)?;

    let refresh_exp = conn.context.config.token.expire.refresh_user.try_into()?;
    let refresh = Refresh::new(user_id, iat, refresh_exp)?;

    models::User::confirm(user_id, conn).await?;

    let resp = api::AuthServiceConfirmResponse {
        token: conn.context.cipher.jwt.encode(&claims)?.into(),
        refresh: conn.context.cipher.refresh.encode(&refresh)?.into(),
    };

    let mut resp = tonic::Response::new(resp);
    resp.metadata_mut()
        .insert("set-cookie", conn.context.cipher.refresh.cookie(&refresh)?);

    Ok(resp)
}

async fn refresh(
    req: tonic::Request<api::AuthServiceRefreshRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::AuthServiceRefreshResponse> {
    let refresh = auth::get_refresh(&req, &conn.context)?;
    let req = req.into_inner();
    let mut decoded = conn.context.cipher.jwt.decode_expired(&req.token)?;
    let refresh = req
        .refresh
        .map(|refresh| conn.context.cipher.refresh.decode(&refresh))
        .transpose()?
        .or(refresh)
        .ok_or_else(|| {
            crate::Error::validation("Need refresh token from cookies or request body")
        })?;

    // For each type of resource, we perform some queries down below to verify that the resource
    // still exists.
    let (resource_type, resource_id) = match decoded.resource() {
        Resource::User(user_id) => {
            models::User::find_by_id(user_id, conn).await?;
            (ResourceType::User, user_id)
        }
        Resource::Org(org_id) => {
            models::Org::find_by_id(org_id, conn).await?;
            (ResourceType::Org, org_id)
        }
        Resource::Host(host_id) => {
            models::Host::find_by_id(host_id, conn).await?;
            (ResourceType::Host, host_id)
        }
        Resource::Node(node_id) => {
            models::Node::find_by_id(node_id, conn).await?;
            (ResourceType::Node, node_id)
        }
    };
    if refresh.resource_id != resource_id {
        super::forbidden!("Jwt and refresh grantee don't match");
    }

    // The following is a workaround that sort of patches existing host tokens. Removeme
    if !decoded.endpoints.includes(Endpoint::HostUpdate) {
        decoded.endpoints = match decoded.endpoints {
            auth::token::Endpoints::Wildcard => auth::token::Endpoints::Wildcard,
            auth::token::Endpoints::Single(e) => auth::token::Endpoints::Single(e),
            auth::token::Endpoints::Multiple(mut es) => {
                es.insert(7, Endpoint::HostUpdate);
                auth::token::Endpoints::Multiple(es)
            }
        }
    }

    let iat = chrono::Utc::now();
    let exp = conn.context.config.token.expire.token.try_into()?;
    let claims = Claims::new_with_data(
        resource_type,
        resource_id,
        iat,
        exp,
        decoded.endpoints,
        decoded.data,
    )?;

    let refresh_exp = refresh.duration();
    let refresh = Refresh::new(resource_id, iat, refresh_exp)?;

    let resp = api::AuthServiceRefreshResponse {
        token: conn.context.cipher.jwt.encode(&claims)?.into(),
        refresh: conn.context.cipher.refresh.encode(&refresh)?.into(),
    };
    let mut resp = tonic::Response::new(resp);
    resp.metadata_mut()
        .insert("set-cookie", conn.context.cipher.refresh.cookie(&refresh)?);

    Ok(resp)
}

/// This endpoint triggers the sending of the reset-password email. The actual resetting is
/// then done through the `update` function.
async fn reset_password(
    req: tonic::Request<api::AuthServiceResetPasswordRequest>,
    conn: &mut models::Conn,
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
    conn: &mut models::Conn,
) -> super::Result<api::AuthServiceUpdatePasswordResponse> {
    let claims = auth::get_claims(&req, Endpoint::AuthUpdatePassword, conn).await?;
    let req = req.into_inner();
    // Only users have passwords; orgs, hosts and nodes do not.
    let Resource::User(user_id) = claims.resource() else { super::forbidden!("Need user_id") };
    let cur_user = models::User::find_by_id(user_id, conn)
        .await?
        .update_password(&req.password, conn)
        .await?;
    let resp = api::AuthServiceUpdatePasswordResponse {};

    // Send notification mail
    MailClient::new(&conn.context.config)
        .update_password(&cur_user)
        .await?;

    Ok(tonic::Response::new(resp))
}

async fn update_ui_password(
    req: tonic::Request<api::AuthServiceUpdateUiPasswordRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::AuthServiceUpdateUiPasswordResponse> {
    let claims = auth::get_claims(&req, Endpoint::AuthUpdateUiPassword, conn).await?;
    let Resource::User(user_id_) = claims.resource() else { super::forbidden!("Must be user") };
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
    MailClient::new(&conn.context.config)
        .update_password(&user)
        .await?;

    Ok(tonic::Response::new(resp))
}
