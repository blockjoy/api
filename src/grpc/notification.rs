use crate::{auth::FindableById, errors::Result, models};
use sqlx::postgres::PgListener;
use std::sync::Arc;
use tokio::sync::broadcast;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Notifier {
    inner: Arc<NotifierInner>,
    db: models::DbPool,
}

#[derive(Debug)]
struct NotifierInner {
    commands: broadcast::Receiver<models::Command>,
    nodes: broadcast::Receiver<models::Node>,
    hosts: broadcast::Receiver<models::Host>,
    orgs: broadcast::Receiver<models::Org>,
}

impl Notifier {
    pub async fn new(db: models::DbPool) -> Result<Self> {
        let (commands_sender, commands) = broadcast::channel(16);
        let mut commands_listener = DbListener::new(db.clone(), commands_sender).await?;
        tokio::spawn(async move { commands_listener.listen().await });

        let (nodes_sender, nodes) = broadcast::channel(16);
        let mut nodes_listener = DbListener::new(db.clone(), nodes_sender).await?;
        tokio::spawn(async move { nodes_listener.listen().await });

        let (hosts_sender, hosts) = broadcast::channel(16);
        let mut hosts_listener = DbListener::new(db.clone(), hosts_sender).await?;
        tokio::spawn(async move { hosts_listener.listen().await });

        let (orgs_sender, orgs) = broadcast::channel(16);
        let mut orgs_listener = DbListener::new(db.clone(), orgs_sender).await?;
        tokio::spawn(async move { orgs_listener.listen().await });

        let inner = NotifierInner {
            commands,
            nodes,
            hosts,
            orgs,
        };
        Ok(Self {
            inner: Arc::new(inner),
            db,
        })
    }

    /// Returns a sender that can be used to send
    pub fn commands_sender(&self) -> Sender<models::Command> {
        Sender::new(self.db.clone())
    }

    pub fn commands_receiver(&self, host_id: Uuid) -> Receiver<models::Command> {
        Receiver::new(host_id, self.inner.commands.resubscribe())
    }

    pub fn nodes_sender(&self) -> Sender<models::Node> {
        Sender::new(self.db.clone())
    }

    pub fn nodes_receiver(&self, host_id: Uuid) -> Receiver<models::Node> {
        Receiver::new(host_id, self.inner.nodes.resubscribe())
    }

    pub fn hosts_sender(&self) -> Sender<models::Host> {
        Sender::new(self.db.clone())
    }

    /// TODO: think about who will listen for updates about hosts, and what makes sense as a
    /// channel identifier.
    pub fn hosts_receiver(&self, something_id: uuid::Uuid) -> Receiver<models::Host> {
        Receiver::new(something_id, self.inner.hosts.resubscribe())
    }

    pub fn orgs_sender(&self) -> Sender<models::Org> {
        Sender::new(self.db.clone())
    }

    /// TODO: think about who will listen for updates about organizations, and what makes sense as
    /// a channel identifier.
    pub fn orgs_receiver(&self, something_id: uuid::Uuid) -> Receiver<models::Org> {
        Receiver::new(something_id, self.inner.orgs.resubscribe())
    }
}

/// The DbListener<T> is a singleton struct that listens for messages coming from the database.
/// When a message comes in, the re
struct DbListener<T: Notify> {
    db: models::DbPool,
    listener: PgListener,
    sender: broadcast::Sender<T>,
}

impl<T: Notify> DbListener<T> {
    async fn new(db: models::DbPool, sender: broadcast::Sender<T>) -> Result<Self> {
        let mut listener = PgListener::connect_with(db.inner()).await?;
        listener.listen(&T::channel()).await?;
        Ok(Self {
            db,
            listener,
            sender,
        })
    }

    async fn listen(&mut self) {
        while let Ok(msg) = self.listener.recv().await {
            let Ok(resource_id) = msg.payload().parse() else { continue };
            let Ok(mut conn) = self.db.conn().await else { continue };
            let Ok(command) = T::find_by_id(resource_id, &mut conn).await else { continue };
            let _ = self.sender.send(command);
        }
    }
}

pub trait Notify: FindableById {
    fn channel() -> String;

    fn in_channel(&self, channel_id: uuid::Uuid) -> bool;
}

pub struct Sender<T: Notify> {
    db: models::DbPool,
    _pd: std::marker::PhantomData<T>,
}

impl<T: Notify> Sender<T> {
    fn new(db: models::DbPool) -> Self {
        Sender {
            db,
            _pd: std::marker::PhantomData,
        }
    }

    pub async fn send(&mut self, resource_id: uuid::Uuid) -> Result<()> {
        let channel = T::channel();
        let mut conn = self.db.conn().await?;
        sqlx::query("SELECT pg_notify($1, $2::text)")
            .bind(&channel)
            .bind(resource_id)
            .execute(&mut conn)
            .await?;
        Ok(())
    }
}

pub struct Receiver<T: Notify> {
    channel_id: uuid::Uuid,
    receiver: broadcast::Receiver<T>,
}

impl<T: Clone + Notify> Receiver<T> {
    fn new(channel_id: uuid::Uuid, receiver: broadcast::Receiver<T>) -> Self {
        Self {
            channel_id,
            receiver,
        }
    }

    pub async fn recv(&mut self) -> Result<T> {
        loop {
            let msg = self.receiver.recv().await?;
            if msg.in_channel(self.channel_id) {
                return Ok(msg);
            }
        }
    }

    pub async fn recv_where(&mut self, f: impl Fn(&T) -> bool) -> Result<T> {
        loop {
            let msg = self.receiver.recv().await?;
            if f(&msg) {
                return Ok(msg);
            }
        }
    }
}

impl Notify for models::Command {
    fn channel() -> String {
        "commands_channel".to_string()
    }

    /// Commands are channeled by host.
    fn in_channel(&self, host_id: uuid::Uuid) -> bool {
        self.host_id == host_id
    }
}

impl Notify for models::Node {
    fn channel() -> String {
        "nodes_channel".to_string()
    }

    /// Commands are channeled by host.
    fn in_channel(&self, host_id: uuid::Uuid) -> bool {
        self.host_id == host_id
    }
}

impl Notify for models::Host {
    fn channel() -> String {
        "hosts_channel".to_string()
    }

    /// TODO: think about how we will use this and how we will channel it.
    fn in_channel(&self, _something_id: uuid::Uuid) -> bool {
        true
    }
}

impl Notify for models::Org {
    fn channel() -> String {
        "orgs_channel".to_string()
    }

    /// TODO: think about how we will use this and how we will channel it.
    fn in_channel(&self, _something_id: uuid::Uuid) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use crate::{models, TestDb};

    use super::*;

    #[tokio::test]
    async fn send_receive_commands() {
        let db = TestDb::setup().await;
        let notifier = Notifier::new(db.pool.clone()).await.unwrap();
        let host_id = db.host().await.id;
        let command = models::Command::create(
            host_id,
            models::CommandRequest {
                cmd: models::HostCmd::CreateNode,
                sub_cmd: None,
                resource_id: host_id,
            },
            &mut db.pool.conn().await.unwrap(),
        )
        .await
        .unwrap();
        let mut receiver = notifier.commands_receiver(host_id);
        notifier.commands_sender().send(command.id).await.unwrap();
        receiver.recv().await.unwrap();
    }

    #[tokio::test]
    async fn send_receive_nodes() {
        let db = TestDb::setup().await;
        let notifier = Notifier::new(db.pool.clone()).await.unwrap();
        let host_id = db.host().await.id;
        let node = db.node().await;
        let mut receiver = notifier.nodes_receiver(host_id);
        notifier.nodes_sender().send(node.id).await.unwrap();
        receiver.recv().await.unwrap();
    }

    #[tokio::test]
    async fn send_receive_hosts() {
        // TODO: this test asserts that the notifier can correctly pass messages through the
        // channel identifier by the `something_id`. We do not yet know what that id will be.
        let db = TestDb::setup().await;
        let notifier = Notifier::new(db.pool.clone()).await.unwrap();
        let something_id = uuid::Uuid::new_v4();
        let host = db.host().await;
        let mut receiver = notifier.hosts_receiver(something_id);
        notifier.hosts_sender().send(host.id).await.unwrap();
        receiver.recv().await.unwrap();
    }

    #[tokio::test]
    async fn send_receive_orgs() {
        // TODO: this test asserts that the notifier can correctly pass messages through the
        // channel identifier by the `something_id`. We do not yet know what that id will be.
        let db = TestDb::setup().await;
        let notifier = Notifier::new(db.pool.clone()).await.unwrap();
        let something_id = uuid::Uuid::new_v4();
        let org = db.org().await;
        let mut receiver = notifier.orgs_receiver(something_id);
        notifier.orgs_sender().send(org.id).await.unwrap();
        receiver.recv().await.unwrap();
    }
}
