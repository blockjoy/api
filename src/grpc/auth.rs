use diesel_async::scoped_futures::ScopedFutureExt;

use crate::auth::claims::{Claims, Expirable};
use crate::auth::endpoint::{Endpoint, Endpoints};
use crate::auth::resource::Resource;
use crate::auth::token::refresh::Refresh;
use crate::auth::token::RequestToken;
use crate::models;

use super::api::{self, auth_service_server};

/// This is a list of all the endpoints that a user is allowed to access with the jwt that they
/// generate on login. It does not contain endpoints like confirm, because those are accessed by a
/// token.
const USER_ENDPOINTS: &[Endpoint] = &[
    Endpoint::ApiKeyAll,
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
    Endpoint::SubscriptionAll,
    Endpoint::UserAll,
];

#[tonic::async_trait]
impl auth_service_server::AuthService for super::Grpc {
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
        self.trx(|c| refresh(req, c).scope_boxed()).await
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

    let expires = conn.context.config.token.expire.token.try_into()?;
    let claims = Claims::user_from_now(expires, user.id, USER_ENDPOINTS);

    let expires = conn.context.config.token.expire.refresh_user.try_into()?;
    let refresh = Refresh::from_now(expires, user.id);

    let resp = api::AuthServiceLoginResponse {
        token: conn.context.cipher().jwt.encode(&claims)?.into(),
        refresh: conn.context.cipher().refresh.encode(&refresh)?.into(),
    };

    let mut resp = tonic::Response::new(resp);
    let cookie = conn.context.cipher().refresh.cookie(&refresh)?;
    resp.metadata_mut().insert("set-cookie", cookie.header()?);

    Ok(resp)
}

async fn confirm(
    req: tonic::Request<api::AuthServiceConfirmRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::AuthServiceConfirmResponse> {
    let claims = conn.claims(&req, Endpoint::AuthConfirm).await?;
    let user_id = match claims.resource().user() {
        Some(id) => id,
        None => super::forbidden!("Must be user"),
    };

    let expires = conn.context.config.token.expire.token.try_into()?;
    let claims = Claims::user_from_now(expires, user_id, USER_ENDPOINTS);

    let expires = conn.context.config.token.expire.refresh_user.try_into()?;
    let refresh = Refresh::from_now(expires, user_id);

    models::User::confirm(user_id, conn).await?;

    let resp = api::AuthServiceConfirmResponse {
        token: conn.context.cipher().jwt.encode(&claims)?.into(),
        refresh: conn.context.cipher().refresh.encode(&refresh)?.into(),
    };

    let mut resp = tonic::Response::new(resp);
    let cookie = conn.context.cipher().refresh.cookie(&refresh)?;
    resp.metadata_mut().insert("set-cookie", cookie.header()?);

    Ok(resp)
}

async fn refresh(
    req: tonic::Request<api::AuthServiceRefreshRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::AuthServiceRefreshResponse> {
    let fallback = conn.context.auth.maybe_refresh(&req)?;

    let req = req.into_inner();
    let mut decoded = if let RequestToken::Bearer(token) = req.token.parse()? {
        conn.context.cipher().jwt.decode_expired(&token)?
    } else {
        return Err(crate::Error::invalid_auth("Not bearer."));
    };

    let refresh = match (req.refresh, fallback) {
        (Some(refresh), _) => conn.context.cipher().refresh.decode(&refresh.into())?,
        (None, Some(fallback)) => fallback,
        (None, None) => {
            return Err(crate::Error::validation(
                "Need refresh token from cookies or request body",
            ))
        }
    };

    // For each type of resource, we perform some queries down below to verify that the resource
    // still exists.
    let resource = decoded.resource();
    let resource_id = match resource {
        Resource::User(user_id) => models::User::find_by_id(user_id, conn)
            .await
            .map(|_| user_id.into()),
        Resource::Org(org_id) => models::Org::find_by_id(org_id, conn)
            .await
            .map(|_| org_id.into()),
        Resource::Host(host_id) => models::Host::find_by_id(host_id, conn)
            .await
            .map(|_| host_id.into()),
        Resource::Node(node_id) => models::Node::find_by_id(node_id, conn)
            .await
            .map(|_| node_id.into()),
    }?;
    if refresh.resource_id() != resource_id {
        super::forbidden!("Jwt and refresh grantee don't match");
    }

    let wrong_endpoints = vec![
        Endpoint::AuthRefresh,
        Endpoint::BabelAll,
        Endpoint::BlockchainAll,
        Endpoint::BundleAll,
        Endpoint::CommandAll,
        Endpoint::CookbookAll,
        Endpoint::DiscoveryAll,
        Endpoint::HostGet,
        Endpoint::HostList,
        Endpoint::HostUpdate,
        Endpoint::KeyFileAll,
        Endpoint::MetricsAll,
        Endpoint::NodeAll,
    ];

    decoded.endpoints = match decoded.endpoints {
        Endpoints::Multiple(endpoints) if endpoints == wrong_endpoints => {
            Endpoints::Multiple(vec![
                Endpoint::AuthRefresh,
                Endpoint::BabelAll,
                Endpoint::BlockchainAll,
                Endpoint::BundleAll,
                Endpoint::CommandAll,
                Endpoint::CookbookAll,
                Endpoint::DiscoveryAll,
                Endpoint::HostGet,
                Endpoint::HostList,
                Endpoint::HostUpdate,
                Endpoint::KeyFileAll,
                Endpoint::ManifestAll,
                Endpoint::MetricsAll,
                Endpoint::NodeAll,
            ])
        }
        other => other,
    };

    let expires = conn.context.config.token.expire.token.try_into()?;
    let expirable = Expirable::from_now(expires);
    let claims = Claims::new(resource, expirable, decoded.endpoints).with_data(decoded.data);
    let token = conn.context.cipher().jwt.encode(&claims)?;

    let expires = refresh.expirable().duration();
    let refresh = Refresh::from_now(expires, resource_id);
    let encoded = conn.context.cipher().refresh.encode(&refresh)?;
    let cookie = conn.context.cipher().refresh.cookie(&refresh)?;

    let resp = api::AuthServiceRefreshResponse {
        token: token.into(),
        refresh: encoded.into(),
    };

    let mut resp = tonic::Response::new(resp);
    resp.metadata_mut().insert("set-cookie", cookie.header()?);

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
    let claims = conn.claims(&req, Endpoint::AuthUpdatePassword).await?;
    let req = req.into_inner();

    // Only users have passwords; orgs, hosts and nodes do not.
    let user_id = match claims.resource().user() {
        Some(id) => id,
        None => super::forbidden!("Need user_id"),
    };

    let cur_user = models::User::find_by_id(user_id, conn)
        .await?
        .update_password(&req.password, conn)
        .await?;
    let resp = api::AuthServiceUpdatePasswordResponse {};

    // Send notification mail
    conn.context.mail.update_password(&cur_user).await?;

    Ok(tonic::Response::new(resp))
}

async fn update_ui_password(
    req: tonic::Request<api::AuthServiceUpdateUiPasswordRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::AuthServiceUpdateUiPasswordResponse> {
    let claims = conn.claims(&req, Endpoint::AuthUpdateUiPassword).await?;
    let req = req.into_inner();
    let user_id = req.user_id.parse()?;
    let claims = claims.ensure_user(user_id)?;

    let user = models::User::find_by_id(claims.user_id(), conn).await?;
    user.verify_password(&req.old_password)?;
    user.update_password(&req.new_password, conn).await?;

    let resp = api::AuthServiceUpdateUiPasswordResponse {};

    // Send notification mail
    conn.context.mail.update_password(&user).await?;

    Ok(tonic::Response::new(resp))
}
