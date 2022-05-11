use api::models::*;
use api::server::*;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post, put};
use axum::{Extension, Router};
use chrono::Utc;
use serde::Deserialize;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::sync::Arc;
use tower::ServiceExt;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

#[tokio::test]
async fn it_should_create_and_login_user() -> anyhow::Result<()> {
    let db_pool = setup().await;
    let app = Router::new()
        .route("/login", post(login))
        .route("/users", post(create_user))
        .layer(Extension(Arc::new(db_pool)))
        .layer(TraceLayer::new_for_http());

    let req = Request::builder()
        .method("POST")
        .uri("/users")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&UserRequest {
            email: "chris@here.com".to_string(),
            password: "password".to_string(),
            password_confirm: "password".to_string(),
        })?))?;

    let resp = app.clone().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    #[derive(Debug, Clone, Deserialize)]
    pub struct UserTest {
        pub id: Uuid,
        pub email: String,
        pub token: Option<String>,
        pub refresh: Option<String>,
    }
    let body = hyper::body::to_bytes(resp.into_body()).await?;
    let user_test: UserTest = serde_json::from_slice(&body)?;
    assert_eq!(user_test.email, "chris@here.com");

    let req = Request::builder()
        .method("POST")
        .uri("/login")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&UserLoginRequest {
            email: "chris@here.com".to_string(),
            password: "password".to_string(),
        })?))?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = hyper::body::to_bytes(resp.into_body()).await?;
    let user_test: UserTest = serde_json::from_slice(&body)?;
    assert_eq!(user_test.email, "chris@here.com");
    assert!(user_test.token.is_some());

    Ok(())
}

#[tokio::test]
async fn it_should_add_host() -> anyhow::Result<()> {
    let db_pool = Arc::new(setup().await);
    let db_pool_cloned = Arc::clone(&db_pool);
    let app = Router::new()
        .route("/login", post(login))
        .route("/hosts", post(create_host))
        .layer(Extension(db_pool_cloned))
        .layer(TraceLayer::new_for_http());

    let admin_user = get_admin_user(&db_pool).await;

    let req = Request::builder()
        .method("POST")
        .uri("/hosts")
        .header("Content-Type", "application/json")
        .header(
            "Authorization",
            format!(
                "Bearer {}",
                admin_user.token.clone().unwrap_or_else(|| "".to_string())
            ),
        )
        .body(Body::from(serde_json::to_string(&HostRequest {
            org_id: None,
            name: "Test user 1".to_string(),
            version: Some("0.1.0".to_string()),
            location: Some("Virgina".to_string()),
            cpu_count: None,
            mem_size: None,
            disk_size: None,
            os: None,
            os_version: None,
            ip_addr: "192.168.8.2".parse().expect("Couldn't parse ip address"),
            val_ip_addrs: Some("192.168.8.3, 192.168.8.4".to_string()),
            token: "1234".to_string(),
            status: ConnectionStatus::Online,
        })?))?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(resp.into_body()).await?;
    let host: Host = serde_json::from_slice(&body)?;
    assert_eq!(host.name, "Test user 1");
    assert!(host.validators.is_some());
    assert_eq!(host.validators.unwrap().len(), 2);

    let res = Host::delete(host.id, &db_pool).await;
    assert_eq!(1, res.unwrap());

    Ok(())
}

#[tokio::test]
async fn it_should_add_host_from_provision() -> anyhow::Result<()> {
    let db_pool = Arc::new(setup().await);
    let db_pool_cloned = Arc::clone(&db_pool);
    let app = Router::new()
        .route("/login", post(login))
        .route("/hosts", post(create_host))
        .layer(Extension(db_pool_cloned))
        .layer(TraceLayer::new_for_http());

    let admin_user = get_admin_user(&db_pool).await;

    let org_id = Uuid::new_v4();
    let cpu_count = 64;
    let mem_size = 64_000_000;
    let disk_size = 128_000_000;
    let os = "Debian".to_string();
    let os_version = "4.1.4".to_string();

    // Insert a host

    let req = Request::builder()
        .method("POST")
        .uri("/hosts")
        .header("Content-Type", "application/json")
        .header(
            "Authorization",
            format!(
                "Bearer {}",
                admin_user.token.clone().unwrap_or_else(|| "".to_string())
            ),
        )
        .body(Body::from(serde_json::to_string(&HostRequest {
            org_id: Some(org_id),
            name: "Test user 1".to_string(),
            version: Some("0.1.0".to_string()),
            location: Some("Virgina".to_string()),
            cpu_count: Some(cpu_count),
            mem_size: Some(mem_size),
            disk_size: Some(disk_size),
            os: Some(os.clone()),
            os_version: Some(os_version.clone()),
            ip_addr: "192.168.8.2".parse().expect("Couldn't parse ip address"),
            val_ip_addrs: None,
            token: "1234".to_string(),
            status: ConnectionStatus::Online,
        })?))?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(resp.into_body()).await?;
    let host: Host = serde_json::from_slice(&body)?;

    assert_eq!(host.name, "Test user 1");
    assert_eq!(host.org_id, Some(org_id));
    assert_eq!(host.cpu_count, Some(cpu_count));
    assert_eq!(host.mem_size, Some(mem_size));
    assert_eq!(host.disk_size, Some(disk_size));
    assert_eq!(host.os, Some(os));
    assert_eq!(host.os_version, Some(os_version));
    assert!(host.validators.is_some());
    assert_eq!(host.validators.unwrap().len(), 0);

    // Delete new host from table
    let res = Host::delete(host.id, &db_pool).await;
    assert_eq!(1, res.unwrap());

    Ok(())
}

#[tokio::test]
async fn it_should_get_host() -> anyhow::Result<()> {
    let db_pool = Arc::new(setup().await);
    let db_pool_cloned = Arc::clone(&db_pool);
    let app = Router::new()
        .route("/hosts/:id", get(get_host))
        .layer(Extension(db_pool_cloned))
        .layer(TraceLayer::new_for_http());

    let host = get_test_host(&db_pool).await;
    let admin_user = get_admin_user(&db_pool).await;

    let req = Request::builder()
        .method("GET")
        .uri(&format!("/hosts/{}", host.id))
        .header(
            "Authorization",
            format!(
                "Bearer {}",
                admin_user.token.clone().unwrap_or_else(|| "".to_string())
            ),
        )
        .body(Body::empty())?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(resp.into_body()).await?;
    let host: Host = serde_json::from_slice(&body)?;
    assert_eq!(host.name, "Test user");

    Ok(())
}

#[tokio::test]
async fn it_should_get_host_by_token() -> anyhow::Result<()> {
    let db_pool = Arc::new(setup().await);
    let db_pool_cloned = Arc::clone(&db_pool);
    let app = Router::new()
        .route("/hosts", get(list_hosts))
        .layer(Extension(db_pool_cloned))
        .layer(TraceLayer::new_for_http());

    let host = Host::find_by_token("123", &db_pool)
        .await
        .expect("Could not read test host from db.");

    let admin_user = get_admin_user(&db_pool).await;

    let req = Request::builder()
        .method("GET")
        .uri(&format!("/hosts?token={}", host.token))
        .header(
            "Authorization",
            format!(
                "Bearer {}",
                admin_user.token.clone().unwrap_or_else(|| "".to_string())
            ),
        )
        .body(Body::empty())?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(resp.into_body()).await?;
    let host: Host = serde_json::from_slice(&body)?;
    assert_eq!(host.name, "Test user");

    Ok(())
}

#[tokio::test]
async fn it_should_update_validator_status() -> anyhow::Result<()> {
    let db_pool = Arc::new(setup().await);
    let db_pool_cloned = Arc::clone(&db_pool);
    let app = Router::new()
        .route("/validators/:id/status", put(update_validator_status))
        .layer(Extension(db_pool_cloned))
        .layer(TraceLayer::new_for_http());

    let host = get_test_host(&db_pool).await;

    let path = format!(
        "/validators/{}/status",
        host.validators.unwrap().first().unwrap().id
    );

    let req = Request::builder()
        .method("PUT")
        .uri(&path)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", host.token))
        .body(Body::from(serde_json::to_string(
            &ValidatorStatusRequest {
                version: Some("1.0".to_string()),
                block_height: Some(192),
                status: ValidatorStatus::Provisioning,
            },
        )?))?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(resp.into_body()).await?;
    let validator: Validator = serde_json::from_slice(&body)?;
    assert_eq!(validator.host_id, host.id);

    Ok(())
}

#[tokio::test]
async fn it_should_update_validator_penalty() -> anyhow::Result<()> {
    let db_pool = Arc::new(setup().await);
    let db_pool_cloned = Arc::clone(&db_pool);
    let app = Router::new()
        .route("/validators/:id/penalty", put(update_validator_penalty))
        .layer(Extension(db_pool_cloned))
        .layer(TraceLayer::new_for_http());

    let host = get_test_host(&db_pool).await;
    let service_token = std::env::var("API_SERVICE_SECRET").expect("Missing API_SERVICE_SECRET");

    let path = format!(
        "/validators/{}/penalty",
        host.validators.unwrap().first().unwrap().id
    );

    let req = Request::builder()
        .method("PUT")
        .uri(&path)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", service_token))
        .body(Body::from(serde_json::to_string(
            &ValidatorPenaltyRequest {
                tenure_penalty: 1.5,
                dkg_penalty: 2.5,
                performance_penalty: 3.5,
                total_penalty: 7.5,
            },
        )?))?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(resp.into_body()).await?;
    let validator: Validator = serde_json::from_slice(&body)?;
    assert_eq!(validator.tenure_penalty, 1.5);
    assert_eq!(validator.dkg_penalty, 2.5);
    assert_eq!(validator.performance_penalty, 3.5);
    assert_eq!(validator.total_penalty, 7.5);

    Ok(())
}

#[tokio::test]
async fn it_should_update_validator_identity() -> anyhow::Result<()> {
    let db_pool = Arc::new(setup().await);
    let db_pool_cloned = Arc::clone(&db_pool);
    let app = Router::new()
        .route("/validators/:id/identity", put(update_validator_identity))
        .layer(Extension(db_pool_cloned))
        .layer(TraceLayer::new_for_http());

    let host = get_test_host(&db_pool).await;
    let validators = host.validators.expect("missing validators");
    let validator = &validators.first().expect("missing validator");

    let path = format!("/validators/{}/identity", validator.id);

    let req = Request::builder()
        .method("PUT")
        .uri(&path)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", host.token))
        .body(Body::from(serde_json::to_string(
            &ValidatorIdentityRequest {
                version: Some("48".to_string()),
                address: Some("Z729x5EeguKsNZbqBJYCh9p7wVg35RybQjNoqxQcx9u81k2jpY".to_string()),
                swarm_key: Some("EN1VKTRg_ym6SlR83y7dWtc0_uDJG380znHFcWeTy2ztBIPxqD93D__U3JK5mrrFjvcDtPtGLbwwRRGp2rr8YfAnQ_OL7S5pSOINHLIxgEqtz00wn8T74A9d9anlTOb-BHM=".to_string()),
            },
        )?))?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(resp.into_body()).await?;
    let resp: Validator = serde_json::from_slice(&body)?;
    assert_eq!(resp.id, validator.id);
    assert_eq!(resp.version, Some("48".to_string()));

    Ok(())
}

#[tokio::test]
async fn it_should_create_command() -> anyhow::Result<()> {
    let db_pool = Arc::new(setup().await);
    let db_pool_cloned = Arc::clone(&db_pool);
    let app = Router::new()
        .route("/hosts/:id/commands", post(create_command))
        .layer(Extension(db_pool_cloned))
        .layer(TraceLayer::new_for_http());

    let host = get_test_host(&db_pool).await;
    let path = format!("/hosts/{}/commands", host.id);

    let req = Request::builder()
        .method("POST")
        .uri(&path)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&CommandRequest {
            cmd: HostCmd::RestartJail,
            sub_cmd: Some("blue_angel".to_string()),
        })?))?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(resp.into_body()).await?;
    let command: Command = serde_json::from_slice(&body)?;
    assert_eq!(command.host_id, host.id);

    Ok(())
}

#[tokio::test]
async fn it_should_stake_one_validator() -> anyhow::Result<()> {
    let db_pool = Arc::new(setup().await);
    let db_pool_cloned = Arc::clone(&db_pool);
    let app = Router::new()
        .route("/users/:user_id/validators", post(stake_validator))
        .layer(Extension(db_pool_cloned))
        .layer(TraceLayer::new_for_http());

    let login_req = UserLoginRequest {
        email: "test@here.com".into(),
        password: "abc12345".into(),
    };

    let user = User::login(login_req, &db_pool)
        .await
        .expect("could not login test user");

    let path = format!("/users/{}/validators", user.id);

    let stake_req = ValidatorStakeRequest { count: 2 };

    let req = Request::builder()
        .method("POST")
        .uri(&path)
        .header("Content-Type", "application/json")
        .header(
            "Authorization",
            format!(
                "Bearer {}",
                user.token.clone().unwrap_or_else(|| "".to_string())
            ),
        )
        .body(Body::from(serde_json::to_string(&stake_req)?))?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(resp.into_body()).await?;
    let validators: Vec<Validator> = serde_json::from_slice(&body)?;
    validators.iter().for_each(|v| {
        assert_eq!(v.stake_status, StakeStatus::Staking);
        assert!(v.staking_height.is_some());
    });

    Ok(())
}

#[tokio::test]
async fn it_should_migrate_one_validator() -> anyhow::Result<()> {
    let db_pool = Arc::new(setup().await);
    let db_pool_cloned = Arc::clone(&db_pool);
    let app = Router::new()
        .route("/validators/:id/migrate", post(migrate_validator))
        .layer(Extension(db_pool_cloned))
        .layer(TraceLayer::new_for_http());

    let admin = get_admin_user(&db_pool).await;
    let host = get_test_host(&db_pool).await;
    let validators = host.validators.expect("host to have validators");
    let validator = &validators.first().expect("validators to have at least one");

    let path = format!("/validators/{}/migrate", validator.id);

    let req = Request::builder()
        .method("POST")
        .uri(&path)
        .header(
            "Authorization",
            format!(
                "Bearer {}",
                admin.token.clone().unwrap_or_else(|| "".to_string())
            ),
        )
        .body(Body::empty())?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(resp.into_body()).await?;
    let new_validator: Validator = serde_json::from_slice(&body)?;
    let new_host = Host::find_by_token("1234", &db_pool)
        .await
        .expect("host to be returned.");

    assert_ne!(host.id, new_validator.host_id);
    assert_eq!(new_validator.host_id, new_host.id);
    assert_eq!(new_validator.address, validator.address);
    assert_eq!(new_validator.address_name, validator.address_name);
    assert_eq!(new_validator.owner_address, validator.owner_address);
    assert_eq!(new_validator.user_id, validator.user_id);
    assert_eq!(new_validator.swarm_key, validator.swarm_key);
    assert_eq!(new_validator.stake_status, validator.stake_status);
    assert_eq!(new_validator.status, ValidatorStatus::Migrating);

    let old_validator = Validator::find_by_id(validator.id, &db_pool)
        .await
        .expect("the old validator to be returned");
    assert_eq!(old_validator.status, ValidatorStatus::Stopped);
    assert_eq!(old_validator.stake_status, StakeStatus::Disabled);

    Ok(())
}

#[tokio::test]
async fn it_should_put_block_height_as_service() -> anyhow::Result<()> {
    let db_pool = setup().await;
    let app = Router::new()
        .route("/block_info", put(update_block_info))
        .layer(Extension(Arc::new(db_pool)))
        .layer(TraceLayer::new_for_http());

    let service_token = std::env::var("API_SERVICE_SECRET").expect("Missing API_SERVICE_SECRET");
    let ir = InfoRequest {
        block_height: 100,
        oracle_price: 10,
    };

    let req = Request::builder()
        .method("PUT")
        .uri("/block_info")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", service_token))
        .body(Body::from(serde_json::to_string(&ir)?))?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    Ok(())
}

#[tokio::test]
async fn it_should_list_validators_staking_as_service() -> anyhow::Result<()> {
    let db_pool = setup().await;
    let app = Router::new()
        .route("/validators/staking", get(list_validators_staking))
        .layer(Extension(Arc::new(db_pool)))
        .layer(TraceLayer::new_for_http());

    let service_token = std::env::var("API_SERVICE_SECRET").expect("Missing API_SERVICE_SECRET");

    let req = Request::builder()
        .method("GET")
        .uri("/validators/staking")
        .header("Authorization", format!("Bearer {}", service_token))
        .body(Body::empty())?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    Ok(())
}

#[tokio::test]
async fn it_should_list_validators_that_need_attention() -> anyhow::Result<()> {
    let db_pool = Arc::new(setup().await);
    let db_pool_cloned = Arc::clone(&db_pool);
    let app = Router::new()
        .route(
            "/validators/needs_attention",
            get(list_validators_attention),
        )
        .layer(Extension(db_pool_cloned))
        .layer(TraceLayer::new_for_http());

    let admin_user = get_admin_user(&db_pool).await;

    let req = Request::builder()
        .method("GET")
        .uri("/validators/needs_attention")
        .header(
            "Authorization",
            format!(
                "Bearer {}",
                admin_user.token.clone().unwrap_or_else(|| "".to_string())
            ),
        )
        .body(Body::empty())?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    Ok(())
}

#[tokio::test]
async fn it_should_get_qr_code() -> anyhow::Result<()> {
    let db_pool = Arc::new(setup().await);
    let db_pool_cloned = Arc::clone(&db_pool);
    let app = Router::new()
        .route("/qr/:user_id", get(get_qr))
        .layer(Extension(db_pool_cloned))
        .layer(TraceLayer::new_for_http());

    let u = User::find_by_email("test@here.com", &db_pool)
        .await
        .expect("Could not fetch test user.");

    let us = User::find_summary_by_user(&db_pool, u.id)
        .await
        .expect("fetch user summary");

    assert_eq!(us.balance(), 1000000000);

    let _inv = Invoice::find_all_by_user(&db_pool, &u.id)
        .await
        .expect("it to get bill")
        .first()
        .expect("should have at least 1 bill")
        .clone();

    let url = format!("/qr/{}", u.id);

    let req = Request::builder()
        .method("GET")
        .uri(&url)
        .body(Body::empty())?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    Ok(())
}

#[tokio::test]
async fn it_should_create_rewards() -> anyhow::Result<()> {
    let db_pool = Arc::new(setup().await);
    let db_pool_cloned = Arc::clone(&db_pool);
    let app = Router::new()
        .route("/rewards", post(create_rewards))
        .route("/users/:user_id/rewards/summary", get(get_reward_summary))
        .layer(Extension(db_pool_cloned))
        .layer(TraceLayer::new_for_http());

    let login_req = UserLoginRequest {
        email: "test@here.com".into(),
        password: "abc12345".into(),
    };

    let user = User::login(login_req, &db_pool)
        .await
        .expect("could not login test user");

    let validator = Validator::find_all(&db_pool)
        .await
        .expect("could not get list of validators")
        .first()
        .expect("could not get first validator")
        .to_owned();

    let rewards: Vec<RewardRequest> = vec![
        RewardRequest {
            block: 1,
            hash: "1".into(),
            txn_time: Utc::now(),
            validator_id: validator.id,
            user_id: Some(user.id),
            account: "1".into(),
            validator: "1".into(),
            amount: 5000,
        },
        RewardRequest {
            block: 1,
            hash: "2".into(),
            txn_time: Utc::now(),
            validator_id: validator.id,
            user_id: Some(user.id),
            account: "1".into(),
            validator: "1".into(),
            amount: 10000,
        },
        RewardRequest {
            block: 1,
            hash: "1".into(),
            txn_time: Utc::now(),
            validator_id: validator.id,
            user_id: Some(user.id),
            account: "1".into(),
            validator: "1".into(),
            amount: 5000,
        },
    ];

    let service_token = std::env::var("API_SERVICE_SECRET").expect("Missing API_SERVICE_SECRET");

    let req = Request::builder()
        .method("POST")
        .uri("/rewards")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", service_token))
        .body(Body::from(serde_json::to_string(&rewards)?))?;

    let resp = app.clone().oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    let path = format!("/users/{}/rewards/summary", user.id);

    let req = Request::builder()
        .method("GET")
        .uri(&path)
        .header("Content-Type", "application/json")
        .header(
            "Authorization",
            format!(
                "Bearer {}",
                user.token.clone().unwrap_or_else(|| "".to_string())
            ),
        )
        .body(Body::empty())?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(resp.into_body()).await?;
    let summary: RewardSummary = serde_json::from_slice(&body)?;

    assert_eq!(summary.total, 15000);
    assert_eq!(summary.last_30, 15000);

    Ok(())
}

#[tokio::test]
async fn it_should_create_payments() -> anyhow::Result<()> {
    let db_pool = Arc::new(setup().await);
    let db_pool_cloned = Arc::clone(&db_pool);
    let app = Router::new()
        .route("/payments", post(create_payments))
        .layer(Extension(db_pool_cloned))
        .layer(TraceLayer::new_for_http());

    let login_req = UserLoginRequest {
        email: "test@here.com".into(),
        password: "abc12345".into(),
    };

    let user = User::login(login_req, &db_pool)
        .await
        .expect("could not login test user");

    let payments: Vec<Payment> = vec![Payment {
        block: 1,
        hash: "1".into(),
        user_id: user.id,
        payer: "123".into(),
        payee: "124".into(),
        amount: 5000,
        oracle_price: 1000,
        created_at: None,
    }];

    let service_token = std::env::var("API_SERVICE_SECRET").expect("Missing API_SERVICE_SECRET");

    let req = Request::builder()
        .method("POST")
        .uri("/payments")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", service_token))
        .body(Body::from(serde_json::to_string(&payments)?))?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    Ok(())
}

#[tokio::test]
async fn it_should_list_invoices() -> anyhow::Result<()> {
    let db_pool = Arc::new(setup().await);
    let db_pool_cloned = Arc::clone(&db_pool);
    let app = Router::new()
        .route("/users/:user_id/invoices", get(list_invoices))
        .layer(Extension(db_pool_cloned))
        .layer(TraceLayer::new_for_http());

    let login_req = UserLoginRequest {
        email: "test@here.com".into(),
        password: "abc12345".into(),
    };

    let user = User::login(login_req, &db_pool)
        .await
        .expect("could not login test user");

    let path = format!("/users/{}/invoices", user.id);

    let req = Request::builder()
        .method("GET")
        .uri(&path)
        .header(
            "Authorization",
            format!(
                "Bearer {}",
                user.token.clone().unwrap_or_else(|| "".to_string())
            ),
        )
        .body(Body::empty())?;

    let resp = app.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    Ok(())
}

async fn setup() -> PgPool {
    dotenv::dotenv().ok();

    let db_url = std::env::var("DATABASE_URL").expect("Missing DATABASE_URL");
    if db_url.contains("digitalocean") {
        panic!("Attempting to use production db?");
    }
    let db_max_conn = std::env::var("DB_MAX_CONN")
        .unwrap_or_else(|_| "10".to_string())
        .parse()
        .unwrap();

    let pool = PgPoolOptions::new()
        .max_connections(db_max_conn)
        .connect(&db_url)
        .await
        .expect("Could not create db connection pool.");

    reset_db(&pool.clone()).await;

    pool
}

async fn reset_db(pool: &PgPool) {
    sqlx::query("DELETE FROM payments")
        .execute(pool)
        .await
        .expect("Error deleting payments");
    sqlx::query("DELETE FROM rewards")
        .execute(pool)
        .await
        .expect("Error deleting rewards");
    sqlx::query("DELETE FROM validators")
        .execute(pool)
        .await
        .expect("Error deleting validators");
    sqlx::query("DELETE FROM hosts")
        .execute(pool)
        .await
        .expect("Error deleting hosts");
    sqlx::query("DELETE FROM users")
        .execute(pool)
        .await
        .expect("Error deleting users");
    sqlx::query("DELETE FROM orgs")
        .execute(pool)
        .await
        .expect("Error deleting orgs");
    sqlx::query("DELETE FROM info")
        .execute(pool)
        .await
        .expect("Error deleting info");
    sqlx::query("DELETE FROM invoices")
        .execute(pool)
        .await
        .expect("Error deleting invoices");
    sqlx::query("INSERT INTO info (block_height) VALUES (99)")
        .execute(pool)
        .await
        .expect("could not update info in test setup");

    let user = UserRequest {
        email: "test@here.com".into(),
        password: "abc12345".into(),
        password_confirm: "abc12345".into(),
    };

    let user = User::create(user, pool)
        .await
        .expect("Could not create test user in db.");

    sqlx::query(
        "UPDATE users set pay_address = '123456', staking_quota = 3 where email = 'test@here.com'",
    )
    .execute(pool)
    .await
    .expect("could not set user's pay address for user test user in sql");

    sqlx::query("INSERT INTO invoices (user_id, earnings, fee_bps, validators_count, amount, starts_at, ends_at, is_paid) values ($1, 99, 200, 1, 1000000000, now(), now(), false)")
        .bind(user.id)
        .execute(pool)
            .await
            .expect("could insert test invoice into db");

    let user = UserRequest {
        email: "admin@here.com".into(),
        password: "abc12345".into(),
        password_confirm: "abc12345".into(),
    };

    User::create(user, pool)
        .await
        .expect("Could not create test user in db.");

    sqlx::query("UPDATE users set role = 'admin' where email = 'admin@here.com'")
        .execute(pool)
        .await
        .expect("could not set admin to admin test user in sql");

    let host = HostRequest {
        org_id: None,
        name: "Test user".into(),
        version: Some("0.1.0".into()),
        location: Some("Virgina".into()),
        cpu_count: None,
        mem_size: None,
        disk_size: None,
        os: None,
        os_version: None,
        ip_addr: "192.168.1.1".into(),
        val_ip_addrs: Some(
            "192.168.0.1, 192.168.0.2, 192.168.0.3, 192.168.0.4, 192.168.0.5".into(),
        ),
        token: "123".into(),
        status: ConnectionStatus::Online,
    };

    Host::create(host, pool)
        .await
        .expect("Could not create test host in db.");

    let host = Host::find_by_token("123", pool)
        .await
        .expect("Could not fetch test host in db.");

    let status = ValidatorStatusRequest {
        version: None,
        block_height: None,
        status: ValidatorStatus::Synced,
    };

    for v in host.validators.expect("No validators.") {
        let _ = Validator::update_status(v.id, status.clone(), pool)
            .await
            .expect("Error updating validator status in db during setup.");
        let _ = Validator::update_stake_status(v.id, StakeStatus::Available, pool)
            .await
            .expect("Error updating validator stake status in db during setup.");
    }

    let host = HostRequest {
        org_id: None,
        name: "Test Host2".into(),
        version: Some("0.1.0".into()),
        location: Some("Ohio".into()),
        cpu_count: None,
        mem_size: None,
        disk_size: None,
        os: None,
        os_version: None,
        ip_addr: "192.168.2.1".into(),
        val_ip_addrs: Some(
            "192.168.3.1, 192.168.3.2, 192.168.3.3, 192.168.3.4, 192.168.3.5".into(),
        ),
        token: "1234".into(),
        status: ConnectionStatus::Online,
    };

    Host::create(host, pool)
        .await
        .expect("Could not create test host in db.");

    let host = Host::find_by_token("1234", pool)
        .await
        .expect("Could not fetch test host in db.");

    let status = ValidatorStatusRequest {
        version: None,
        block_height: None,
        status: ValidatorStatus::Synced,
    };

    for v in host.validators.expect("No validators.") {
        let _ = Validator::update_status(v.id, status.clone(), pool)
            .await
            .expect("Error updating validator status in db during setup.");
        let _ = Validator::update_stake_status(v.id, StakeStatus::Available, pool)
            .await
            .expect("Error updating validator stake status in db during setup.");
    }
}

async fn get_test_host(db_pool: &PgPool) -> Host {
    Host::find_by_token("123", db_pool)
        .await
        .expect("Could not read test host from db.")
}

async fn get_admin_user(db_pool: &PgPool) -> User {
    User::find_by_email("admin@here.com", db_pool)
        .await
        .expect("Could not get admin test user from db.")
        .set_jwt()
        .expect("Could not set JWT.")
}
