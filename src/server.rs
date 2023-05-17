use crate::auth::key_provider::KeyProvider;
use crate::grpc::server as grpc_server;
use crate::http::server as http_server;
use crate::hybrid_server::hybrid as hybrid_server;
use crate::models;
use diesel::{ConnectionError, ConnectionResult};
use diesel_async::pooled_connection::bb8::Pool;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::AsyncPgConnection;
use futures_util::future::BoxFuture;
use futures_util::FutureExt;
use std::sync::Arc;
use std::time::Duration;

pub async fn start() -> anyhow::Result<()> {
    let db_url = KeyProvider::get_var(models::DATABASE_URL)?;
    let db_max_conn: u32 = std::env::var("DB_MAX_CONN")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);
    let db_min_conn: u32 = std::env::var("DB_MIN_CONN")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2);

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let bind_ip = std::env::var("BIND_IP").unwrap_or_else(|_| "0.0.0.0".to_string());
    let addr = format!("{bind_ip}:{port}");

    // let config = AsyncDieselConnectionManager::<AsyncPgConnection>::new(&db_url);
    let mgr = AsyncDieselConnectionManager::<AsyncPgConnection>::new_with_setup(
        db_url,
        establish_connection,
    );
    let pool = Pool::builder()
        .max_size(db_max_conn)
        .min_idle(Some(db_min_conn))
        .max_lifetime(Some(Duration::from_secs(60 * 60 * 24)))
        .idle_timeout(Some(Duration::from_secs(60 * 2)))
        .build(mgr)
        .await?;

    let db = models::DbPool::new(pool);

    let rest = http_server(db.clone()).await.into_make_service();
    let grpc = grpc_server(db).await.into_service();
    let hybrid = hybrid_server(rest, grpc);

    Ok(axum::Server::bind(&addr.parse()?).serve(hybrid).await?)
}

fn root_certs() -> rustls::RootCertStore {
    let mut roots = rustls::RootCertStore::empty();
    let certs = rustls_native_certs::load_native_certs().expect("Certs not loadable!");
    let certs: Vec<_> = certs.into_iter().map(|cert| cert.0).collect();
    roots.add_parsable_certificates(&certs);
    roots
}

/// This function is a custom establish function for a new `AsyncPgConnection`. The difference
/// between this one and the standard one is that is function requires TLS.
fn establish_connection(config: &str) -> BoxFuture<ConnectionResult<AsyncPgConnection>> {
    let fut = async {
        let rustls_config = rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_custom_certificate_verifier(Arc::new(DontVerifyHostName::new(root_certs())))
            .with_no_client_auth();
        let tls = tokio_postgres_rustls::MakeRustlsConnect::new(rustls_config);
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

/// And now we come upon a sad state of affairs. The database is served not from a host name but
/// from an IP-address. This means that we cannot verify the hostname of the SSL certificate and we
/// have to implement a custom certificate verifier for our certificate. The custom implementation
/// falls back to the stardard `WebPkiVerifier`, but when it sees an `UnsupportedNameType` error
/// being returned from the verification process, it marks the verification as succeeded. This
/// emulates the default behaviour of SQLx and libpq.
struct DontVerifyHostName {
    pki: rustls::client::WebPkiVerifier,
}

impl DontVerifyHostName {
    fn new(roots: rustls::RootCertStore) -> Self {
        Self {
            pki: rustls::client::WebPkiVerifier::new(roots, None),
        }
    }
}

impl rustls::client::ServerCertVerifier for DontVerifyHostName {
    fn verify_server_cert(
        &self,
        end_entity: &rustls::Certificate,
        intermediates: &[rustls::Certificate],
        server_name: &rustls::ServerName,
        scts: &mut dyn Iterator<Item = &[u8]>,
        ocsp_response: &[u8],
        now: std::time::SystemTime,
    ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
        // We do the standard authentication process, check for the expected error, and mark it as
        // a success.
        let outcome = self.pki.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            scts,
            ocsp_response,
            now,
        );
        match outcome {
            Ok(o) => Ok(o),
            Err(rustls::Error::UnsupportedNameType) => {
                Ok(rustls::client::ServerCertVerified::assertion())
            }
            Err(e) => Err(e),
        }
    }
}
