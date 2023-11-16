mod recover;
mod success;

use cidr_utils::cidr::IpCidr;
use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::error;

use crate::auth::rbac::CommandPerm;
use crate::auth::Authorize;
use crate::database::{Conn, ReadConn, Transaction, WriteConn};
use crate::grpc::api::command_service_server::CommandService;
use crate::grpc::{api, Grpc};
use crate::models::blockchain::{Blockchain, BlockchainProperty, BlockchainVersion};
use crate::models::command::UpdateCommand;
use crate::models::node::{FilteredIpAddr, NodeStatus};
use crate::models::{Command, CommandType, Host, Node};
use crate::util::NanosUtc;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Command blockchain error: {0}
    Blockchain(#[from] crate::models::blockchain::Error),
    /// Command blockchain property error: {0}
    BlockchainProperty(#[from] crate::models::blockchain::property::Error),
    /// Command blockchain version error: {0}
    BlockchainVersion(#[from] crate::models::blockchain::version::Error),
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Command model failure: {0}
    Command(#[from] crate::models::command::Error),
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Command host error: {0}
    Host(#[from] crate::models::host::Error),
    /// IP is not a CIDR.
    IpNotCidr,
    /// Missing `command.node_id`.
    MissingNodeId,
    /// Command node error: {0}
    Node(#[from] crate::models::node::Error),
    /// Not implemented.
    NotImplemented,
    /// Failed to parse HostId: {0}
    ParseHostId(uuid::Error),
    /// Failed to parse CommandId: {0}
    ParseId(uuid::Error),
    /// Resource error: {0}
    Resource(#[from] crate::auth::resource::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            MissingNodeId => Status::invalid_argument("command.node_id"),
            ParseHostId(_) => Status::invalid_argument("host_id"),
            ParseId(_) => Status::invalid_argument("id"),
            Diesel(_) | IpNotCidr | NotImplemented => Status::internal("Internal error."),
            Auth(err) => err.into(),
            Blockchain(err) => err.into(),
            BlockchainProperty(err) => err.into(),
            BlockchainVersion(err) => err.into(),
            Claims(err) => err.into(),
            Command(err) => err.into(),
            Host(err) => err.into(),
            Node(err) => err.into(),
            Resource(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl CommandService for Grpc {
    async fn update(
        &self,
        req: Request<api::CommandServiceUpdateRequest>,
    ) -> Result<Response<api::CommandServiceUpdateResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| update(req, meta, write).scope_boxed())
            .await
    }

    async fn ack(
        &self,
        req: Request<api::CommandServiceAckRequest>,
    ) -> Result<Response<api::CommandServiceAckResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| ack(req, meta, write).scope_boxed())
            .await
    }

    async fn pending(
        &self,
        req: Request<api::CommandServicePendingRequest>,
    ) -> Result<Response<api::CommandServicePendingResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| pending(req, meta, read).scope_boxed())
            .await
    }
}

async fn update(
    req: api::CommandServiceUpdateRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::CommandServiceUpdateResponse, Error> {
    let id = req.id.parse().map_err(Error::ParseId)?;
    let command = Command::find_by_id(id, &mut write).await?;

    write
        .auth(&meta, CommandPerm::Update, command.host_id)
        .await?;

    let update_cmd = req.as_update()?;
    let cmd = update_cmd.update(&mut write).await?;

    match cmd.exit_status {
        Some(0) => {
            // Some responses require us to register success.
            success::register(&cmd, &mut write).await;
        }
        // Will match any integer other than 0.
        Some(_) => {
            // We got back an error status code. In practice, blockvisord sends 0 for
            // success and 1 for failure, but we treat every non-zero exit code as an
            // error, not just 1.
            recover::recover(&cmd, &mut write)
                .await
                .unwrap_or_default()
                .into_iter()
                .for_each(|cmd| write.mqtt(cmd));
        }
        None => (),
    };

    let command = api::Command::from_model(&cmd, &mut write).await?;
    write.mqtt(command.clone());

    Ok(api::CommandServiceUpdateResponse {
        command: Some(command),
    })
}

async fn ack(
    req: api::CommandServiceAckRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::CommandServiceAckResponse, Error> {
    let id = req.id.parse().map_err(Error::ParseId)?;
    let command = Command::find_by_id(id, &mut write).await?;
    write.auth_all(&meta, CommandPerm::Ack).await?;
    if command.acked_at.is_none() {
        command.ack(&mut write).await?;
        if let Some(node) = command.node(&mut write).await? {
            ack_update_node_status(node, command, &mut write).await?;
        }
    }

    Ok(api::CommandServiceAckResponse {})
}

async fn pending(
    req: api::CommandServicePendingRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::CommandServicePendingResponse, Error> {
    let host_id = req.host_id.parse().map_err(Error::ParseHostId)?;

    read.auth(&meta, CommandPerm::Pending, host_id).await?;
    Host::find_by_id(host_id, &mut read).await?;

    let pending = Command::find_pending_by_host(host_id, &mut read).await?;
    let mut commands = Vec::with_capacity(pending.len());
    for command in pending {
        commands.push(api::Command::from_model(&command, &mut read).await?);
    }

    Ok(api::CommandServicePendingResponse { commands })
}

async fn ack_update_node_status(
    mut node: Node,
    command: Command,
    write: &mut WriteConn<'_, '_>,
) -> Result<(), Error> {
    let new_status = match command.cmd {
        // If the create node command gets acknowledged, we move the node from `ProvisioningPending`
        // to `Provisioning`.
        CommandType::CreateNode => NodeStatus::Provisioning,
        // If the delete node command gets acknowledged, we move the node from `DeletePending` to
        // `Deleting`.
        CommandType::DeleteNode => NodeStatus::Deleting,
        // If the update node command gets acknowledged, we move the node from `UpdatePending` to
        // `Deleting`.
        CommandType::UpdateNode => NodeStatus::Updating,
        // The rest of the acks do not require us updating the status of our nodes.
        _ => return Ok(()),
    };
    node.node_status = new_status;
    node.update(write).await?;
    Ok(())
}

impl api::Command {
    pub async fn from_model(model: &Command, conn: &mut Conn<'_>) -> Result<api::Command, Error> {
        use api::command;
        use api::node_command::Command;

        use crate::models::command::CommandType::*;

        // Extract the node id from the model, if there is one.
        let node_id = || model.node_id.ok_or(Error::MissingNodeId);

        let node_cmd = |command| {
            Ok(api::Command {
                id: model.id.to_string(),
                response: model.response.clone(),
                exit_code: model.exit_status,
                acked_at: model.acked_at.map(NanosUtc::from).map(Into::into),
                command: Some(command::Command::Node(api::NodeCommand {
                    node_id: node_id()?.to_string(),
                    host_id: model.host_id.to_string(),
                    command: Some(command),
                    api_command_id: model.id.to_string(),
                    created_at: Some(NanosUtc::from(model.created_at).into()),
                })),
            })
        };

        let host_cmd = |host_id| {
            Ok(api::Command {
                id: model.id.to_string(),
                response: model.response.clone(),
                exit_code: model.exit_status,
                acked_at: model.acked_at.map(NanosUtc::from).map(Into::into),
                command: Some(command::Command::Host(api::HostCommand { host_id })),
            })
        };

        match model.cmd {
            RestartNode => node_cmd(Command::Restart(api::NodeRestart {})),
            KillNode => node_cmd(Command::Stop(api::NodeStop {})),
            ShutdownNode => node_cmd(Command::Stop(api::NodeStop {})),
            UpdateNode => {
                let node = Node::find_by_id(node_id()?, conn).await?;
                let cmd = Command::Update(api::NodeUpdate {
                    rules: Self::rules(&node)?,
                });
                node_cmd(cmd)
            }
            UpgradeNode => {
                let node = Node::find_by_id(node_id()?, conn).await?;
                let blockchain = Blockchain::find_by_id(node.blockchain_id, conn).await?;
                let mut image = api::ContainerImage {
                    protocol: blockchain.name,
                    node_version: node.version.as_ref().to_lowercase(),
                    node_type: 0, // We use the setter to set this field for type-safety
                };
                image.set_node_type(node.node_type.into());
                let cmd = Command::Upgrade(api::NodeUpgrade { image: Some(image) });
                node_cmd(cmd)
            }
            MigrateNode => Err(Error::NotImplemented),
            GetNodeVersion => node_cmd(Command::InfoGet(api::NodeGet {})),
            CreateNode => {
                let node = Node::find_by_id(node_id()?, conn).await?;
                let blockchain = Blockchain::find_by_id(node.blockchain_id, conn).await?;
                let version =
                    BlockchainVersion::find(blockchain.id, node.node_type, &node.version, conn)
                        .await?;
                let id_to_name_map = BlockchainProperty::id_to_name_map(version.id, conn).await?;
                let mut image = api::ContainerImage {
                    protocol: blockchain.name,
                    node_version: node.version.as_ref().to_lowercase(),
                    node_type: 0, // We use the setter to set this field for type-safety
                };
                image.set_node_type(node.node_type.into());
                let properties = node
                    .properties(conn)
                    .await?
                    .into_iter()
                    .map(|p| (&id_to_name_map[&p.blockchain_property_id], p.value))
                    .map(|(name, value)| api::Parameter::new(name, &value))
                    .collect();
                let mut node_create = api::NodeCreate {
                    name: node.name.clone(),
                    blockchain: node.blockchain_id.to_string(),
                    image: Some(image),
                    node_type: 0, // We use the setter to set this field for type-safety
                    ip: node.ip_addr.clone(),
                    gateway: node.ip_gateway.clone(),
                    properties,
                    rules: Self::rules(&node)?,
                    network: node.network,
                };
                node_create.set_node_type(node.node_type.into());
                let cmd = Command::Create(node_create);

                node_cmd(cmd)
            }
            DeleteNode => {
                let cmd = Command::Delete(api::NodeDelete {});
                node_cmd(cmd)
            }

            GetBVSVersion => host_cmd(model.host_id.to_string()),
            UpdateBVS => host_cmd(model.host_id.to_string()),
            RestartBVS => host_cmd(model.host_id.to_string()),
            RemoveBVS => host_cmd(model.host_id.to_string()),
            CreateBVS => host_cmd(model.host_id.to_string()),
            StopBVS => host_cmd(model.host_id.to_string()),
        }
    }

    pub fn rules(node: &Node) -> Result<Vec<api::Rule>, Error> {
        fn firewall_rules(
            // I'll leave the Vec for now, maybe we need it later
            denied_or_allowed_ips: Vec<FilteredIpAddr>,
            action: api::Action,
        ) -> Result<Vec<api::Rule>, Error> {
            let mut rules = vec![];
            for ip in denied_or_allowed_ips {
                // TODO: newtype validation
                if !IpCidr::is_ip_cidr(&ip.ip) {
                    return Err(Error::IpNotCidr);
                }

                rules.push(api::Rule {
                    name: String::new(),
                    action: action as i32,
                    direction: api::Direction::In as i32,
                    protocol: api::Protocol::Both as i32,
                    ips: Some(ip.ip),
                    ports: vec![],
                });
            }

            Ok(rules)
        }

        let rules = firewall_rules(node.allow_ips()?, api::Action::Allow)?
            .into_iter()
            .chain(firewall_rules(node.deny_ips()?, api::Action::Deny)?)
            .collect();
        Ok(rules)
    }
}

impl api::CommandServiceUpdateRequest {
    fn as_update(&self) -> Result<UpdateCommand<'_>, Error> {
        Ok(UpdateCommand {
            id: self.id.parse().map_err(Error::ParseId)?,
            response: self.response.as_deref(),
            exit_status: self.exit_code,
            completed_at: self.exit_code.map(|_| chrono::Utc::now()),
        })
    }
}

impl api::Parameter {
    fn new(name: &str, val: &str) -> Self {
        Self {
            name: name.to_owned(),
            value: val.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Context;

    use super::*;

    #[tokio::test]
    async fn test_create_firewall_rules() {
        let (_ctx, db) = Context::with_mocked().await.unwrap();
        api::Command::rules(&db.seed.node).unwrap();
    }
}
