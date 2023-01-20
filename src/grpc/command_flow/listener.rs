use crate::errors::{ApiError, Result};
use crate::grpc::helpers::required;
use crate::grpc::{blockjoy, convert, notification};
use crate::models;
use anyhow::anyhow;
use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;

type Message = Result<blockjoy::Command, tonic::Status>;

/// Sets up a set of channels that communicate with each other to handle the messages we need to
/// handle. These are:
/// 1. The updates sent by the host that the user is wanting to listen to. These need to be sent to
///    filtered for relevance (i.e. they need to be related to the current host_id), and then sent
///    to the user.
/// 2. The commands to the host that are sent by the user. Those need to be written to the
///    database.
/// 3. Since we are listening to updates sent by the host, our host-listening-task will never
///    finish. Therefore we need to give a shutdown message to make our host listener stop whenever
///    our user listener stops listening.
pub async fn channels(
    host_id: uuid::Uuid,
    notifier: notification::Notifier,
    db: models::DbPool,
) -> Result<(mpsc::Receiver<Message>, DbListener, BvListener)> {
    let (stop_tx, stop_rx) = mpsc::channel(1);
    let (tx, rx) = mpsc::channel(buffer_size());

    let db_listener = DbListener {
        host_id,
        sender: tx.clone(),
        stop: stop_rx,
        messages: notifier.commands_receiver(host_id).await?,
        db: db.clone(),
    };
    let bv_listener = BvListener {
        host_id,
        sender: tx,
        stop: stop_tx,
        db,
    };
    Ok((rx, db_listener, bv_listener))
}

fn buffer_size() -> usize {
    std::env::var("BIDI_BUFFER_SIZE")
        .ok()
        .and_then(|bs| bs.parse().ok())
        .unwrap_or(128)
}

/// This struct listens to the messages being sent by .
pub struct DbListener {
    /// The id of the currently considered host.
    host_id: uuid::Uuid,
    /// This is the channel we can use to send messages to the user.
    sender: mpsc::Sender<Message>,
    /// The messages that are being broadcast by the system.
    messages: notification::Receiver<models::Command>,
    /// When this channel yields a message we can stop listening.
    stop: mpsc::Receiver<()>,
    /// A reference to a database pool.
    db: models::DbPool,
}

impl DbListener {
    /// Starts the DbListener by listening for messages from the host channel, and from the stop
    /// channel. When we receive a message from the host channel, we offload to the
    /// `process_notification` function.
    pub async fn recv(mut self) -> Result<(), tonic::Status> {
        tracing::info!("Starting handling channel notifications");
        loop {
            tokio::select! {
                message = self.messages.recv() => {
                    tracing::info!("Received notification");
                    match message {
                        Ok(cmd) => self.process_notification(cmd).await?,
                        Err(e) => {
                            tracing::error!("Channel returned error: {e:?}");
                            break;
                        }
                    }
                },
                // When we receive a stop message, we break the loop
                _ = self.stop.recv() => break,
            }
        }
        // Connection broke
        let mut tx = self.db.begin().await?;
        models::Host::toggle_online(self.host_id, false, &mut tx).await?;
        tx.commit().await?;
        Ok(())
    }

    /// In this function we decide what to do with the provided notification and then do it. This
    /// means that we get the relevant command from the database, then filter it to decide if it
    /// should be sent to the user, and if so, we perform the action
    async fn process_notification(&self, command: models::Command) -> Result<()> {
        tracing::info!("Testing for command with ID {}", command.id);

        let msg = convert::db_command_to_grpc_command(command, &self.db).await?;
        match self.sender.send(Ok(msg)).await {
            Err(e) => Err(ApiError::UnexpectedError(anyhow!("Sender error: {e}"))),
            _ => {
                tracing::info!("Sent channel notification");
                Ok(())
            } // just return unit type if all went well
        }
    }
}

/// This struct listens to the messages coming from blockvisor. For each message that comes in we
/// write the result to the database, and when the messages are done, we have to signal the
/// `BvListener` to also finish by using the `stop` channel.
pub struct BvListener {
    /// The host we are sending messages about.
    host_id: uuid::Uuid,
    /// This is the channel we can use to send messages to the user.
    sender: Sender<Message>,
    /// We can use this channel to inform the host listener that it should stop listening for
    /// messages, since it will never stop on its own.
    stop: mpsc::Sender<()>,
    /// A database pool.
    db: models::DbPool,
}

impl BvListener {
    /// Start receiving messages from the `messsages` channel. It is specified as an argument to
    /// the recv function rather than as a field of the `BvListener` struct because the
    /// `tonic::Streaming` type is not `Sync`, meaning we cannot hold a reference to it across
    /// await points, meaning we would not be able to use `&self` anywhere.
    pub async fn recv(self, mut messages: tonic::Streaming<blockjoy::InfoUpdate>) -> Result<()> {
        tracing::debug!("Started waiting for InfoUpdates");
        while let Some(Ok(update)) = messages.next().await {
            self.process_info_update(update).await?;
        }

        tracing::debug!("Stopped waiting for InfoUpdates");
        // Since we are done, we should instruct the other task to also stop.
        self.stop
            .send(())
            .await
            .map_err(|_| tonic::Status::internal("Channel error"))?;

        // Connection broke or closed
        let mut tx = self.db.begin().await?;
        models::Host::toggle_online(self.host_id, false, &mut tx).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn process_info_update(&self, update: blockjoy::InfoUpdate) -> Result<()> {
        use blockjoy::info_update::Info;

        let mut tx = self.db.begin().await?;
        match update.info.ok_or_else(required("update.info"))? {
            Info::Command(cmd_info) => {
                let res = Self::update_info::<models::Command, _>(cmd_info, &mut tx).await;
                self.handle_err(res).await?;
            }
            Info::Host(host_info) => {
                let res = Self::update_info::<models::Host, _>(host_info, &mut tx).await;
                self.handle_err(res).await?;
            }
            Info::Node(node_info) => {
                let res = Self::update_info::<models::Node, _>(node_info, &mut tx).await;
                self.handle_err(res).await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }

    /// Actually perform info update on an identified resource
    async fn update_info<R, T>(info: T, tx: &mut models::DbTrx<'_>) -> Result<R>
    where
        R: models::UpdateInfo<T, R>,
    {
        R::update_info(info, tx).await
    }

    /// This function is used by `process_info_update` to send messages about failures to the user.
    async fn handle_err<T>(&self, res: Result<T>) -> Result<()> {
        // If we had an `Ok`, there is no message about failure to send, so we break off.
        let err = match res {
            Ok(_) => return Ok(()),
            Err(e) => e,
        };
        // Try to send the message to the user and if that fails, our handlig function fails as well.
        match self.sender.send(Err(err.into())).await {
            Ok(_) => Ok(()),
            Err(e) => Err(ApiError::UnexpectedError(anyhow!("Sender error: {e}"))),
        }
    }
}
