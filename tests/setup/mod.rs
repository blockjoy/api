mod helper_traits;

use api::models::validator::{StakeStatus, Validator, ValidatorStatus, ValidatorStatusRequest};
use api::models::{Blockchain, Org, TokenRole};
use api::models::{ConnectionStatus, Host, HostRequest};
use api::models::{User, UserRequest};
use helper_traits::GrpcClient;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::convert::TryFrom;
use std::fmt::Debug;
use std::future::Future;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::net::{UnixListener, UnixStream};
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;

#[macro_export]
macro_rules! assert_grpc_request {
    ($m:tt, $r:expr, $s:expr, $db: expr, $client: ty) => {{
        let (serve_future, mut client) = server_and_client_stub::<$client>($db).await;

        let request_future = async {
            match client.$m($r).await {
                Ok(response) => {
                    let inner = response.into_inner();
                    println!("response OK: {:?}", inner);
                }
                Err(e) => {
                    let s = Status::from(e);
                    println!("response ERROR: {:?}", s);
                    assert_eq!($s, s.code());
                }
            }
        };

        // Wait for completion, when the client request future completes
        tokio::select! {
            _ = serve_future => panic!("server returned first"),
            _ = request_future => (),
        }
    }};
}

#[allow(dead_code)]
pub async fn server_and_client_stub<Client>(db: Arc<PgPool>) -> (impl Future<Output = ()>, Client)
where
    Client: GrpcClient<Channel> + Debug,
{
    let socket = NamedTempFile::new().unwrap();
    let socket = Arc::new(socket.into_temp_path());
    std::fs::remove_file(&*socket).unwrap();

    let uds = UnixListener::bind(&*socket).unwrap();
    let stream = UnixListenerStream::new(uds);
    let serve_future = async {
        let result = api::grpc::server(db)
            .await
            .serve_with_incoming(stream)
            .await;

        assert!(result.is_ok());
    };

    let socket = Arc::clone(&socket);
    // Connect to the server over a Unix socket
    // The URL will be ignored.
    let channel = Endpoint::try_from("http://any.url")
        .unwrap()
        .connect_with_connector(service_fn(move |_: Uri| {
            let socket = Arc::clone(&socket);
            async move { UnixStream::connect(&*socket).await }
        }))
        .await
        .unwrap();

    let client = Client::create(channel);

    (serve_future, client)
}

pub async fn setup() -> PgPool {
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

pub async fn reset_db(pool: &PgPool) {
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
    sqlx::query("DELETE FROM tokens")
        .execute(pool)
        .await
        .expect("Error deleting tokens");
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
    sqlx::query("DELETE FROM blockchains")
        .execute(pool)
        .await
        .expect("Error deleting blockchains");
    sqlx::query("DELETE FROM host_provisions")
        .execute(pool)
        .await
        .expect("Error deleting host_provisions");
    sqlx::query("INSERT INTO info (block_height) VALUES (99)")
        .execute(pool)
        .await
        .expect("could not update info in test setup");
    sqlx::query("INSERT INTO blockchains (name,status) values ('Helium', 'production')")
        .execute(pool)
        .await
        .expect("Error inserting blockchains");
    sqlx::query("DELETE FROM broadcast_filters")
        .execute(pool)
        .await
        .expect("Error deleting broadcast_filters");

    let user = UserRequest {
        email: "test@here.com".into(),
        password: "abc12345".into(),
        password_confirm: "abc12345".into(),
    };

    let user = User::create(user, pool, None)
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

    let admin = User::create(user, pool, Some(TokenRole::Admin))
        .await
        .expect("Could not create test user in db.");

    let orgs = Org::find_all_by_user(admin.id, pool).await.unwrap();
    let org = orgs.first().unwrap();

    let host = HostRequest {
        org_id: Some(org.id),
        name: "Host-1".into(),
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
        status: ConnectionStatus::Online,
    };

    let host = Host::create(host, pool)
        .await
        .expect("Could not create test host in db.");

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
        name: "Host-2".into(),
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
        status: ConnectionStatus::Online,
    };

    let host = Host::create(host, pool)
        .await
        .expect("Could not create test host in db.");

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

pub async fn get_test_host(db: &PgPool) -> Host {
    sqlx::query("select h.*, t.token, t.role from hosts h right join tokens t on h.id = t.host_id where name = 'Host-1'")
        .map(Host::from)
        .fetch_one(db)
        .await
        .unwrap()
}

pub async fn get_admin_user(db: &PgPool) -> User {
    User::find_by_email("admin@here.com", db)
        .await
        .expect("Could not get admin test user from db.")
}

#[allow(dead_code)]
pub async fn get_blockchain(db: &PgPool) -> Blockchain {
    let chains = Blockchain::find_all(db)
        .await
        .expect("To have at least one blockchain");
    chains
        .first()
        .expect("To have a test blockchain")
        .to_owned()
}
