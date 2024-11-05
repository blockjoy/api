#[cfg(any(test, feature = "integration-test"))]
pub mod seed;

use std::future::Future;
use std::sync::Arc;

use derive_more::{Deref, DerefMut};
use diesel::{ConnectionError, ConnectionResult};
use diesel_async::pooled_connection::bb8::{self, PooledConnection};
use diesel_async::pooled_connection::{AsyncDieselConnectionManager, ManagerConfig};
use diesel_async::scoped_futures::{ScopedBoxFuture, ScopedFutureExt};
use diesel_async::{AsyncConnection, AsyncPgConnection};
use diesel_migrations::EmbeddedMigrations;
use displaydoc::Display;
use futures_util::future::BoxFuture;
use futures_util::FutureExt;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::client::WebPkiServerVerifier;
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{CertificateError, ClientConfig, DigitallySignedStruct, RootCertStore};
use thiserror::Error;
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio_postgres_rustls::MakeRustlsConnect;
use tonic::metadata::AsciiMetadataValue;
use tracing::warn;

use crate::auth::rbac::Perms;
use crate::auth::resource::Resources;
use crate::auth::{self, AuthZ, Authorize};
use crate::config::database::Config;
use crate::config::Context;
use crate::grpc::{self, Metadata, ResponseMessage, Status};
use crate::model::rbac::{RbacPerm, RbacRole};
use crate::mqtt::Message;

pub const MIGRATIONS: EmbeddedMigrations = diesel_migrations::embed_migrations!();

// returns `impl Future` for async trait with `pub` visibility
pub trait Database {
    /// Return a new connection to the database.
    fn conn(&self) -> impl Future<Output = Result<Conn<'_>, Error>>;
}

pub(crate) trait Transaction {
    /// Run a non-transactional closure to read from the database.
    ///
    /// Note that the function parameter constraints are not strictly necessary
    /// but mimic `Transaction::write` to make it easy to switch between each.=
    async fn read<'a, F, Response, ResponseInner, ErrInner, ErrOuter>(
        &'a self,
        f: F,
    ) -> Result<Response, ErrOuter>
    where
        F: for<'c> FnOnce(
                ReadConn<'c, 'a>,
            ) -> ScopedBoxFuture<'a, 'c, Result<ResponseInner, ErrInner>>
            + Send
            + 'a,
        Response: ResponseMessage<ResponseInner>,
        ResponseInner: Send + 'a,
        ErrInner: std::error::Error + From<diesel::result::Error> + Send + 'a,
        Status: From<ErrInner>,
        ErrOuter: From<Status> + From<Error>;

    /// Run a transactional closure to write to the database.
    async fn write<'a, F, Response, ResponseInner, ErrInner, ErrOuter>(
        &'a self,
        f: F,
    ) -> Result<Response, ErrOuter>
    where
        F: for<'c> FnOnce(
                WriteConn<'c, 'a>,
            ) -> ScopedBoxFuture<'a, 'c, Result<ResponseInner, ErrInner>>
            + Send
            + 'a,
        Response: ResponseMessage<ResponseInner>,
        ResponseInner: Send + 'a,
        ErrInner: std::error::Error + From<diesel::result::Error> + Send + 'a,
        Status: From<ErrInner>,
        ErrOuter: From<Status> + From<Error>;
}

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to build database pool: {0}
    BuildPool(diesel_async::pooled_connection::PoolError),
    /// Failed to create RBAC perms: {0}
    CreatePerms(crate::model::rbac::Error),
    /// Failed to create RBAC roles: {0}
    CreateRoles(crate::model::rbac::Error),
    /// Failed to create a pool connection: {0}
    PoolConnection(bb8::RunError),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            BuildPool(_) | PoolConnection(_) => Status::internal("Internal error."),
            CreatePerms(err) | CreateRoles(err) => err.into(),
        }
    }
}

impl From<Error> for tonic::Status {
    fn from(value: Error) -> Self {
        match value {
            Error::BuildPool(_err) => tonic::Status::internal("Failed to create db pool"),
            Error::CreatePerms(err) => {
                let status: Status = err.into();
                status.into()
            }
            Error::CreateRoles(err) => {
                let status: Status = err.into();
                status.into()
            }
            Error::PoolConnection(_err) => {
                tonic::Status::internal("Failed to create db connection")
            }
        }
    }
}

/// A `Conn` is an open connection to the database from the `Pool`.
#[derive(Deref, DerefMut)]
pub struct Conn<'c>(PooledConnection<'c, AsyncPgConnection>);

/// A `ReadConn` is an open, non-transaction connection to the database.
#[derive(Deref, DerefMut)]
pub struct ReadConn<'c, 't> {
    #[deref]
    #[deref_mut]
    pub conn: &'c mut Conn<'t>,
    pub ctx: &'t Context,
}

impl<'c, 't> Authorize for ReadConn<'c, 't> {
    async fn authorize(
        &mut self,
        meta: &grpc::Metadata,
        perms: Perms,
        resources: Option<Resources>,
    ) -> Result<AuthZ, auth::Error> {
        self.ctx
            .auth
            .authorize_metadata(meta, perms, resources, self)
            .await
    }
}

/// A `WriteConn` is an open transactional connection to the database.
///
/// Any messages sent over `mqtt_tx` will be forwared to MQTT only after the
/// database transaction has been committed.
#[derive(Deref, DerefMut)]
pub struct WriteConn<'c, 't> {
    #[deref]
    #[deref_mut]
    pub conn: &'c mut Conn<'t>,
    pub ctx: &'t Context,

    pub meta_tx: UnboundedSender<(&'static str, AsciiMetadataValue)>,
    pub mqtt_tx: UnboundedSender<Message>,
}

impl<'c, 't> Authorize for WriteConn<'c, 't> {
    async fn authorize(
        &mut self,
        meta: &grpc::Metadata,
        perms: Perms,
        resources: Option<Resources>,
    ) -> Result<AuthZ, auth::Error> {
        self.ctx
            .auth
            .authorize_metadata(meta, perms, resources, self)
            .await
    }
}

impl<'c, 't> WriteConn<'c, 't> {
    pub fn meta(&mut self, key: &'static str, val: AsciiMetadataValue) {
        // safety: meta_rx is open for the lifetime of WriteConn
        self.meta_tx.send((key, val)).expect("meta_rx");
    }

    pub fn mqtt<M>(&mut self, message: M)
    where
        M: Into<Message>,
    {
        // safety: mqtt_rx is open for the lifetime of WriteConn
        self.mqtt_tx.send(message.into()).expect("mqtt_rx");
    }
}

#[derive(Clone, Deref, DerefMut)]
pub struct Pool(bb8::Pool<AsyncPgConnection>);

impl Pool {
    pub async fn new(config: &Config) -> Result<Self, Error> {
        let mut manager_config = ManagerConfig::default();
        manager_config.custom_setup = Box::new(establish_connection);
        let manager = AsyncDieselConnectionManager::<AsyncPgConnection>::new_with_config(
            config.url.as_str(),
            manager_config,
        );

        bb8::Pool::builder()
            .max_size(config.max_conns)
            .min_idle(Some(config.min_conns))
            .max_lifetime(Some(*config.max_lifetime))
            .idle_timeout(Some(*config.idle_timeout))
            .build(manager)
            .await
            .map(Self)
            .map_err(Error::BuildPool)
    }

    pub fn is_open(&self) -> bool {
        self.state().connections > 0
    }
}

impl Database for Pool {
    async fn conn(&self) -> Result<Conn<'_>, Error> {
        self.get().await.map(Conn).map_err(Error::PoolConnection)
    }
}

impl Database for Context {
    async fn conn(&self) -> Result<Conn<'_>, Error> {
        self.pool.conn().await
    }
}

impl<C> Transaction for C
where
    C: AsRef<Context> + Send + Sync,
{
    async fn read<'a, F, Response, ResponseInner, ErrInner, ErrOuter>(
        &'a self,
        f: F,
    ) -> Result<Response, ErrOuter>
    where
        F: for<'c> FnOnce(
                ReadConn<'c, 'a>,
            ) -> ScopedBoxFuture<'a, 'c, Result<ResponseInner, ErrInner>>
            + Send
            + 'a,
        Response: ResponseMessage<ResponseInner>,
        ResponseInner: Send + 'a,
        ErrInner: std::error::Error + From<diesel::result::Error> + Send + 'a,
        Status: From<ErrInner>,
        ErrOuter: From<Status> + From<Error>,
    {
        let ctx = self.as_ref();
        let conn = &mut ctx.conn().await?;
        let read = ReadConn { conn, ctx };
        let response = f(read).await.map_err(Status::from)?;
        Ok(Response::construct(response, Metadata::new()))
    }

    async fn write<'a, F, Response, ResponseInner, ErrInner, ErrOuter>(
        &'a self,
        f: F,
    ) -> Result<Response, ErrOuter>
    where
        F: for<'c> FnOnce(
                WriteConn<'c, 'a>,
            ) -> ScopedBoxFuture<'a, 'c, Result<ResponseInner, ErrInner>>
            + Send
            + 'a,
        Response: ResponseMessage<ResponseInner>,
        ResponseInner: Send + 'a,
        ErrInner: std::error::Error + From<diesel::result::Error> + Send + 'a,
        Status: From<ErrInner>,
        ErrOuter: From<Status> + From<Error>,
    {
        let ctx = self.as_ref();
        let conn = &mut ctx.conn().await?;

        let (meta_tx, mut meta_rx) = mpsc::unbounded_channel();
        let (mqtt_tx, mut mqtt_rx) = mpsc::unbounded_channel();

        let response = conn
            .transaction(|conn| {
                let write = WriteConn {
                    conn,
                    ctx,
                    meta_tx,
                    mqtt_tx,
                };
                f(write).scope_boxed()
            })
            .await
            .map_err(Status::from)?;

        while let Some(msg) = mqtt_rx.recv().await {
            if let Err(err) = ctx.notifier.send(msg).await {
                warn!("Failed to send MQTT message: {err}");
            }
        }

        let mut meta = Metadata::new();
        while let Some((key, val)) = meta_rx.recv().await {
            meta.insert_grpc(key, val);
        }

        Ok(Response::construct(response, meta))
    }
}

/// A custom establish function for a new `AsyncPgConnection` that requires TLS.
fn establish_connection(config: &str) -> BoxFuture<'_, ConnectionResult<AsyncPgConnection>> {
    let fut = async {
        let client_config = ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(IgnoreIssuer::new()))
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

// Because the server certificate has an IP address rather than hostname, we
// ignore `CertificateError::UnknownIssuer` server verification errors.
#[derive(Debug)]
struct IgnoreIssuer {
    pki: Arc<WebPkiServerVerifier>,
    provider: Arc<CryptoProvider>,
}

impl IgnoreIssuer {
    fn new() -> Self {
        let mut root_certs = RootCertStore::empty();
        for cert in rustls_native_certs::load_native_certs().expect("load native certs") {
            root_certs.add(cert).expect("add root cert");
        }

        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let pki = WebPkiServerVerifier::builder_with_provider(root_certs.into(), provider.clone())
            .build()
            .expect("pki cert verifier");

        IgnoreIssuer { pki, provider }
    }
}

impl ServerCertVerifier for IgnoreIssuer {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        match self
            .pki
            .verify_server_cert(end_entity, intermediates, server_name, ocsp, now)
        {
            Ok(verified) => Ok(verified),
            Err(rustls::Error::InvalidCertificate(CertificateError::UnknownIssuer)) => {
                Ok(ServerCertVerified::assertion())
            }
            Err(err) => Err(err),
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        let algorithms = &self.provider.signature_verification_algorithms;
        verify_tls12_signature(message, cert, dss, algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        let algorithms = &self.provider.signature_verification_algorithms;
        verify_tls13_signature(message, cert, dss, algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Ensure that all RBAC roles and permissions exist in the database.
pub async fn create_roles_and_perms(conn: &mut Conn<'_>) -> Result<(), Error> {
    RbacRole::create_all(conn)
        .await
        .map_err(Error::CreateRoles)?;
    RbacPerm::create_all(conn).await.map_err(Error::CreatePerms)
}

#[cfg(any(test, feature = "integration-test"))]
pub mod tests {
    use diesel::migration::MigrationSource;
    use diesel::prelude::*;
    use diesel_async::pooled_connection::bb8;
    use diesel_async::pooled_connection::AsyncDieselConnectionManager;
    use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
    use rand::{Rng, RngCore};

    use super::seed::Seed;
    use super::*;

    pub struct TestDb {
        pub pool: Pool,
        pub seed: Seed,
        pub test_db_name: String,
        pub main_db_url: String,
    }

    impl TestDb {
        /// Sets up a new test database.
        ///
        /// This creates a new db with a random name, runs all migrations, and
        /// fills it with seed data.
        pub async fn new<R>(config: &Config, rng: &mut R) -> TestDb
        where
            R: RngCore + Send,
        {
            let main_db_url = config.url.to_string();
            let test_db_name = Self::db_name(rng);

            // First we connect to the main db to run the `CREATE DATABASE` query.
            let mut conn = AsyncPgConnection::establish(&main_db_url).await.unwrap();
            diesel::sql_query(format!("CREATE DATABASE {test_db_name};"))
                .execute(&mut conn)
                .await
                .unwrap();

            // Then we connect to the new test database and run all migrations.
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
                .max_size(config.max_conns)
                .build(manager)
                .await
                .map(Pool)
                .unwrap();

            // Finally we seed the new database with test data.
            let seed = Seed::new(&mut pool.conn().await.unwrap()).await;

            TestDb {
                pool,
                seed,
                test_db_name,
                main_db_url,
            }
        }

        fn db_name<R: RngCore>(rng: &mut R) -> String {
            const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
            let mut db_name = "test_".to_string();
            for _ in 0..10 {
                db_name.push(CHARSET[rng.gen_range(0..26)] as char);
            }
            db_name
        }

        async fn tear_down(test_db_name: String, main_db_url: String) {
            let mut conn = AsyncPgConnection::establish(&main_db_url).await.unwrap();
            diesel::sql_query(format!("DROP DATABASE {test_db_name}"))
                .execute(&mut conn)
                .await
                .unwrap();
        }

        pub fn pool(&self) -> Pool {
            self.pool.clone()
        }

        pub async fn conn(&self) -> Conn<'_> {
            self.pool.conn().await.unwrap()
        }
    }

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
}
