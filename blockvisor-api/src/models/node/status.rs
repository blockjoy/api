use diesel_derive_enum::DbEnum;

use crate::grpc::api;
use crate::models::schema::sql_types;

/// `ContainerStatus` reflects blockjoy.api.v1.node.NodeInfo.SyncStatus in node.proto
#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumContainerStatus"]
pub enum ContainerStatus {
    Unknown,
    Creating,
    Running,
    Starting,
    Stopping,
    Stopped,
    Upgrading,
    Upgraded,
    Deleting,
    Deleted,
    Installing,
    Snapshotting,
    Failed,
    Busy,
}

/// `NodeSyncStatus` reflects blockjoy.api.v1.node.NodeInfo.SyncStatus in node.proto
#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumNodeSyncStatus"]
pub enum NodeSyncStatus {
    Unknown,
    Syncing,
    Synced,
}

/// `NodeStakingStatus` reflects blockjoy.api.v1.node.NodeInfo.StakingStatus in node.proto
#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumNodeStakingStatus"]
pub enum NodeStakingStatus {
    Unknown,
    Follower,
    Staked,
    Staking,
    Validating,
    Consensus,
    Unstaked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumNodeStatus"]
pub enum NodeStatus {
    ProvisioningPending,
    Provisioning,
    Broadcasting,
    Cancelled,
    Delegating,
    Delinquent,
    Disabled,
    Earning,
    Electing,
    Elected,
    Exported,
    Ingesting,
    Mining,
    Minting,
    Processing,
    Relaying,
    DeletePending,
    Deleting,
    Deleted,
    UpdatePending,
    Updating,
}

impl api::ContainerStatus {
    pub const fn from_model(model: ContainerStatus) -> Self {
        match model {
            ContainerStatus::Unknown => Self::Unspecified,
            ContainerStatus::Creating => Self::Creating,
            ContainerStatus::Running => Self::Running,
            ContainerStatus::Starting => Self::Starting,
            ContainerStatus::Stopping => Self::Stopping,
            ContainerStatus::Stopped => Self::Stopped,
            ContainerStatus::Upgrading => Self::Upgrading,
            ContainerStatus::Upgraded => Self::Upgraded,
            ContainerStatus::Deleting => Self::Deleting,
            ContainerStatus::Deleted => Self::Deleted,
            ContainerStatus::Installing => Self::Installing,
            ContainerStatus::Snapshotting => Self::Snapshotting,
            ContainerStatus::Failed => Self::Failed,
            ContainerStatus::Busy => Self::Busy,
        }
    }

    pub const fn into_model(self) -> ContainerStatus {
        match self {
            Self::Unspecified => ContainerStatus::Unknown,
            Self::Creating => ContainerStatus::Creating,
            Self::Running => ContainerStatus::Running,
            Self::Starting => ContainerStatus::Starting,
            Self::Stopping => ContainerStatus::Stopping,
            Self::Stopped => ContainerStatus::Stopped,
            Self::Upgrading => ContainerStatus::Upgrading,
            Self::Upgraded => ContainerStatus::Upgraded,
            Self::Deleting => ContainerStatus::Deleting,
            Self::Deleted => ContainerStatus::Deleted,
            Self::Installing => ContainerStatus::Installing,
            Self::Snapshotting => ContainerStatus::Snapshotting,
            Self::Failed => ContainerStatus::Failed,
            Self::Busy => ContainerStatus::Busy,
        }
    }
}

impl api::NodeStatus {
    pub const fn from_model(model: NodeStatus) -> Self {
        match model {
            NodeStatus::ProvisioningPending => Self::ProvisioningPending,
            NodeStatus::Provisioning => Self::Provisioning,
            NodeStatus::Broadcasting => Self::Broadcasting,
            NodeStatus::Cancelled => Self::Cancelled,
            NodeStatus::Delegating => Self::Delegating,
            NodeStatus::Delinquent => Self::Delinquent,
            NodeStatus::Disabled => Self::Disabled,
            NodeStatus::Earning => Self::Earning,
            NodeStatus::Electing => Self::Electing,
            NodeStatus::Elected => Self::Elected,
            NodeStatus::Exported => Self::Exported,
            NodeStatus::Ingesting => Self::Ingesting,
            NodeStatus::Mining => Self::Mining,
            NodeStatus::Minting => Self::Minting,
            NodeStatus::Processing => Self::Processing,
            NodeStatus::Relaying => Self::Relaying,
            NodeStatus::DeletePending => Self::DeletePending,
            NodeStatus::Deleting => Self::Deleting,
            NodeStatus::Deleted => Self::Deleted,
            NodeStatus::UpdatePending => Self::UpdatePending,
            NodeStatus::Updating => Self::Updating,
        }
    }

    pub const fn into_model(self) -> Option<NodeStatus> {
        match self {
            Self::Unspecified => None,
            Self::ProvisioningPending => Some(NodeStatus::ProvisioningPending),
            Self::Provisioning => Some(NodeStatus::Provisioning),
            Self::Broadcasting => Some(NodeStatus::Broadcasting),
            Self::Cancelled => Some(NodeStatus::Cancelled),
            Self::Delegating => Some(NodeStatus::Delegating),
            Self::Delinquent => Some(NodeStatus::Delinquent),
            Self::Disabled => Some(NodeStatus::Disabled),
            Self::Earning => Some(NodeStatus::Earning),
            Self::Electing => Some(NodeStatus::Electing),
            Self::Elected => Some(NodeStatus::Elected),
            Self::Exported => Some(NodeStatus::Exported),
            Self::Ingesting => Some(NodeStatus::Ingesting),
            Self::Mining => Some(NodeStatus::Mining),
            Self::Minting => Some(NodeStatus::Minting),
            Self::Processing => Some(NodeStatus::Processing),
            Self::Relaying => Some(NodeStatus::Relaying),
            Self::DeletePending => Some(NodeStatus::DeletePending),
            Self::Deleting => Some(NodeStatus::Deleting),
            Self::Deleted => Some(NodeStatus::Deleted),
            Self::UpdatePending => Some(NodeStatus::UpdatePending),
            Self::Updating => Some(NodeStatus::Updating),
        }
    }
}

impl api::StakingStatus {
    pub const fn from_model(model: NodeStakingStatus) -> Self {
        match model {
            NodeStakingStatus::Unknown => Self::Unspecified,
            NodeStakingStatus::Follower => Self::Follower,
            NodeStakingStatus::Staked => Self::Staked,
            NodeStakingStatus::Staking => Self::Staking,
            NodeStakingStatus::Validating => Self::Validating,
            NodeStakingStatus::Consensus => Self::Consensus,
            NodeStakingStatus::Unstaked => Self::Unstaked,
        }
    }

    pub const fn into_model(self) -> NodeStakingStatus {
        match self {
            Self::Unspecified => NodeStakingStatus::Unknown,
            Self::Follower => NodeStakingStatus::Follower,
            Self::Staked => NodeStakingStatus::Staked,
            Self::Staking => NodeStakingStatus::Staking,
            Self::Validating => NodeStakingStatus::Validating,
            Self::Consensus => NodeStakingStatus::Consensus,
            Self::Unstaked => NodeStakingStatus::Unstaked,
        }
    }
}

impl api::SyncStatus {
    pub const fn from_model(model: NodeSyncStatus) -> Self {
        match model {
            NodeSyncStatus::Unknown => Self::Unspecified,
            NodeSyncStatus::Syncing => Self::Syncing,
            NodeSyncStatus::Synced => Self::Synced,
        }
    }

    #[must_use]
    pub const fn into_model(self) -> NodeSyncStatus {
        match self {
            Self::Unspecified => NodeSyncStatus::Unknown,
            Self::Syncing => NodeSyncStatus::Syncing,
            Self::Synced => NodeSyncStatus::Synced,
        }
    }
}
