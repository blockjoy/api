pub mod auth;
pub mod cookbook;
pub mod errors;
pub mod grpc;
pub mod http;
pub mod hybrid_server;
pub mod mail;
pub mod models;
pub mod server;

use errors::Result;

pub const MIGRATIONS: diesel_migrations::EmbeddedMigrations =
    diesel_migrations::embed_migrations!();

// #[cfg(test)]
pub use test::TestDb;
// #[cfg(test)]
mod test {
    use crate::auth::expiration_provider::ExpirationProvider;
    use crate::auth::{
        HostRefreshToken, JwtToken, TokenClaim, TokenRole, TokenType, UserRefreshToken,
    };
    use crate::models;
    use crate::models::schema::{blockchains, commands, nodes, orgs};
    use diesel::migration::MigrationSource;
    use diesel::prelude::*;
    use diesel_async::pooled_connection::bb8::Pool;
    use diesel_async::pooled_connection::AsyncDieselConnectionManager;
    use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
    use rand::Rng;
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
            let mut conn = AsyncPgConnection::establish(&main_db_url).await.unwrap();
            diesel::sql_query(&format!("CREATE DATABASE {db_name};"))
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

            let config = AsyncDieselConnectionManager::<diesel_async::AsyncPgConnection>::new(
                db_url.clone(),
            );
            let pool = Pool::builder()
                .max_size(db_max_conn)
                .build(config)
                .await
                .unwrap();

            // With our constructed pool, we can create a tester, migrate the database and seed it
            // with some data for our tests.
            let db = TestDb {
                pool: models::DbPool::new(pool),
                test_db_name: db_name,
                main_db_url: db_url,
            };
            for migration in super::MIGRATIONS.migrations().unwrap() {
                migration
                    .run(&mut PgConnection::establish(&db.test_db_name).unwrap())
                    .unwrap();
            }
            db.seed().await;
            db
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
            // self.pool
            //     .trx(|c| async move { Self::_seed(c).await })
            //     .await
            //     .expect("Could not seed db");
        }

        async fn _seed(conn: &mut diesel_async::AsyncPgConnection) -> crate::Result<()> {
            diesel::sql_query("INSERT INTO info (block_height) VALUES (99)")
                .execute(conn)
                .await
                .expect("could not update info in test setup");
            /*
            diesel::sql_query("INSERT INTO blockchains (id,name,status,supported_node_types) values ('fd5e2a49-f741-4eb2-a8b1-ee6222146ced','DeletedChain', 'deleted', '[{ \"id\": 2, \"properties\": [{\"name\": \"ip\",\"label\": \"IP address\",\"default\": \"\",\"type\": \"string\"},{\"name\": \"managed\",\"label\": \"Self hosted or managed?\",\"default\": \"true\",\"type\": \"boolean\"}]},{\"id\": 3,\"properties\": []}]')")
            .execute(conn)
            .await?;
            diesel::sql_query("INSERT INTO blockchains (name,status,supported_node_types) values ('Pocket', 'production', '[{ \"id\": 2, \"properties\": [{\"name\": \"ip\",\"label\": \"IP address\",\"default\": \"\",\"type\": \"string\"},{\"name\": \"managed\",\"label\": \"Self hosted or managed?\",\"default\": \"true\",\"type\": \"boolean\"}]},{\"id\": 3,\"properties\": []}]')")
            .execute(conn)
            .await?;
            diesel::sql_query("INSERT INTO blockchains (name,status,supported_node_types) values ('Cosmos', 'production', '[{ \"id\": 2, \"properties\": [{\"name\": \"ip\",\"label\": \"IP address\",\"default\": \"\",\"type\": \"string\"},{\"name\": \"managed\",\"label\": \"Self hosted or managed?\",\"default\": \"true\",\"type\": \"boolean\"}]},{\"id\": 3,\"properties\": []}]');")
            .execute(conn)
            .await?;
            diesel::sql_query("INSERT INTO blockchains (name,status,supported_node_types) values ('Etherium', 'production', '[{ \"id\": 2, \"properties\": [{\"name\": \"ip\",\"label\": \"IP address\",\"default\": \"\",\"type\": \"string\"},{\"name\": \"managed\",\"label\": \"Self hosted or managed?\",\"default\": \"true\",\"type\": \"boolean\"}]},{\"id\": 3,\"properties\": []}]');")
            .execute(conn)
            .await?;
            diesel::sql_query("INSERT INTO blockchains (name,status,supported_node_types) values ('Lightning', 'production', '[{ \"id\": 2, \"properties\": [{\"name\": \"ip\",\"label\": \"IP address\",\"default\": \"\",\"type\": \"string\"},{\"name\": \"managed\",\"label\": \"Self hosted or managed?\",\"default\": \"true\",\"type\": \"boolean\"}]},{\"id\": 3,\"properties\": []}]');")
            .execute(conn)
            .await?;
            diesel::sql_query("INSERT INTO blockchains (name,status,supported_node_types) values ('Algorand', 'production', '[{ \"id\": 2, \"properties\": [{\"name\": \"ip\",\"label\": \"IP address\",\"default\": \"\",\"type\": \"string\"},{\"name\": \"managed\",\"label\": \"Self hosted or managed?\",\"default\": \"true\",\"type\": \"boolean\"}]},{\"id\": 3,\"properties\": []}]');")
            .execute(conn)
            .await?;
             */

            diesel::sql_query("INSERT INTO blockchains (id,name,status,supported_node_types) values ('1fdbf4c3-ff16-489a-8d3d-87c8620b963c','Helium', 'production', '[]')")
                .execute(conn)
                .await?;
            diesel::sql_query("INSERT INTO blockchains (name,status,supported_node_types) values ('Ethereum', 'production', '[{\"id\":3,\"version\": \"3.3.0\", \"properties\":[{\"name\":\"keystore-file\",\"ui_type\":\"key-upload\",\"default\":\"\",\"disabled\":false,\"required\":true},{\"name\":\"self-hosted\",\"ui_type\":\"switch\",\"default\":\"false\",\"disabled\":true,\"required\":true}]}]');")
                .execute(conn)
                .await?;
            // let blockchain: models::Blockchain = diesel::sql_query("INSERT INTO blockchains (name,status,supported_node_types) values ('Helium', 'production', '[{\"id\":3, \"version\": \"0.0.3\",\"properties\":[{\"name\":\"keystore-file\",\"ui_type\":\"key-upload\",\"default\":\"\",\"disabled\":false,\"required\":true},{\"name\":\"self-hosted\",\"ui_type\":\"switch\",\"default\":\"false\",\"disabled\":true,\"required\":true}]}]') RETURNING *;")
            let blockchain: models::Blockchain = diesel::insert_into(blockchains::table)
                .values((
                    blockchains::name.eq("Helium"),
                    blockchains::status.eq(models::BlockchainStatus::Production),
                    blockchains::supported_node_types.eq(serde_json::json!(
                        [
                            {
                                "id": 3,
                                "version": "0.0.3",
                                "properties": [
                                    {
                                        "name": "keystore-file",
                                        "ui_type": "key-upload",
                                        "default": "",
                                        "disabled": false,
                                        "required": true
                                    },
                                    {
                                        "name": "self-hosted",
                                        "ui_type": "switch",
                                        "default": "false",
                                        "disabled": true,
                                        "required": true
                                    },
                                ],
                            },
                        ]
                    )),
                ))
                .get_result(conn)
                .await?;

            let user = models::NewUser::new("test@here.com", "Luuk", "Tester", "abc12345").unwrap();
            let admin = models::NewUser::new("admin@here.com", "Mr", "Admin", "abc12345").unwrap();

            let user = user.create(conn).await?;

            admin.create(conn).await?;

            diesel::sql_query(
                "UPDATE users set pay_address = '123456', staking_quota = 3 where email = 'test@here.com'",
            )
            .execute(conn)
            .await
            ?;

            diesel::sql_query("
            INSERT INTO
                invoices (user_id, earnings, fee_bps, validators_count, amount, starts_at, ends_at, is_paid)
            VALUES
                ($1, 99, 200, 1, 1000000000, now(), now(), false);")
                .bind::<diesel::sql_types::Uuid, _>(user.id)
                .execute(conn)
                .await
                ?;

            let host1 = models::NewHost {
                name: "Host-1",
                version: Some("0.1.0"),
                location: Some("Virginia"),
                cpu_count: Some(16),
                mem_size: Some(1612312312),
                disk_size: Some(161212312),
                os: None,
                os_version: None,
                ip_addr: "192.168.1.1",
                status: models::ConnectionStatus::Online,
                ip_range_from: "192.168.0.10".parse()?,
                ip_range_to: "192.168.0.100".parse()?,
                ip_gateway: "192.168.0.1".parse()?,
            };

            let host1 = host1.create(conn).await?;

            let host2 = models::NewHost {
                name: "Host-2",
                version: Some("0.1.0"),
                location: Some("Ohio"),
                cpu_count: Some(16),
                mem_size: Some(1612312312),
                disk_size: Some(161212312),
                os: None,
                os_version: None,
                ip_addr: "192.168.2.1",
                status: models::ConnectionStatus::Online,
                ip_range_from: "192.12.0.10".parse()?,
                ip_range_to: "192.12.0.20".parse()?,
                ip_gateway: "192.12.0.1".parse()?,
            };

            host2.create(conn).await?;

            let org_id: uuid::Uuid = diesel::insert_into(orgs::table)
                .values(orgs::name.eq("the blockboys"))
                .returning(orgs::id)
                .get_result(conn)
                .await
                .unwrap();

            diesel::insert_into(nodes::table)
                .values((
                    nodes::org_id.eq(org_id),
                    nodes::host_id.eq(host1.id),
                    nodes::blockchain_id.eq(blockchain.id),
                    nodes::node_type.eq(serde_json::json!({"id": 1})),
                    nodes::block_age.eq(0),
                    nodes::consensus.eq(true),
                ))
                .execute(conn)
                .await
                .unwrap();
            Ok(())
        }

        pub async fn host(&self) -> models::Host {
            models::Host::find_by_name("Host-1", &mut self.pool.conn().await.unwrap())
                .await
                .unwrap()
        }

        pub async fn node(&self) -> models::Node {
            nodes::table
                .limit(1)
                .get_result(&mut self.pool.conn().await.unwrap())
                .await
                .unwrap()
        }

        pub async fn org(&self) -> models::Org {
            models::Org::find_all(&mut self.pool.conn().await.unwrap())
                .await
                .unwrap()
                .pop()
                .unwrap()
        }

        pub async fn command(&self) -> models::Command {
            let host = self.host().await;
            let id: Uuid = "eab8a84b-8e3d-4b02-bf14-4160e76c177b".parse().unwrap();
            diesel::insert_into(commands::table)
                .values((
                    commands::id.eq(id),
                    commands::host_id.eq(host.id),
                    commands::cmd.eq(models::HostCmd::RestartNode),
                ))
                .get_result(&mut self.pool.conn().await.unwrap())
                .await
                .unwrap()
        }

        pub async fn admin_user(&self) -> models::User {
            models::User::find_by_email("admin@here.com", &mut self.pool.conn().await.unwrap())
                .await
                .expect("Could not get admin test user from db.")
        }

        pub async fn blockchain(&self) -> models::Blockchain {
            blockchains::table
                .filter(blockchains::name.eq("Ethereum"))
                .get_result(&mut self.pool.conn().await.unwrap())
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
