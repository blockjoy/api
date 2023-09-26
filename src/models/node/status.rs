use diesel_derive_enum::DbEnum;

use crate::grpc::api;
use crate::models::schema::sql_types;

/// ContainerStatus reflects blockjoy.api.v1.node.NodeInfo.SyncStatus in node.proto
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
}

/// NodeSyncStatus reflects blockjoy.api.v1.node.NodeInfo.SyncStatus in node.proto
#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumNodeSyncStatus"]
pub enum NodeSyncStatus {
    Unknown,
    Syncing,
    Synced,
}

/// NodeStakingStatus reflects blockjoy.api.v1.node.NodeInfo.StakingStatus in node.proto
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

/// NodeChainStatus reflects blockjoy.api.v1.node.NodeInfo.ApplicationStatus in node.proto
#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumNodeChainStatus"]
pub enum NodeChainStatus {
    Unknown,
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
    Removed,
    Removing,
}

impl api::ContainerStatus {
    pub fn from_model(model: ContainerStatus) -> Self {
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
        }
    }

    pub fn into_model(self) -> ContainerStatus {
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
        }
    }
}

impl api::NodeStatus {
    pub fn from_model(model: NodeChainStatus) -> Self {
        match model {
            NodeChainStatus::Unknown => Self::Unspecified,
            NodeChainStatus::Provisioning => Self::Provisioning,
            NodeChainStatus::Broadcasting => Self::Broadcasting,
            NodeChainStatus::Cancelled => Self::Cancelled,
            NodeChainStatus::Delegating => Self::Delegating,
            NodeChainStatus::Delinquent => Self::Delinquent,
            NodeChainStatus::Disabled => Self::Disabled,
            NodeChainStatus::Earning => Self::Earning,
            NodeChainStatus::Electing => Self::Electing,
            NodeChainStatus::Elected => Self::Elected,
            NodeChainStatus::Exported => Self::Exported,
            NodeChainStatus::Ingesting => Self::Ingesting,
            NodeChainStatus::Mining => Self::Mining,
            NodeChainStatus::Minting => Self::Minting,
            NodeChainStatus::Processing => Self::Processing,
            NodeChainStatus::Relaying => Self::Relaying,
            NodeChainStatus::Removed => Self::Removed,
            NodeChainStatus::Removing => Self::Removing,
        }
    }

    pub fn into_model(self) -> NodeChainStatus {
        match self {
            Self::Unspecified => NodeChainStatus::Unknown,
            Self::Provisioning => NodeChainStatus::Provisioning,
            Self::Broadcasting => NodeChainStatus::Broadcasting,
            Self::Cancelled => NodeChainStatus::Cancelled,
            Self::Delegating => NodeChainStatus::Delegating,
            Self::Delinquent => NodeChainStatus::Delinquent,
            Self::Disabled => NodeChainStatus::Disabled,
            Self::Earning => NodeChainStatus::Earning,
            Self::Electing => NodeChainStatus::Electing,
            Self::Elected => NodeChainStatus::Elected,
            Self::Exported => NodeChainStatus::Exported,
            Self::Ingesting => NodeChainStatus::Ingesting,
            Self::Mining => NodeChainStatus::Mining,
            Self::Minting => NodeChainStatus::Minting,
            Self::Processing => NodeChainStatus::Processing,
            Self::Relaying => NodeChainStatus::Relaying,
            Self::Removed => NodeChainStatus::Removed,
            Self::Removing => NodeChainStatus::Removing,
        }
    }
}

impl api::StakingStatus {
    pub fn from_model(model: NodeStakingStatus) -> Self {
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

    pub fn into_model(self) -> NodeStakingStatus {
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
    pub fn from_model(model: NodeSyncStatus) -> Self {
        match model {
            NodeSyncStatus::Unknown => Self::Unspecified,
            NodeSyncStatus::Syncing => Self::Syncing,
            NodeSyncStatus::Synced => Self::Synced,
        }
    }

    pub fn into_model(self) -> NodeSyncStatus {
        match self {
            Self::Unspecified => NodeSyncStatus::Unknown,
            Self::Syncing => NodeSyncStatus::Syncing,
            Self::Synced => NodeSyncStatus::Synced,
        }
    }
}
