use diesel_async::scoped_futures::ScopedFutureExt;

use crate::auth::claims::{Claims, Expirable};
use crate::auth::endpoint::{Endpoint, Endpoints};
use crate::auth::resource::Resource;
use crate::auth::token::refresh::Refresh;
use crate::auth::token::RequestToken;
use crate::config::Context;
use crate::database::{Conn, Transaction};
use crate::models::{Host, Node, Org, User};

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
        self.context
            .write(|conn, ctx| login(req, conn, ctx).scope_boxed())
            .await
    }

    async fn confirm(
        &self,
        req: tonic::Request<api::AuthServiceConfirmRequest>,
    ) -> super::Resp<api::AuthServiceConfirmResponse> {
        self.context
            .write(|conn, ctx| confirm(req, conn, ctx).scope_boxed())
            .await
    }

    async fn refresh(
        &self,
        req: tonic::Request<api::AuthServiceRefreshRequest>,
    ) -> super::Resp<api::AuthServiceRefreshResponse> {
        self.context
            .write(|conn, ctx| refresh(req, conn, ctx).scope_boxed())
            .await
    }

    /// This endpoint triggers the sending of the reset-password email. The actual resetting is
    /// then done through the `update` function.
    async fn reset_password(
        &self,
        req: tonic::Request<api::AuthServiceResetPasswordRequest>,
    ) -> super::Resp<api::AuthServiceResetPasswordResponse> {
        self.context
            .write(|conn, ctx| reset_password(req, conn, ctx).scope_boxed())
            .await
    }

    async fn update_password(
        &self,
        req: tonic::Request<api::AuthServiceUpdatePasswordRequest>,
    ) -> super::Resp<api::AuthServiceUpdatePasswordResponse> {
        self.context
            .write(|conn, ctx| update_password(req, conn, ctx).scope_boxed())
            .await
    }

    async fn update_ui_password(
        &self,
        req: tonic::Request<api::AuthServiceUpdateUiPasswordRequest>,
    ) -> super::Resp<api::AuthServiceUpdateUiPasswordResponse> {
        self.context
            .write(|conn, ctx| update_ui_password(req, conn, ctx).scope_boxed())
            .await
    }
}

async fn login(
    req: tonic::Request<api::AuthServiceLoginRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::AuthServiceLoginResponse> {
    // This endpoint requires no auth, it is where you get your token from.
    let inner = req.into_inner();
    let user = User::login(&inner.email, &inner.password, conn).await?;

    let expires = ctx.config.token.expire.token.try_into()?;
    let claims = Claims::user_from_now(expires, user.id, USER_ENDPOINTS);

    let expires = ctx.config.token.expire.refresh_user.try_into()?;
    let refresh = Refresh::from_now(expires, user.id);

    let resp = api::AuthServiceLoginResponse {
        token: ctx.cipher().jwt.encode(&claims)?.into(),
        refresh: ctx.cipher().refresh.encode(&refresh)?.into(),
    };

    let mut resp = tonic::Response::new(resp);
    let cookie = ctx.cipher().refresh.cookie(&refresh)?;
    resp.metadata_mut().insert("set-cookie", cookie.header()?);

    Ok(resp)
}

async fn confirm(
    req: tonic::Request<api::AuthServiceConfirmRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::AuthServiceConfirmResponse> {
    let claims = ctx.claims(&req, Endpoint::AuthConfirm).await?;
    let user_id = match claims.resource().user() {
        Some(id) => id,
        None => super::forbidden!("Must be user"),
    };

    let expires = ctx.config.token.expire.token.try_into()?;
    let claims = Claims::user_from_now(expires, user_id, USER_ENDPOINTS);

    let expires = ctx.config.token.expire.refresh_user.try_into()?;
    let refresh = Refresh::from_now(expires, user_id);

    User::confirm(user_id, conn).await?;

    let resp = api::AuthServiceConfirmResponse {
        token: ctx.cipher().jwt.encode(&claims)?.into(),
        refresh: ctx.cipher().refresh.encode(&refresh)?.into(),
    };

    let mut resp = tonic::Response::new(resp);
    let cookie = ctx.cipher().refresh.cookie(&refresh)?;
    resp.metadata_mut().insert("set-cookie", cookie.header()?);

    Ok(resp)
}

async fn refresh(
    req: tonic::Request<api::AuthServiceRefreshRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::AuthServiceRefreshResponse> {
    let fallback = ctx.auth.maybe_refresh(&req)?;

    let req = req.into_inner();
    let mut decoded = if let RequestToken::Bearer(token) = req.token.parse()? {
        ctx.cipher().jwt.decode_expired(&token)?
    } else {
        return Err(crate::Error::invalid_auth("Not bearer."));
    };

    let refresh = match (req.refresh, fallback) {
        (Some(refresh), _) => ctx.cipher().refresh.decode(&refresh.into())?,
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
        Resource::User(user_id) => User::find_by_id(user_id, conn)
            .await
            .map(|_| user_id.into()),
        Resource::Org(org_id) => Org::find_by_id(org_id, conn).await.map(|_| org_id.into()),
        Resource::Host(host_id) => Host::find_by_id(host_id, conn)
            .await
            .map(|_| host_id.into()),
        Resource::Node(node_id) => Node::find_by_id(node_id, conn)
            .await
            .map(|_| node_id.into()),
    }?;
    if refresh.resource_id() != resource_id {
        super::forbidden!("Jwt and refresh grantee don't match");
    }

    // The following is a workaround that sort of patches existing host tokens. Removeme
    if !decoded.endpoints.includes(Endpoint::CookbookAll) {
        decoded.endpoints = match decoded.endpoints {
            Endpoints::Wildcard => Endpoints::Wildcard,
            Endpoints::Single(e) => Endpoints::Single(e),
            Endpoints::Multiple(mut es) => {
                es.push(Endpoint::BundleAll);
                es.push(Endpoint::CookbookAll);
                Endpoints::Multiple(es)
            }
        }
    }

    let expires = ctx.config.token.expire.token.try_into()?;
    let expirable = Expirable::from_now(expires);
    let claims = Claims::new(resource, expirable, decoded.endpoints).with_data(decoded.data);
    let token = ctx.cipher().jwt.encode(&claims)?;

    let expires = refresh.expirable().duration();
    let refresh = Refresh::from_now(expires, resource_id);
    let encoded = ctx.cipher().refresh.encode(&refresh)?;
    let cookie = ctx.cipher().refresh.cookie(&refresh)?;

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
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::AuthServiceResetPasswordResponse> {
    let req = req.into_inner();
    // We are going to query the user and send them an email, but when something goes wrong we
    // are not going to return an error. This hides whether or not a user is registered with
    // us to the caller of the api, because this info may be sensitive and this endpoint is not
    // protected by any authentication.
    let user = User::find_by_email(&req.email, conn).await;
    if let Ok(user) = user {
        let _ = ctx.mail.reset_password(&user).await;
    }

    let resp = api::AuthServiceResetPasswordResponse {};
    Ok(tonic::Response::new(resp))
}

async fn update_password(
    req: tonic::Request<api::AuthServiceUpdatePasswordRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::AuthServiceUpdatePasswordResponse> {
    let claims = ctx.claims(&req, Endpoint::AuthUpdatePassword).await?;
    let req = req.into_inner();

    // Only users have passwords; orgs, hosts and nodes do not.
    let user_id = match claims.resource().user() {
        Some(id) => id,
        None => super::forbidden!("Need user_id"),
    };

    let cur_user = User::find_by_id(user_id, conn)
        .await?
        .update_password(&req.password, conn)
        .await?;
    let resp = api::AuthServiceUpdatePasswordResponse {};

    // Send notification mail
    ctx.mail.update_password(&cur_user).await?;

    Ok(tonic::Response::new(resp))
}

async fn update_ui_password(
    req: tonic::Request<api::AuthServiceUpdateUiPasswordRequest>,
    conn: &mut Conn<'_>,
    ctx: &Context,
) -> super::Result<api::AuthServiceUpdateUiPasswordResponse> {
    let claims = ctx.claims(&req, Endpoint::AuthUpdateUiPassword).await?;
    let req = req.into_inner();
    let user_id = req.user_id.parse()?;
    let claims = claims.ensure_user(user_id)?;

    let user = User::find_by_id(claims.user_id(), conn).await?;
    user.verify_password(&req.old_password)?;
    user.update_password(&req.new_password, conn).await?;

    let resp = api::AuthServiceUpdateUiPasswordResponse {};

    // Send notification mail
    ctx.mail.update_password(&user).await?;

    Ok(tonic::Response::new(resp))
}
