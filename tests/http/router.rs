use axum::http::{Request, StatusCode};
use hyper::Body;
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
        ("/mqtt/auth", "POST", StatusCode::BAD_REQUEST),
        ("/mqtt/acl", "POST", StatusCode::BAD_REQUEST),
    ];

    for (route, method, status) in routes.into_iter() {
        let req = Request::builder()
            .method(method)
            .header("content-type", "application/json")
            .uri(route)
            .body(Body::empty())
            .unwrap();

        let resp = http::router(&context).oneshot(req).await.unwrap();
        assert_eq!(resp.status(), status, "{method} {route} failed");
    }
}
