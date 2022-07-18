use std::env;
use api::errors::Result as ApiResult;
use axum::{
    body::Body,
    extract::Extension,
    http::{Request, StatusCode},
    Json,
    middleware,
    middleware::Next,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use casbin_macros::validate_privileges;
use tower::util::ServiceExt;
use casbin_authorization::auth;
use casbin_authorization::auth::Authorizable;

#[derive(Clone, Debug)]
struct User(pub String);

impl Authorizable for User {
    fn get_role(&self) -> String {
        self.0.clone()
    }
}

fn setup() {
    env::set_var("CASBIN_MODEL", "./conf/model.conf");
    env::set_var("CASBIN_POLICIES", "./conf/policies.csv");
}

#[allow(unused_variables)]
#[axum_macros::debug_handler]
#[validate_privileges(object = "host_commands", action = "read_all")]
async fn some_handler(Extension(user): Extension<User>) -> ApiResult<impl IntoResponse> {
    Ok((StatusCode::OK, Json(vec!["Authorized"])))
}

#[allow(unused_variables)]
#[axum_macros::debug_handler]
#[validate_privileges(object = "no_object", action = "unknown_action")]
async fn some_unauthorized_handler(Extension(user): Extension<User>) -> ApiResult<impl IntoResponse> {
    Ok((StatusCode::OK, Json(vec!["Authorized"])))
}

async fn subject_extension_from_token<B, R>(
    mut request: Request<B>,
    next: Next<B>,
) -> Result<impl IntoResponse, Response>
{
    let user = User("host".to_string());
    request.extensions_mut().insert(user);

    Ok(next.run(request).await)
}

fn get_test_router() -> Router {
    Router::new()
        .route("/authorized", get(some_handler))
        .route("/unauthorized", get(some_unauthorized_handler))
        .layer(middleware::from_fn(subject_extension_from_token::<_, User>))
}

#[tokio::test]
async fn should_authorize_user() -> anyhow::Result<()> {
    setup();

    let app = get_test_router();
    let req = Request::builder()
        .method("GET")
        .uri("/authorized")
        .body(Body::from(""))?;
    let resp = app.clone().oneshot(req).await?;

    assert_eq!(resp.status(), StatusCode::OK);

    Ok(())
}

#[tokio::test]
async fn should_not_authorize_user() -> anyhow::Result<()> {
    setup();

    let app = get_test_router();
    let req = Request::builder()
        .method("GET")
        .uri("/unauthorized")
        .body(Body::from(""))?;
    let resp = app.clone().oneshot(req).await?;

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    Ok(())
}
