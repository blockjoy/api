mod setup;

use axum::http::{Request, StatusCode};
use blockvisor_api::config::Context;
use blockvisor_api::tests::TestDb;
use hyper::Body;
use tower::ServiceExt;

fn possible_routes() -> Vec<(&'static str, &'static str, StatusCode)> {
    vec![
        // Non nested routes
        ("/health", "GET", StatusCode::OK),
        // MQTT routes
        ("/mqtt/auth", "POST", StatusCode::BAD_REQUEST),
        ("/mqtt/acl", "POST", StatusCode::BAD_REQUEST),
    ]
}

#[tokio::test]
async fn test_possible_routes() -> anyhow::Result<()> {
    let context = Context::from_default_toml().await.unwrap();
    let db = TestDb::setup(context).await;

    let routes = possible_routes();
    let app = blockvisor_api::http::server(db.pool.clone()).await;

    let mut cnt = 1;

    for item in routes {
        let route = item.0;
        let method = item.1;
        let expected_response_code = item.2;

        let req = Request::builder()
            .method(method)
            .header("content-type", "application/json")
            .uri(route)
            .body(Body::empty())?;
        let response = app.clone().oneshot(req).await?;

        assert_eq!(
            response.status(),
            expected_response_code,
            "#{cnt} {method} {route} failed"
        );
        cnt += 1;
    }

    Ok(())
}
