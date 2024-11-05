mod recover;
mod success;

use cidr::IpCidr;
use diesel_async::scoped_futures::ScopedFutureExt;
use displaydoc::Display;
use thiserror::Error;
use tonic::{Request, Response};
use tracing::{error, warn};

use crate::auth::rbac::{CommandAdminPerm, CommandPerm};
use crate::auth::resource::Resource;
use crate::auth::{AuthZ, Authorize};
use crate::database::{Conn, ReadConn, Transaction, WriteConn};
use crate::grpc::api::command_service_server::CommandService;
use crate::grpc::common::{FirewallAction, FirewallDirection, FirewallProtocol, FirewallRule};
use crate::grpc::{api, common, Grpc, Status};
use crate::model::blockchain::{Blockchain, BlockchainProperty, BlockchainVersion};
use crate::model::command::{CommandFilter, ExitCode, UpdateCommand};
use crate::model::node::{NodeStatus, UpdateNode};
use crate::model::{Command, CommandType, Host, Node};
use crate::util::NanosUtc;

use super::Metadata;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Auth check failed: {0}
    Auth(#[from] crate::auth::Error),
    /// Command blockchain error: {0}
    Blockchain(#[from] crate::model::blockchain::Error),
    /// Command blockchain property error: {0}
    BlockchainProperty(#[from] crate::model::blockchain::property::Error),
    /// Command blockchain version error: {0}
    BlockchainVersion(#[from] crate::model::blockchain::version::Error),
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Command model failure: {0}
    Command(#[from] crate::model::command::Error),
    /// Diesel failure: {0}
    Diesel(#[from] diesel::result::Error),
    /// Error creating a gRPC representation of a node: {0}
    GrpcHost(Box<crate::grpc::node::Error>),
    /// Command host error: {0}
    Host(#[from] crate::model::host::Error),
    /// Missing BlockchainPropertyId. This should not happen.
    MissingBlockchainPropertyId,
    /// Missing `command.node_id`.
    MissingNodeId,
    /// Command node error: {0}
    Node(#[from] crate::model::node::Error),
    /// Not implemented.
    NotImplemented,
    /// Failed to parse ExitCode.
    ParseExitCode,
    /// Failed to parse HostId: {0}
    ParseHostId(uuid::Error),
    /// Failed to parse allowed IP as CIDR: {0}
    ParseIpAllow(cidr::errors::NetworkParseError),
    /// Failed to parse denied IP as CIDR: {0}
    ParseIpDeny(cidr::errors::NetworkParseError),
    /// Failed to parse NodeId: {0}
    ParseNodeId(uuid::Error),
    /// Failed to parse CommandId: {0}
    ParseId(uuid::Error),
    /// Unable to cast retry hint from u64 to i64: {0}
    RetryHint(std::num::TryFromIntError),
    /// Resource error: {0}
    Resource(#[from] crate::auth::resource::Error),
    /// Command success error: {0}
    Success(#[from] self::success::Error),
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
            ParseIpAllow(_) => Status::invalid_argument("allow_ips"),
            ParseIpDeny(_) => Status::invalid_argument("deny_ips"),
            RetryHint(_) => Status::invalid_argument("retry_hint_seconds"),
            Diesel(_) | GrpcHost(_) | MissingBlockchainPropertyId | NotImplemented => {
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
            Success(err) => err.into(),
        }
    }
}

#[tonic::async_trait]
impl CommandService for Grpc {
    async fn list(
        &self,
        req: Request<api::CommandServiceListRequest>,
    ) -> Result<Response<api::CommandServiceListResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| list(req, meta.into(), read).scope_boxed())
            .await
    }

    async fn update(
        &self,
        req: Request<api::CommandServiceUpdateRequest>,
    ) -> Result<Response<api::CommandServiceUpdateResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| update(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn ack(
        &self,
        req: Request<api::CommandServiceAckRequest>,
    ) -> Result<Response<api::CommandServiceAckResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.write(|write| ack(req, meta.into(), write).scope_boxed())
            .await
    }

    async fn pending(
        &self,
        req: Request<api::CommandServicePendingRequest>,
    ) -> Result<Response<api::CommandServicePendingResponse>, tonic::Status> {
        let (meta, _, req) = req.into_parts();
        self.read(|read| pending(req, meta.into(), read).scope_boxed())
            .await
    }
}

async fn list(
    req: api::CommandServiceListRequest,
    meta: Metadata,
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
    meta: Metadata,
    mut write: WriteConn<'_, '_>,
) -> Result<api::CommandServiceUpdateResponse, Error> {
    let id = req.id.parse().map_err(Error::ParseId)?;
    let command = Command::by_id(id, &mut write).await?;

    let mut resources = vec![Resource::from(command.host_id)];
    if let Some(node_id) = command.node_id {
        resources.push(Resource::from(node_id));
    }
    let authz = write.auth(&meta, CommandPerm::Update, &resources).await?;

    let updated = UpdateCommand::from_request(req)?.update(&mut write).await?;
    let cmd = api::Command::from_model(&updated, &authz, &mut write).await?;
    write.mqtt(cmd.clone());

    match updated.exit_code {
        Some(ExitCode::Ok) => success::register(&updated, &authz, &mut write).await?,
        Some(_) => recover::recover(&updated, &authz, &mut write)
            .await
            .unwrap_or_default()
            .into_iter()
            .for_each(|cmd| write.mqtt(cmd)),
        None => (),
    };

    Ok(api::CommandServiceUpdateResponse { command: Some(cmd) })
}

async fn ack(
    req: api::CommandServiceAckRequest,
    meta: Metadata,
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
    meta: Metadata,
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
    let node = node.update(&update, write).await?;

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
        let host = Host::by_id(model.host_id, conn).await?;
        match model.command_type {
            CommandType::HostStart => host_start(model, host),
            CommandType::HostStop => host_stop(model, host),
            CommandType::HostRestart => host_restart(model, host),
            CommandType::HostPending => host_pending(model, host),
            CommandType::NodeCreate => {
                let node = Node::by_id(model.node_id.ok_or(Error::MissingNodeId)?, conn).await?;
                node_create(model, node, host, authz, conn).await
            }
            CommandType::NodeStart => {
                let node = Node::by_id(model.node_id.ok_or(Error::MissingNodeId)?, conn).await?;
                node_start(model, node, host)
            }
            CommandType::NodeStop => {
                let node = Node::by_id(model.node_id.ok_or(Error::MissingNodeId)?, conn).await?;
                node_stop(model, node, host)
            }
            CommandType::NodeRestart => {
                let node = Node::by_id(model.node_id.ok_or(Error::MissingNodeId)?, conn).await?;
                node_restart(model, node, host)
            }
            CommandType::NodeUpgrade => {
                let node = Node::by_id(model.node_id.ok_or(Error::MissingNodeId)?, conn).await?;
                node_upgrade(model, node, host, authz, conn).await
            }
            CommandType::NodeUpdate => {
                let node = Node::by_id(model.node_id.ok_or(Error::MissingNodeId)?, conn).await?;
                node_update(model, node, host)
            }
            CommandType::NodeDelete => {
                let node = Node::by_id(model.node_id.ok_or(Error::MissingNodeId)?, conn).await?;
                node_delete(model, node, host)
            }
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
    host: Host,
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
            host_name: host.name,
            command: Some(host_cmd),
        })),
    })
}

fn host_start(command: &Command, host: Host) -> Result<api::Command, Error> {
    let host_cmd = api::host_command::Command::Start(api::HostStart {});
    host_command(command, host_cmd, host)
}

fn host_stop(command: &Command, host: Host) -> Result<api::Command, Error> {
    let host_cmd = api::host_command::Command::Stop(api::HostStop {});
    host_command(command, host_cmd, host)
}

fn host_restart(command: &Command, host: Host) -> Result<api::Command, Error> {
    let host_cmd = api::host_command::Command::Restart(api::HostRestart {});
    host_command(command, host_cmd, host)
}

pub fn host_pending(command: &Command, host: Host) -> Result<api::Command, Error> {
    let host_cmd = api::host_command::Command::Pending(api::HostPending {});
    host_command(command, host_cmd, host)
}

/// Create a new `api::NodeCommand` from a `Command`.
fn node_command(
    command: &Command,
    node: Node,
    host: Host,
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
            host_id: host.id.to_string(),
            host_name: host.name,
            node_id: node.id.to_string(),
            node_name: node.node_name,
            command: Some(node_cmd),
        })),
    })
}

async fn node_create(
    command: &Command,
    node: Node,
    host: Host,
    authz: &AuthZ,
    conn: &mut Conn<'_>,
) -> Result<api::Command, Error> {
    let blockchain = Blockchain::by_id(node.blockchain_id, authz, conn).await?;
    let version =
        BlockchainVersion::find(blockchain.id, node.node_type, &node.version, conn).await?;

    let id_to_names = BlockchainProperty::id_to_names(version.id, conn).await?;
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
        node_name: node.node_name.clone(),
        dns_name: node.dns_name.clone(),
        org_id: node.org_id.to_string(),
        blockchain: node.blockchain_id.to_string(),
        image: Some(common::ImageIdentifier {
            protocol: blockchain.name,
            node_version: node.version.as_ref().to_lowercase(),
            node_type: common::NodeType::from(node.node_type).into(),
        }),
        node_type: common::NodeType::from(node.node_type).into(),
        ip: node.ip.ip().to_string(),
        gateway: node.ip_gateway.clone(),
        properties,
        rules: firewall_rules(&node)?,
        network: node.network.clone(),
    });

    node_command(command, node, host, node_cmd)
}

fn node_start(command: &Command, node: Node, host: Host) -> Result<api::Command, Error> {
    let node_cmd = api::node_command::Command::Start(api::NodeStart {});
    node_command(command, node, host, node_cmd)
}

fn node_stop(command: &Command, node: Node, host: Host) -> Result<api::Command, Error> {
    let node_cmd = api::node_command::Command::Stop(api::NodeStop {});
    node_command(command, node, host, node_cmd)
}

fn node_restart(command: &Command, node: Node, host: Host) -> Result<api::Command, Error> {
    let node_cmd = api::node_command::Command::Restart(api::NodeRestart {});
    node_command(command, node, host, node_cmd)
}

async fn node_upgrade(
    command: &Command,
    node: Node,
    host: Host,
    authz: &AuthZ,
    conn: &mut Conn<'_>,
) -> Result<api::Command, Error> {
    let blockchain = Blockchain::by_id(node.blockchain_id, authz, conn).await?;
    let node_cmd = api::node_command::Command::Upgrade(api::NodeUpgrade {
        image: Some(common::ImageIdentifier {
            protocol: blockchain.name,
            node_version: node.version.as_ref().to_lowercase(),
            node_type: common::NodeType::from(node.node_type).into(),
        }),
    });
    node_command(command, node, host, node_cmd)
}

pub fn node_update(command: &Command, node: Node, host: Host) -> Result<api::Command, Error> {
    let node_cmd = api::node_command::Command::Update(api::NodeUpdate {
        rules: firewall_rules(&node)?,
        org_id: node.org_id.to_string(),
    });
    node_command(command, node, host, node_cmd)
}

pub fn node_delete(command: &Command, node: Node, host: Host) -> Result<api::Command, Error> {
    let node_cmd = api::node_command::Command::Delete(api::NodeDelete {});
    node_command(command, node, host, node_cmd)
}

fn firewall_rules(node: &Node) -> Result<Vec<FirewallRule>, Error> {
    let mut rules = vec![];

    // TODO: newtype with cidr checks for FilteredIpAddr
    for ip in node.allow_ips()? {
        let _cidr: IpCidr = ip.ip.parse().map_err(Error::ParseIpAllow)?;

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
        let _cidr: IpCidr = ip.ip.parse().map_err(Error::ParseIpDeny)?;

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
