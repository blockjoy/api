use std::sync::Arc;

use derive_more::{Deref, DerefMut};
use diesel::{ConnectionError, ConnectionResult};
use diesel_async::pooled_connection::bb8::{self, PooledConnection};
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::scoped_futures::{ScopedBoxFuture, ScopedFutureExt};
use diesel_async::{AsyncConnection, AsyncPgConnection};
use diesel_migrations::EmbeddedMigrations;
use displaydoc::Display;
use futures_util::future::BoxFuture;
use futures_util::FutureExt;
use rustls::client::{ServerCertVerified, ServerCertVerifier, WebPkiVerifier};
use rustls::{Certificate, ClientConfig, RootCertStore, ServerName};
use thiserror::Error;
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio_postgres_rustls::MakeRustlsConnect;
use tracing::{error, warn};

use crate::config::database::Config;
use crate::config::Context;
use crate::mqtt::Message;

pub const MIGRATIONS: EmbeddedMigrations = diesel_migrations::embed_migrations!();

#[tonic::async_trait]
pub trait Database {
    /// Return a new connection to the database.
    async fn conn(&self) -> Result<Conn<'_>, Error>;
}

#[tonic::async_trait]
pub trait Transaction {
    /// Run a non-transactional closure to read from the database.
    ///
    /// Note that the function parameter constraints are not strictly necessary
    /// but mimic `Transaction::write` to make it easy to switch between each.
    async fn read<'a, F, T, E>(&'a self, f: F) -> Result<T, tonic::Status>
    where
        F: for<'c> FnOnce(ReadConn<'c, 'a>) -> ScopedBoxFuture<'a, 'c, Result<T, E>> + Send + 'a,
        T: Send + 'a,
        E: From<diesel::result::Error> + Into<tonic::Status> + Send + 'a;

    /// Run a transactional closure to write to the database.
    async fn write<'a, F, T, E>(&'a self, f: F) -> Result<T, tonic::Status>
    where
        F: for<'c> FnOnce(WriteConn<'c, 'a>) -> ScopedBoxFuture<'a, 'c, Result<T, E>> + Send + 'a,
        T: Send + 'a,
        E: From<diesel::result::Error> + Into<tonic::Status> + Send + 'a;
}

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to build database pool: {0}
    BuildPool(diesel_async::pooled_connection::PoolError),
    /// Failed to create a pool connection: {0}
    PoolConnection(bb8::RunError),
}

impl From<Error> for tonic::Status {
    fn from(err: Error) -> Self {
        error!("{}: {err}", std::any::type_name::<Error>());

        use Error::*;
        match err {
            BuildPool(_) | PoolConnection(_) => tonic::Status::internal("Failed."),
        }
    }
}

/// A `Conn` is an open connection to the database from the `Pool`.
#[derive(Deref, DerefMut)]
pub struct Conn<'c>(PooledConnection<'c, AsyncPgConnection>);

/// A `ReadConn` is an open, non-transaction connection to the database.
pub struct ReadConn<'c, 't> {
    pub conn: &'c mut Conn<'t>,
    pub ctx: &'t Context,
}

/// A `WriteConn` is an open transactional connection to the database.
///
/// Any messages sent over `mqtt_tx` will be forwared to MQTT only after the
/// database transaction has been committed.
pub struct WriteConn<'c, 't> {
    pub conn: &'c mut Conn<'t>,
    pub ctx: &'t Context,
    pub mqtt_tx: UnboundedSender<Message>,
}

#[derive(Clone, Deref, DerefMut)]
pub struct Pool(bb8::Pool<AsyncPgConnection>);

impl Pool {
    pub async fn new(config: &Config) -> Result<Self, Error> {
        let manager = AsyncDieselConnectionManager::<AsyncPgConnection>::new_with_setup(
            config.url.as_str(),
            establish_connection,
        );

        bb8::Pool::builder()
            .max_size(config.pool.max_conns)
            .min_idle(Some(config.pool.min_conns))
            .max_lifetime(Some(*config.pool.max_lifetime))
            .idle_timeout(Some(*config.pool.idle_timeout))
            .build(manager)
            .await
            .map(Self)
            .map_err(Error::BuildPool)
    }

    pub fn is_open(&self) -> bool {
        self.state().connections > 0
    }
}

#[tonic::async_trait]
impl Database for Pool {
    async fn conn(&self) -> Result<Conn<'_>, Error> {
        self.get().await.map(Conn).map_err(Error::PoolConnection)
    }
}

#[tonic::async_trait]
impl Database for Context {
    async fn conn(&self) -> Result<Conn<'_>, Error> {
        self.pool.conn().await
    }
}

#[tonic::async_trait]
impl<C> Transaction for C
where
    C: AsRef<Context> + Send + Sync,
{
    async fn read<'a, F, T, E>(&'a self, f: F) -> Result<T, tonic::Status>
    where
        F: for<'c> FnOnce(ReadConn<'c, 'a>) -> ScopedBoxFuture<'a, 'c, Result<T, E>> + Send + 'a,
        T: Send + 'a,
        E: From<diesel::result::Error> + Into<tonic::Status> + Send + 'a,
    {
        let ctx = self.as_ref();
        let mut conn = ctx.conn().await?;
        let read = ReadConn {
            conn: &mut conn,
            ctx,
        };

        f(read).await.map_err(Into::into)
    }

    async fn write<'a, F, T, E>(&'a self, f: F) -> Result<T, tonic::Status>
    where
        F: for<'c> FnOnce(WriteConn<'c, 'a>) -> ScopedBoxFuture<'a, 'c, Result<T, E>> + Send + 'a,
        T: Send + 'a,
        E: From<diesel::result::Error> + Into<tonic::Status> + Send + 'a,
    {
        let ctx = self.as_ref();
        let (mqtt_tx, mut mqtt_rx) = mpsc::unbounded_channel();

        let response = ctx
            .conn()
            .await?
            .transaction(|conn| {
                let write = WriteConn { conn, ctx, mqtt_tx };

                f(write).scope_boxed()
            })
            .await
            .map_err(Into::into)?;

        while let Some(msg) = mqtt_rx.recv().await {
            if let Err(err) = ctx.notifier.send(msg).await {
                warn!("Failed to send MQTT message: {err}");
            }
        }

        Ok(response)
    }
}

/// A custom establish function for a new `AsyncPgConnection` that requires TLS.
fn establish_connection(config: &str) -> BoxFuture<'_, ConnectionResult<AsyncPgConnection>> {
    let fut = async {
        let client_config = ClientConfig::builder()
            .with_safe_defaults()
            .with_custom_certificate_verifier(Arc::new(DontVerifyHostName::new(root_certs())))
            .with_no_client_auth();
        let tls = MakeRustlsConnect::new(client_config);

        let (client, conn) = tokio_postgres::connect(config, tls)
            .await
            .map_err(|e| ConnectionError::BadConnection(e.to_string()))?;

        tokio::spawn(async move {
            if let Err(e) = conn.await {
                eprintln!("Database connection: {e}");
            }
        });

        AsyncPgConnection::try_from(client).await
    };

    fut.boxed()
}

fn root_certs() -> RootCertStore {
    let mut roots = RootCertStore::empty();
    let certs = rustls_native_certs::load_native_certs().expect("Certs not loadable!");
    let certs: Vec<_> = certs.into_iter().map(|cert| cert.0).collect();
    roots.add_parsable_certificates(&certs);
    roots
}

/// And now we come upon a sad state of affairs. The database is served not from a host name but
/// from an IP-address. This means that we cannot verify the hostname of the SSL certificate and we
/// have to implement a custom certificate verifier for our certificate. The custom implementation
/// falls back to the stardard `WebPkiVerifier`, but when it sees an `UnsupportedNameType` error
/// being returned from the verification process, it marks the verification as succeeded. This
/// emulates the default behaviour of SQLx and libpq.
struct DontVerifyHostName {
    pki: WebPkiVerifier,
}

impl DontVerifyHostName {
    fn new(roots: RootCertStore) -> Self {
        Self {
            pki: WebPkiVerifier::new(roots, None),
        }
    }
}

impl ServerCertVerifier for DontVerifyHostName {
    fn verify_server_cert(
        &self,
        end_entity: &Certificate,
        intermediates: &[Certificate],
        server_name: &ServerName,
        signed_cert_timestamps: &mut dyn Iterator<Item = &[u8]>,
        ocsp_response: &[u8],
        now: std::time::SystemTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        // We do the standard authentication process, check for the expected error, and mark it as
        // a success.
        let outcome = self.pki.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            signed_cert_timestamps,
            ocsp_response,
            now,
        );

        // TODO: fix error handling
        match outcome {
            Ok(o) => Ok(o),
            // Err(rustls::Error::UnsupportedNameType) => {
            //     Ok(rustls::client::ServerCertVerified::assertion())
            // }
            // Err(e) => Err(e),
            Err(_) => Ok(ServerCertVerified::assertion()),
        }
    }
}

#[cfg(any(test, feature = "integration-test"))]
pub mod tests {
    use diesel::migration::MigrationSource;
    use diesel::prelude::*;
    use diesel_async::pooled_connection::bb8;
    use diesel_async::pooled_connection::AsyncDieselConnectionManager;
    use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
    use rand::Rng;
    use uuid::Uuid;

    use crate::auth::resource::{NodeId, OrgId};
    use crate::models::schema::{blockchains, nodes, orgs};
    use crate::models::{Blockchain, Host, Node, Org, Region, User};

    use super::*;

    pub struct TestDb {
        pool: Pool,
        test_db_name: String,
        main_db_url: String,
    }

    impl TestDb {
        /// Sets up a new test database.
        ///
        /// This creates a new db with a random name, runs all migrations, then
        /// fils it with seed data.
        pub async fn new(config: &Config) -> TestDb {
            let main_db_url = config.url.to_string();
            let test_db_name = Self::db_name();

            // First we connect to the main db to run the `CREATE DATABASE` query.
            let mut conn = AsyncPgConnection::establish(&main_db_url).await.unwrap();
            diesel::sql_query(&format!("CREATE DATABASE {test_db_name};"))
                .execute(&mut conn)
                .await
                .unwrap();

            // Now we connect to the new test database then run all migrations.
            let test_db_url = match config.url.as_str().rsplit_once('/') {
                Some((prefix, _suffix)) => format!("{prefix}/{test_db_name}"),
                None => panic!("Failed to strip database name from url: {0}", config.url),
            };

            let mut conn = PgConnection::establish(&test_db_url).unwrap();
            for migration in MIGRATIONS.migrations().unwrap() {
                migration.run(&mut conn).unwrap();
            }

            // Next we construct a database pool over the test database.
            let manager =
                AsyncDieselConnectionManager::<AsyncPgConnection>::new(test_db_url.clone());
            let pool = bb8::Pool::builder()
                .max_size(config.pool.max_conns)
                .build(manager)
                .await
                .map(Pool)
                .expect("Pool");

            let db = TestDb {
                pool,
                test_db_name,
                main_db_url,
            };

            // Finally we seed the new database with test data.
            seed::all(&mut db.conn().await).await;

            db
        }

        pub fn pool(&self) -> Pool {
            self.pool.clone()
        }

        pub async fn conn(&self) -> Conn<'_> {
            self.pool.conn().await.unwrap()
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

        pub async fn blockchain(&self) -> Blockchain {
            let mut conn = self.conn().await;
            Blockchain::find_by_name("Ethereum", &mut conn)
                .await
                .unwrap()
        }

        pub async fn user(&self) -> User {
            let mut conn = self.conn().await;
            User::find_by_email("admin@here.com", &mut conn)
                .await
                .expect("Could not get admin test user from db.")
        }

        /// This user is unconfirmed.
        pub async fn unconfirmed_user(&self) -> User {
            let mut conn = self.conn().await;
            User::find_by_email("test@here.com", &mut conn)
                .await
                .expect("Could not get pleb test user from db.")
        }

        pub async fn org(&self) -> Org {
            let id = seed::ORG_ID.parse().unwrap();
            let mut conn = self.conn().await;
            Org::find_by_id(id, &mut conn).await.unwrap()
        }

        pub async fn host(&self) -> Host {
            let mut conn = self.conn().await;
            Host::find_by_name("Host-1", &mut conn).await.unwrap()
        }

        pub async fn host2(&self) -> Host {
            let mut conn = self.conn().await;
            Host::find_by_name("Host-2", &mut conn).await.unwrap()
        }

        pub async fn node(&self) -> Node {
            let id = seed::NODE_ID.parse().unwrap();
            let mut conn = self.conn().await;
            Node::find_by_id(id, &mut conn).await.unwrap()
        }
    }

    #[tonic::async_trait]
    impl Database for TestDb {
        async fn conn(&self) -> Result<Conn<'_>, Error> {
            self.pool.conn().await
        }
    }

    impl Drop for TestDb {
        fn drop(&mut self) {
            let test_db_name = self.test_db_name.clone();
            let main_db_url = self.main_db_url.clone();
            tokio::task::spawn(Self::tear_down(test_db_name, main_db_url));
        }
    }

    pub mod seed {
        use crate::grpc::common;
        use crate::models::blockchain::BlockchainId;
        use crate::models::host::{MonthlyCostUsd, NewHost};
        use crate::models::ip_address::NewIpAddressRange;
        use crate::models::org::NewOrgUser;
        use crate::models::user::NewUser;
        use crate::models::{
            Blockchain, ConnectionStatus, Host, HostType, IpAddress, NodeChainStatus, NodeProperty,
            NodeType, OrgRole, ResourceAffinity, User,
        };

        use super::*;

        pub const ORG_ID: &str = "08dede71-b97d-47c1-a91d-6ba0997b3cdd";
        pub const NODE_ID: &str = "cdbbc736-f399-42ab-86cf-617ce983011d";

        pub const BLOCKCHAIN_ID: &str = "ab5d8cfc-77b1-4265-9fee-ba71ba9de092";
        pub const BLOCKCHAIN_NODE_TYPE_ID: &str = "fb56b151-443b-491a-a2a5-50fc12343a91";
        pub const BLOCKCHAIN_VERSION_ID: &str = "a69e7195-8a78-4e3a-a79e-4ac89edf1d68";
        pub const BLOCKCHAIN_PROPERTY_KEYSTORE: &str = "5972a35a-333c-421f-ab64-a77f4ae17533";
        pub const BLOCKCHAIN_PROPERTY_SELF_HOSTED: &str = "a989ad08-b455-4a57-9fe0-696405947e48";

        pub async fn all(conn: &mut Conn<'_>) {
            let blockchain = blockchains(conn).await;
            let org_id = orgs(conn).await;
            let user = users(org_id, conn).await;
            let region = region(conn).await;
            let host = hosts(user, org_id, &region, conn).await;
            let (ip_gateway, ip_addr) = ip_addresses(&host, conn).await;
            nodes(org_id, host, blockchain, ip_gateway, ip_addr, conn).await;
        }

        async fn blockchains(conn: &mut Conn<'_>) -> Blockchain {
            let queries = [
                format!("INSERT INTO blockchains (id, name) VALUES ('{BLOCKCHAIN_ID}','Ethereum');"),
                format!("INSERT INTO blockchain_node_types (id, blockchain_id, node_type) VALUES ('{BLOCKCHAIN_NODE_TYPE_ID}', '{BLOCKCHAIN_ID}', 'validator');"),
                format!("INSERT INTO blockchain_versions (id, blockchain_id, blockchain_node_type_id, version) VALUES ('{BLOCKCHAIN_VERSION_ID}', '{BLOCKCHAIN_ID}', '{BLOCKCHAIN_NODE_TYPE_ID}', '3.3.0');"),
                format!("INSERT INTO blockchain_properties VALUES ('{BLOCKCHAIN_PROPERTY_KEYSTORE}', '{BLOCKCHAIN_ID}', 'keystore-file', NULL, 'file_upload', FALSE, FALSE, '{BLOCKCHAIN_NODE_TYPE_ID}', '{BLOCKCHAIN_VERSION_ID}', 'Keystore file contents');"),
                format!("INSERT INTO blockchain_properties VALUES ('{BLOCKCHAIN_PROPERTY_SELF_HOSTED}', '{BLOCKCHAIN_ID}', 'self-hosted', NULL, 'switch', FALSE, FALSE, '{BLOCKCHAIN_NODE_TYPE_ID}', '{BLOCKCHAIN_VERSION_ID}', 'Is this noderoni self hosted?');"),
            ];

            for query in queries {
                diesel::sql_query(query).execute(conn).await.unwrap();
            }

            let blockchain_id: BlockchainId = BLOCKCHAIN_ID.parse().unwrap();
            blockchains::table
                .filter(blockchains::id.eq(blockchain_id))
                .get_result(conn)
                .await
                .unwrap()
        }

        async fn orgs(conn: &mut Conn<'_>) -> OrgId {
            let org_id: OrgId = ORG_ID.parse().unwrap();

            diesel::insert_into(orgs::table)
                .values((
                    orgs::id.eq(org_id),
                    orgs::name.eq("the blockboys"),
                    orgs::is_personal.eq(false),
                ))
                .execute(conn)
                .await
                .unwrap();

            org_id
        }

        async fn users(org_id: OrgId, conn: &mut Conn<'_>) -> User {
            let user = NewUser::new("test@here.com", "Luuk", "Tester", "abc12345").unwrap();
            let admin = NewUser::new("admin@here.com", "Mr", "Admin", "abc12345").unwrap();

            let user = user.create(conn).await.unwrap();
            let admin = admin.create(conn).await.unwrap();

            NewOrgUser::new(org_id, admin.id, OrgRole::Admin)
                .create(conn)
                .await
                .unwrap();

            User::confirm(admin.id, conn).await.unwrap();

            user
        }

        async fn region(conn: &mut Conn<'_>) -> Region {
            Region::get_or_create("moneyland", conn).await.unwrap()
        }

        async fn hosts(user: User, org_id: OrgId, region: &Region, conn: &mut Conn<'_>) -> Host {
            let billing = common::BillingAmount {
                amount: Some(common::Amount {
                    currency: common::Currency::Usd as i32,
                    value: 123,
                }),
                period: common::Period::Monthly as i32,
            };

            let host1 = NewHost {
                name: "Host-1",
                version: "0.1.0",
                cpu_count: 16,
                mem_size_bytes: 1_612_312_312_000,   // 1.6 TB
                disk_size_bytes: 16_121_231_200_000, // 16 TB
                os: "LuukOS",
                os_version: "3",
                ip_addr: "192.168.1.1",
                status: ConnectionStatus::Online,
                ip_range_from: "192.168.0.10".parse().unwrap(),
                ip_range_to: "192.168.0.100".parse().unwrap(),
                ip_gateway: "192.168.0.1".parse().unwrap(),
                org_id,
                created_by: user.id,
                region_id: Some(region.id),
                host_type: HostType::Cloud,
                monthly_cost_in_usd: Some(MonthlyCostUsd::from_proto(&billing).unwrap()),
            };

            let host1 = host1.create(conn).await.unwrap();

            let host2 = NewHost {
                name: "Host-2",
                version: "0.1.0",
                cpu_count: 16,
                mem_size_bytes: 1_612_312_123_123,  // 1.6 TB
                disk_size_bytes: 1_612_312_123_123, // 1.6 TB
                os: "LuukOS",
                os_version: "3",
                ip_addr: "192.168.2.1",
                status: ConnectionStatus::Online,
                ip_range_from: "192.12.0.10".parse().unwrap(),
                ip_range_to: "192.12.0.20".parse().unwrap(),
                ip_gateway: "192.12.0.1".parse().unwrap(),
                org_id,
                created_by: user.id,
                region_id: Some(region.id),
                host_type: HostType::Cloud,
                monthly_cost_in_usd: None,
            };

            host2.create(conn).await.unwrap();

            host1
        }

        async fn ip_addresses(host: &Host, conn: &mut Conn<'_>) -> (String, String) {
            NewIpAddressRange::try_new(
                "127.0.0.1".parse().unwrap(),
                "127.0.0.10".parse().unwrap(),
                host.id,
            )
            .unwrap()
            .create(&[], conn)
            .await
            .unwrap();

            let ip_gateway = host.ip_gateway.ip().to_string();
            let ip_addr = IpAddress::next_for_host(host.id, conn)
                .await
                .unwrap()
                .ip
                .ip()
                .to_string();

            (ip_gateway, ip_addr)
        }

        async fn nodes(
            org_id: OrgId,
            host: Host,
            blockchain: Blockchain,
            ip_gateway: String,
            ip_addr: String,
            conn: &mut Conn<'_>,
        ) {
            let node_id: NodeId = NODE_ID.parse().unwrap();

            diesel::insert_into(nodes::table)
                .values((
                    nodes::id.eq(node_id),
                    nodes::name.eq("Test Node"),
                    nodes::org_id.eq(org_id),
                    nodes::host_id.eq(host.id),
                    nodes::blockchain_id.eq(blockchain.id),
                    nodes::block_age.eq(0),
                    nodes::consensus.eq(true),
                    nodes::chain_status.eq(NodeChainStatus::Broadcasting),
                    nodes::ip_gateway.eq(ip_gateway),
                    nodes::ip_addr.eq(ip_addr),
                    nodes::node_type.eq(NodeType::Validator),
                    nodes::dns_record_id.eq("The id"),
                    nodes::vcpu_count.eq(2),
                    nodes::disk_size_bytes.eq(8 * 1024 * 1024 * 1024),
                    nodes::mem_size_bytes.eq(1024 * 1024 * 1024),
                    nodes::scheduler_resource.eq(ResourceAffinity::LeastResources),
                    nodes::version.eq("3.3.0"),
                ))
                .execute(conn)
                .await
                .unwrap();

            NodeProperty::bulk_create(test_node_properties(node_id), conn)
                .await
                .unwrap();
        }

        fn test_node_properties(node_id: NodeId) -> Vec<NodeProperty> {
            vec![
                NodeProperty {
                    id: Uuid::new_v4(),
                    node_id,
                    blockchain_property_id: BLOCKCHAIN_PROPERTY_KEYSTORE.parse().unwrap(),
                    value: "Sneaky file content".to_string(),
                },
                NodeProperty {
                    id: Uuid::new_v4(),
                    node_id,
                    blockchain_property_id: BLOCKCHAIN_PROPERTY_SELF_HOSTED.parse().unwrap(),
                    value: "false".to_string(),
                },
            ]
        }
    }
}
