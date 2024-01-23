mod recover;
mod success;

use cidr_utils::cidr::IpCidr;
use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::metadata::MetadataMap;
use tonic::{Request, Response, Status};
use tracing::{error, warn};

use crate::auth::rbac::{CommandAdminPerm, CommandPerm};
use crate::auth::resource::{NodeId, Resource};
use crate::auth::{AuthZ, Authorize};
use crate::database::{Conn, ReadConn, Transaction, WriteConn};
use crate::grpc::api::command_service_server::CommandService;
use crate::grpc::common::{FirewallAction, FirewallDirection, FirewallProtocol, FirewallRule};
use crate::grpc::{api, common, Grpc};
use crate::models::blockchain::{Blockchain, BlockchainProperty, BlockchainVersion};
use crate::models::command::{CommandFilter, ExitCode, UpdateCommand};
use crate::models::node::{NodeStatus, UpdateNode};
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
    /// Error creating a gRPC representation of a node: {0}
    GrpcHost(Box<crate::grpc::node::Error>),
    /// Command host error: {0}
    Host(#[from] crate::models::host::Error),
    /// IP is not a CIDR.
    IpNotCidr,
    /// Missing BlockchainPropertyId. This should not happen.
    MissingBlockchainPropertyId,
    /// Missing `command.node_id`.
    MissingNodeId,
    /// Command node error: {0}
    Node(#[from] crate::models::node::Error),
    /// Not implemented.
    NotImplemented,
    /// Failed to parse ExitCode.
    ParseExitCode,
    /// Failed to parse HostId: {0}
    ParseHostId(uuid::Error),
    /// Failed to parse NodeId: {0}
    ParseNodeId(uuid::Error),
    /// Failed to parse CommandId: {0}
    ParseId(uuid::Error),
    /// Unable to cast retry hint from u64 to i64: {0}
    RetryHint(std::num::TryFromIntError),
    /// Resource error: {0}
    Resource(#[from] crate::auth::resource::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        error!("{err}");
        match err {
            MissingNodeId => Status::invalid_argument("command.node_id"),
            ParseExitCode => Status::invalid_argument("exit_code"),
            ParseNodeId(_) => Status::invalid_argument("node_id"),
            ParseHostId(_) => Status::invalid_argument("host_id"),
            ParseId(_) => Status::invalid_argument("id"),
            RetryHint(_) => Status::invalid_argument("retry_hint_seconds"),
            Diesel(_) | IpNotCidr | MissingBlockchainPropertyId | NotImplemented | GrpcHost(_) => {
                Status::internal("Internal error.")
            }
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
    async fn list(
        &self,
        req: Request<api::CommandServiceListRequest>,
    ) -> Result<Response<api::CommandServiceListResponse>, Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list(req, meta, read).scope_boxed()).await
    }

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

async fn list(
    req: api::CommandServiceListRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::CommandServiceListResponse, Error> {
    let filter = req.as_filter()?;
    let authz = if let Some(node_id) = filter.node_id {
        read.auth_or_all(&meta, CommandAdminPerm::List, CommandPerm::List, node_id)
            .await?
    } else if let Some(host_id) = filter.host_id {
        read.auth_or_all(&meta, CommandAdminPerm::List, CommandPerm::List, host_id)
            .await?
    } else {
        read.auth_all(&meta, CommandAdminPerm::List).await?
    };
    let models = Command::filter(req.as_filter()?, &mut read).await?;
    let mut commands = Vec::with_capacity(models.len());
    for command in models {
        commands.push(api::Command::from_model(&command, &authz, &mut read).await?);
    }
    Ok(api::CommandServiceListResponse { commands })
}

async fn update(
    req: api::CommandServiceUpdateRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::CommandServiceUpdateResponse, Error> {
    let id = req.id.parse().map_err(Error::ParseId)?;
    let command = Command::by_id(id, &mut write).await?;
    let authz = write
        .auth(&meta, CommandPerm::Update, command.host_id)
        .await?;

    let updated = UpdateCommand::from_request(req)?.update(&mut write).await?;
    match updated.exit_code {
        Some(ExitCode::Ok) => success::register(&updated, &authz, &mut write).await,
        Some(_) => recover::recover(&updated, &authz, &mut write)
            .await
            .unwrap_or_default()
            .into_iter()
            .for_each(|cmd| write.mqtt(cmd)),
        None => (),
    };

    let cmd = api::Command::from_model(&updated, &authz, &mut write).await?;
    write.mqtt(cmd.clone());

    Ok(api::CommandServiceUpdateResponse { command: Some(cmd) })
}

async fn ack(
    req: api::CommandServiceAckRequest,
    meta: MetadataMap,
    mut write: WriteConn<'_, '_>,
) -> Result<api::CommandServiceAckResponse, Error> {
    let id = req.id.parse().map_err(Error::ParseId)?;
    let command = Command::by_id(id, &mut write).await?;

    let resource: Resource = command.node_id.map_or(command.host_id.into(), Into::into);
    let authz = write.auth(&meta, CommandPerm::Ack, resource).await?;

    if command.acked_at.is_none() {
        command.ack(&mut write).await?;
    } else {
        warn!("Duplicate ack for command id: {0}", command.id);
    }

    if let Some(node) = command.node(&mut write).await? {
        ack_node_transition(node, &command, &authz, &mut write).await?;
    }

    Ok(api::CommandServiceAckResponse {})
}

async fn pending(
    req: api::CommandServicePendingRequest,
    meta: MetadataMap,
    mut read: ReadConn<'_, '_>,
) -> Result<api::CommandServicePendingResponse, Error> {
    let host_id = req.host_id.parse().map_err(Error::ParseHostId)?;
    let authz = read.auth(&meta, CommandPerm::Pending, host_id).await?;
    Host::by_id(host_id, &mut read).await?;

    let pending = Command::host_pending(host_id, &mut read).await?;
    let mut commands = Vec::with_capacity(pending.len());
    for command in pending {
        commands.push(api::Command::from_model(&command, &authz, &mut read).await?);
    }

    Ok(api::CommandServicePendingResponse { commands })
}

/// Apply state transition after acknowledging a node command.
async fn ack_node_transition(
    node: Node,
    command: &Command,
    authz: &AuthZ,
    write: &mut WriteConn<'_, '_>,
) -> Result<(), Error> {
    let next_status = match (command.command_type, node.node_status) {
        (CommandType::NodeCreate, NodeStatus::ProvisioningPending) => NodeStatus::Provisioning,
        (CommandType::NodeCreate, status) => {
            warn!("Moving node {} from {status:?} to Provisioning", node.id);
            NodeStatus::Provisioning
        }

        (CommandType::NodeUpdate, NodeStatus::UpdatePending) => NodeStatus::Updating,
        (CommandType::NodeUpdate, status) => {
            warn!("Moving node {} from {status:?} to Updating", node.id);
            NodeStatus::Updating
        }

        (CommandType::NodeDelete, NodeStatus::DeletePending) => NodeStatus::Deleting,
        (CommandType::NodeDelete, status) => {
            warn!("Moving node {} from {status:?} to Deleting", node.id);
            NodeStatus::Deleting
        }

        _ => return Ok(()),
    };

    let update = UpdateNode {
        node_status: Some(next_status),
        ..Default::default()
    };
    let node = node.update(update, write).await?;

    let node = api::Node::from_model(node, authz, write)
        .await
        .map_err(|err| Error::GrpcHost(Box::new(err)))?;
    let updated_by = common::EntityUpdate::from_resource(authz, write).await?;
    let msg = api::NodeMessage::updated(node, updated_by);
    write.mqtt(msg);

    Ok(())
}

impl api::Command {
    pub async fn from_model(
        model: &Command,
        authz: &AuthZ,
        conn: &mut Conn<'_>,
    ) -> Result<Self, Error> {
        match model.command_type {
            CommandType::HostStart => host_start(model),
            CommandType::HostStop => host_stop(model),
            CommandType::HostRestart => host_restart(model),
            CommandType::HostPending => host_pending(model),
            CommandType::NodeCreate => node_create(model, authz, conn).await,
            CommandType::NodeStart => node_start(model),
            CommandType::NodeStop => node_stop(model),
            CommandType::NodeRestart => node_restart(model),
            CommandType::NodeUpgrade => node_upgrade(model, authz, conn).await,
            CommandType::NodeUpdate => node_update(model, conn).await,
            CommandType::NodeDelete => node_delete(model),
        }
    }
}

impl api::CommandServiceListRequest {
    fn as_filter(&self) -> Result<CommandFilter, Error> {
        Ok(CommandFilter {
            node_id: self
                .node_id
                .as_deref()
                .map(|id| id.parse().map_err(Error::ParseNodeId))
                .transpose()?,
            host_id: self
                .host_id
                .as_deref()
                .map(|id| id.parse().map_err(Error::ParseHostId))
                .transpose()?,
            exit_code: self
                .exit_code
                .map(|code| api::CommandExitCode::try_from(code).map_err(|_| Error::ParseExitCode))
                .transpose()?
                .map(|code| code.into_model().ok_or(Error::ParseExitCode))
                .transpose()?,
        })
    }
}

impl api::CommandExitCode {
    const fn into_model(self) -> Option<ExitCode> {
        match self {
            api::CommandExitCode::Unspecified => None,
            api::CommandExitCode::Ok => Some(ExitCode::Ok),
            api::CommandExitCode::InternalError => Some(ExitCode::InternalError),
            api::CommandExitCode::NodeNotFound => Some(ExitCode::NodeNotFound),
            api::CommandExitCode::BlockingJobRunning => Some(ExitCode::BlockingJobRunning),
            api::CommandExitCode::ServiceNotReady => Some(ExitCode::ServiceNotReady),
            api::CommandExitCode::ServiceBroken => Some(ExitCode::ServiceBroken),
            api::CommandExitCode::NotSupported => Some(ExitCode::NotSupported),
        }
    }
}

/// Create a new `api::HostCommand` from a `Command`.
fn host_command(
    command: &Command,
    host_cmd: api::host_command::Command,
) -> Result<api::Command, Error> {
    let exit_code = command
        .exit_code
        .map(|code| api::CommandExitCode::from(code).into());
    let retry_hint_seconds = command
        .retry_hint_seconds
        .map(|hint| hint.try_into().map_err(Error::RetryHint))
        .transpose()?;

    Ok(api::Command {
        id: command.id.to_string(),
        exit_code,
        exit_message: command.exit_message.clone(),
        retry_hint_seconds,
        created_at: Some(NanosUtc::from(command.created_at).into()),
        acked_at: command.acked_at.map(NanosUtc::from).map(Into::into),
        command: Some(api::command::Command::Host(api::HostCommand {
            host_id: command.host_id.to_string(),
            command: Some(host_cmd),
        })),
    })
}

fn host_start(command: &Command) -> Result<api::Command, Error> {
    let host_cmd = api::host_command::Command::Start(api::HostStart {});
    host_command(command, host_cmd)
}

fn host_stop(command: &Command) -> Result<api::Command, Error> {
    let host_cmd = api::host_command::Command::Stop(api::HostStop {});
    host_command(command, host_cmd)
}

fn host_restart(command: &Command) -> Result<api::Command, Error> {
    let host_cmd = api::host_command::Command::Restart(api::HostRestart {});
    host_command(command, host_cmd)
}

pub fn host_pending(command: &Command) -> Result<api::Command, Error> {
    let host_cmd = api::host_command::Command::Pending(api::HostPending {});
    host_command(command, host_cmd)
}

/// Create a new `api::NodeCommand` from a `Command`.
fn node_command(
    command: &Command,
    node_id: NodeId,
    node_cmd: api::node_command::Command,
) -> Result<api::Command, Error> {
    let exit_code = command
        .exit_code
        .map(|code| api::CommandExitCode::from(code).into());
    let retry_hint_seconds = command
        .retry_hint_seconds
        .map(|hint| hint.try_into().map_err(Error::RetryHint))
        .transpose()?;

    Ok(api::Command {
        id: command.id.to_string(),
        exit_code,
        exit_message: command.exit_message.clone(),
        retry_hint_seconds,
        created_at: Some(NanosUtc::from(command.created_at).into()),
        acked_at: command.acked_at.map(NanosUtc::from).map(Into::into),
        command: Some(api::command::Command::Node(api::NodeCommand {
            host_id: command.host_id.to_string(),
            node_id: node_id.to_string(),
            command: Some(node_cmd),
        })),
    })
}

async fn node_create(
    command: &Command,
    authz: &AuthZ,
    conn: &mut Conn<'_>,
) -> Result<api::Command, Error> {
    let node_id = command.node_id.ok_or(Error::MissingNodeId)?;
    let node = Node::by_id(node_id, conn).await?;

    let blockchain = Blockchain::by_id(node.blockchain_id, authz, conn).await?;
    let version =
        BlockchainVersion::find(blockchain.id, node.node_type, &node.version, conn).await?;

    let id_to_names = BlockchainProperty::id_to_name_map(version.id, conn).await?;
    let properties = node
        .properties(conn)
        .await?
        .into_iter()
        .map(|prop| {
            let name = id_to_names
                .get(&prop.blockchain_property_id)
                .ok_or(Error::MissingBlockchainPropertyId)?;

            Ok::<_, Error>(api::Parameter {
                name: name.clone(),
                value: prop.value,
            })
        })
        .collect::<Result<_, _>>()?;

    let node_cmd = api::node_command::Command::Create(api::NodeCreate {
        name: node.name.clone(),
        blockchain: node.blockchain_id.to_string(),
        image: Some(common::ImageIdentifier {
            protocol: blockchain.name,
            node_version: node.version.as_ref().to_lowercase(),
            node_type: common::NodeType::from(node.node_type).into(),
        }),
        node_type: common::NodeType::from(node.node_type).into(),
        ip: node.ip_addr.clone(),
        gateway: node.ip_gateway.clone(),
        properties,
        rules: firewall_rules(&node)?,
        network: node.network,
    });

    node_command(command, node_id, node_cmd)
}

fn node_start(command: &Command) -> Result<api::Command, Error> {
    let node_id = command.node_id.ok_or(Error::MissingNodeId)?;
    let node_cmd = api::node_command::Command::Start(api::NodeStart {});
    node_command(command, node_id, node_cmd)
}

fn node_stop(command: &Command) -> Result<api::Command, Error> {
    let node_id = command.node_id.ok_or(Error::MissingNodeId)?;
    let node_cmd = api::node_command::Command::Stop(api::NodeStop {});
    node_command(command, node_id, node_cmd)
}

fn node_restart(command: &Command) -> Result<api::Command, Error> {
    let node_id = command.node_id.ok_or(Error::MissingNodeId)?;
    let node_cmd = api::node_command::Command::Restart(api::NodeRestart {});
    node_command(command, node_id, node_cmd)
}

async fn node_upgrade(
    command: &Command,
    authz: &AuthZ,
    conn: &mut Conn<'_>,
) -> Result<api::Command, Error> {
    let node_id = command.node_id.ok_or(Error::MissingNodeId)?;
    let node = Node::by_id(node_id, conn).await?;
    let blockchain = Blockchain::by_id(node.blockchain_id, authz, conn).await?;

    let node_cmd = api::node_command::Command::Upgrade(api::NodeUpgrade {
        image: Some(common::ImageIdentifier {
            protocol: blockchain.name,
            node_version: node.version.as_ref().to_lowercase(),
            node_type: common::NodeType::from(node.node_type).into(),
        }),
    });
    node_command(command, node_id, node_cmd)
}

async fn node_update(command: &Command, conn: &mut Conn<'_>) -> Result<api::Command, Error> {
    let node_id = command.node_id.ok_or(Error::MissingNodeId)?;
    let node = Node::by_id(node_id, conn).await?;
    let node_cmd = api::node_command::Command::Update(api::NodeUpdate {
        rules: firewall_rules(&node)?,
    });
    node_command(command, node_id, node_cmd)
}

pub fn node_delete(command: &Command) -> Result<api::Command, Error> {
    let node_id = command.node_id.ok_or(Error::MissingNodeId)?;
    let node_cmd = api::node_command::Command::Delete(api::NodeDelete {});
    node_command(command, node_id, node_cmd)
}

fn firewall_rules(node: &Node) -> Result<Vec<FirewallRule>, Error> {
    let mut rules = vec![];

    // TODO: newtype with cidr checks for FilteredIpAddr
    for ip in node.allow_ips()? {
        if !IpCidr::is_ip_cidr(&ip.ip) {
            return Err(Error::IpNotCidr);
        }

        rules.push(FirewallRule {
            name: format!("allow: {}", ip.ip),
            action: FirewallAction::Allow.into(),
            direction: FirewallDirection::Inbound.into(),
            protocol: Some(FirewallProtocol::Both.into()),
            ips: Some(ip.ip),
            ports: vec![],
        });
    }

    for ip in node.deny_ips()? {
        if !IpCidr::is_ip_cidr(&ip.ip) {
            return Err(Error::IpNotCidr);
        }

        rules.push(FirewallRule {
            name: format!("deny: {}", ip.ip),
            action: FirewallAction::Deny.into(),
            direction: FirewallDirection::Inbound.into(),
            protocol: Some(FirewallProtocol::Both.into()),
            ips: Some(ip.ip),
            ports: vec![],
        });
    }

    Ok(rules)
}

#[cfg(test)]
mod tests {
    use crate::config::Context;

    use super::*;

    #[tokio::test]
    async fn test_create_firewall_rules() {
        let (_ctx, db) = Context::with_mocked().await.unwrap();

        firewall_rules(&db.seed.node).unwrap();
    }
}
