use crate::cloudflare::CloudflareApi;
use crate::config::Context;
use crate::cookbook::Cookbook;
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

pub async fn start(context: Arc<Context>) -> anyhow::Result<()> {
    let config = context.config.as_ref();

    let manager = AsyncDieselConnectionManager::<AsyncPgConnection>::new_with_setup(
        config.database.url.as_str(),
        establish_connection,
    );
    let pool = Pool::builder()
        .max_size(config.database.pool.max_conns)
        .min_idle(Some(config.database.pool.min_conns))
        .max_lifetime(Some(*config.database.pool.max_lifetime))
        .idle_timeout(Some(*config.database.pool.idle_timeout))
        .build(manager)
        .await?;

    let db = models::DbPool::new(pool, context.clone());
    let cloudflare = CloudflareApi::new(config.cloudflare.clone());
    let cookbook = Cookbook::new_s3(&config.cookbook);

    let rest = http_server(db.clone()).await.into_make_service();
    let grpc = grpc_server(db, cloudflare, cookbook).await.into_service();
    let hybrid = hybrid_server(rest, grpc);

    axum::Server::bind(&config.database.bind_addr())
        .serve(hybrid)
        .await
        .map_err(Into::into)
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
