use crate::{auth::FindableById, errors::Result, models};
use sqlx::postgres::PgListener;
use uuid::Uuid;

#[derive(Debug)]
pub struct Notifier {
    db: models::DbPool,
}

impl Notifier {
    pub fn new(db: models::DbPool) -> Self {
        Self { db }
    }

    /// Returns a sender that can be used to send
    pub fn commands_sender(&self, host_id: Uuid) -> Sender<models::Command> {
        Sender::new(host_id, self.db.clone())
    }

    pub async fn commands_receiver(&self, host_id: Uuid) -> Result<Receiver<models::Command>> {
        Receiver::new(host_id, self.db.clone()).await
    }

    pub fn nodes_sender(&self, host_id: Uuid) -> Sender<models::Node> {
        Sender::new(host_id, self.db.clone())
    }

    pub fn nodes_broadcast(&self, node_id: Uuid) -> Sender<models::Node> {
        Sender::new(node_id, self.db.clone())
    }

    pub async fn nodes_receiver(&self, org_id: Uuid) -> Result<Receiver<models::Node>> {
        Receiver::new(org_id, self.db.clone()).await
    }

    /// TODO: think about who will listen for updates about hosts, and what makes sense as a
    /// channel identifier.
    pub fn hosts_sender(&self, something_id: uuid::Uuid) -> Sender<models::Host> {
        Sender::new(something_id, self.db.clone())
    }

    /// TODO: think about who will listen for updates about hosts, and what makes sense as a
    /// channel identifier.
    pub async fn hosts_receiver(&self, something_id: uuid::Uuid) -> Result<Receiver<models::Host>> {
        Receiver::new(something_id, self.db.clone()).await
    }

    /// TODO: think about who will listen for updates about organizations, and what makes sense as
    /// a channel identifier.
    pub fn orgs_sender(&self, something_id: uuid::Uuid) -> Sender<models::Org> {
        Sender::new(something_id, self.db.clone())
    }

    /// TODO: think about who will listen for updates about organizations, and what makes sense as
    /// a channel identifier.
    pub async fn orgs_receiver(&self, something_id: uuid::Uuid) -> Result<Receiver<models::Org>> {
        Receiver::new(something_id, self.db.clone()).await
    }
}

pub trait Notify: FindableById {
    fn channel(id: uuid::Uuid) -> String;

    fn broadcast_channel(_id: uuid::Uuid) -> String {
        todo!()
    }
}

pub struct Sender<T: Notify> {
    db: models::DbPool,
    channel_id: uuid::Uuid,
    _pd: std::marker::PhantomData<T>,
}

impl<T: Notify> Sender<T> {
    fn new(channel_id: uuid::Uuid, db: models::DbPool) -> Self {
        Sender {
            db,
            channel_id,
            _pd: std::marker::PhantomData,
        }
    }

    pub async fn send(&mut self, resource_id: uuid::Uuid) -> Result<()> {
        let channel = T::channel(self.channel_id);
        let mut conn = self.db.conn().await?;
        sqlx::query("SELECT pg_notify($1, $2::text)")
            .bind(&channel)
            .bind(resource_id)
            .execute(&mut conn)
            .await?;
        Ok(())
    }

    pub async fn broadcast(&mut self, resource_id: uuid::Uuid) -> Result<()> {
        let channel = T::broadcast_channel(self.channel_id);
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
    listener: PgListener,
    db: models::DbPool,
    _pd: std::marker::PhantomData<T>,
}

impl<T: Notify> Receiver<T> {
    async fn new(channel_id: uuid::Uuid, db: models::DbPool) -> Result<Self> {
        let listener = PgListener::connect_with(db.inner()).await?;
        let mut receiver = Self {
            db,
            listener,
            _pd: std::marker::PhantomData,
        };
        let channel = T::channel(channel_id);
        receiver.listener.listen(&channel).await?;
        Ok(receiver)
    }

    pub async fn recv(&mut self) -> Result<T> {
        let msg = self.listener.recv().await?;
        let resource_id = msg.payload().parse()?;
        let mut conn = self.db.conn().await?;
        T::find_by_id(resource_id, &mut conn).await
    }
}

impl Notify for models::Command {
    fn channel(id: uuid::Uuid) -> String {
        format!("commands_for_host_{id}")
    }
}

impl Notify for models::Node {
    fn channel(id: uuid::Uuid) -> String {
        format!("nodes_for_host_{id}")
    }

    fn broadcast_channel(org_id: Uuid) -> String {
        format!("node_broadcast_{org_id}")
    }
}

impl Notify for models::Host {
    fn channel(id: uuid::Uuid) -> String {
        format!("hosts_for_something_{id}")
    }
}

impl Notify for models::Org {
    fn channel(id: uuid::Uuid) -> String {
        format!("orgs_for_something_{id}")
    }
}

#[cfg(test)]
mod tests {
    use crate::{models, TestDb};

    use super::*;

    #[tokio::test]
    async fn send_receive_commands() {
        let db = TestDb::setup().await;
        let notifier = Notifier::new(db.pool.clone());
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
        let mut receiver = notifier.commands_receiver(host_id).await.unwrap();
        notifier
            .commands_sender(host_id)
            .send(command.id)
            .await
            .unwrap();
        receiver.recv().await.unwrap();
    }

    #[tokio::test]
    async fn send_receive_nodes() {
        let db = TestDb::setup().await;
        let notifier = Notifier::new(db.pool.clone());
        let host_id = db.host().await.id;
        let node = db.node().await;
        let mut receiver = notifier.nodes_receiver(host_id).await.unwrap();
        notifier.nodes_sender(host_id).send(node.id).await.unwrap();
        receiver.recv().await.unwrap();
    }

    #[tokio::test]
    async fn send_receive_hosts() {
        // TODO: this test asserts that the notifier can correctly pass messages through the
        // channel identifier by the `something_id`. We do not yet know what that id will be.
        let db = TestDb::setup().await;
        let notifier = Notifier::new(db.pool.clone());
        let something_id = uuid::Uuid::new_v4();
        let host = db.host().await;
        let mut receiver = notifier.hosts_receiver(something_id).await.unwrap();
        notifier
            .hosts_sender(something_id)
            .send(host.id)
            .await
            .unwrap();
        receiver.recv().await.unwrap();
    }

    #[tokio::test]
    async fn send_receive_orgs() {
        // TODO: this test asserts that the notifier can correctly pass messages through the
        // channel identifier by the `something_id`. We do not yet know what that id will be.
        let db = TestDb::setup().await;
        let notifier = Notifier::new(db.pool.clone());
        let something_id = uuid::Uuid::new_v4();
        let org = db.org().await;
        let mut receiver = notifier.orgs_receiver(something_id).await.unwrap();
        notifier
            .orgs_sender(something_id)
            .send(org.id)
            .await
            .unwrap();
        receiver.recv().await.unwrap();
    }
}
