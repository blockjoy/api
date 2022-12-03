pub mod auth;
pub mod errors;
pub mod grpc;
pub mod http;
pub mod hybrid_server;
pub mod mail;
pub mod managed_hosts;
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
    use crate::models::{self, validator};
    use rand::Rng;
    use sqlx::Connection;
    use std::net::IpAddr;
    use std::str::FromStr;
    use uuid::Uuid;

    pub struct TestDb {
        pub pool: sqlx::PgPool,
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
        pub async fn setup() -> TestDb {
            dotenv::dotenv().ok();

            let main_db_url = std::env::var("DATABASE_URL").expect("Missing DATABASE_URL");
            let db_name = Self::db_name();
            let mut conn = sqlx::PgConnection::connect(&main_db_url).await.unwrap();
            sqlx::query(&format!("CREATE DATABASE {db_name};"))
                .execute(&mut conn)
                .await
                .unwrap();

            let db_url_prefix =
                std::env::var("DATABASE_URL_NAKED").expect("Missing DATABASE_URL_NAKED");
            let db_url = format!("{db_url_prefix}/{db_name}");
            /*if db_url.contains("digitalocean") {
                panic!("Attempting to use production db?");
            }*/
            let db_max_conn = std::env::var("DB_MAX_CONN")
                .unwrap_or_else(|_| "10".to_string())
                .parse()
                .unwrap();

            let pool = sqlx::postgres::PgPoolOptions::new()
                .max_connections(db_max_conn)
                .connect(&db_url)
                .await
                .expect("Could not create db connection pool.");

            let db = TestDb {
                pool,
                test_db_name: db_name,
                main_db_url,
            };
            sqlx::migrate!("./migrations").run(&db.pool).await.unwrap();
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
        pub async fn seed(&self) {
            sqlx::query("INSERT INTO info (block_height) VALUES (99)")
                .execute(&self.pool)
                .await
                .expect("could not update info in test setup");
            sqlx::query("INSERT INTO blockchains (id,name,status,supported_node_types) values ('1fdbf4c3-ff16-489a-8d3d-87c8620b963c','Helium', 'production', '[{ \"id\": 2, \"properties\": [{\"name\": \"ip\",\"label\": \"IP address\",\"default\": \"\",\"type\": \"string\"},{\"name\": \"managed\",\"label\": \"Self hosted or managed?\",\"default\": \"true\",\"type\": \"boolean\"}]},{\"id\": 3,\"properties\": []}]')")
            .execute(&self.pool)
            .await
            .expect("Error inserting blockchain");
            sqlx::query("INSERT INTO blockchains (id,name,status,supported_node_types) values ('fd5e2a49-f741-4eb2-a8b1-ee6222146ced','DeletedChain', 'deleted', '[{ \"id\": 2, \"properties\": [{\"name\": \"ip\",\"label\": \"IP address\",\"default\": \"\",\"type\": \"string\"},{\"name\": \"managed\",\"label\": \"Self hosted or managed?\",\"default\": \"true\",\"type\": \"boolean\"}]},{\"id\": 3,\"properties\": []}]')")
            .execute(&self.pool)
            .await
            .expect("Error inserting blockchain");
            sqlx::query("INSERT INTO blockchains (name,status,supported_node_types) values ('Pocket', 'production', '[{ \"id\": 2, \"properties\": [{\"name\": \"ip\",\"label\": \"IP address\",\"default\": \"\",\"type\": \"string\"},{\"name\": \"managed\",\"label\": \"Self hosted or managed?\",\"default\": \"true\",\"type\": \"boolean\"}]},{\"id\": 3,\"properties\": []}]')")
            .execute(&self.pool)
            .await
            .expect("Error inserting blockchain");
            sqlx::query("INSERT INTO blockchains (name,status,supported_node_types) values ('Cosmos', 'production', '[{ \"id\": 2, \"properties\": [{\"name\": \"ip\",\"label\": \"IP address\",\"default\": \"\",\"type\": \"string\"},{\"name\": \"managed\",\"label\": \"Self hosted or managed?\",\"default\": \"true\",\"type\": \"boolean\"}]},{\"id\": 3,\"properties\": []}]');")
            .execute(&self.pool)
            .await
            .expect("Error inserting blockchain");
            sqlx::query("INSERT INTO blockchains (name,status,supported_node_types) values ('Etherium', 'production', '[{ \"id\": 2, \"properties\": [{\"name\": \"ip\",\"label\": \"IP address\",\"default\": \"\",\"type\": \"string\"},{\"name\": \"managed\",\"label\": \"Self hosted or managed?\",\"default\": \"true\",\"type\": \"boolean\"}]},{\"id\": 3,\"properties\": []}]');")
            .execute(&self.pool)
            .await
            .expect("Error inserting blockchain");
            sqlx::query("INSERT INTO blockchains (name,status,supported_node_types) values ('Etherium PoS', 'production', '[{ \"id\": 2, \"properties\": [{\"name\": \"ip\",\"label\": \"IP address\",\"default\": \"\",\"type\": \"string\"},{\"name\": \"managed\",\"label\": \"Self hosted or managed?\",\"default\": \"true\",\"type\": \"boolean\"}]},{\"id\": 3,\"properties\": []},{\"id\": 8,\"properties\": []},{\"id\": 3,\"properties\": []},{\"id\": 9,\"properties\": []},{\"id\": 7,\"properties\": []}]');")
            .execute(&self.pool)
            .await
            .expect("Error inserting blockchain");
            sqlx::query("INSERT INTO blockchains (name,status,supported_node_types) values ('Lightning', 'production', '[{ \"id\": 2, \"properties\": [{\"name\": \"ip\",\"label\": \"IP address\",\"default\": \"\",\"type\": \"string\"},{\"name\": \"managed\",\"label\": \"Self hosted or managed?\",\"default\": \"true\",\"type\": \"boolean\"}]},{\"id\": 3,\"properties\": []}]');")
            .execute(&self.pool)
            .await
            .expect("Error inserting blockchain");
            sqlx::query("INSERT INTO blockchains (name,status,supported_node_types) values ('Algorand', 'production', '[{ \"id\": 2, \"properties\": [{\"name\": \"ip\",\"label\": \"IP address\",\"default\": \"\",\"type\": \"string\"},{\"name\": \"managed\",\"label\": \"Self hosted or managed?\",\"default\": \"true\",\"type\": \"boolean\"}]},{\"id\": 3,\"properties\": []}]');")
            .execute(&self.pool)
            .await
            .expect("Error inserting blockchain");

            let user = models::UserRequest {
                email: "test@here.com".into(),
                first_name: "Luuk".into(),
                last_name: "Tester".into(),
                password: "abc12345".into(),
                password_confirm: "abc12345".into(),
            };

            let user = models::User::create(user, &self.pool, None)
                .await
                .expect("Could not create test user in db.");

            sqlx::query(
                "UPDATE users set pay_address = '123456', staking_quota = 3 where email = 'test@here.com'",
            )
            .execute(&self.pool)
            .await
            .expect("could not set user's pay address for user test user in sql");

            sqlx::query("INSERT INTO invoices (user_id, earnings, fee_bps, validators_count, amount, starts_at, ends_at, is_paid) values ($1, 99, 200, 1, 1000000000, now(), now(), false)")
            .bind(user.id)
            .execute(&self.pool)
            .await
            .expect("could insert test invoice into db");

            let user = models::UserRequest {
                email: "admin@here.com".into(),
                first_name: "Mister".into(),
                last_name: "Sister".into(),
                password: "abc12345".into(),
                password_confirm: "abc12345".into(),
            };

            let admin = models::User::create(user, &self.pool, Some(TokenRole::Admin))
                .await
                .expect("Could not create test user in db.");

            let orgs = models::Org::find_all_by_user(admin.id, &self.pool)
                .await
                .unwrap();
            let org = orgs.first().unwrap();

            let host = models::HostRequest {
                org_id: Some(org.id),
                name: "Host-1".into(),
                version: Some("0.1.0".into()),
                location: Some("Virginia".into()),
                cpu_count: None,
                mem_size: None,
                disk_size: None,
                os: None,
                os_version: None,
                ip_addr: "192.168.1.1".into(),
                val_ip_addrs: Some(
                    "192.168.0.1, 192.168.0.2, 192.168.0.3, 192.168.0.4, 192.168.0.5".into(),
                ),
                status: models::ConnectionStatus::Online,
                ip_range_from: Some(IpAddr::from_str("192.168.0.10").expect("invalid ip")),
                ip_range_to: Some(IpAddr::from_str("192.168.0.100").expect("invalid ip")),
                ip_gateway: Some(IpAddr::from_str("192.168.0.1").expect("invalid ip")),
            };

            let host = models::Host::create(host, &self.pool)
                .await
                .expect("Could not create test host in db.");

            let status = validator::ValidatorStatusRequest {
                version: None,
                block_height: None,
                status: validator::ValidatorStatus::Synced,
            };

            for v in host.validators.expect("No validators.") {
                let _ = validator::Validator::update_status(v.id, status.clone(), &self.pool)
                    .await
                    .expect("Error updating validator status in db during setup.");
                let _ = validator::Validator::update_stake_status(
                    v.id,
                    validator::StakeStatus::Available,
                    &self.pool,
                )
                .await
                .expect("Error updating validator stake status in db during setup.");
            }

            let host = models::HostRequest {
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
                status: models::ConnectionStatus::Online,
                ip_range_from: Some(IpAddr::from_str("192.12.0.10").expect("invalid ip")),
                ip_range_to: Some(IpAddr::from_str("192.12.0.20").expect("invalid ip")),
                ip_gateway: Some(IpAddr::from_str("192.12.0.1").expect("invalid ip")),
            };

            let host = models::Host::create(host, &self.pool)
                .await
                .expect("Could not create test host in db.");

            let status = validator::ValidatorStatusRequest {
                version: None,
                block_height: None,
                status: validator::ValidatorStatus::Synced,
            };

            for v in host.validators.expect("No validators.") {
                let _ = validator::Validator::update_status(v.id, status.clone(), &self.pool)
                    .await
                    .expect("Error updating validator status in db during setup.");
                let _ = validator::Validator::update_stake_status(
                    v.id,
                    validator::StakeStatus::Available,
                    &self.pool,
                )
                .await
                .expect("Error updating validator stake status in db during setup.");
            }
        }

        pub async fn test_host(&self) -> models::Host {
            sqlx::query("select h.* from hosts h where name = 'Host-1'")
                .map(|row| models::Host::try_from(row).unwrap_or_default())
                .fetch_one(&self.pool)
                .await
                .unwrap()
        }

        pub async fn admin_user(&self) -> models::User {
            models::User::find_by_email("admin@here.com", &self.pool)
                .await
                .expect("Could not get admin test user from db.")
        }

        #[allow(dead_code)]
        pub async fn blockchain(&self) -> models::Blockchain {
            let chains = models::Blockchain::find_all(&self.pool)
                .await
                .expect("To have at least one blockchain");
            chains
                .first()
                .expect("To have a test blockchain")
                .to_owned()
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
