use super::{blockjoy, blockjoy_ui};
use crate::errors::Result;

/// Presents the following senders:
///
/// |---------------|---------------------------------------------------------|
/// | private api   | topics                                                  |
/// |---------------|---------------------------------------------------------|
/// | organizations | -                                                       |
/// |---------------|---------------------------------------------------------|
/// | hosts         | /bv/hosts/<host_id>                                     |
/// |---------------|---------------------------------------------------------|
/// | nodes         | /bv/hosts/<host_id>/nodes/<node_id>                     |
/// |               | /bv/nodes/<node_id>                                     |
/// |---------------|---------------------------------------------------------|
/// | commands      | /bv/hosts/<host_id>/nodes/<node_id>/commands            |
/// |               | /bv/hosts/<host_id>/commands                            |
/// |               | /bv/commands                                            |
/// |---------------|---------------------------------------------------------|
///
/// |---------------|---------------------------------------------------------|
/// | public api    | topics                                                  |
/// |---------------|---------------------------------------------------------|
/// | organizations | /orgs/<org_id>                                          |
/// |---------------|---------------------------------------------------------|
/// | hosts         | /orgs/<org_id>/hosts/<host_id>                          |
/// |               | /hosts/<host_id>                                        |
/// |---------------|---------------------------------------------------------|
/// | nodes         | /orgs/<org_id>/hosts/<host_id>/nodes/<node_id>          |
/// |               | /hosts/<host_id>/nodes/<node_id>                        |
/// |               | /nodes/<node_id>                                        |
/// |---------------|---------------------------------------------------------|
/// | commands      | /orgs/<org_id>/hosts/<host_id>/nodes/<node_id>/commands |
/// |               | /hosts/<host_id>/nodes/<node_id>/commands               |
/// |               | /nodes/<node_id>/commands                               |
/// |               | /commands                                               |
/// |---------------|---------------------------------------------------------|
#[derive(Debug, Clone, Default)]
pub struct Notifier {}

impl Notifier {
    pub fn new() -> Self {
        Self {}
    }

    // bv_orgs_sender does not exist, blockvisor does not care about organizations.

    pub fn bv_hosts_sender(&self) -> MqttClient<blockjoy::HostInfo> {
        MqttClient::new()
    }

    pub fn bv_nodes_sender(&self) -> MqttClient<blockjoy::NodeInfo> {
        MqttClient::new()
    }

    pub fn bv_commands_sender(&self) -> MqttClient<blockjoy::Command> {
        MqttClient::new()
    }

    pub fn ui_orgs_sender(&self) -> MqttClient<blockjoy_ui::Organization> {
        MqttClient::new()
    }

    pub fn ui_hosts_sender(&self) -> MqttClient<blockjoy_ui::Host> {
        MqttClient::new()
    }

    pub fn ui_nodes_sender(&self) -> MqttClient<blockjoy_ui::Node> {
        MqttClient::new()
    }

    // pub fn ui_commands_sender(&self) -> MqttClient<blockjoy_ui::Command> {
    //     MqttClient::new()
    // }
}

/// The DbListener<T> is a singleton struct that listens for messages coming from the database.
/// When a message comes in, the re
pub struct MqttClient<T> {
    client: rumqttc::AsyncClient,
    _event_loop: rumqttc::EventLoop,
    _pd: std::marker::PhantomData<T>,
}

impl<T: Notify + prost::Message> MqttClient<T> {
    fn new() -> Self {
        let mut options = rumqttc::MqttOptions::new("1", "35.237.162.218", 1883);
        options.set_credentials("blockvisor-api", "PH*rE:\\ZQlecB9/I?[#R$q3M;5yCb]Y+");
        let (client, event_loop) = rumqttc::AsyncClient::new(options, 10);
        Self {
            client,
            _event_loop: event_loop,
            _pd: std::marker::PhantomData,
        }
    }

    pub async fn send(&mut self, msg: &T) -> Result<()> {
        const RETAIN: bool = false;
        const QOS: rumqttc::QoS = rumqttc::QoS::ExactlyOnce;
        let payload = msg.encode_to_vec();

        for channel in msg.channels() {
            self.client
                .publish(&channel, QOS, RETAIN, payload.clone())
                .await?;
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

impl Notify for blockjoy_ui::Organization {
    fn channels(&self) -> Vec<String> {
        let org_id = self.id.as_ref().unwrap();

        vec![format!("/orgs/{org_id}")]
    }
}

impl Notify for blockjoy_ui::Host {
    fn channels(&self) -> Vec<String> {
        let host_id = self.id.as_ref().unwrap();
        let org_id = self.org_id.as_ref();

        let mut res = vec![format!("/hosts/{host_id}")];
        res.extend(org_id.map(|o| format!("/orgs/{o}/hosts/{host_id}")));
        res
    }
}

impl Notify for blockjoy_ui::Node {
    fn channels(&self) -> Vec<String> {
        let host_id = self.host_id.as_ref().unwrap();
        let node_id = self.id.as_ref().unwrap();
        vec![format!("/hosts/{host_id}/nodes/{node_id}")]
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
        let host = host.try_into().unwrap();
        let notifier = Notifier::new();
        notifier.bv_hosts_sender().send(&host).await.unwrap();
    }

    #[tokio::test]
    async fn test_bv_nodes_sender() {
        let db = crate::TestDb::setup().await;
        let node = db.node().await;
        let node = node.try_into().unwrap();
        let notifier = Notifier::new();
        notifier.bv_nodes_sender().send(&node).await.unwrap();
    }

    #[tokio::test]
    async fn test_bv_commands_sender() {
        let db = crate::TestDb::setup().await;
        let command = db.command().await;
        let mut conn = db.pool.conn().await.unwrap();
        let command = convert::db_command_to_grpc_command(&command, &mut conn)
            .await
            .unwrap();
        let notifier = Notifier::new();
        notifier.bv_commands_sender().send(&command).await.unwrap();
    }

    #[tokio::test]
    async fn test_ui_hosts_sender() {
        let db = crate::TestDb::setup().await;
        let host = db.host().await;
        let host = host.try_into().unwrap();
        let notifier = Notifier::new();
        notifier.ui_hosts_sender().send(&host).await.unwrap();
    }

    #[tokio::test]
    async fn test_ui_nodes_sender() {
        let db = crate::TestDb::setup().await;
        let node = db.node().await;
        let node = node.try_into().unwrap();
        let notifier = Notifier::new();
        notifier.ui_nodes_sender().send(&node).await.unwrap();
    }
}
