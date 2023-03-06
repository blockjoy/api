use super::blockjoy::Parameter;
use crate::auth::FindableById;
use crate::errors::Result as ApiResult;
use crate::grpc::blockjoy::container_image::StatusName;
use crate::grpc::blockjoy::{
    self, node_command, Command as GrpcCommand, ContainerImage, NodeCommand, NodeCreate,
    NodeDelete, NodeInfoGet, NodeRestart, NodeStop,
};
use crate::grpc::helpers::required;
use crate::models::{Blockchain, Command, HostCmd, Node, NodeTypeKey};
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
    let mut node_cmd = NodeCommand {
        node_id: cmd.resource_id.to_string(),
        host_id: cmd.host_id.to_string(),
        command: None,
        api_command_id: cmd.id.to_string(),
        created_at: None,
    };

    node_cmd.command = match cmd.cmd {
        HostCmd::RestartNode => Some(node_command::Command::Restart(NodeRestart {})),
        HostCmd::KillNode => {
            tracing::debug!("Using NodeStop for KillNode");
            Some(node_command::Command::Stop(NodeStop {}))
        }
        HostCmd::ShutdownNode => Some(node_command::Command::Stop(NodeStop {})),
        HostCmd::UpdateNode => {
            tracing::debug!("Using NodeUpgrade for UpdateNode");

            // TODO: add image
            // Self {
            //     r#type: Some(command::Type::Node(NodeUpgrade {
            //         node_id: node_cmd.node_id.clone(),
            //     })),
            // }

            let node = Node::find_by_id(cmd.resource_id, conn).await?;
            let network = Parameter::new("network", &node.network);
            let node_type = node.node_type()?;
            let cmd = blockjoy::NodeInfoUpdate {
                name: Some(node.name),
                self_update: Some(node.self_update),
                properties: node_type
                    .iter_props()
                    .flat_map(|p| p.value.as_ref().map(|v| (&p.name, v)))
                    .map(|(name, value)| Parameter::new(name, value))
                    .chain([network])
                    .collect(),
            };

            Some(node_command::Command::Update(cmd))
        }
        HostCmd::MigrateNode => {
            tracing::debug!("Using NodeGenericCommand for MigrateNode");
            unimplemented!();
            /*
            node_cmd.command = Some(node_command::Command::Generic(NodeGenericCommand {
                node_id: node_cmd.node_id.clone(),
            }))
             */
        }
        HostCmd::GetNodeVersion => {
            tracing::debug!("Using NodeInfoGet for GetNodeVersion");
            Some(node_command::Command::InfoGet(NodeInfoGet {}))
        }
        // The following should be HostCommands
        HostCmd::CreateNode => {
            let node = dbg!(Node::find_by_id(cmd.resource_id, conn).await?);
            let blockchain = Blockchain::find_by_id(node.blockchain_id, conn).await?;
            let image = ContainerImage {
                protocol: blockchain.name,
                node_type: NodeTypeKey::str_from_value(node.node_type()?.get_id()).to_lowercase(),
                node_version: node.version.as_deref().unwrap_or("latest").to_lowercase(),
                status: StatusName::Development.into(),
            };
            let network = Parameter::new("network", &node.network);
            let node_type = node.node_type()?;
            let create_cmd = NodeCreate {
                name: node.name,
                blockchain: node.blockchain_id.to_string(),
                image: Some(image),
                r#type: node_type.to_json()?,
                ip: node.ip_addr.ok_or_else(required("node.ip_addr"))?,
                gateway: node.ip_gateway,
                self_update: node.self_update,
                properties: node_type
                    .iter_props()
                    .flat_map(|p| p.value.as_ref().map(|v| (&p.name, v)))
                    .map(|(name, value)| Parameter::new(name, value))
                    .chain([network])
                    .collect(),
            };

            Some(node_command::Command::Create(create_cmd))
        }
        HostCmd::DeleteNode => Some(node_command::Command::Delete(NodeDelete {})),
        HostCmd::GetBVSVersion => unimplemented!(),
        HostCmd::UpdateBVS => unimplemented!(),
        HostCmd::RestartBVS => unimplemented!(),
        HostCmd::RemoveBVS => unimplemented!(),
        HostCmd::CreateBVS => unimplemented!(),
        HostCmd::StopBVS => unimplemented!(),
        // TODO: Missing
        // NodeStart, NodeUpgrade
    };

    Ok(GrpcCommand {
        r#type: Some(blockjoy::command::Type::Node(node_cmd)),
    })
}

/// Function to convert the datetimes from the database into the API representation of a timestamp.
pub fn try_dt_to_ts(datetime: chrono::DateTime<chrono::Utc>) -> ApiResult<Timestamp> {
    const NANOS_PER_SEC: i64 = 1_000_000_000;
    let nanos = datetime.timestamp_nanos();
    let timestamp = Timestamp {
        seconds: nanos / NANOS_PER_SEC,
        // This _should_ never fail because 1_000_000_000 fits into an i32, but using `as` was
        // hiding this bug in the first place. Therefore I have left the `try_into` call here.
        nanos: (nanos % NANOS_PER_SEC).try_into()?,
    };
    Ok(timestamp)
}

pub mod from {
    use super::try_dt_to_ts;
    use crate::auth::{JwtToken, UserAuthToken};
    use crate::cookbook::cookbook_grpc::NetworkConfiguration;
    use crate::errors::ApiError;
    use crate::grpc;
    use crate::grpc::blockjoy::Keyfile;
    use crate::grpc::blockjoy_ui::blockchain_network::NetworkType;
    use crate::grpc::blockjoy_ui::BlockchainNetwork;
    use crate::grpc::blockjoy_ui::{
        self, node::NodeStatus as GrpcNodeStatus, node::StakingStatus as GrpcStakingStatus,
        node::SyncStatus as GrpcSyncStatus, Node as GrpcNode,
    };
    use crate::grpc::helpers::required;
    use crate::models::{self, NodeChainStatus, NodeKeyFile, NodeStakingStatus, NodeSyncStatus};
    use anyhow::anyhow;
    use tonic::{Code, Status};

    // impl TryFrom<&blockjoy_ui::User> for UpdateUser<'_> {
    //     type Error = ApiError;

    //     fn try_from(user: &blockjoy_ui::User) -> crate::Result<Self> {
    //         Ok(Self {
    //             id: user.id.as_ref().ok_or_else(required("user.id"))?.parse()?,
    //             first_name: user.first_name.as_deref(),
    //             last_name: user.last_name.as_deref(),
    //             fee_bps: None,
    //             staking_quota: None,
    //             refresh: None,
    //         })
    //     }
    // }

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

    impl TryFrom<models::HostProvision> for blockjoy_ui::HostProvision {
        type Error = ApiError;

        fn try_from(hp: models::HostProvision) -> Result<Self, Self::Error> {
            let install_cmd = hp.install_cmd();
            let hp = Self {
                id: Some(hp.id),
                host_id: hp.host_id.map(|id| id.to_string()),
                created_at: Some(try_dt_to_ts(hp.created_at)?),
                claimed_at: hp.claimed_at.map(try_dt_to_ts).transpose()?,
                install_cmd: Some(install_cmd),
                ip_range_from: hp
                    .ip_range_from
                    .map(|ip| ip.to_string())
                    .ok_or_else(required("host_provision.ip_range_from"))?,
                ip_range_to: hp
                    .ip_range_to
                    .map(|ip| ip.to_string())
                    .ok_or_else(required("host_provision.ip_range_to"))?,
                ip_gateway: hp
                    .ip_gateway
                    .map(|ip| ip.to_string())
                    .ok_or_else(required("host_provision.ip_gateway"))?,
                org_id: None,
            };
            Ok(hp)
        }
    }

    impl TryFrom<models::Host> for blockjoy_ui::Host {
        type Error = ApiError;

        fn try_from(value: models::Host) -> Result<Self, Self::Error> {
            let host = Self {
                id: Some(value.id.to_string()),
                name: Some(value.name),
                version: value.version,
                location: value.location,
                cpu_count: value.cpu_count,
                mem_size: value.mem_size,
                disk_size: value.disk_size,
                os: value.os,
                os_version: value.os_version,
                ip: Some(value.ip_addr),
                status: Some(value.status as i32),
                nodes: vec![],
                created_at: Some(try_dt_to_ts(value.created_at)?),
                ip_range_from: value.ip_range_from.map(|ip| ip.to_string()),
                ip_range_to: value.ip_range_to.map(|ip| ip.to_string()),
                ip_gateway: value.ip_gateway.map(|ip| ip.to_string()),
                org_id: None, // TODO
            };
            Ok(host)
        }
    }

    impl From<ApiError> for Status {
        fn from(e: ApiError) -> Self {
            let msg = format!("{e:?}");

            match e {
                ApiError::ValidationError(_) => Status::invalid_argument(msg),
                ApiError::NotFoundError(_) => Status::not_found(msg),
                ApiError::DuplicateResource { .. } => Status::invalid_argument(msg),
                ApiError::InvalidAuthentication(_) => Status::unauthenticated(msg),
                ApiError::InsufficientPermissionsError => Status::permission_denied(msg),
                ApiError::UuidParseError(_) => Status::invalid_argument(msg),
                ApiError::InvalidArgument(s) => s,
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

    impl TryFrom<models::Node> for GrpcNode {
        type Error = ApiError;

        fn try_from(node: models::Node) -> Result<Self, Self::Error> {
            let node_type = node.node_type()?.to_json()?;
            let res = Self {
                id: Some(node.id.to_string()),
                org_id: Some(node.org_id.to_string()),
                host_id: Some(node.host_id.to_string()),
                host_name: Some(node.host_name),
                blockchain_id: Some(node.blockchain_id.to_string()),
                name: Some(node.name),
                groups: vec![],
                version: node.version,
                ip: node.ip_addr,
                r#type: Some(node_type),
                address: node.address,
                wallet_address: node.wallet_address,
                block_height: node.block_height,
                node_data: None,
                created_at: Some(try_dt_to_ts(node.created_at)?),
                updated_at: Some(try_dt_to_ts(node.updated_at)?),
                status: Some(node.chain_status as i32),
                sync_status: Some(node.sync_status as i32),
                staking_status: node.staking_status.map(|ss| ss as i32),
                ip_gateway: Some(node.ip_gateway),
                self_update: Some(node.self_update),
                network: Some(node.network.clone()),
                blockchain_name: Some(node.network),
            };
            Ok(res)
        }
    }

    impl From<NodeChainStatus> for GrpcNodeStatus {
        fn from(ncs: NodeChainStatus) -> Self {
            match ncs {
                NodeChainStatus::Unknown => GrpcNodeStatus::UndefinedApplicationStatus,
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
                NodeSyncStatus::Unknown => GrpcSyncStatus::UndefinedSyncStatus,
                NodeSyncStatus::Synced => GrpcSyncStatus::Synced,
                NodeSyncStatus::Syncing => GrpcSyncStatus::Syncing,
            }
        }
    }

    impl From<NodeStakingStatus> for GrpcStakingStatus {
        fn from(nss: NodeStakingStatus) -> Self {
            match nss {
                NodeStakingStatus::Unknown => GrpcStakingStatus::UndefinedStakingStatus,
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

        fn try_from(value: BlockchainNetwork) -> Result<Self, Self::Error> {
            Ok(Self {
                name: value.name,
                url: value.url,
                network_type: NetworkType::from_i32(value.net_type)
                    .ok_or_else(|| ApiError::UnexpectedError(anyhow!("Unknown network type")))?,
            })
        }
    }

    impl From<&NetworkConfiguration> for crate::cookbook::BlockchainNetwork {
        fn from(value: &NetworkConfiguration) -> Self {
            Self {
                name: value.name.clone(),
                url: value.url.clone(),
                network_type: NetworkType::from_i32(value.net_type).unwrap_or_default(),
            }
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

    impl TryFrom<&models::Blockchain> for blockjoy_ui::Blockchain {
        type Error = ApiError;

        fn try_from(model: &models::Blockchain) -> Result<Self, Self::Error> {
            let json = &model.supported_node_types()?;
            let json = serde_json::to_string(&json)
                .map_err(|e| anyhow!("Could not serialize supported node types: {e}"))?;

            let blockchain = Self {
                id: Some(model.id.to_string()),
                name: Some(model.name.clone()),
                description: model.description.clone(),
                status: model.status as i32,
                project_url: model.project_url.clone(),
                repo_url: model.repo_url.clone(),
                supports_etl: model.supports_etl,
                supports_node: model.supports_node,
                supports_staking: model.supports_staking,
                supports_broadcast: model.supports_broadcast,
                version: model.version.clone(),
                supported_nodes_types: json,
                created_at: Some(try_dt_to_ts(model.created_at)?),
                updated_at: Some(try_dt_to_ts(model.updated_at)?),
                networks: vec![],
            };
            Ok(blockchain)
        }
    }

    impl TryFrom<models::Blockchain> for blockjoy_ui::Blockchain {
        type Error = ApiError;

        fn try_from(model: models::Blockchain) -> Result<Self, Self::Error> {
            Self::try_from(&model)
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

pub mod into {
    use crate::{
        errors::ApiError,
        grpc::{
            blockjoy::{HostInfo, HostInfoUpdateRequest},
            helpers::required,
        },
    };
    use tonic::Request;

    pub trait IntoData<R, T> {
        type Error;

        fn into_data(self) -> Result<T, Self::Error>;
    }

    impl IntoData<Request<HostInfoUpdateRequest>, (String, HostInfo)>
        for Request<HostInfoUpdateRequest>
    {
        type Error = ApiError;

        fn into_data(self) -> Result<(String, HostInfo), Self::Error> {
            let inner = self.into_inner();
            let id = inner.request_id.unwrap_or_default();
            let info = inner.info.ok_or_else(required("info"))?;

            Ok((id, info))
        }
    }
}
