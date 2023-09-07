use std::sync::Arc;

use displaydoc::Display;
use rand::rngs::OsRng;
use thiserror::Error;
use tokio::sync::Mutex;

use crate::auth::claims::Claims;
use crate::auth::endpoint::Endpoint;
use crate::auth::{self, Auth};
use crate::cookbook::Cookbook;
use crate::database::{Conn, Pool};
use crate::dns::{Cloudflare, Dns};
use crate::mail::MailClient;
use crate::mqtt::Notifier;
use crate::server::Alert;

use super::Config;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to create Auth: {0}
    Auth(crate::auth::Error),
    /// Failed to build Config: {0}
    Config(super::Error),
    /// Builder is missing Alert.
    MissingAlert,
    /// Builder is missing Auth.
    MissingAuth,
    /// Builder is missing Cookbook.
    MissingCookbook,
    /// Builder is missing Config.
    MissingConfig,
    /// Builder is missing Cloudflare DNS.
    MissingDns,
    /// Builder is missing Mail.
    MissingMail,
    /// Builder is missing Notifier.
    MissingNotifier,
    /// Builder is missing Pool.
    MissingPool,
    /// Failed to create Notifier: {0}
    Notifier(crate::mqtt::notifier::Error),
    /// Failed to create database Pool: {0}
    Pool(crate::database::Error),
}

/// Service `Context` containing metadata that can be passed down to handlers.
///
/// Each field is wrapped in an Arc (or cloneable) so other structs may retain
/// their own internal reference.
#[derive(Clone)]
pub struct Context {
    pub alert: Alert,
    pub auth: Arc<Auth>,
    pub cookbook: Arc<Cookbook>,
    pub config: Arc<Config>,
    pub dns: Arc<Box<dyn Dns + Send + Sync + 'static>>,
    pub mail: Arc<MailClient>,
    pub notifier: Arc<Notifier>,
    pub pool: Pool,
    pub rng: Arc<Mutex<OsRng>>,
}

impl Context {
    pub async fn new() -> Result<Arc<Self>, Error> {
        let config = Config::new().map_err(Error::Config)?;
        Self::builder_from(config).await?.build()
    }

    pub async fn from_default_toml() -> Result<Arc<Self>, Error> {
        let config = Config::from_default_toml().map_err(Error::Config)?;
        Self::builder_from(config).await?.build()
    }

    pub async fn builder_from(config: Config) -> Result<Builder, Error> {
        let alert = Alert::default();
        let auth = Auth::new(&config.token).map_err(Error::Auth)?;
        let cookbook = Cookbook::new_s3(&config.cookbook);
        let dns = Cloudflare::new(config.cloudflare.clone());
        let mail = MailClient::new(&config, auth.cipher.clone());
        let pool = Pool::new(&config.database).await.map_err(Error::Pool)?;
        let notifier = Notifier::new(config.mqtt.options())
            .await
            .map_err(Error::Notifier)?;

        Ok(Builder::default()
            .alert(alert)
            .auth(auth)
            .cookbook(cookbook)
            .dns(dns)
            .mail(mail)
            .notifier(notifier)
            .pool(pool)
            .config(config))
    }

    #[cfg(any(test, feature = "integration-test"))]
    pub async fn with_mocked() -> Result<(Arc<Self>, crate::database::tests::TestDb), Error> {
        use crate::database::tests::TestDb;
        use crate::dns::tests::MockDns;
        use crate::tests::TestCookbook;

        let config = Config::from_default_toml().map_err(Error::Config)?;
        let db = TestDb::new(&config.database).await;

        let alert = Alert::default();
        let auth = Auth::new(&config.token).map_err(Error::Auth)?;
        let cookbook = TestCookbook::new().await.get_cookbook_api();
        let dns = MockDns::new().await;
        let mail = MailClient::new_mocked(&config, auth.cipher.clone());
        let notifier = Notifier::new(config.mqtt.options())
            .await
            .map_err(Error::Notifier)?;
        let pool = db.pool();

        Builder::default()
            .alert(alert)
            .auth(auth)
            .cookbook(cookbook)
            .dns(dns)
            .mail(mail)
            .notifier(notifier)
            .pool(pool)
            .config(config)
            .build()
            .map(|ctx| (ctx, db))
    }

    pub async fn claims<T>(
        &self,
        req: &tonic::Request<T>,
        endpoint: Endpoint,
        conn: &mut Conn<'_>,
    ) -> Result<Claims, auth::Error> {
        self.auth.claims(req, endpoint, conn).await
    }
}

/// Incrementally build a new `Context` from constituent parts.
#[derive(Default)]
pub struct Builder {
    alert: Option<Alert>,
    auth: Option<Auth>,
    cookbook: Option<Cookbook>,
    config: Option<Config>,
    dns: Option<Box<dyn Dns + Send + Sync + 'static>>,
    mail: Option<MailClient>,
    notifier: Option<Notifier>,
    pool: Option<Pool>,
    rng: Option<OsRng>,
}

impl Builder {
    pub fn build(self) -> Result<Arc<Context>, Error> {
        Ok(Arc::new(Context {
            alert: self.alert.ok_or(Error::MissingAlert)?,
            auth: self.auth.ok_or(Error::MissingAuth).map(Arc::new)?,
            cookbook: self.cookbook.ok_or(Error::MissingCookbook).map(Arc::new)?,
            config: self.config.ok_or(Error::MissingConfig).map(Arc::new)?,
            dns: self.dns.ok_or(Error::MissingDns).map(Arc::new)?,
            mail: self.mail.ok_or(Error::MissingMail).map(Arc::new)?,
            notifier: self.notifier.ok_or(Error::MissingNotifier).map(Arc::new)?,
            pool: self.pool.ok_or(Error::MissingPool)?,
            rng: Arc::new(Mutex::new(self.rng.unwrap_or_default())),
        }))
    }

    pub fn alert(mut self, alert: Alert) -> Self {
        self.alert = Some(alert);
        self
    }

    pub fn auth(mut self, auth: Auth) -> Self {
        self.auth = Some(auth);
        self
    }

    pub fn cookbook(mut self, cookbook: Cookbook) -> Self {
        self.cookbook = Some(cookbook);
        self
    }

    pub fn config(mut self, config: Config) -> Self {
        self.config = Some(config);
        self
    }

    pub fn dns<D>(mut self, dns: D) -> Self
    where
        D: Dns + Send + Sync + 'static,
    {
        self.dns = Some(Box::new(dns));
        self
    }

    pub fn mail(mut self, mail: MailClient) -> Self {
        self.mail = Some(mail);
        self
    }

    pub fn notifier(mut self, notifier: Notifier) -> Self {
        self.notifier = Some(notifier);
        self
    }

    pub fn pool(mut self, pool: Pool) -> Self {
        self.pool = Some(pool);
        self
    }

    pub fn rng(mut self, rng: OsRng) -> Self {
        self.rng = Some(rng);
        self
    }
}
