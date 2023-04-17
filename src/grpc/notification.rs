use anyhow::anyhow;
use diesel_async::AsyncPgConnection;

use super::api::{self, host_message, node_message, org_message};
use crate::{auth::key_provider::KeyProvider, models, Result};

/// Presents the following senders:
///
///
/// |---------------|----------------------------------------------|
/// | public api    | topics                                       |
/// |---------------|----------------------------------------------|
/// | organizations | /orgs/<org_id>                               |
/// |---------------|----------------------------------------------|
/// | hosts         | /hosts/<host_id>                             |
/// |---------------|----------------------------------------------|
/// | nodes         | /orgs/<org_id>/nodes                         |
/// |               | /hosts/<host_id>/nodes                       |
/// |               | /nodes/<node_id>                             |
/// |---------------|----------------------------------------------|
/// | commands      | /hosts/<host_id>/nodes/<node_id>/commands    |
/// |               | /hosts/<host_id>/commands                    |
/// |               | /nodes/<node_id>/commands                    |
/// |---------------|----------------------------------------------|
#[derive(Debug, Clone)]
pub struct Notifier {
    client: rumqttc::AsyncClient,
}

impl Notifier {
    pub async fn new() -> Result<Self> {
        let options = Self::get_mqtt_options()?;
        let (client, mut event_loop) = rumqttc::AsyncClient::new(options, 10);
        client
            .subscribe("/bv/hosts/#", rumqttc::QoS::AtLeastOnce)
            .await
            .unwrap();
        tokio::spawn(async move {
            loop {
                match event_loop.poll().await {
                    Ok(event) => println!("Successful polling event: {event:?}"),
                    Err(e) => {
                        tracing::warn!("MQTT failure, ignoring and continuing to poll: {e}");
                        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                    }
                }
            }
        });

        Ok(Self { client })
    }

    pub fn orgs_sender(&self) -> MqttClient<api::OrgMessage> {
        MqttClient::new(self.client.clone())
    }

    pub fn hosts_sender(&self) -> MqttClient<api::HostMessage> {
        MqttClient::new(self.client.clone())
    }

    pub fn nodes_sender(&self) -> MqttClient<api::NodeMessage> {
        MqttClient::new(self.client.clone())
    }

    pub fn commands_sender(&self) -> MqttClient<api::Command> {
        MqttClient::new(self.client.clone())
    }

    fn get_mqtt_options() -> Result<rumqttc::MqttOptions> {
        // let client_id = KeyProvider::get_var("MQTT_CLIENT_ID")?.value;
        let client_id = format!("blockvisor-api-{}", uuid::Uuid::new_v4());
        let host = KeyProvider::get_var("MQTT_SERVER_ADDRESS")?.value;
        let port = KeyProvider::get_var("MQTT_SERVER_PORT")?
            .value
            .parse()
            .map_err(|_| anyhow!("Could not parse MQTT_SERVER_PORT as u16"))?;
        let username = KeyProvider::get_var("MQTT_USERNAME")?.value;
        let password = KeyProvider::get_var("MQTT_PASSWORD")?.value;
        let mut options = rumqttc::MqttOptions::new(client_id, host, port);
        options.set_credentials(username, password);
        Ok(options)
    }
}

/// The DbListener<T> is a singleton struct that listens for messages coming from the database.
/// When a message comes in, the re
pub struct MqttClient<T> {
    client: rumqttc::AsyncClient,
    _pd: std::marker::PhantomData<T>,
}

impl<T: Notify + prost::Message> MqttClient<T> {
    fn new(client: rumqttc::AsyncClient) -> Self {
        Self {
            client,
            _pd: std::marker::PhantomData,
        }
    }

    pub async fn send(&mut self, msg: &T) -> Result<()>
    where
        T: std::fmt::Debug,
    {
        const RETAIN: bool = false;
        const QOS: rumqttc::QoS = rumqttc::QoS::ExactlyOnce;
        let payload = msg.encode_to_vec();

        tracing::info!("Sending {msg:?} over channels: {:?}", msg.channels());
        for channel in msg.channels() {
            self.client
                .publish(&channel, QOS, RETAIN, payload.clone())
                .await?;
            tracing::info!("Sent {msg:?} over channel {channel}");
        }
        Ok(())
    }
}

pub trait Notify {
    fn channels(&self) -> Vec<String>;
}

// There is a couple of unwrap here below. This is because our messages have fields that are of the
// type Option which are always Some. We ensure to always populate those fields, but it is not
// possible to make a nested object required in gRPC :(.

impl Notify for api::OrgMessage {
    fn channels(&self) -> Vec<String> {
        let org_id = self.org_id().unwrap();

        vec![format!("/orgs/{org_id}")]
    }
}

impl Notify for api::HostMessage {
    fn channels(&self) -> Vec<String> {
        let host_id = self.host_id().unwrap();
        vec![format!("/hosts/{host_id}")]
    }
}

impl Notify for api::NodeMessage {
    fn channels(&self) -> Vec<String> {
        let org_id = self.org_id().unwrap();
        let host_id = self.host_id().unwrap();
        let node_id = self.node_id().unwrap();

        vec![
            format!("/orgs/{org_id}/nodes"),
            format!("/hosts/{host_id}/nodes"),
            format!("/nodes/{node_id}"),
        ]
    }
}

impl Notify for api::Command {
    fn channels(&self) -> Vec<String> {
        // There is always a host id for a given command.
        let host_id = self.host_id().unwrap();
        // But there is not always a node id.
        let node_id = self.node_id();

        let mut res = vec![format!("/hosts/{host_id}/commands")];
        res.extend(node_id.map(|node_id| format!("/hosts/{host_id}/nodes/{node_id}/commands")));
        res.extend(node_id.map(|node_id| format!("/nodes/{node_id}/commands")));
        res
    }
}

impl api::OrgMessage {
    fn org_id(&self) -> Option<uuid::Uuid> {
        use org_message::Message::*;
        match self.message.as_ref()? {
            Created(api::OrgCreated { org, .. }) => org.as_ref()?.id.parse().ok(),
            Updated(api::OrgUpdated { org, .. }) => org.as_ref()?.id.parse().ok(),
            Deleted(api::OrgDeleted { org_id, .. }) => org_id.parse().ok(),
        }
    }

    pub async fn created(
        model: models::Org,
        user: models::User,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<Self> {
        Ok(Self {
            message: Some(org_message::Message::Created(api::OrgCreated {
                // Over MQTT, there is no current user so we pass None as a second argument.
                org: Some(api::Org::from_model(model, conn).await?),
                created_by: user.id.to_string(),
                created_by_name: format!("{} {}", user.first_name, user.last_name),
                created_by_email: user.email,
            })),
        })
    }

    pub async fn updated(
        model: models::Org,
        user: models::User,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<Self> {
        Ok(Self {
            message: Some(org_message::Message::Updated(api::OrgUpdated {
                // Over MQTT, there is no current user so we pass None as a second argument.
                org: Some(api::Org::from_model(model, conn).await?),
                updated_by: user.id.to_string(),
                updated_by_name: format!("{} {}", user.first_name, user.last_name),
                updated_by_email: user.email,
            })),
        })
    }

    pub fn deleted(model: models::Org, user: models::User) -> Self {
        Self {
            message: Some(org_message::Message::Deleted(api::OrgDeleted {
                org_id: model.id.to_string(),
                deleted_by: user.id.to_string(),
                deleted_by_name: format!("{} {}", user.first_name, user.last_name),
                deleted_by_email: user.email,
            })),
        }
    }
}

impl api::HostMessage {
    fn host_id(&self) -> Option<uuid::Uuid> {
        use host_message::Message::*;
        match self.message.as_ref()? {
            Created(api::HostCreated { host, .. }) => host.as_ref()?.id.parse().ok(),
            Updated(api::HostUpdated { host, .. }) => host.as_ref()?.id.parse().ok(),
            Deleted(api::HostDeleted { host_id, .. }) => host_id.parse().ok(),
        }
    }

    pub async fn created(model: models::Host, user: models::User) -> crate::Result<Self> {
        Ok(Self {
            message: Some(host_message::Message::Created(api::HostCreated {
                host: Some(api::Host::from_model(model).await?),
                created_by: user.id.to_string(),
                created_by_name: format!("{} {}", user.first_name, user.last_name),
                created_by_email: user.email,
            })),
        })
    }

    pub async fn updated(model: models::Host, user: models::User) -> crate::Result<Self> {
        Ok(Self {
            message: Some(host_message::Message::Updated(api::HostUpdated {
                host: Some(api::Host::from_model(model).await?),
                updated_by: user.id.to_string(),
                updated_by_name: format!("{} {}", user.first_name, user.last_name),
                updated_by_email: user.email,
            })),
        })
    }

    pub fn deleted(model: models::Host, user: models::User) -> Self {
        Self {
            message: Some(host_message::Message::Deleted(api::HostDeleted {
                host_id: model.id.to_string(),
                deleted_by: user.id.to_string(),
                deleted_by_name: format!("{} {}", user.first_name, user.last_name),
                deleted_by_email: user.email,
            })),
        }
    }
}

impl api::NodeMessage {
    fn node_id(&self) -> Option<uuid::Uuid> {
        use node_message::Message::*;
        match self.message.as_ref()? {
            Created(api::NodeCreated { node, .. }) => node.as_ref()?.id.parse().ok(),
            Updated(api::NodeUpdated { node, .. }) => node.as_ref()?.id.parse().ok(),
            Deleted(api::NodeDeleted { node_id, .. }) => node_id.parse().ok(),
        }
    }

    fn host_id(&self) -> Option<uuid::Uuid> {
        use node_message::Message::*;
        match self.message.as_ref()? {
            Created(api::NodeCreated { node, .. }) => node.as_ref()?.host_id.parse().ok(),
            Updated(api::NodeUpdated { node, .. }) => node.as_ref()?.host_id.parse().ok(),
            Deleted(api::NodeDeleted { host_id, .. }) => host_id.parse().ok(),
        }
    }

    fn org_id(&self) -> Option<uuid::Uuid> {
        use node_message::Message::*;
        match self.message.as_ref()? {
            Created(api::NodeCreated { node, .. }) => node.as_ref()?.org_id.parse().ok(),
            Updated(api::NodeUpdated { node, .. }) => node.as_ref()?.org_id.parse().ok(),
            Deleted(api::NodeDeleted { org_id, .. }) => org_id.parse().ok(),
        }
    }

    pub async fn created(
        model: models::Node,
        user: models::User,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<Self> {
        Ok(Self {
            message: Some(node_message::Message::Created(api::NodeCreated {
                node: Some(api::Node::from_model(model, conn).await?),
                created_by: user.id.to_string(),
                created_by_name: format!("{} {}", user.first_name, user.last_name),
                created_by_email: user.email,
            })),
        })
    }

    pub async fn updated(
        model: models::Node,
        user: Option<models::User>,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<Self> {
        Ok(Self {
            message: Some(node_message::Message::Updated(api::NodeUpdated {
                node: Some(api::Node::from_model(model, conn).await?),
                updated_by: user.as_ref().map(|u| u.id.to_string()).unwrap_or_default(),
                updated_by_name: user
                    .as_ref()
                    .map(|u| u.name())
                    .unwrap_or_else(|| "BlockJoy System".to_string()),
                updated_by_email: user.map(|u| u.email).unwrap_or_default(),
            })),
        })
    }

    pub fn deleted(model: models::Node, user: models::User) -> Self {
        Self {
            message: Some(node_message::Message::Deleted(api::NodeDeleted {
                node_id: model.id.to_string(),
                host_id: model.host_id.to_string(),
                org_id: model.org_id.to_string(),
                deleted_by: user.id.to_string(),
                deleted_by_name: format!("{} {}", user.first_name, user.last_name),
                deleted_by_email: user.email,
            })),
        }
    }
}

impl api::Command {
    fn host_id(&self) -> Option<&str> {
        match self.command.as_ref()? {
            api::command::Command::Node(cmd) => Some(&cmd.host_id),
            api::command::Command::Host(cmd) => Some(&cmd.host_id),
        }
    }

    fn node_id(&self) -> Option<&str> {
        match self.command.as_ref()? {
            api::command::Command::Node(cmd) => Some(&cmd.node_id),
            api::command::Command::Host(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_hosts_sender() {
        let db = crate::TestDb::setup().await;
        let host = db.host().await;
        let user = db.admin_user().await;

        let msg = api::HostMessage::created(host.clone(), user.clone())
            .await
            .unwrap();
        let notifier = Notifier::new().await.unwrap();
        notifier.hosts_sender().send(&msg).await.unwrap();

        let msg = api::HostMessage::updated(host.clone(), user.clone())
            .await
            .unwrap();
        let notifier = Notifier::new().await.unwrap();
        notifier.hosts_sender().send(&msg).await.unwrap();

        let msg = api::HostMessage::deleted(host, user);
        let notifier = Notifier::new().await.unwrap();
        notifier.hosts_sender().send(&msg).await.unwrap();
    }

    #[tokio::test]
    async fn test_nodes_sender() {
        let db = crate::TestDb::setup().await;
        let mut conn = db.pool.conn().await.unwrap();
        let node = db.node().await;
        let user = db.admin_user().await;

        let msg = api::NodeMessage::created(node.clone(), user.clone(), &mut conn)
            .await
            .unwrap();
        let notifier = Notifier::new().await.unwrap();
        notifier.nodes_sender().send(&msg).await.unwrap();

        let msg = api::NodeMessage::updated(node.clone(), Some(user.clone()), &mut conn)
            .await
            .unwrap();
        let notifier = Notifier::new().await.unwrap();
        notifier.nodes_sender().send(&msg).await.unwrap();

        let msg = api::NodeMessage::deleted(node, user);
        let notifier = Notifier::new().await.unwrap();
        notifier.nodes_sender().send(&msg).await.unwrap();
    }

    #[tokio::test]
    async fn test_commands_sender() {
        let db = crate::TestDb::setup().await;
        let command = db.command().await;
        let mut conn = db.pool.conn().await.unwrap();

        let command = api::Command::from_model(&command, &mut conn).await.unwrap();
        let notifier = Notifier::new().await.unwrap();
        notifier.commands_sender().send(&command).await.unwrap();
    }
}
