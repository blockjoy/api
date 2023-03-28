use super::blockjoy::Parameter;
use crate::auth::FindableById;
use crate::errors::{ApiError, Result as ApiResult};
use crate::grpc::blockjoy::container_image::StatusName;
use crate::grpc::blockjoy::{
    self, node_command, Command as GrpcCommand, ContainerImage, NodeCommand, NodeCreate,
    NodeDelete, NodeRestart, NodeStop,
};
use crate::grpc::helpers::required;
use crate::models::{self, Blockchain, Command, HostCmd, Node};
use anyhow::anyhow;
use diesel_async::AsyncPgConnection;
use prost_types::Timestamp;

impl Parameter {
    fn new(name: &str, val: &str) -> Self {
        Self {
            name: name.to_owned(),
            value: val.to_owned(),
        }
    }
}

pub async fn db_command_to_grpc_command(
    cmd: &Command,
    conn: &mut AsyncPgConnection,
) -> ApiResult<GrpcCommand> {
    use blockjoy::command::Type;
    use node_command::Command;

    // Closure to conveniently construct a NodeCommand from the data that we need to have.
    let node_cmd = |command, node_id| {
        Ok(GrpcCommand {
            r#type: Some(Type::Node(NodeCommand {
                node_id,
                host_id: cmd.host_id.to_string(),
                command: Some(command),
                api_command_id: cmd.id.to_string(),
                created_at: Some(try_dt_to_ts(cmd.created_at)?),
            })),
        })
    };

    match cmd.cmd {
        HostCmd::RestartNode => {
            let node_id = cmd.node_id.ok_or_else(required("command.node_id"))?;
            let cmd = Command::Restart(NodeRestart {});
            node_cmd(cmd, node_id.to_string())
        }
        HostCmd::KillNode => {
            tracing::debug!("Using NodeStop for KillNode");
            let node_id = cmd.node_id.ok_or_else(required("command.node_id"))?;
            let cmd = Command::Stop(NodeStop {});
            node_cmd(cmd, node_id.to_string())
        }
        HostCmd::ShutdownNode => {
            let node_id = cmd.node_id.ok_or_else(required("command.node_id"))?;
            let cmd = Command::Stop(NodeStop {});
            node_cmd(cmd, node_id.to_string())
        }
        HostCmd::UpdateNode => {
            tracing::debug!("Using NodeUpgrade for UpdateNode");

            let node_id = cmd.node_id.ok_or_else(required("command.node_id"))?;
            let node = Node::find_by_id(node_id, conn).await?;
            let cmd = Command::Update(blockjoy::NodeUpdate {
                self_update: Some(node.self_update),
            });

            node_cmd(cmd, node_id.to_string())
        }
        HostCmd::MigrateNode => {
            tracing::error!("Using NodeGenericCommand for MigrateNode");
            Err(ApiError::UnexpectedError(anyhow!("Not implemented")))
        }
        HostCmd::GetNodeVersion => {
            tracing::debug!("Using NodeInfoGet for GetNodeVersion");
            let node_id = cmd.node_id.ok_or_else(required("command.node_id"))?;
            let cmd = Command::InfoGet(blockjoy::NodeGet {});
            node_cmd(cmd, node_id.to_string())
        }
        // The following should be HostCommands
        HostCmd::CreateNode => {
            let node_id = cmd.node_id.ok_or_else(required("command.node_id"))?;
            let node = Node::find_by_id(node_id, conn).await?;
            let blockchain = Blockchain::find_by_id(node.blockchain_id, conn).await?;
            let image = ContainerImage {
                protocol: blockchain.name,
                node_type: node.node_type.to_string().to_lowercase(),
                node_version: node.version.as_deref().unwrap_or("latest").to_lowercase(),
                status: StatusName::Development.into(),
            };
            let network = Parameter::new("network", &node.network);
            let r#type = models::NodePropertiesWithId {
                id: node.node_type.into(),
                props: node.properties()?,
            };
            let properties = node
                .properties()?
                .iter_props()
                .flat_map(|p| p.value.as_ref().map(|v| (&p.name, v)))
                .map(|(name, value)| Parameter::new(name, value))
                .chain([network])
                .collect();
            let cmd = Command::Create(NodeCreate {
                name: node.name,
                blockchain: node.blockchain_id.to_string(),
                image: Some(image),
                r#type: serde_json::to_string(&r#type)?,
                ip: node.ip_addr,
                gateway: node.ip_gateway,
                self_update: node.self_update,
                properties,
            });

            node_cmd(cmd, node_id.to_string())
        }
        HostCmd::DeleteNode => {
            let node_id = cmd
                .sub_cmd
                .clone()
                .ok_or_else(required("command.node_id"))?;
            let cmd = Command::Delete(NodeDelete {});
            node_cmd(cmd, node_id)
        }
        HostCmd::GetBVSVersion => Err(ApiError::UnexpectedError(anyhow!("Not implemented"))),
        HostCmd::UpdateBVS => Err(ApiError::UnexpectedError(anyhow!("Not implemented"))),
        HostCmd::RestartBVS => Err(ApiError::UnexpectedError(anyhow!("Not implemented"))),
        HostCmd::RemoveBVS => Err(ApiError::UnexpectedError(anyhow!("Not implemented"))),
        HostCmd::CreateBVS => Err(ApiError::UnexpectedError(anyhow!("Not implemented"))),
        HostCmd::StopBVS => Err(ApiError::UnexpectedError(anyhow!("Not implemented"))),
        // TODO: Missing
        // NodeStart, NodeUpgrade
    }
}

/// Function to convert the datetimes from the database into the API representation of a timestamp.
pub fn try_dt_to_ts(datetime: chrono::DateTime<chrono::Utc>) -> ApiResult<Timestamp> {
    const NANOS_PER_SEC: i64 = 1_000_000_000;
    let nanos = datetime.timestamp_nanos();
    let timestamp = Timestamp {
        seconds: nanos / NANOS_PER_SEC,
        // This _should_ never fail because 1_000_000_000 fits into an i32, but using `as` was
        // hiding a bug here at first, therefore I have left the `try_into` call here.
        nanos: (nanos % NANOS_PER_SEC).try_into()?,
    };
    Ok(timestamp)
}

pub mod from {
    use crate::auth::{JwtToken, UserAuthToken};
    use crate::cookbook::cookbook_grpc::NetworkConfiguration;
    use crate::errors::ApiError;
    use crate::grpc;
    use crate::grpc::blockjoy::Keyfile;
    use crate::grpc::blockjoy_ui::blockchain_network::NetworkType;
    use crate::grpc::blockjoy_ui::BlockchainNetwork;
    use crate::grpc::blockjoy_ui::{
        node::NodeStatus as GrpcNodeStatus, node::StakingStatus as GrpcStakingStatus,
        node::SyncStatus as GrpcSyncStatus,
    };
    use crate::models::{self, NodeChainStatus, NodeKeyFile, NodeStakingStatus, NodeSyncStatus};
    use anyhow::anyhow;
    use tonic::{Code, Status};

    impl TryFrom<&UserAuthToken> for grpc::blockjoy_ui::ApiToken {
        type Error = ApiError;

        fn try_from(value: &UserAuthToken) -> Result<Self, Self::Error> {
            Ok(Self {
                value: value.encode()?,
            })
        }
    }

    impl TryFrom<UserAuthToken> for grpc::blockjoy_ui::ApiToken {
        type Error = ApiError;

        fn try_from(value: UserAuthToken) -> Result<Self, Self::Error> {
            Self::try_from(&value)
        }
    }

    impl From<ApiError> for Status {
        fn from(e: ApiError) -> Self {
            use ApiError::*;

            let msg = format!("{e:?}");

            match e {
                ValidationError(_) => Status::invalid_argument(msg),
                NotFoundError(_) => Status::not_found(msg),
                DuplicateResource { .. } => Status::invalid_argument(msg),
                InvalidAuthentication(_) => Status::unauthenticated(msg),
                InsufficientPermissionsError => Status::permission_denied(msg),
                UuidParseError(_) | IpParseError(_) => Status::invalid_argument(msg),
                InvalidArgument(s) => s,
                _ => Status::internal(msg),
            }
        }
    }

    impl From<Status> for ApiError {
        fn from(status: Status) -> Self {
            let e = anyhow!(format!("{status:?}"));

            match status.code() {
                Code::Unauthenticated => ApiError::InvalidAuthentication(e.to_string()),
                Code::PermissionDenied => ApiError::InsufficientPermissionsError,
                Code::InvalidArgument => ApiError::InvalidArgument(status),
                _ => ApiError::UnexpectedError(e),
            }
        }
    }

    impl From<models::OrgUser> for grpc::blockjoy_ui::OrgUser {
        fn from(value: models::OrgUser) -> Self {
            Self {
                user_id: value.user_id.to_string(),
                org_id: value.org_id.to_string(),
                role: value.role as i32,
            }
        }
    }

    impl From<NodeChainStatus> for GrpcNodeStatus {
        fn from(ncs: NodeChainStatus) -> Self {
            match ncs {
                NodeChainStatus::Unknown => GrpcNodeStatus::Unspecified,
                NodeChainStatus::Provisioning => GrpcNodeStatus::Provisioning,
                NodeChainStatus::Broadcasting => GrpcNodeStatus::Broadcasting,
                NodeChainStatus::Cancelled => GrpcNodeStatus::Cancelled,
                NodeChainStatus::Delegating => GrpcNodeStatus::Delegating,
                NodeChainStatus::Delinquent => GrpcNodeStatus::Delinquent,
                NodeChainStatus::Disabled => GrpcNodeStatus::Disabled,
                NodeChainStatus::Earning => GrpcNodeStatus::Earning,
                NodeChainStatus::Elected => GrpcNodeStatus::Elected,
                NodeChainStatus::Electing => GrpcNodeStatus::Electing,
                NodeChainStatus::Exported => GrpcNodeStatus::Exported,
                NodeChainStatus::Ingesting => GrpcNodeStatus::Ingesting,
                NodeChainStatus::Mining => GrpcNodeStatus::Mining,
                NodeChainStatus::Minting => GrpcNodeStatus::Minting,
                NodeChainStatus::Processing => GrpcNodeStatus::Processing,
                NodeChainStatus::Relaying => GrpcNodeStatus::Relaying,
                NodeChainStatus::Removed => GrpcNodeStatus::Removed,
                NodeChainStatus::Removing => GrpcNodeStatus::Removing,
            }
        }
    }

    impl From<NodeSyncStatus> for GrpcSyncStatus {
        fn from(nss: NodeSyncStatus) -> Self {
            match nss {
                NodeSyncStatus::Unknown => GrpcSyncStatus::Unspecified,
                NodeSyncStatus::Synced => GrpcSyncStatus::Synced,
                NodeSyncStatus::Syncing => GrpcSyncStatus::Syncing,
            }
        }
    }

    impl From<NodeStakingStatus> for GrpcStakingStatus {
        fn from(nss: NodeStakingStatus) -> Self {
            match nss {
                NodeStakingStatus::Unknown => GrpcStakingStatus::Unspecified,
                NodeStakingStatus::Staked => GrpcStakingStatus::Staked,
                NodeStakingStatus::Staking => GrpcStakingStatus::Staking,
                NodeStakingStatus::Validating => GrpcStakingStatus::Validating,
                NodeStakingStatus::Follower => GrpcStakingStatus::Follower,
                NodeStakingStatus::Consensus => GrpcStakingStatus::Consensus,
                NodeStakingStatus::Unstaked => GrpcStakingStatus::Unstaked,
            }
        }
    }

    impl TryFrom<BlockchainNetwork> for crate::cookbook::BlockchainNetwork {
        type Error = ApiError;

        fn try_from(value: BlockchainNetwork) -> crate::Result<Self> {
            Ok(Self {
                name: value.name,
                url: value.url,
                network_type: NetworkType::from_i32(value.net_type)
                    .ok_or_else(|| anyhow!("Unknown network type: {}", value.net_type))?,
            })
        }
    }

    impl TryFrom<&NetworkConfiguration> for crate::cookbook::BlockchainNetwork {
        type Error = ApiError;

        fn try_from(value: &NetworkConfiguration) -> crate::Result<Self> {
            Ok(Self {
                name: value.name.clone(),
                url: value.url.clone(),
                network_type: NetworkType::from_i32(value.net_type)
                    .ok_or_else(|| anyhow!("Unknown network type: {}", value.net_type))?,
            })
        }
    }

    impl From<&crate::cookbook::BlockchainNetwork> for BlockchainNetwork {
        fn from(value: &crate::cookbook::BlockchainNetwork) -> Self {
            Self {
                name: value.name.clone(),
                url: value.url.clone(),
                net_type: value.network_type.into(),
            }
        }
    }

    impl TryFrom<NodeKeyFile> for Keyfile {
        type Error = ApiError;

        fn try_from(value: NodeKeyFile) -> Result<Self, Self::Error> {
            Ok(Self {
                name: value.name.clone(),
                content: value.content.into_bytes(),
            })
        }
    }
}
