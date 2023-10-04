use std::sync::Arc;

use displaydoc::Display;
use futures::select;
use futures_util::FutureExt;
use thiserror::Error;
use tokio::sync::broadcast::{self, Receiver, Sender};
use tracing::error;

use crate::config::Context;
use crate::hybrid_server::hybrid;
use crate::{grpc, http};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Server error: {0}
    Server(hyper::Error),
    /// Stopping server because of: {0}
    Shutdown(Shutdown),
}

pub async fn start(context: Arc<Context>) -> Result<(), Error> {
    let http = http::router(&context).into_make_service();
    let grpc = grpc::server(&context).into_service();
    let both = hybrid(http, grpc);

    let server = axum::Server::bind(&context.config.database.bind_addr()).serve(both);
    let mut shutdown_rx = context.alert.shutdown_rx();

    select! {
        result = server.fuse() => result.map_err(Error::Server),
        reason = shutdown_rx.recv().fuse() => Err(Error::Shutdown(reason.expect("shutdown_tx")))
    }
}

#[derive(Clone)]
pub struct Alert {
    shutdown_tx: Sender<Shutdown>,
}

impl Alert {
    pub fn new(capacity: usize) -> Self {
        let (shutdown_tx, _) = broadcast::channel(capacity);

        Alert { shutdown_tx }
    }

    pub fn shutdown_rx(&self) -> Receiver<Shutdown> {
        self.shutdown_tx.subscribe()
    }

    pub fn shutdown<S: Into<Shutdown>>(&self, reason: S) {
        if let Err(err) = self.shutdown_tx.send(reason.into()) {
            error!("Failed to send shutdown signal: {err}");
        }
    }
}

impl Default for Alert {
    fn default() -> Self {
        Alert::new(1)
    }
}

#[derive(Clone, Debug, Display)]
pub enum Shutdown {
    /// Shutdown Error: {0}
    Error(Arc<Box<dyn std::error::Error + Send + Sync + 'static>>),
    /// Shutdown reason: {0}
    Reason(String),
}

impl<E> From<E> for Shutdown
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn from(err: E) -> Self {
        Shutdown::Error(Arc::new(Box::new(err)))
    }
}
