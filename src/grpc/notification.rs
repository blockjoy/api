use std::time::Duration;

use prost::Message;
use tracing::warn;

use crate::config::mqtt;
use crate::models;

use super::api::{self, host_message, node_message, org_message};

/// Presents the following senders:
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
    pub async fn new(config: &mqtt::Config) -> crate::Result<Self> {
        let options = config.new_options();

        let (client, mut event_loop) = rumqttc::AsyncClient::new(options, 10);
        client
            .subscribe("/bv/hosts/#", rumqttc::QoS::AtLeastOnce)
            .await?;

        tokio::spawn(async move {
            loop {
                if let Err(err) = event_loop.poll().await {
                    warn!("MQTT polling failure: {err}");
                    tokio::time::sleep(Duration::from_secs(10)).await;
                }
            }
        });

        Ok(Self { client })
    }

    pub fn sender(&self) -> MqttClient {
        MqttClient::new(self.client.clone())
    }
}

pub struct MqttClient {
    client: rumqttc::AsyncClient,
}

impl MqttClient {
    fn new(client: rumqttc::AsyncClient) -> Self {
        Self { client }
    }

    pub async fn send(&mut self, msg: impl Into<MqttMessage>) -> crate::Result<()> {
        const RETAIN: bool = false;
        const QOS: rumqttc::QoS = rumqttc::QoS::ExactlyOnce;
        let msg = msg.into();
        let payload = msg.encode();
        for channel in msg.channels() {
            self.client
                .publish(&channel, QOS, RETAIN, payload.clone())
                .await?;
        }
        Ok(())
    }
}

#[derive(derive_more::From)]
pub enum MqttMessage {
    OrgMessage(api::OrgMessage),
    HostMessage(api::HostMessage),
    NodeMessage(Box<api::NodeMessage>),
    Command(api::Command),
}

impl From<api::NodeMessage> for MqttMessage {
    fn from(value: api::NodeMessage) -> Self {
        Self::NodeMessage(Box::new(value))
    }
}

impl MqttMessage {
    fn encode(&self) -> Vec<u8> {
        match self {
            MqttMessage::OrgMessage(msg) => msg.encode_to_vec(),
            MqttMessage::HostMessage(msg) => msg.encode_to_vec(),
            MqttMessage::NodeMessage(msg) => msg.encode_to_vec(),
            MqttMessage::Command(msg) => msg.encode_to_vec(),
        }
    }

    fn channels(&self) -> Vec<String> {
        match self {
            MqttMessage::OrgMessage(msg) => msg.channels(),
            MqttMessage::HostMessage(msg) => msg.channels(),
            MqttMessage::NodeMessage(msg) => msg.channels(),
            MqttMessage::Command(msg) => msg.channels(),
        }
    }
}

// There is a couple of unwrap here below. This is because our messages have fields that are of the
// type Option which are always Some. We ensure to always populate those fields, but it is not
// possible to make a nested object required in gRPC :(.

impl api::OrgMessage {
    fn channels(&self) -> Vec<String> {
        let org_id = self.org_id().unwrap();

        vec![format!("/orgs/{org_id}")]
    }
}

impl api::HostMessage {
    fn channels(&self) -> Vec<String> {
        let host_id = self.host_id().unwrap();
        vec![format!("/hosts/{host_id}")]
    }
}

impl api::NodeMessage {
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

impl api::Command {
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
            InvitationCreated(api::InvitationCreated { org_id, .. }) => org_id.parse().ok(),
            InvitationAccepted(api::InvitationAccepted { org_id, .. }) => org_id.parse().ok(),
            InvitationDeclined(api::InvitationDeclined { org_id, .. }) => org_id.parse().ok(),
        }
    }

    pub fn created(model: api::Org, user: models::User) -> Self {
        Self {
            message: Some(org_message::Message::Created(api::OrgCreated {
                // Over MQTT, there is no current user so we pass None as a second argument.
                org: Some(model),
                created_by: user.id.to_string(),
                created_by_name: user.name(),
                created_by_email: user.email,
            })),
        }
    }

    pub fn updated(model: api::Org, user: models::User) -> Self {
        Self {
            message: Some(org_message::Message::Updated(api::OrgUpdated {
                // Over MQTT, there is no current user so we pass None as a second argument.
                org: Some(model),
                updated_by: user.id.to_string(),
                updated_by_name: user.name(),
                updated_by_email: user.email,
            })),
        }
    }

    pub fn deleted(model: models::Org, user: models::User) -> Self {
        Self {
            message: Some(org_message::Message::Deleted(api::OrgDeleted {
                org_id: model.id.to_string(),
                deleted_by: user.id.to_string(),
                deleted_by_name: user.name(),
                deleted_by_email: user.email,
            })),
        }
    }

    pub async fn invitation_created(
        model: models::Org,
        invitation: models::Invitation,
        conn: &mut models::Conn,
    ) -> crate::Result<Self> {
        let invitation = api::Invitation::from_model(invitation, conn).await?;
        Ok(Self {
            message: Some(org_message::Message::InvitationCreated(
                api::InvitationCreated {
                    org_id: model.id.to_string(),
                    invitation: Some(invitation),
                },
            )),
        })
    }

    pub async fn invitation_accepted(
        model: models::Org,
        invitation: models::Invitation,
        user: models::User,
        conn: &mut models::Conn,
    ) -> crate::Result<Self> {
        let invitation = api::Invitation::from_model(invitation, conn).await?;
        let user = api::User::from_model(user)?;
        Ok(Self {
            message: Some(org_message::Message::InvitationAccepted(
                api::InvitationAccepted {
                    org_id: model.id.to_string(),
                    invitation: Some(invitation),
                    user: Some(user),
                },
            )),
        })
    }

    pub async fn invitation_declined(
        model: models::Org,
        invitation: models::Invitation,
        conn: &mut models::Conn,
    ) -> crate::Result<Self> {
        let invitation = api::Invitation::from_model(invitation, conn).await?;
        Ok(Self {
            message: Some(org_message::Message::InvitationDeclined(
                api::InvitationDeclined {
                    org_id: model.id.to_string(),
                    invitation: Some(invitation),
                },
            )),
        })
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

    pub async fn created(
        model: models::Host,
        user: models::User,
        conn: &mut models::Conn,
    ) -> crate::Result<Self> {
        Ok(Self {
            message: Some(host_message::Message::Created(api::HostCreated {
                host: Some(api::Host::from_model(model, conn).await?),
                created_by: user.id.to_string(),
                created_by_name: user.name(),
                created_by_email: user.email,
            })),
        })
    }

    pub async fn updated(
        model: models::Host,
        user: models::User,
        conn: &mut models::Conn,
    ) -> crate::Result<Self> {
        Ok(Self {
            message: Some(host_message::Message::Updated(api::HostUpdated {
                host: Some(api::Host::from_model(model, conn).await?),
                updated_by: Some(user.id.to_string()),
                updated_by_name: Some(user.name()),
                updated_by_email: Some(user.email),
            })),
        })
    }

    pub async fn updated_many(
        models: Vec<models::Host>,
        conn: &mut models::Conn,
    ) -> crate::Result<Vec<Self>> {
        api::Host::from_models(models, conn)
            .await?
            .into_iter()
            .map(|host| {
                Ok(Self {
                    message: Some(host_message::Message::Updated(api::HostUpdated {
                        host: Some(host),
                        updated_by: None,
                        updated_by_name: None,
                        updated_by_email: None,
                    })),
                })
            })
            .collect()
    }

    pub fn deleted(model: models::Host, user: models::User) -> Self {
        Self {
            message: Some(host_message::Message::Deleted(api::HostDeleted {
                host_id: model.id.to_string(),
                deleted_by: user.id.to_string(),
                deleted_by_name: user.name(),
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

    pub fn created(model: api::Node, user: models::User) -> Self {
        Self {
            message: Some(node_message::Message::Created(api::NodeCreated {
                node: Some(model),
                created_by: user.id.to_string(),
                created_by_name: user.name(),
                created_by_email: user.email,
            })),
        }
    }

    pub async fn updated(
        model: models::Node,
        user: Option<models::User>,
        conn: &mut models::Conn,
    ) -> crate::Result<Self> {
        Ok(Self {
            message: Some(node_message::Message::Updated(api::NodeUpdated {
                node: Some(api::Node::from_model(model, conn).await?),
                updated_by: user.as_ref().map(|u| u.id.to_string()),
                updated_by_name: user.as_ref().map(|u| u.name()),
                updated_by_email: user.map(|u| u.email),
            })),
        })
    }

    pub async fn updated_many(
        models: Vec<models::Node>,
        conn: &mut models::Conn,
    ) -> crate::Result<Vec<Self>> {
        api::Node::from_models(models, conn)
            .await?
            .into_iter()
            .map(|node| {
                Ok(Self {
                    message: Some(node_message::Message::Updated(api::NodeUpdated {
                        node: Some(node),
                        updated_by: None,
                        updated_by_name: None,
                        updated_by_email: None,
                    })),
                })
            })
            .collect()
    }

    pub fn deleted(model: models::Node, user: models::User) -> Self {
        Self {
            message: Some(node_message::Message::Deleted(api::NodeDeleted {
                node_id: model.id.to_string(),
                host_id: model.host_id.to_string(),
                org_id: model.org_id.to_string(),
                deleted_by: user.id.to_string(),
                deleted_by_name: user.name(),
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
    use crate::config::Context;
    use crate::tests::TestDb;

    #[tokio::test]
    async fn test_send_hosts() {
        let context = Context::from_default_toml().await.unwrap();
        let db = TestDb::setup(context.clone()).await;
        let mut conn = db.conn().await;

        let host = db.host().await;
        let user = db.user().await;

        let msg = api::HostMessage::created(host.clone(), user.clone(), &mut conn)
            .await
            .unwrap();
        context.notifier.sender().send(msg).await.unwrap();

        let msg = api::HostMessage::updated(host.clone(), user.clone(), &mut conn)
            .await
            .unwrap();
        context.notifier.sender().send(msg).await.unwrap();

        let msg = api::HostMessage::deleted(host, user);
        context.notifier.sender().send(msg).await.unwrap();
    }

    #[tokio::test]
    async fn test_send_nodes() {
        let context = Context::from_default_toml().await.unwrap();
        let db = TestDb::setup(context.clone()).await;
        let mut conn = db.conn().await;

        let node = db.node().await;
        let user = db.user().await;

        let node_model = api::Node::from_model(node.clone(), &mut conn)
            .await
            .unwrap();
        let msg = api::NodeMessage::created(node_model.clone(), user.clone());
        context.notifier.sender().send(msg).await.unwrap();

        let msg = api::NodeMessage::updated(node.clone(), Some(user.clone()), &mut conn)
            .await
            .unwrap();
        context.notifier.sender().send(msg).await.unwrap();

        let msg = api::NodeMessage::deleted(node, user);
        context.notifier.sender().send(msg).await.unwrap();
    }

    #[tokio::test]
    async fn test_send_commands() {
        let context = Context::from_default_toml().await.unwrap();
        let db = TestDb::setup(context.clone()).await;

        let command = db.command().await;
        let mut conn = db.conn().await;

        let command = api::Command::from_model(&command, &mut conn).await.unwrap();
        context.notifier.sender().send(command).await.unwrap();
    }
}
