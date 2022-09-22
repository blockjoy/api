use crate::errors::Result as ApiResult;
use crate::grpc::blockjoy::{
    command, node_command, Command as GrpcCommand, CommandMeta, NodeCommand, NodeCreate,
    NodeDelete, NodeInfoGet, NodeRestart, NodeStop, Uuid as GrpcUuid,
};
use crate::grpc::helpers::pb_current_timestamp;
use crate::models::{Command, HostCmd, Node};
use crate::server::DbPool;

pub async fn db_command_to_grpc_command(cmd: Command, db: DbPool) -> ApiResult<GrpcCommand> {
    let meta = Some(CommandMeta {
        api_command_id: Some(GrpcUuid::from(cmd.id)),
        created_at: Some(pb_current_timestamp()),
    });
    let mut node_cmd = NodeCommand {
        id: Some(GrpcUuid::from(cmd.resource_id)),
        command: None,
        meta,
    };

    node_cmd.command = match cmd.cmd {
        HostCmd::RestartNode => Some(node_command::Command::Restart(NodeRestart::default())),
        HostCmd::KillNode => {
            tracing::debug!("Using NodeStop for KillNode");
            Some(node_command::Command::Stop(NodeStop::default()))
        }
        HostCmd::ShutdownNode => Some(node_command::Command::Stop(NodeStop::default())),
        HostCmd::UpdateNode => {
            tracing::debug!("Using NodeUpgrade for UpdateNode");
            unimplemented!();
            /*
            // TODO: add image
            Self {
                r#type: Some(command::Type::Node(NodeUpgrade {})),
            }
             */
        }
        HostCmd::MigrateNode => {
            tracing::debug!("Using NodeGenericCommand for MigrateNode");
            unimplemented!();
            /*
            node_cmd.command = Some(node_command::Command::Generic(NodeGenericCommand::default()))
             */
        }
        HostCmd::GetNodeVersion => {
            tracing::debug!("Using NodeInfoGet for GetNodeVersion");
            Some(node_command::Command::InfoGet(NodeInfoGet::default()))
        }
        // The following should be HostCommands
        HostCmd::CreateNode => {
            let node = Node::find_by_id(&cmd.resource_id, &db).await?;
            let create_cmd = NodeCreate {
                name: node.name.unwrap_or_default(),
                blockchain: node.blockchain_id.to_string(),
                image: None,
                r#type: node.node_type.to_json(),
            };

            Some(node_command::Command::Create(create_cmd))
        }
        HostCmd::DeleteNode => Some(node_command::Command::Delete(NodeDelete::default())),
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
        r#type: Some(command::Type::Node(node_cmd)),
    })
}

pub mod from {
    use crate::errors::ApiError;
    use crate::grpc::blockjoy::{HostInfo, Uuid as GrpcUuid};
    use crate::grpc::blockjoy_ui::{
        self, node::NodeStatus as GrpcNodeStatus, Host as GrpcHost,
        HostProvision as GrpcHostProvision, Node as GrpcNode, Organization, User as GrpcUiUser,
        Uuid as GrpcUiUuid,
    };
    use crate::models::{
        self, ConnectionStatus, ContainerStatus, HostProvision, HostRequest, Node, NodeChainStatus,
        NodeCreateRequest, NodeInfo, NodeStakingStatus, NodeSyncStatus, NodeType, Org, User,
    };
    use crate::models::{Host, HostSelectiveUpdate};
    use anyhow::anyhow;
    use chrono::{DateTime, Utc};
    use prost_types::Timestamp;
    use serde_json::Value;
    use std::i64;
    use std::str::FromStr;
    use tonic::{Code, Status};
    use uuid::Uuid;

    impl FromStr for GrpcUuid {
        type Err = ApiError;

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            // Assuming 's' is some UUID
            match Uuid::parse_str(s) {
                Ok(_) => Ok(Self { value: s.into() }),
                Err(e) => Err(ApiError::UnexpectedError(anyhow!(e))),
            }
        }
    }

    impl From<HostSelectiveUpdate> for HostInfo {
        fn from(update: HostSelectiveUpdate) -> Self {
            Self {
                id: None,
                name: update.name,
                version: update.version,
                location: update.location,
                cpu_count: update.cpu_count,
                mem_size: update.mem_size,
                disk_size: update.disk_size,
                os: update.os,
                os_version: update.os_version,
                ip: None,
            }
        }
    }

    impl From<GrpcHost> for HostSelectiveUpdate {
        fn from(host: GrpcHost) -> Self {
            Self {
                org_id: host.org_id.map(Uuid::from),
                name: host.name.map(String::from),
                version: host.version.map(String::from),
                location: host.location.map(String::from),
                cpu_count: host.cpu_count.map(i64::from),
                mem_size: host.mem_size.map(i64::from),
                disk_size: host.disk_size.map(i64::from),
                os: host.os.map(String::from),
                os_version: host.os_version.map(String::from),
                ip_addr: host.ip.map(String::from),
                val_ip_addrs: None,
                status: None,
            }
        }
    }

    impl From<HostProvision> for GrpcHostProvision {
        fn from(hp: HostProvision) -> Self {
            Self {
                id: Some(hp.id),
                org_id: Some(GrpcUiUuid::from(hp.org_id)),
                host_id: hp.host_id.map(GrpcUiUuid::from),
                created_at: Some(Timestamp {
                    seconds: hp.created_at.timestamp(),
                    nanos: hp.created_at.timestamp_nanos() as i32,
                }),
                claimed_at: hp.claimed_at.map(|ts| Timestamp {
                    seconds: ts.timestamp(),
                    nanos: ts.timestamp_nanos() as i32,
                }),
                install_cmd: hp.install_cmd.map(String::from),
            }
        }
    }

    impl From<GrpcHost> for HostRequest {
        fn from(host: GrpcHost) -> Self {
            Self {
                org_id: host.org_id.map(Uuid::from),
                name: host.name.map(String::from).unwrap(),
                version: host.version.map(String::from),
                location: host.location.map(String::from),
                cpu_count: host.cpu_count.map(i64::from),
                mem_size: host.mem_size.map(i64::from),
                disk_size: host.disk_size.map(i64::from),
                os: host.os.map(String::from),
                os_version: host.os_version.map(String::from),
                ip_addr: host.ip.map(String::from).unwrap(),
                val_ip_addrs: None,
                status: ConnectionStatus::Online,
            }
        }
    }

    impl From<GrpcUuid> for Uuid {
        fn from(id: GrpcUuid) -> Self {
            Uuid::parse_str(id.value.as_str()).unwrap()
        }
    }

    impl From<Uuid> for GrpcUiUuid {
        fn from(id: Uuid) -> Self {
            Self {
                value: id.to_string(),
            }
        }
    }

    impl From<Uuid> for GrpcUuid {
        fn from(id: Uuid) -> Self {
            Self {
                value: id.to_string(),
            }
        }
    }

    impl From<GrpcUiUuid> for Uuid {
        fn from(id: GrpcUiUuid) -> Self {
            Uuid::parse_str(id.value.as_str()).unwrap()
        }
    }

    impl From<&User> for GrpcUiUser {
        fn from(user: &User) -> Self {
            Self {
                id: Some(GrpcUiUuid::from(user.id)),
                email: Some(user.email.clone()),
                first_name: Some(user.first_name.clone()),
                last_name: Some(user.last_name.clone()),
                created_at: Some(Timestamp {
                    seconds: user.created_at.timestamp(),
                    nanos: user.created_at.timestamp_nanos() as i32,
                }),
                updated_at: None,
            }
        }
    }

    impl From<Option<String>> for GrpcUuid {
        fn from(id: Option<String>) -> Self {
            Self { value: id.unwrap() }
        }
    }

    impl From<ApiError> for Status {
        fn from(e: ApiError) -> Self {
            let msg = format!("{:?}", e);

            match e {
                ApiError::ValidationError(_) => Status::invalid_argument(msg),
                ApiError::NotFoundError(_) => Status::not_found(msg),
                ApiError::DuplicateResource => Status::invalid_argument(msg),
                ApiError::InvalidAuthentication(_) => Status::unauthenticated(msg),
                ApiError::InsufficientPermissionsError => Status::permission_denied(msg),
                _ => Status::internal(msg),
            }
        }
    }

    impl From<Status> for ApiError {
        fn from(status: Status) -> Self {
            let e = anyhow!(format!("{:?}", status));

            match status.code() {
                Code::Unauthenticated => ApiError::InvalidAuthentication(e),
                Code::PermissionDenied => ApiError::InsufficientPermissionsError,
                _ => ApiError::UnexpectedError(e),
            }
        }
    }

    impl From<User> for GrpcUiUser {
        fn from(user: User) -> Self {
            Self {
                id: Some(GrpcUiUuid::from(user.id)),
                email: Some(user.email),
                first_name: Some(user.first_name),
                last_name: Some(user.last_name),
                created_at: Some(Timestamp {
                    seconds: user.created_at.timestamp(),
                    nanos: user.created_at.timestamp_nanos() as i32,
                }),
                updated_at: None,
            }
        }
    }

    impl From<Org> for Organization {
        fn from(org: Org) -> Self {
            Organization::from(&org)
        }
    }

    impl From<&Org> for Organization {
        fn from(org: &Org) -> Self {
            Self {
                id: Some(GrpcUiUuid::from(org.id)),
                name: Some(org.name.clone()),
                personal: Some(org.is_personal),
                member_count: org.member_count,
                created_at: Some(Timestamp {
                    seconds: org.created_at.timestamp(),
                    nanos: org.created_at.timestamp_nanos() as i32,
                }),
                updated_at: Some(Timestamp {
                    seconds: org.updated_at.timestamp(),
                    nanos: org.updated_at.timestamp_nanos() as i32,
                }),
            }
        }
    }

    impl From<Host> for GrpcHost {
        fn from(host: Host) -> Self {
            GrpcHost::from(&host)
        }
    }

    impl From<&Host> for GrpcHost {
        fn from(host: &Host) -> Self {
            Self {
                id: Some(GrpcUiUuid::from(host.id)),
                org_id: host.org_id.map(GrpcUiUuid::from),
                name: Some(host.name.clone()),
                version: host.version.clone().map(String::from),
                location: host.location.clone().map(String::from),
                cpu_count: host.cpu_count.map(i64::from),
                mem_size: host.mem_size.map(i64::from),
                disk_size: host.disk_size.map(i64::from),
                os: host.os.clone().map(String::from),
                os_version: host.os_version.clone().map(String::from),
                ip: Some(host.ip_addr.clone()),
                status: None,
                nodes: vec![],
                created_at: Some(Timestamp {
                    seconds: host.created_at.timestamp(),
                    nanos: host.created_at.timestamp_nanos() as i32,
                }),
            }
        }
    }

    impl From<Node> for GrpcNode {
        fn from(node: Node) -> Self {
            Self::from(&node)
        }
    }

    impl From<&Node> for GrpcNode {
        fn from(node: &Node) -> Self {
            Self {
                id: Some(GrpcUiUuid::from(node.id)),
                org_id: Some(GrpcUiUuid::from(node.org_id)),
                host_id: Some(GrpcUiUuid::from(node.host_id)),
                blockchain_id: Some(GrpcUiUuid::from(node.blockchain_id)),
                name: node.name.clone().map(String::from),
                // TODO: get node groups
                groups: vec![],
                version: node.version.clone().map(String::from),
                ip: node.ip_addr.clone().map(String::from),
                r#type: Some(node.node_type.to_json()),
                address: node.address.clone().map(String::from),
                wallet_address: node.wallet_address.clone().map(String::from),
                block_height: node.block_height.map(i64::from),
                // TODO: Get node data
                node_data: None,
                created_at: Some(Timestamp {
                    seconds: node.created_at.timestamp(),
                    nanos: node.created_at.timestamp_nanos() as i32,
                }),
                updated_at: Some(Timestamp {
                    seconds: node.updated_at.timestamp(),
                    nanos: node.updated_at.timestamp_nanos() as i32,
                }),
                status: Some(GrpcNodeStatus::from(node.chain_status) as i32),
            }
        }
    }

    impl From<&NodeCreateRequest> for GrpcNode {
        fn from(req: &NodeCreateRequest) -> Self {
            Self {
                id: None,
                org_id: Some(GrpcUiUuid::from(req.org_id)),
                host_id: Some(GrpcUiUuid::from(req.host_id)),
                blockchain_id: Some(GrpcUiUuid::from(req.blockchain_id)),
                name: req.name.clone().map(String::from),
                // TODO
                groups: vec![],
                version: req.version.clone().map(String::from),
                ip: req.ip_addr.clone().map(String::from),
                // TODO
                r#type: None,
                address: req.address.clone().map(String::from),
                wallet_address: req.wallet_address.clone().map(String::from),
                block_height: req.block_height.map(i64::from),
                node_data: None,
                created_at: None,
                updated_at: None,
                status: Some(GrpcNodeStatus::from(req.chain_status) as i32),
            }
        }
    }

    impl From<NodeCreateRequest> for GrpcNode {
        fn from(req: NodeCreateRequest) -> Self {
            Self::from(&req)
        }
    }

    impl From<GrpcNode> for NodeCreateRequest {
        fn from(node: GrpcNode) -> Self {
            Self {
                org_id: node.org_id.map(Uuid::from).unwrap_or_default(),
                host_id: node.host_id.map(Uuid::from).unwrap_or_default(),
                name: node.name.map(String::from),
                groups: Some(node.groups.join(",")),
                version: node.version.map(String::from),
                ip_addr: node.ip.map(String::from),
                blockchain_id: node.blockchain_id.map(Uuid::from).unwrap_or_default(),
                node_type: sqlx::types::Json(NodeType::from(node.r#type.unwrap_or_default())),
                address: node.address.map(String::from),
                wallet_address: node.wallet_address.map(String::from),
                block_height: node.block_height.map(i64::from),
                node_data: node.node_data.map(Value::from),
                chain_status: NodeChainStatus::from(node.status.unwrap_or_default()),
                sync_status: NodeSyncStatus::Unknown,
                staking_status: Some(NodeStakingStatus::Unknown),
                container_status: ContainerStatus::Unknown,
            }
        }
    }

    impl From<GrpcNode> for NodeInfo {
        fn from(node: GrpcNode) -> Self {
            Self {
                version: node.version,
                ip_addr: node.ip,
                block_height: node.block_height,
                node_data: node.node_data.map(Value::from),
                chain_status: node.status.map(NodeChainStatus::from),
                sync_status: None,
                staking_status: None,
                container_status: None,
            }
        }
    }

    impl From<NodeChainStatus> for GrpcNodeStatus {
        fn from(ncs: NodeChainStatus) -> Self {
            match ncs {
                NodeChainStatus::Unknown => GrpcNodeStatus::UndefinedApplicationStatus,
                NodeChainStatus::Broadcasting => GrpcNodeStatus::Broadcasting,
                NodeChainStatus::Cancelled => GrpcNodeStatus::Cancelled,
                // TODO
                NodeChainStatus::Consensus => GrpcNodeStatus::UndefinedApplicationStatus,
                NodeChainStatus::Delegating => GrpcNodeStatus::Delegating,
                NodeChainStatus::Delinquent => GrpcNodeStatus::Delinquent,
                NodeChainStatus::Disabled => GrpcNodeStatus::Disabled,
                NodeChainStatus::Earning => GrpcNodeStatus::Earning,
                NodeChainStatus::Elected => GrpcNodeStatus::Elected,
                NodeChainStatus::Electing => GrpcNodeStatus::Electing,
                NodeChainStatus::Exporting => GrpcNodeStatus::Exporting,
                // TODO
                NodeChainStatus::Follower => GrpcNodeStatus::UndefinedApplicationStatus,
                NodeChainStatus::Ingesting => GrpcNodeStatus::Ingesting,
                NodeChainStatus::Mining => GrpcNodeStatus::Mining,
                NodeChainStatus::Minting => GrpcNodeStatus::Minting,
                NodeChainStatus::Processing => GrpcNodeStatus::Processing,
                NodeChainStatus::Relaying => GrpcNodeStatus::Relaying,
                NodeChainStatus::Removed => GrpcNodeStatus::Removed,
                NodeChainStatus::Removing => GrpcNodeStatus::Removing,
                // TODO
                NodeChainStatus::Staked => GrpcNodeStatus::UndefinedApplicationStatus,
                // TODO
                NodeChainStatus::Staking => GrpcNodeStatus::UndefinedApplicationStatus,
                // TODO
                NodeChainStatus::Validating => GrpcNodeStatus::UndefinedApplicationStatus,
            }
        }
    }

    impl From<models::Blockchain> for blockjoy_ui::Blockchain {
        fn from(model: models::Blockchain) -> Self {
            let convert_dt = |dt: DateTime<Utc>| Timestamp {
                seconds: dt.timestamp(),
                nanos: dt.timestamp_nanos() as i32,
            };
            let json = model.supported_node_types.0;
            let json = serde_json::to_string::<Vec<NodeType>>(&json).unwrap();

            tracing::info!("sending json: {}", &json);

            Self {
                id: Some(model.id.into()),
                name: model.name,
                description: model.description,
                status: model.status as i32,
                project_url: model.project_url,
                repo_url: model.repo_url,
                supports_etl: model.supports_etl,
                supports_node: model.supports_node,
                supports_staking: model.supports_staking,
                supports_broadcast: model.supports_broadcast,
                version: model.version,
                supported_nodes_types: json,
                created_at: Some(convert_dt(model.created_at)),
                updated_at: Some(convert_dt(model.updated_at)),
            }
        }
    }
}

pub mod into {
    use crate::grpc::blockjoy::{HostInfo, HostInfoUpdateRequest, Uuid as GrpcUuid};
    use tonic::Request;

    impl ToString for GrpcUuid {
        fn to_string(&self) -> String {
            self.value.clone()
        }
    }

    pub trait IntoData<R, T> {
        fn into_data(self) -> T;
    }

    impl IntoData<Request<HostInfoUpdateRequest>, (GrpcUuid, HostInfo)>
        for Request<HostInfoUpdateRequest>
    {
        fn into_data(self) -> (GrpcUuid, HostInfo) {
            let inner = self.into_inner();
            let id = inner.request_id.unwrap();
            let info = inner.info.unwrap();

            (id, info)
        }
    }
}
