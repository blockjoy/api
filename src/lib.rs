#![recursion_limit = "256"]

pub mod auth;
pub mod cloudflare;
pub mod config;
pub mod cookbook;
pub mod error;
pub mod grpc;
pub mod http;
pub mod hybrid_server;
pub mod mail;
pub mod models;
pub mod server;

use error::{Error, Result};

pub const MIGRATIONS: diesel_migrations::EmbeddedMigrations =
    diesel_migrations::embed_migrations!();

pub use test::{TestCloudflareApi, TestCookbook, TestDb};

mod test {
    use crate::auth::token::refresh::Refresh;
    use crate::cloudflare::CloudflareApi;
    use crate::config::cloudflare::{ApiConfig, Config as CloudflareConfig, DnsConfig};
    use crate::config::Context;
    use crate::cookbook::{self, Cookbook};
    use crate::models;
    use crate::models::schema::{blockchains, commands, nodes, orgs};
    use crate::models::Conn;
    use diesel::migration::MigrationSource;
    use diesel::prelude::*;
    use diesel_async::pooled_connection::bb8::Pool;
    use diesel_async::pooled_connection::AsyncDieselConnectionManager;
    use diesel_async::scoped_futures::ScopedFutureExt;
    use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
    use rand::Rng;
    use std::sync::Arc;
    use std::time::Duration;
    use uuid::Uuid;

    pub struct TestCloudflareApi {
        mock: mockito::ServerGuard,
    }

    impl TestCloudflareApi {
        pub async fn new() -> Self {
            let mock = Self::mock_cloudflare_api().await;
            Self { mock }
        }

        pub fn get_cloudflare_api(&self) -> CloudflareApi {
            CloudflareApi::new(self.mock_config())
        }

        async fn mock_cloudflare_api() -> mockito::ServerGuard {
            let mut cloudfare_server = mockito::Server::new_async().await;

            let mut rng = rand::thread_rng();
            let id_dns = rng.gen_range(200000..5000000);
            cloudfare_server
                .mock(
                    "POST",
                    mockito::Matcher::Regex(r"^/zones/.*/dns_records$".to_string()),
                )
                .with_status(200)
                .with_body(format!("{{\"result\":{{\"id\":\"{:x}\"}}}}", id_dns))
                .create_async()
                .await;
            cloudfare_server
                .mock(
                    "DELETE",
                    mockito::Matcher::Regex(r"^/zones/.*/dns_records/.*$".to_string()),
                )
                .with_status(200)
                .create_async()
                .await;
            cloudfare_server
        }

        pub fn mock_config(&self) -> Arc<CloudflareConfig> {
            let config = CloudflareConfig {
                api: ApiConfig {
                    base_url: self.mock.url(),
                    zone_id: "zone_id".into(),
                    token: "token".parse().unwrap(),
                },
                dns: DnsConfig {
                    base: "base".into(),
                    ttl: 3600,
                },
            };
            Arc::new(config)
        }
    }

    pub struct TestCookbook {
        mock: mockito::ServerGuard,
    }

    struct MockStorage {}

    #[tonic::async_trait]
    impl cookbook::Client for MockStorage {
        async fn read_file(&self, _: &str, _: &str) -> crate::Result<Vec<u8>> {
            Ok(cookbook::script::TEST_SCRIPT.bytes().collect())
        }

        async fn download_url(&self, _: &str, _: &str, _: Duration) -> crate::Result<String> {
            panic!("We're not using this in tests.")
        }

        async fn list(&self, _: &str, _: &str) -> crate::Result<Vec<String>> {
            panic!("We're not using this in tests.")
        }
    }

    impl TestCookbook {
        pub async fn new() -> Self {
            let mock = Self::mock_cookbook_api().await;
            Self { mock }
        }

        pub fn get_cookbook_api(&self) -> Cookbook {
            Cookbook::new_with_client(&self.mock_config(), MockStorage {})
        }

        async fn mock_cookbook_api() -> mockito::ServerGuard {
            let mut r2_server = mockito::Server::new_async().await;
            r2_server
                .mock("POST", mockito::Matcher::Regex(r"^/*".to_string()))
                .with_status(200)
                .with_body("{\"data\":\"id\"}")
                .create_async()
                .await;
            r2_server
        }

        pub fn mock_config(&self) -> Arc<crate::config::cookbook::Config> {
            let config = crate::config::cookbook::Config {
                dir_chains_prefix: "fake".to_string(),
                r2_bucket: "news".to_string(),
                r2_url: self.mock.url().parse().unwrap(),
                presigned_url_expiration: "1d".parse().unwrap(),
                region: "eu-west-3".to_string(),
                key_id: "not actually a".parse().unwrap(),
                key: "key".parse().unwrap(),
                bundle_dir: "bundles".to_string(),
                bundle_stage: "prod".to_string(),
            };
            Arc::new(config)
        }
    }

    #[derive(Clone)]
    pub struct TestDb {
        pub pool: models::DbPool,
        test_db_name: String,
        test_db_url: String,
        main_db_url: String,
    }

    impl Drop for TestDb {
        fn drop(&mut self) {
            let test_db_name = self.test_db_name.clone();
            let main_db_url = self.main_db_url.clone();
            tokio::task::spawn(Self::tear_down(test_db_name, main_db_url));
        }
    }

    impl TestDb {
        /// Sets up a new test database. That means creating a new db with a random name, connecting
        /// to that new database and then migrating it and filling it with our seed data.
        pub async fn setup(context: Arc<Context>) -> TestDb {
            let config = context.config.as_ref();

            let main_db_url = config.database.url.to_string();
            let test_db_name = Self::db_name();

            // First we open up a connection to the main db. This is for running the
            // `CREATE DATABASE` query.
            let mut conn = AsyncPgConnection::establish(&main_db_url).await.unwrap();
            diesel::sql_query(&format!("CREATE DATABASE {test_db_name};"))
                .execute(&mut conn)
                .await
                .unwrap();

            // Now we construct the url to our newly created database and connect to it.
            let test_db_url = match config.database.url.as_str().rsplit_once('/') {
                Some((prefix, _suffix)) => format!("{prefix}/{test_db_name}"),
                None => panic!(
                    "Failed to strip database name from url: {0}",
                    config.database.url
                ),
            };

            let manager = AsyncDieselConnectionManager::<diesel_async::AsyncPgConnection>::new(
                test_db_url.clone(),
            );
            let pool = Pool::builder()
                .max_size(config.database.pool.max_conns)
                .build(manager)
                .await
                .expect("Pool");

            // With our constructed pool, we can create a tester, migrate the database and seed it
            // with some data for our tests.
            let db = TestDb {
                pool: models::DbPool::new(pool, context.clone()),
                test_db_name,
                test_db_url,
                main_db_url,
            };

            let mut conn = PgConnection::establish(&db.test_db_url).unwrap();
            for migration in super::MIGRATIONS.migrations().unwrap() {
                migration.run(&mut conn).unwrap();
            }

            db.seed().await;
            db
        }

        pub async fn conn(&self) -> Conn {
            self.pool.conn().await.unwrap()
        }

        pub async fn create_node(
            node: &models::NewNode<'_>,
            host_id_param: &uuid::Uuid,
            ip_add_param: &str,
            dns_id: &str,
            conn: &mut AsyncPgConnection,
        ) {
            diesel::insert_into(nodes::table)
                .values((
                    node,
                    nodes::host_id.eq(host_id_param),
                    nodes::ip_addr.eq(ip_add_param),
                    nodes::dns_record_id.eq(dns_id),
                ))
                .execute(conn)
                .await
                .unwrap();
        }

        async fn tear_down(test_db_name: String, main_db_url: String) {
            let mut conn = AsyncPgConnection::establish(&main_db_url).await.unwrap();
            diesel::sql_query(&format!("DROP DATABASE {test_db_name}"))
                .execute(&mut conn)
                .await
                .unwrap();
        }

        fn db_name() -> String {
            const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
            let mut rng = rand::thread_rng();
            let mut db_name = "test_".to_string();
            for _ in 0..10 {
                db_name.push(CHARSET[rng.gen_range(0..26)] as char);
            }
            db_name
        }

        /// Seeds the database with some initial data that we need for running tests.
        async fn seed(&self) {
            self.pool
                .trx(|c| Self::seed_(c).scope_boxed())
                .await
                .expect("Could not seed db");
        }

        async fn seed_(conn: &mut Conn) -> crate::Result<()> {
            diesel::sql_query("INSERT INTO blockchains (id, name) values ('ab5d8cfc-77b1-4265-9fee-ba71ba9de092','Ethereum');")
                .execute(conn)
                .await.unwrap();
            diesel::sql_query("INSERT INTO blockchain_properties VALUES ('5972a35a-333c-421f-ab64-a77f4ae17533', 'ab5d8cfc-77b1-4265-9fee-ba71ba9de092', '3.3.0', 'validator', 'keystore-file', NULL, 'file_upload', FALSE, FALSE);").execute(conn).await.unwrap();
            diesel::sql_query("INSERT INTO blockchain_properties VALUES ('a989ad08-b455-4a57-9fe0-696405947e48', 'ab5d8cfc-77b1-4265-9fee-ba71ba9de092', '3.3.0', 'validator', 'self-hosted',   NULL, 'switch',      FALSE, FALSE);").execute(conn).await.unwrap();
            let blockchain_id: uuid::Uuid = "ab5d8cfc-77b1-4265-9fee-ba71ba9de092".parse().unwrap();
            let blockchain: models::Blockchain = blockchains::table
                .filter(blockchains::id.eq(blockchain_id))
                .get_result(conn)
                .await
                .unwrap();

            let org_id: uuid::Uuid = "08dede71-b97d-47c1-a91d-6ba0997b3cdd".parse().unwrap();
            diesel::insert_into(orgs::table)
                .values((
                    orgs::id.eq(org_id),
                    orgs::name.eq("the blockboys"),
                    orgs::is_personal.eq(false),
                ))
                .execute(conn)
                .await
                .unwrap();

            let user = models::NewUser::new("test@here.com", "Luuk", "Tester", "abc12345").unwrap();
            let admin = models::NewUser::new("admin@here.com", "Mr", "Admin", "abc12345").unwrap();

            let user = user.create(conn).await.unwrap();
            let admin = admin.create(conn).await.unwrap();

            models::NewOrgUser::new(org_id, admin.id, models::OrgRole::Admin)
                .create(conn)
                .await
                .unwrap();

            models::User::confirm(admin.id, conn).await.unwrap();

            let host1 = models::NewHost {
                name: "Host-1",
                version: "0.1.0",
                cpu_count: 16,
                mem_size_bytes: 1_612_312_312_000,   // 1.6 TB
                disk_size_bytes: 16_121_231_200_000, // 16 TB
                os: "LuukOS",
                os_version: "3",
                ip_addr: "192.168.1.1",
                status: models::ConnectionStatus::Online,
                ip_range_from: "192.168.0.10".parse().unwrap(),
                ip_range_to: "192.168.0.100".parse().unwrap(),
                ip_gateway: "192.168.0.1".parse().unwrap(),
                org_id: None,
                created_by: user.id,
            };

            host1.create(conn).await.unwrap();

            let host2 = models::NewHost {
                name: "Host-2",
                version: "0.1.0",
                cpu_count: 16,
                mem_size_bytes: 1_612_312,  // 1.6 MB
                disk_size_bytes: 1_612_312, // 1.6 MB
                os: "LuukOS",
                os_version: "3",
                ip_addr: "192.168.2.1",
                status: models::ConnectionStatus::Online,
                ip_range_from: "192.12.0.10".parse().unwrap(),
                ip_range_to: "192.12.0.20".parse().unwrap(),
                ip_gateway: "192.12.0.1".parse().unwrap(),
                org_id: Some(org_id),
                created_by: user.id,
            };

            let host2 = host2.create(conn).await.unwrap();

            models::NewIpAddressRange::try_new(
                "127.0.0.1".parse().unwrap(),
                "127.0.0.10".parse().unwrap(),
                host2.id,
            )
            .unwrap()
            .create(conn)
            .await
            .unwrap();

            let ip_gateway = host2.ip_gateway.ip().to_string();
            let ip_addr = models::IpAddress::next_for_host(host2.id, conn)
                .await
                .unwrap()
                .ip
                .ip()
                .to_string();

            let node_id: uuid::Uuid = "cdbbc736-f399-42ab-86cf-617ce983011d".parse().unwrap();
            diesel::insert_into(nodes::table)
                .values((
                    nodes::id.eq(node_id),
                    nodes::name.eq("Test Node"),
                    nodes::org_id.eq(org_id),
                    nodes::host_id.eq(host2.id),
                    nodes::blockchain_id.eq(blockchain.id),
                    nodes::block_age.eq(0),
                    nodes::consensus.eq(true),
                    nodes::chain_status.eq(models::NodeChainStatus::Broadcasting),
                    nodes::ip_gateway.eq(ip_gateway),
                    nodes::ip_addr.eq(ip_addr),
                    nodes::node_type.eq(models::NodeType::Validator),
                    nodes::dns_record_id.eq("The id"),
                    nodes::vcpu_count.eq(2),
                    nodes::disk_size_bytes.eq(8 * 1024 * 1024 * 1024),
                    nodes::mem_size_bytes.eq(1024 * 1024 * 1024),
                    nodes::scheduler_resource.eq(models::ResourceAffinity::LeastResources),
                    nodes::version.eq("3.3.0"),
                ))
                .execute(conn)
                .await
                .unwrap();
            models::NodeProperty::bulk_create(Self::test_node_properties(node_id), conn)
                .await
                .unwrap();
            Ok(())
        }

        pub async fn host(&self) -> models::Host {
            let mut conn = self.conn().await;
            models::Host::find_by_name("Host-1", &mut conn)
                .await
                .unwrap()
        }

        pub async fn node(&self) -> models::Node {
            nodes::table
                .limit(1)
                .get_result(&mut self.conn().await)
                .await
                .unwrap()
        }

        pub async fn org(&self) -> models::Org {
            let id = "08dede71-b97d-47c1-a91d-6ba0997b3cdd".parse().unwrap();
            let mut conn = self.conn().await;
            models::Org::find_by_id(id, &mut conn).await.unwrap()
        }

        pub async fn command(&self) -> models::Command {
            let host = self.host().await;
            let node = self.node().await;
            let id: Uuid = "eab8a84b-8e3d-4b02-bf14-4160e76c177b".parse().unwrap();
            diesel::insert_into(commands::table)
                .values((
                    commands::id.eq(id),
                    commands::host_id.eq(host.id),
                    commands::node_id.eq(node.id),
                    commands::cmd.eq(models::CommandType::RestartNode),
                ))
                .get_result(&mut self.conn().await)
                .await
                .unwrap()
        }

        pub async fn user(&self) -> models::User {
            let mut conn = self.conn().await;
            models::User::find_by_email("admin@here.com", &mut conn)
                .await
                .expect("Could not get admin test user from db.")
        }

        /// This user is unconfirmed.
        pub async fn unconfirmed_user(&self) -> models::User {
            let mut conn = self.conn().await;
            models::User::find_by_email("test@here.com", &mut conn)
                .await
                .expect("Could not get pleb test user from db.")
        }

        pub async fn blockchain(&self) -> models::Blockchain {
            blockchains::table
                .filter(blockchains::name.eq("Ethereum"))
                .get_result(&mut self.conn().await)
                .await
                .unwrap()
        }

        pub fn user_refresh_token(&self, user_id: Uuid) -> Refresh {
            let iat = chrono::Utc::now();
            let refresh_exp = self
                .pool
                .context
                .config
                .token
                .expire
                .refresh_user
                .try_into()
                .unwrap();
            Refresh::new(user_id, iat, refresh_exp).unwrap()
        }

        pub fn host_refresh_token(&self, host_id: Uuid) -> Refresh {
            let iat = chrono::Utc::now();
            let refresh_exp = self
                .pool
                .context
                .config
                .token
                .expire
                .refresh_host
                .try_into()
                .unwrap();
            Refresh::new(host_id, iat, refresh_exp).unwrap()
        }

        fn test_node_properties(node_id: uuid::Uuid) -> Vec<models::NodeProperty> {
            vec![
                models::NodeProperty {
                    id: uuid::Uuid::new_v4(),
                    node_id,
                    blockchain_property_id: "5972a35a-333c-421f-ab64-a77f4ae17533".parse().unwrap(),
                    value: "Sneaky file content".to_string(),
                },
                models::NodeProperty {
                    id: uuid::Uuid::new_v4(),
                    node_id,
                    blockchain_property_id: "a989ad08-b455-4a57-9fe0-696405947e48".parse().unwrap(),
                    value: "false".to_string(),
                },
            ]
        }
    }
}
