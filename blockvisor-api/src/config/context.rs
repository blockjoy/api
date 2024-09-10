use std::sync::Arc;

use displaydoc::Display;
use rand::rngs::OsRng;
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};

use crate::auth::Auth;
use crate::cloudflare::{Cloudflare, Dns};
use crate::database::Pool;
use crate::email::Email;
use crate::mqtt::Notifier;
use crate::store::{Store, Vault};
use crate::stripe::{Stripe, Subscription};

use super::Config;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to build Config: {0}
    Config(super::Error),
    /// Failed to create Cloudflare: {0}
    Cloudflare(crate::cloudflare::Error),
    /// Failed to create Email: {0}
    Email(crate::email::Error),
    /// Builder is missing Auth.
    MissingAuth,
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
    /// Builder is missing Stripe.
    MissingStripe,
    /// Builder is missing Store.
    MissingStore,
    /// Builder is missing Vault.
    MissingVault,
    /// Failed to create MQTT options: {0}
    Mqtt(#[from] super::mqtt::Error),
    /// Failed to create Notifier: {0}
    Notifier(crate::mqtt::notifier::Error),
    /// Failed to create database Pool: {0}
    Pool(crate::database::Error),
    /// Failed to create Stripe: {0}
    Stripe(crate::stripe::Error),
    /// Failed to create Vault: {0}
    Vault(crate::store::vault::Error),
}

/// Service `Context` containing metadata that can be passed down to handlers.
///
/// Each field is wrapped in an Arc (or cloneable) so other structs may retain
/// their own internal reference.
#[derive(Clone)]
pub struct Context {
    pub auth: Arc<Auth>,
    pub config: Arc<Config>,
    pub dns: Arc<Box<dyn Dns + Send + Sync + 'static>>,
    pub email: Arc<Email>,
    pub notifier: Arc<Notifier>,
    pub pool: Pool,
    pub rng: Arc<Mutex<OsRng>>,
    pub store: Arc<Store>,
    pub stripe: Arc<Box<dyn Subscription + Send + Sync + 'static>>,
    pub vault: Arc<RwLock<Vault>>,
}

impl Context {
    pub async fn new() -> Result<Arc<Self>, Error> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let config = Config::new().map_err(Error::Config)?;
        Self::builder_from(config).await?.build()
    }

    pub async fn from_default_toml() -> Result<Arc<Self>, Error> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let config = Config::from_default_toml().map_err(Error::Config)?;
        Self::builder_from(config).await?.build()
    }

    pub async fn builder_from(config: Config) -> Result<Builder, Error> {
        let auth = Auth::new(&config.token);
        let dns = Cloudflare::new(config.cloudflare.clone()).map_err(Error::Cloudflare)?;
        let email = Email::new(&config, auth.cipher.clone()).map_err(Error::Email)?;
        let pool = Pool::new(&config.database).await.map_err(Error::Pool)?;
        let notifier = Notifier::new(config.mqtt.options()?, pool.clone())
            .await
            .map_err(Error::Notifier)?;
        let store = Store::new_s3(&config.store);
        let stripe = Stripe::new(config.stripe.clone()).map_err(Error::Stripe)?;
        let vault = Vault::new(config.vault.clone())
            .await
            .map_err(Error::Vault)?;

        Ok(Builder::default()
            .auth(auth)
            .dns(dns)
            .email(email)
            .notifier(notifier)
            .pool(pool)
            .store(store)
            .stripe(stripe)
            .vault(vault)
            .config(config))
    }

    #[cfg(any(test, feature = "integration-test"))]
    pub async fn with_mocked() -> Result<(Arc<Self>, crate::database::tests::TestDb), Error> {
        use crate::cloudflare::tests::MockCloudflare;
        use crate::database::tests::TestDb;
        use crate::store::tests::TestStore;
        use crate::stripe::tests::MockStripe;

        let _ = rustls::crypto::ring::default_provider().install_default();
        let config = Config::from_default_toml().map_err(Error::Config)?;
        let mut rng = OsRng;
        let db = TestDb::new(&config.database, &mut rng).await;

        let auth = Auth::new(&config.token);
        let dns = MockCloudflare::new(&mut rng).await;
        let email = Email::new_mocked(&config, auth.cipher.clone()).map_err(Error::Email)?;
        let pool = db.pool();
        let notifier = Notifier::new(config.mqtt.options()?, pool.clone())
            .await
            .map_err(Error::Notifier)?;
        let store = TestStore::new().await.mock_store();
        let stripe = MockStripe::new().await;
        let vault = Vault::new(config.vault.clone())
            .await
            .map_err(Error::Vault)?;

        Builder::default()
            .auth(auth)
            .dns(dns)
            .email(email)
            .notifier(notifier)
            .pool(pool)
            .rng(rng)
            .store(store)
            .stripe(stripe)
            .vault(vault)
            .config(config)
            .build()
            .map(|ctx| (ctx, db))
    }
}

/// Incrementally build a new `Context` from constituent parts.
#[derive(Default)]
pub struct Builder {
    auth: Option<Auth>,
    config: Option<Config>,
    dns: Option<Box<dyn Dns + Send + Sync + 'static>>,
    email: Option<Email>,
    notifier: Option<Arc<Notifier>>,
    pool: Option<Pool>,
    rng: Option<OsRng>,
    store: Option<Store>,
    stripe: Option<Box<dyn Subscription + Send + Sync + 'static>>,
    vault: Option<Arc<RwLock<Vault>>>,
}

impl Builder {
    pub fn build(self) -> Result<Arc<Context>, Error> {
        Ok(Arc::new(Context {
            auth: self.auth.ok_or(Error::MissingAuth).map(Arc::new)?,
            config: self.config.ok_or(Error::MissingConfig).map(Arc::new)?,
            dns: self.dns.ok_or(Error::MissingDns).map(Arc::new)?,
            email: self.email.ok_or(Error::MissingEmail).map(Arc::new)?,
            notifier: self.notifier.ok_or(Error::MissingNotifier)?,
            pool: self.pool.ok_or(Error::MissingPool)?,
            rng: Arc::new(Mutex::new(self.rng.unwrap_or_default())),
            store: self.store.ok_or(Error::MissingStore).map(Arc::new)?,
            stripe: self.stripe.ok_or(Error::MissingStripe).map(Arc::new)?,
            vault: self.vault.ok_or(Error::MissingVault)?,
        }))
    }

    #[must_use]
    pub fn auth(mut self, auth: Auth) -> Self {
        self.auth = Some(auth);
        self
    }

    #[must_use]
    pub fn config(mut self, config: Config) -> Self {
        self.config = Some(config);
        self
    }

    #[must_use]
    pub fn dns<D>(mut self, dns: D) -> Self
    where
        D: Dns + Send + Sync + 'static,
    {
        self.dns = Some(Box::new(dns));
        self
    }

    #[must_use]
    pub fn email(mut self, email: Email) -> Self {
        self.email = Some(email);
        self
    }

    #[must_use]
    pub fn notifier(mut self, notifier: Arc<Notifier>) -> Self {
        self.notifier = Some(notifier);
        self
    }

    #[must_use]
    pub fn pool(mut self, pool: Pool) -> Self {
        self.pool = Some(pool);
        self
    }

    #[must_use]
    pub const fn rng(mut self, rng: OsRng) -> Self {
        self.rng = Some(rng);
        self
    }

    #[must_use]
    pub fn store(mut self, store: Store) -> Self {
        self.store = Some(store);
        self
    }

    #[must_use]
    pub fn stripe<S>(mut self, stripe: S) -> Self
    where
        S: Subscription + Send + Sync + 'static,
    {
        self.stripe = Some(Box::new(stripe));
        self
    }

    #[must_use]
    pub fn vault(mut self, vault: Arc<RwLock<Vault>>) -> Self {
        self.vault = Some(vault);
        self
    }
}
