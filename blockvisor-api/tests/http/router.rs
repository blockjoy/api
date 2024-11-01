use axum::http::{Request, StatusCode};
use http_body_util::Empty;
use tower::ServiceExt;

use blockvisor_api::config::Context;
use blockvisor_api::http;

#[tokio::test]
async fn test_possible_routes() {
    let (context, _db) = Context::with_mocked().await.unwrap();

    let routes = vec![
        // Non nested routes
        ("/health", "GET", StatusCode::OK),
        // MQTT routes
        ("/mqtt/auth", "POST", StatusCode::UNPROCESSABLE_ENTITY),
        ("/mqtt/acl", "POST", StatusCode::UNPROCESSABLE_ENTITY),
        (
            "/chargebee/callback/asdfasdf",
            "POST",
            StatusCode::NOT_FOUND,
        ),
    ];

    for (route, method, status) in routes.into_iter() {
        let req = Request::builder()
            .method(method)
            .header("content-type", "application/json")
            .uri(route)
            .body(Empty::new())
            .unwrap();

        let resp = http::router(&context).oneshot(req).await.unwrap();
        assert_eq!(resp.status(), status, "{method} {route} failed");
    }
}
