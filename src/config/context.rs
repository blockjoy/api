use std::sync::Arc;

use displaydoc::Display;
use rand::rngs::OsRng;
use thiserror::Error;
use tokio::sync::Mutex;

use crate::auth::Auth;
use crate::cookbook::Cookbook;
use crate::database::Pool;
use crate::dns::{Cloudflare, Dns};
use crate::email::Email;
use crate::mqtt::Notifier;
use crate::server::Alert;

use super::Config;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to build Config: {0}
    Config(super::Error),
    /// Failed to create Email: {0}
    Email(crate::email::Error),
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
    /// Builder is missing Email.
    MissingEmail,
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
    pub email: Arc<Email>,
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
        let auth = Auth::new(&config.token);
        let cookbook = Cookbook::new_s3(&config.cookbook);
        let dns = Cloudflare::new(config.cloudflare.clone());
        let email = Email::new(&config, auth.cipher.clone()).map_err(Error::Email)?;
        let pool = Pool::new(&config.database).await.map_err(Error::Pool)?;
        let notifier = Notifier::new(config.mqtt.options())
            .await
            .map_err(Error::Notifier)?;

        Ok(Builder::default()
            .alert(alert)
            .auth(auth)
            .cookbook(cookbook)
            .dns(dns)
            .email(email)
            .notifier(notifier)
            .pool(pool)
            .config(config))
    }

    #[cfg(any(test, feature = "integration-test"))]
    pub async fn with_mocked() -> Result<(Arc<Self>, crate::database::tests::TestDb), Error> {
        use crate::cookbook::tests::TestCookbook;
        use crate::database::tests::TestDb;
        use crate::dns::tests::MockDns;

        let config = Config::from_default_toml().map_err(Error::Config)?;
        let mut rng = OsRng;
        let db = TestDb::new(&config.database, &mut rng).await;

        let alert = Alert::default();
        let auth = Auth::new(&config.token);
        let cookbook = TestCookbook::new().await.get_cookbook_api();
        let dns = MockDns::new().await;
        let email = Email::new_mocked(&config, auth.cipher.clone()).map_err(Error::Email)?;
        let notifier = Notifier::new(config.mqtt.options())
            .await
            .map_err(Error::Notifier)?;
        let pool = db.pool();

        Builder::default()
            .alert(alert)
            .auth(auth)
            .cookbook(cookbook)
            .dns(dns)
            .email(email)
            .notifier(notifier)
            .pool(pool)
            .rng(rng)
            .config(config)
            .build()
            .map(|ctx| (ctx, db))
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
    email: Option<Email>,
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
            email: self.email.ok_or(Error::MissingEmail).map(Arc::new)?,
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

    pub fn email(mut self, email: Email) -> Self {
        self.email = Some(email);
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
