use api::routes::api_router;
use axum::http::{Request, StatusCode};
use hyper::Body;
use tower::ServiceExt;
use tower_http::trace::TraceLayer;

fn possible_routes() -> Vec<(&'static str, &'static str, StatusCode)> {
    vec![
        // Non nested routes
        ("/reset", "POST", StatusCode::INTERNAL_SERVER_ERROR),
        ("/reset", "PUT", StatusCode::INTERNAL_SERVER_ERROR),
        ("/login", "POST", StatusCode::INTERNAL_SERVER_ERROR),
        ("/refresh", "POST", StatusCode::INTERNAL_SERVER_ERROR),
        ("/whoami", "GET", StatusCode::INTERNAL_SERVER_ERROR),
        ("/block_height", "GET", StatusCode::INTERNAL_SERVER_ERROR),
        ("/block_info", "GET", StatusCode::INTERNAL_SERVER_ERROR),
        ("/block_info", "PUT", StatusCode::INTERNAL_SERVER_ERROR),
        ("/payments_due", "GET", StatusCode::INTERNAL_SERVER_ERROR),
        ("/pay_adresses", "GET", StatusCode::INTERNAL_SERVER_ERROR),
        ("/rewards", "POST", StatusCode::INTERNAL_SERVER_ERROR),
        ("/payments", "POST", StatusCode::INTERNAL_SERVER_ERROR),
        ("/qr/id", "GET", StatusCode::INTERNAL_SERVER_ERROR),
        ("/blockchains", "GET", StatusCode::INTERNAL_SERVER_ERROR),
        // Group routes
        ("/groups/nodes", "GET", StatusCode::INTERNAL_SERVER_ERROR),
        ("/groups/nodes/id", "GET", StatusCode::INTERNAL_SERVER_ERROR),
        // Node routes
        ("/nodes", "POST", StatusCode::INTERNAL_SERVER_ERROR),
        ("/nodes/id", "GET", StatusCode::INTERNAL_SERVER_ERROR),
        ("/nodes/id/info", "PUT", StatusCode::INTERNAL_SERVER_ERROR),
        // Command routes
        ("/commands/id", "GET", StatusCode::INTERNAL_SERVER_ERROR),
        ("/commands/id", "DELETE", StatusCode::INTERNAL_SERVER_ERROR),
        (
            "/commands/id/response",
            "PUT",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        // Validator routes
        ("/validators", "GET", StatusCode::INTERNAL_SERVER_ERROR),
        ("/validators/id", "GET", StatusCode::INTERNAL_SERVER_ERROR),
        (
            "/validators/id/migrate",
            "POST",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            "/validators/id/status",
            "PUT",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            "/validators/id/stake_status",
            "PUT",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            "/validators/id/owner_address",
            "PUT",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            "/validators/id/penalty",
            "PUT",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            "/validators/id/identity",
            "PUT",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            "/validators/staking",
            "GET",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            "/validators/consensus",
            "GET",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            "/validators/needs_attention",
            "GET",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            "/validators/inventory/count",
            "GET",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        // Broadcast filter routes
        (
            "/broadcast_filters",
            "POST",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            "/broadcast_filters/id",
            "GET",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            "/broadcast_filters/id",
            "PUT",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            "/broadcast_filters/id",
            "DELETE",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        // Organization routes
        ("/orgs", "POST", StatusCode::INTERNAL_SERVER_ERROR),
        ("/orgs/id", "GET", StatusCode::INTERNAL_SERVER_ERROR),
        ("/orgs/id", "DELETE", StatusCode::INTERNAL_SERVER_ERROR),
        ("/orgs/id", "PUT", StatusCode::INTERNAL_SERVER_ERROR),
        ("/orgs/id/members", "GET", StatusCode::INTERNAL_SERVER_ERROR),
        (
            "/orgs/id/broadcast_filters",
            "GET",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        // User routes
        ("/users", "POST", StatusCode::INTERNAL_SERVER_ERROR),
        ("/users/id/orgs", "GET", StatusCode::INTERNAL_SERVER_ERROR),
        (
            "/users/id/summary",
            "GET",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            "/users/id/payments",
            "GET",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            "/users/id/rewards/summary",
            "GET",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            "/users/id/validators",
            "GET",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            "/users/id/validators",
            "POST",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            "/users/id/validators/staking/export",
            "GET",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            "/users/id/invoices",
            "GET",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        ("/users/summary", "GET", StatusCode::INTERNAL_SERVER_ERROR),
        // Host routes
        ("/hosts", "POST", StatusCode::INTERNAL_SERVER_ERROR),
        ("/hosts", "GET", StatusCode::INTERNAL_SERVER_ERROR),
        ("/hosts/id", "GET", StatusCode::INTERNAL_SERVER_ERROR),
        ("/hosts/id", "PUT", StatusCode::INTERNAL_SERVER_ERROR),
        ("/hosts/id", "DELETE", StatusCode::INTERNAL_SERVER_ERROR),
        ("/hosts/id/status", "PUT", StatusCode::INTERNAL_SERVER_ERROR),
        (
            "/hosts/id/commands",
            "POST",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            "/hosts/id/commands",
            "GET",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            "/hosts/id/commands/pending",
            "GET",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            "/hosts/token/:token",
            "GET",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        // Host provisions routes
        (
            "/host_provisions",
            "POST",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            "/host_provisions/id",
            "GET",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
        (
            "/host_provisions/id/hosts",
            "POST",
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
    ]
}

#[tokio::test]
async fn test_possible_routes() -> anyhow::Result<()> {
    let routes = possible_routes();
    let app = api_router().layer(TraceLayer::new_for_http());
    let mut cnt = 1;

    for item in routes {
        let route = item.0;
        let method = item.1;
        let expected_response_code = item.2;

        println!("testing route #{} {} {}", cnt, method, route);

        let req = Request::builder()
            .method(method)
            .uri(route)
            .body(Body::from(""))?;
        let response = app.clone().oneshot(req).await?;

        assert_eq!(response.status(), expected_response_code);
        cnt += 1;
    }

    Ok(())
}
