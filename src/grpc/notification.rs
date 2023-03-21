use anyhow::anyhow;
use diesel_async::AsyncPgConnection;

use super::{
    blockjoy,
    blockjoy_ui::{self, node_message, org_message},
};
use crate::{auth::key_provider::KeyProvider, errors::Result, models};

/// Presents the following senders:
///
/// |---------------|----------------------------------------------|
/// | private api   | topics                                       |
/// |---------------|----------------------------------------------|
/// | organizations | -                                            |
/// |---------------|----------------------------------------------|
/// | hosts         | /bv/hosts/<host_id>                          |
/// |---------------|----------------------------------------------|
/// | nodes         | /bv/hosts/<host_id>/nodes/<node_id>          |
/// |               | /bv/nodes/<node_id>                          |
/// |---------------|----------------------------------------------|
/// | commands      | /bv/hosts/<host_id>/nodes/<node_id>/commands |
/// |               | /bv/hosts/<host_id>/commands                 |
/// |---------------|----------------------------------------------|
///
/// |---------------|----------------------------------------------|
/// | public api    | topics                                       |
/// |---------------|----------------------------------------------|
/// | organizations | /orgs/<org_id>                               |
/// |---------------|----------------------------------------------|
/// | hosts         | -                                            |
/// |---------------|----------------------------------------------|
/// | nodes         | /orgs/<org_id>/nodes                         |
/// |               | /nodes/<node_id>                             |
/// |---------------|----------------------------------------------|
/// | commands      | /orgs/<org_id>/nodes/<node_id>/commands      |
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

    // bv_orgs_sender does not exist, blockvisor does not care about organizations.

    pub fn bv_hosts_sender(&self) -> Result<MqttClient<blockjoy::HostInfo>> {
        MqttClient::new(self.client.clone())
    }

    pub fn bv_nodes_sender(&self) -> Result<MqttClient<blockjoy::NodeInfo>> {
        tracing::info!("Making a sender for node messages");
        MqttClient::new(self.client.clone())
    }

    pub fn bv_commands_sender(&self) -> Result<MqttClient<blockjoy::Command>> {
        MqttClient::new(self.client.clone())
    }

    pub fn ui_orgs_sender(&self) -> Result<MqttClient<blockjoy_ui::OrgMessage>> {
        MqttClient::new(self.client.clone())
    }

    pub fn ui_hosts_sender(&self) -> Result<MqttClient<blockjoy_ui::Host>> {
        MqttClient::new(self.client.clone())
    }

    pub fn ui_nodes_sender(&self) -> Result<MqttClient<blockjoy_ui::NodeMessage>> {
        MqttClient::new(self.client.clone())
    }

    // pub fn ui_commands_sender(&self) -> Result<MqttClient<blockjoy_ui::Command>> {
    //     MqttClient::new()
    // }

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
    fn new(client: rumqttc::AsyncClient) -> Result<Self> {
        Ok(Self {
            client,
            _pd: std::marker::PhantomData,
        })
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
// type Option which are always Some.

impl Notify for blockjoy::HostInfo {
    fn channels(&self) -> Vec<String> {
        let host_id = self.id.as_ref().unwrap();
        vec![format!("/bv/hosts/{host_id}")]
    }
}

impl Notify for blockjoy::NodeInfo {
    fn channels(&self) -> Vec<String> {
        let host_id = self.host_id.as_ref().unwrap();
        let node_id = &self.id;
        vec![format!("/bv/hosts/{host_id}/nodes/{node_id}")]
    }
}

impl blockjoy::Command {
    fn host_id(&self) -> Option<&str> {
        match self.r#type.as_ref()? {
            blockjoy::command::Type::Node(cmd) => Some(&cmd.host_id),
            blockjoy::command::Type::Host(cmd) => Some(&cmd.host_id),
        }
    }

    fn node_id(&self) -> Option<&str> {
        match self.r#type.as_ref()? {
            blockjoy::command::Type::Node(cmd) => Some(&cmd.node_id),
            blockjoy::command::Type::Host(_) => None,
        }
    }
}

impl Notify for blockjoy::Command {
    fn channels(&self) -> Vec<String> {
        let node_id = self.node_id();
        let host_id = self.host_id();

        let mut res = vec![format!("/bv/commands")];
        res.extend(node_id.map(|n| format!("/bv/nodes/{n}/commands")));
        let both = host_id.zip(node_id);
        res.extend(both.map(|(h, n)| format!("/bv/hosts/{h}/nodes/{n}/commands")));
        res
    }
}

impl blockjoy_ui::OrgMessage {
    fn org_id(&self) -> Option<uuid::Uuid> {
        use org_message::Message::*;
        match self.message.as_ref()? {
            Created(blockjoy_ui::OrgCreated { org, .. }) => org.as_ref()?.id.as_ref()?.parse().ok(),
            Updated(blockjoy_ui::OrgUpdated { org, .. }) => org.as_ref()?.id.as_ref()?.parse().ok(),
            Deleted(blockjoy_ui::OrgDeleted {
                organization_id, ..
            }) => organization_id.parse().ok(),
        }
    }

    pub fn created(model: models::Org, user: models::User) -> crate::Result<Self> {
        Ok(Self {
            message: Some(org_message::Message::Created(blockjoy_ui::OrgCreated {
                org: Some(blockjoy_ui::Organization::from_model(model)?),
                created_by: user.id.to_string(),
                created_by_name: format!("{} {}", user.first_name, user.last_name),
                created_by_email: user.email,
            })),
        })
    }

    pub fn updated(model: models::Org, user: models::User) -> crate::Result<Self> {
        Ok(Self {
            message: Some(org_message::Message::Updated(blockjoy_ui::OrgUpdated {
                org: Some(blockjoy_ui::Organization::from_model(model)?),
                updated_by: user.id.to_string(),
                updated_by_name: format!("{} {}", user.first_name, user.last_name),
                updated_by_email: user.email,
            })),
        })
    }

    pub fn deleted(model: models::Org, user: models::User) -> Self {
        Self {
            message: Some(org_message::Message::Deleted(blockjoy_ui::OrgDeleted {
                organization_id: model.id.to_string(),
                deleted_by: user.id.to_string(),
                deleted_by_name: format!("{} {}", user.first_name, user.last_name),
                deleted_by_email: user.email,
            })),
        }
    }
}

impl Notify for blockjoy_ui::OrgMessage {
    fn channels(&self) -> Vec<String> {
        let org_id = self.org_id().unwrap();

        vec![format!("/orgs/{org_id}")]
    }
}

impl Notify for blockjoy_ui::Host {
    fn channels(&self) -> Vec<String> {
        vec![]
    }
}

impl blockjoy_ui::NodeMessage {
    fn node_id(&self) -> Option<uuid::Uuid> {
        use node_message::Message::*;
        match self.message.as_ref()? {
            Created(blockjoy_ui::NodeCreated { node })
            | Updated(blockjoy_ui::NodeUpdated { node, .. }) => {
                node.as_ref()?.id.as_ref()?.parse().ok()
            }
            Deleted(blockjoy_ui::NodeDeleted { node_id, .. }) => node_id.parse().ok(),
        }
    }

    fn org_id(&self) -> Option<uuid::Uuid> {
        use node_message::Message::*;
        match self.message.as_ref()? {
            Created(blockjoy_ui::NodeCreated { node })
            | Updated(blockjoy_ui::NodeUpdated { node, .. }) => {
                node.as_ref()?.org_id.as_ref()?.parse().ok()
            }
            Deleted(blockjoy_ui::NodeDeleted {
                organization_id, ..
            }) => organization_id.parse().ok(),
        }
    }

    pub async fn created(model: models::Node, conn: &mut AsyncPgConnection) -> crate::Result<Self> {
        Ok(Self {
            message: Some(node_message::Message::Created(blockjoy_ui::NodeCreated {
                node: Some(blockjoy_ui::Node::from_model(model, conn).await?),
            })),
        })
    }

    pub async fn updated(
        model: models::Node,
        user: models::User,
        conn: &mut AsyncPgConnection,
    ) -> crate::Result<Self> {
        Ok(Self {
            message: Some(node_message::Message::Updated(blockjoy_ui::NodeUpdated {
                node: Some(blockjoy_ui::Node::from_model(model, conn).await?),
                updated_by: user.id.to_string(),
                updated_by_name: format!("{} {}", user.first_name, user.last_name),
                updated_by_email: user.email,
            })),
        })
    }

    pub fn deleted(model: models::Node, user: models::User) -> Self {
        Self {
            message: Some(node_message::Message::Deleted(blockjoy_ui::NodeDeleted {
                node_id: model.id.to_string(),
                host_id: model.host_id.to_string(),
                organization_id: model.org_id.to_string(),
                deleted_by: user.id.to_string(),
                deleted_by_name: format!("{} {}", user.first_name, user.last_name),
                deleted_by_email: user.email,
            })),
        }
    }
}

impl Notify for blockjoy_ui::NodeMessage {
    fn channels(&self) -> Vec<String> {
        let node_id = self.node_id().unwrap();
        let org_id = self.org_id().unwrap();
        vec![format!("/orgs/{org_id}/nodes"), format!("/nodes/{node_id}")]
    }
}

#[cfg(test)]
mod tests {
    use crate::grpc::convert;

    use super::*;

    #[tokio::test]
    async fn test_bv_hosts_sender() {
        let db = crate::TestDb::setup().await;
        let host = db.host().await;
        let host = blockjoy::HostInfo {
            id: Some(host.id.to_string()),
            ..Default::default()
        };
        let notifier = Notifier::new().await.unwrap();
        notifier
            .bv_hosts_sender()
            .unwrap()
            .send(&host)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_bv_nodes_sender() {
        let db = crate::TestDb::setup().await;
        let node = db.node().await;
        let node = blockjoy::NodeInfo::from_model(node);
        let notifier = Notifier::new().await.unwrap();
        notifier
            .bv_nodes_sender()
            .unwrap()
            .send(&node)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_bv_commands_sender() {
        let db = crate::TestDb::setup().await;
        let command = db.command().await;
        let mut conn = db.pool.conn().await.unwrap();
        let command = convert::db_command_to_grpc_command(&command, &mut conn)
            .await
            .unwrap();
        let notifier = Notifier::new().await.unwrap();
        notifier
            .bv_commands_sender()
            .unwrap()
            .send(&command)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_ui_hosts_sender() {
        let db = crate::TestDb::setup().await;
        let host = db.host().await;
        let host = host.try_into().unwrap();
        let notifier = Notifier::new().await.unwrap();
        notifier
            .ui_hosts_sender()
            .unwrap()
            .send(&host)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_ui_nodes_sender() {
        let db = crate::TestDb::setup().await;
        let mut conn = db.pool.conn().await.unwrap();
        let node = db.node().await;
        let node = blockjoy_ui::NodeMessage::created(node, &mut conn)
            .await
            .unwrap();
        let notifier = Notifier::new().await.unwrap();
        notifier
            .ui_nodes_sender()
            .unwrap()
            .send(&node)
            .await
            .unwrap();
    }
}
