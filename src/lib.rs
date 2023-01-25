pub mod auth;
pub mod cookbook;
pub mod errors;
pub mod grpc;
pub mod http;
pub mod hybrid_server;
pub mod mail;
pub mod models;
pub mod server;

// #[cfg(test)]
pub use test::TestDb;
// #[cfg(test)]
mod test {
    use crate::auth::expiration_provider::ExpirationProvider;
    use crate::auth::{
        HostRefreshToken, JwtToken, TokenClaim, TokenRole, TokenType, UserRefreshToken,
    };
    use crate::models;
    use rand::Rng;
    use sqlx::Connection;
    use std::net::IpAddr;
    use std::str::FromStr;
    use uuid::Uuid;

    #[derive(Clone)]
    pub struct TestDb {
        pub pool: models::DbPool,
        test_db_name: String,
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
        /// Sets up a new test database. That means creating a new db with a random name,
        /// connecting to that new database and then migrating it and filling it with our seed
        /// data.
        pub async fn setup() -> TestDb {
            dotenv::dotenv().ok();

            // First we open up a connection to the main db. This is for running the
            // `CREATE DATABASE` query.
            let main_db_url = std::env::var("DATABASE_URL").expect("Missing DATABASE_URL");
            let db_name = Self::db_name();
            let mut conn = sqlx::PgConnection::connect(&main_db_url).await.unwrap();
            sqlx::query(&format!("CREATE DATABASE {db_name};"))
                .execute(&mut conn)
                .await
                .unwrap();

            // Now we construct the url to our newly clreated database and connect to it.
            let db_url_prefix =
                std::env::var("DATABASE_URL_NAKED").expect("Missing DATABASE_URL_NAKED");
            let db_url = format!("{db_url_prefix}/{db_name}");
            let db_max_conn = std::env::var("DB_MAX_CONN")
                .unwrap_or_else(|_| "10".to_string())
                .parse()
                .unwrap();
            let pool = sqlx::postgres::PgPoolOptions::new()
                .max_connections(db_max_conn)
                .connect(&db_url)
                .await
                .expect("Could not create db connection pool.");

            // With our constructed pool, we can create a tester, migrate the database and seed it
            // with some data for our tests.
            let db = TestDb {
                pool: models::DbPool::new(pool),
                test_db_name: db_name,
                main_db_url,
            };
            sqlx::migrate!("./migrations")
                .run(&mut db.pool.conn().await.unwrap())
                .await
                .unwrap();
            db.seed().await;
            db
        }

        async fn tear_down(test_db_name: String, main_db_url: String) {
            let mut conn = sqlx::PgConnection::connect(&main_db_url).await.unwrap();
            sqlx::query(&format!("DROP DATABASE {test_db_name}"))
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
            let mut tx = self.pool.begin().await.unwrap();
            sqlx::query("INSERT INTO info (block_height) VALUES (99)")
                .execute(&mut tx)
                .await
                .expect("could not update info in test setup");
            /*
            sqlx::query("INSERT INTO blockchains (id,name,status,supported_node_types) values ('fd5e2a49-f741-4eb2-a8b1-ee6222146ced','DeletedChain', 'deleted', '[{ \"id\": 2, \"properties\": [{\"name\": \"ip\",\"label\": \"IP address\",\"default\": \"\",\"type\": \"string\"},{\"name\": \"managed\",\"label\": \"Self hosted or managed?\",\"default\": \"true\",\"type\": \"boolean\"}]},{\"id\": 3,\"properties\": []}]')")
            .execute(&mut tx)
            .await
            .expect("Error inserting blockchain");
            sqlx::query("INSERT INTO blockchains (name,status,supported_node_types) values ('Pocket', 'production', '[{ \"id\": 2, \"properties\": [{\"name\": \"ip\",\"label\": \"IP address\",\"default\": \"\",\"type\": \"string\"},{\"name\": \"managed\",\"label\": \"Self hosted or managed?\",\"default\": \"true\",\"type\": \"boolean\"}]},{\"id\": 3,\"properties\": []}]')")
            .execute(&mut tx)
            .await
            .expect("Error inserting blockchain");
            sqlx::query("INSERT INTO blockchains (name,status,supported_node_types) values ('Cosmos', 'production', '[{ \"id\": 2, \"properties\": [{\"name\": \"ip\",\"label\": \"IP address\",\"default\": \"\",\"type\": \"string\"},{\"name\": \"managed\",\"label\": \"Self hosted or managed?\",\"default\": \"true\",\"type\": \"boolean\"}]},{\"id\": 3,\"properties\": []}]');")
            .execute(&mut tx)
            .await
            .expect("Error inserting blockchain");
            sqlx::query("INSERT INTO blockchains (name,status,supported_node_types) values ('Etherium', 'production', '[{ \"id\": 2, \"properties\": [{\"name\": \"ip\",\"label\": \"IP address\",\"default\": \"\",\"type\": \"string\"},{\"name\": \"managed\",\"label\": \"Self hosted or managed?\",\"default\": \"true\",\"type\": \"boolean\"}]},{\"id\": 3,\"properties\": []}]');")
            .execute(&mut tx)
            .await
            .expect("Error inserting blockchain");
            sqlx::query("INSERT INTO blockchains (name,status,supported_node_types) values ('Lightning', 'production', '[{ \"id\": 2, \"properties\": [{\"name\": \"ip\",\"label\": \"IP address\",\"default\": \"\",\"type\": \"string\"},{\"name\": \"managed\",\"label\": \"Self hosted or managed?\",\"default\": \"true\",\"type\": \"boolean\"}]},{\"id\": 3,\"properties\": []}]');")
            .execute(&mut tx)
            .await
            .expect("Error inserting blockchain");
            sqlx::query("INSERT INTO blockchains (name,status,supported_node_types) values ('Algorand', 'production', '[{ \"id\": 2, \"properties\": [{\"name\": \"ip\",\"label\": \"IP address\",\"default\": \"\",\"type\": \"string\"},{\"name\": \"managed\",\"label\": \"Self hosted or managed?\",\"default\": \"true\",\"type\": \"boolean\"}]},{\"id\": 3,\"properties\": []}]');")
            .execute(&mut tx)
            .await
            .expect("Error inserting blockchain");
             */

            sqlx::query("INSERT INTO blockchains (id,name,status,supported_node_types) values ('1fdbf4c3-ff16-489a-8d3d-87c8620b963c','Helium', 'production', '[]')")
                .execute(&mut tx)
                .await
                .expect("Error inserting blockchain");
            sqlx::query("INSERT INTO blockchains (name,status,supported_node_types) values ('Ethereum', 'production', '[{\"id\":3,\"version\": \"3.3.0\", \"properties\":[{\"name\":\"keystore-file\",\"ui_type\":\"key-upload\",\"default\":\"\",\"disabled\":false,\"required\":true},{\"name\":\"self-hosted\",\"ui_type\":\"switch\",\"default\":\"false\",\"disabled\":true,\"required\":true}]},{\"id\":1, \"version\": \"3.3.0\",\"properties\":[{\"name\":\"keystore-file\",\"ui_type\":\"key-upload\",\"default\":\"\",\"disabled\":false,\"required\":true},{\"name\":\"self-hosted\",\"ui_type\":\"switch\",\"default\":\"false\",\"disabled\":true,\"required\":true}]}]');")
                .execute(&mut tx)
                .await
                .expect("Error inserting blockchain");
            let blockchain: models::Blockchain = sqlx::query_as("INSERT INTO blockchains (name,status,supported_node_types) values ('Helium', 'production', '[{\"id\":3, \"version\": \"0.0.3\",\"properties\":[{\"name\":\"keystore-file\",\"ui_type\":\"key-upload\",\"default\":\"\",\"disabled\":false,\"required\":true},{\"name\":\"self-hosted\",\"ui_type\":\"switch\",\"default\":\"false\",\"disabled\":true,\"required\":true}]},{\"id\":1, \"version\": \"0.0.3\",\"properties\":[{\"name\":\"keystore-file\",\"ui_type\":\"key-upload\",\"default\":\"\",\"disabled\":false,\"required\":true},{\"name\":\"self-hosted\",\"ui_type\":\"switch\",\"default\":\"false\",\"disabled\":true,\"required\":true}]}]') RETURNING *;")
                .fetch_one(&mut tx)
                .await
                .expect("Error inserting blockchain");

            let user = models::UserRequest {
                email: "test@here.com".into(),
                first_name: "Luuk".into(),
                last_name: "Tester".into(),
                password: "abc12345".into(),
                password_confirm: "abc12345".into(),
            };

            let admin = models::UserRequest {
                email: "admin@here.com".into(),
                first_name: "Master".into(),
                last_name: "Admin".into(),
                password: "abc12345".into(),
                password_confirm: "abc12345".into(),
            };

            let user = models::User::create(user, None, &mut tx)
                .await
                .expect("Could not create test user in db.");

            let _ = models::User::create(admin, None, &mut tx)
                .await
                .expect("Could not create admin user in db.");

            sqlx::query(
                "UPDATE users set pay_address = '123456', staking_quota = 3 where email = 'test@here.com'",
            )
            .execute(&mut tx)
            .await
            .expect("could not set user's pay address for user test user in sql");

            sqlx::query("INSERT INTO invoices (user_id, earnings, fee_bps, validators_count, amount, starts_at, ends_at, is_paid) values ($1, 99, 200, 1, 1000000000, now(), now(), false)")
            .bind(user.id)
            .execute(&mut tx)
            .await
            .expect("could insert test invoice into db");

            let host1 = models::HostRequest {
                name: "Host-1".into(),
                version: Some("0.1.0".into()),
                location: Some("Virginia".into()),
                cpu_count: None,
                mem_size: None,
                disk_size: None,
                os: None,
                os_version: None,
                ip_addr: "192.168.1.1".into(),
                status: models::ConnectionStatus::Online,
                ip_range_from: Some(IpAddr::from_str("192.168.0.10").expect("invalid ip")),
                ip_range_to: Some(IpAddr::from_str("192.168.0.100").expect("invalid ip")),
                ip_gateway: Some(IpAddr::from_str("192.168.0.1").expect("invalid ip")),
            };

            let host1 = models::Host::create(host1, &mut tx)
                .await
                .expect("Could not create test host in db.");

            let host2 = models::HostRequest {
                name: "Host-2".into(),
                version: Some("0.1.0".into()),
                location: Some("Ohio".into()),
                cpu_count: None,
                mem_size: None,
                disk_size: None,
                os: None,
                os_version: None,
                ip_addr: "192.168.2.1".into(),
                status: models::ConnectionStatus::Online,
                ip_range_from: Some(IpAddr::from_str("192.12.0.10").expect("invalid ip")),
                ip_range_to: Some(IpAddr::from_str("192.12.0.20").expect("invalid ip")),
                ip_gateway: Some(IpAddr::from_str("192.12.0.1").expect("invalid ip")),
            };

            let _host2 = models::Host::create(host2, &mut tx)
                .await
                .expect("Could not create test host in db.");

            let org: models::Org =
                sqlx::query_as("INSERT INTO orgs (name) VALUES ('the blockboys') RETURNING *")
                    .fetch_one(&mut tx)
                    .await
                    .unwrap();
            sqlx::query(
                "INSERT INTO nodes
                    (org_id, host_id, blockchain_id, node_type, block_age, consensus)
                VALUES
                    ($1, $2, $3, $4::jsonb, 0, true)",
            )
            .bind(org.id)
            .bind(host1.id)
            .bind(blockchain.id)
            .bind("{\"id\": 1}")
            .execute(&mut tx)
            .await
            .unwrap();

            tx.commit().await.unwrap();
        }

        pub async fn host(&self) -> models::Host {
            sqlx::query("SELECT * FROM hosts WHERE name = 'Host-1'")
                .try_map(models::Host::try_from)
                .fetch_one(&mut self.pool.conn().await.unwrap())
                .await
                .unwrap()
        }

        pub async fn node(&self) -> models::Node {
            sqlx::query_as("SELECT * FROM nodes LIMIT 1")
                .fetch_one(&mut self.pool.conn().await.unwrap())
                .await
                .unwrap()
        }

        pub async fn org(&self) -> models::Org {
            sqlx::query_as("SELECT * FROM orgs LIMIT 1")
                .fetch_one(&mut self.pool.conn().await.unwrap())
                .await
                .unwrap()
        }

        pub async fn admin_user(&self) -> models::User {
            models::User::find_by_email("admin@here.com", &mut self.pool.conn().await.unwrap())
                .await
                .expect("Could not get admin test user from db.")
        }

        pub async fn blockchain(&self) -> models::Blockchain {
            sqlx::query_as("SELECT * FROM blockchains WHERE name = 'Ethereum'")
                .fetch_one(&mut self.pool.conn().await.unwrap())
                .await
                .unwrap()
        }

        pub fn user_refresh_token(&self, id: Uuid) -> UserRefreshToken {
            let claim = TokenClaim::new(
                id,
                ExpirationProvider::expiration(TokenType::UserRefresh),
                TokenType::UserRefresh,
                TokenRole::User,
                None,
            );

            UserRefreshToken::try_new(claim).unwrap()
        }

        pub fn host_refresh_token(&self, id: Uuid) -> HostRefreshToken {
            let claim = TokenClaim::new(
                id,
                ExpirationProvider::expiration(TokenType::HostRefresh),
                TokenType::HostRefresh,
                TokenRole::Service,
                None,
            );

            HostRefreshToken::try_new(claim).unwrap()
        }
    }
}
