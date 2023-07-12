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
    Shutdown(Arc<Shutdown>),
}

pub async fn start(context: Arc<Context>) -> Result<(), Error> {
    let http = http::router(context.clone()).into_make_service();
    let grpc = grpc::server(context.clone()).await.into_service();
    let both = hybrid(http, grpc);

    let server = axum::Server::bind(&context.config.database.bind_addr()).serve(both);
    let mut shutdown_rx = context.alert.shutdown_rx();

    select! {
        result = server.fuse() => result.map_err(Error::Server),
        reason = shutdown_rx.recv().fuse() => Err(Error::Shutdown(reason.expect("shutdown_tx")))
    }
}

pub struct Alert {
    shutdown_tx: Sender<Arc<Shutdown>>,
}

impl Alert {
    pub fn new(capacity: usize) -> Self {
        let (shutdown_tx, _) = broadcast::channel(capacity);

        Alert { shutdown_tx }
    }

    pub fn shutdown_rx(&self) -> Receiver<Arc<Shutdown>> {
        self.shutdown_tx.subscribe()
    }

    pub fn shutdown(&self, reason: Shutdown) {
        if let Err(err) = self.shutdown_tx.send(Arc::new(reason)) {
            error!("Failed to send shutdown signal: {err}");
        }
    }
}

impl Default for Alert {
    fn default() -> Self {
        Alert::new(1)
    }
}

#[derive(Debug, Display)]
pub enum Shutdown {
    /// Shutdown Error: {0}
    Error(Box<dyn std::error::Error + Send + Sync + 'static>),
    /// Shutdown reason: {0}
    Reason(String),
}
