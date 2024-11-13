use std::sync::Arc;

use displaydoc::Display;
use rand::rngs::OsRng;
use thiserror::Error;
use tokio::sync::Mutex;

use crate::auth::Auth;
use crate::cloudflare::{Cloudflare, Dns};
use crate::database::Pool;
use crate::email::Email;
use crate::mqtt::Notifier;
use crate::storage::Storage;
use crate::stripe::{Payment, Stripe};

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
    /// Builder is missing Storage.
    MissingStorage,
    /// Failed to create MQTT options: {0}
    Mqtt(#[from] super::mqtt::Error),
    /// Failed to create Notifier: {0}
    Notifier(crate::mqtt::notifier::Error),
    /// Failed to create database Pool: {0}
    Pool(crate::database::Error),
    /// Failed to create Stripe: {0}
    Stripe(crate::stripe::Error),
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
    pub notifier: Option<Arc<Notifier>>,
    pub pool: Pool,
    pub rng: Arc<Mutex<OsRng>>,
    pub storage: Arc<Storage>,
    pub stripe: Arc<Box<dyn Payment + Send + Sync + 'static>>,
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
        let notifier = match Notifier::new(config.mqtt.options()?, pool.clone()).await {
            Ok(notifier) => Some(notifier),
            Err(err) => {
                println!("Warning: Starting without MQTT: {:?}", Error::Notifier(err));
                None
            }
        };
        let storage = Storage::new_s3(&config.storage);
        let stripe = Stripe::new(config.stripe.clone()).map_err(Error::Stripe)?;

        Ok(Builder::default()
            .auth(auth)
            .dns(dns)
            .email(email)
            .notifier(notifier)
            .pool(pool)
            .storage(storage)
            .stripe(stripe)
            .config(config))
    }

    #[cfg(any(test, feature = "integration-test"))]
    pub async fn with_mocked() -> Result<(Arc<Self>, crate::database::tests::TestDb), Error> {
        use crate::cloudflare::tests::MockCloudflare;
        use crate::database::tests::TestDb;
        use crate::storage::tests::TestStorage;
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
        let storage = TestStorage::new().await.new_mock();
        let stripe = MockStripe::new().await;

        Builder::default()
            .auth(auth)
            .dns(dns)
            .email(email)
            .notifier(Some(notifier))
            .pool(pool)
            .rng(rng)
            .storage(storage)
            .stripe(stripe)
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
    storage: Option<Storage>,
    stripe: Option<Box<dyn Payment + Send + Sync + 'static>>,
}

impl Builder {
    pub fn build(self) -> Result<Arc<Context>, Error> {
        Ok(Arc::new(Context {
            auth: self.auth.ok_or(Error::MissingAuth).map(Arc::new)?,
            config: self.config.ok_or(Error::MissingConfig).map(Arc::new)?,
            dns: self.dns.ok_or(Error::MissingDns).map(Arc::new)?,
            email: self.email.ok_or(Error::MissingEmail).map(Arc::new)?,
            notifier: match self.notifier {
                Some(notifier) => Some(notifier),
                None => {
                    println!("WARNING: Running without mqtt");
                    None
                }
            },
            pool: self.pool.ok_or(Error::MissingPool)?,
            rng: Arc::new(Mutex::new(self.rng.unwrap_or_default())),
            storage: self.storage.ok_or(Error::MissingStorage).map(Arc::new)?,
            stripe: self.stripe.ok_or(Error::MissingStripe).map(Arc::new)?,
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
    pub fn notifier(mut self, notifier: Option<Arc<Notifier>>) -> Self {
        self.notifier = notifier;
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
    pub fn storage(mut self, storage: Storage) -> Self {
        self.storage = Some(storage);
        self
    }

    #[must_use]
    pub fn stripe<S>(mut self, stripe: S) -> Self
    where
        S: Payment + Send + Sync + 'static,
    {
        self.stripe = Some(Box::new(stripe));
        self
    }
}
