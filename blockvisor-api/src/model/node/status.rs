use diesel_derive_enum::DbEnum;

use crate::grpc::common;
use crate::model::schema::sql_types;

#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumNodeStatus"]
pub enum NodeStatus {
    Unknown,
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
    Initializing,
    Downloading,
    Uploading,
    Starting,
    Active,
    Jailed,
}

impl From<NodeStatus> for common::NodeStatus {
    fn from(status: NodeStatus) -> Self {
        match status {
            NodeStatus::Unknown => Self::Unspecified,
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
            NodeStatus::Initializing => Self::Initializing,
            NodeStatus::Downloading => Self::Downloading,
            NodeStatus::Uploading => Self::Uploading,
            NodeStatus::Starting => Self::Starting,
            NodeStatus::Active => Self::Active,
            NodeStatus::Jailed => Self::Jailed,
        }
    }
}

impl common::NodeStatus {
    pub const fn into_model(self) -> Option<NodeStatus> {
        match self {
            common::NodeStatus::Unspecified => None,
            common::NodeStatus::ProvisioningPending => Some(NodeStatus::ProvisioningPending),
            common::NodeStatus::Provisioning => Some(NodeStatus::Provisioning),
            common::NodeStatus::Broadcasting => Some(NodeStatus::Broadcasting),
            common::NodeStatus::Cancelled => Some(NodeStatus::Cancelled),
            common::NodeStatus::Delegating => Some(NodeStatus::Delegating),
            common::NodeStatus::Delinquent => Some(NodeStatus::Delinquent),
            common::NodeStatus::Disabled => Some(NodeStatus::Disabled),
            common::NodeStatus::Earning => Some(NodeStatus::Earning),
            common::NodeStatus::Electing => Some(NodeStatus::Electing),
            common::NodeStatus::Elected => Some(NodeStatus::Elected),
            common::NodeStatus::Exported => Some(NodeStatus::Exported),
            common::NodeStatus::Ingesting => Some(NodeStatus::Ingesting),
            common::NodeStatus::Mining => Some(NodeStatus::Mining),
            common::NodeStatus::Minting => Some(NodeStatus::Minting),
            common::NodeStatus::Processing => Some(NodeStatus::Processing),
            common::NodeStatus::Relaying => Some(NodeStatus::Relaying),
            common::NodeStatus::DeletePending => Some(NodeStatus::DeletePending),
            common::NodeStatus::Deleting => Some(NodeStatus::Deleting),
            common::NodeStatus::Deleted => Some(NodeStatus::Deleted),
            common::NodeStatus::UpdatePending => Some(NodeStatus::UpdatePending),
            common::NodeStatus::Updating => Some(NodeStatus::Updating),
            common::NodeStatus::Initializing => Some(NodeStatus::Initializing),
            common::NodeStatus::Downloading => Some(NodeStatus::Downloading),
            common::NodeStatus::Uploading => Some(NodeStatus::Uploading),
            common::NodeStatus::Starting => Some(NodeStatus::Starting),
            common::NodeStatus::Active => Some(NodeStatus::Active),
            common::NodeStatus::Jailed => Some(NodeStatus::Jailed),
        }
    }
}

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

impl From<ContainerStatus> for common::ContainerStatus {
    fn from(status: ContainerStatus) -> Self {
        match status {
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
}

impl From<common::ContainerStatus> for ContainerStatus {
    fn from(status: common::ContainerStatus) -> Self {
        match status {
            common::ContainerStatus::Unspecified => ContainerStatus::Unknown,
            common::ContainerStatus::Creating => ContainerStatus::Creating,
            common::ContainerStatus::Running => ContainerStatus::Running,
            common::ContainerStatus::Starting => ContainerStatus::Starting,
            common::ContainerStatus::Stopping => ContainerStatus::Stopping,
            common::ContainerStatus::Stopped => ContainerStatus::Stopped,
            common::ContainerStatus::Upgrading => ContainerStatus::Upgrading,
            common::ContainerStatus::Upgraded => ContainerStatus::Upgraded,
            common::ContainerStatus::Deleting => ContainerStatus::Deleting,
            common::ContainerStatus::Deleted => ContainerStatus::Deleted,
            common::ContainerStatus::Installing => ContainerStatus::Installing,
            common::ContainerStatus::Snapshotting => ContainerStatus::Snapshotting,
            common::ContainerStatus::Failed => ContainerStatus::Failed,
            common::ContainerStatus::Busy => ContainerStatus::Busy,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumNodeSyncStatus"]
pub enum SyncStatus {
    Unknown,
    Syncing,
    Synced,
}

impl From<SyncStatus> for common::SyncStatus {
    fn from(status: SyncStatus) -> Self {
        match status {
            SyncStatus::Unknown => Self::Unspecified,
            SyncStatus::Syncing => Self::Syncing,
            SyncStatus::Synced => Self::Synced,
        }
    }
}

impl common::SyncStatus {
    pub const fn into_model(self) -> Option<SyncStatus> {
        match self {
            common::SyncStatus::Unspecified => None,
            common::SyncStatus::Syncing => Some(SyncStatus::Syncing),
            common::SyncStatus::Synced => Some(SyncStatus::Synced),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumNodeStakingStatus"]
pub enum StakingStatus {
    Unknown,
    Follower,
    Staked,
    Staking,
    Validating,
    Consensus,
    Unstaked,
}

impl From<StakingStatus> for common::StakingStatus {
    fn from(status: StakingStatus) -> Self {
        match status {
            StakingStatus::Unknown => Self::Unspecified,
            StakingStatus::Follower => Self::Follower,
            StakingStatus::Staked => Self::Staked,
            StakingStatus::Staking => Self::Staking,
            StakingStatus::Validating => Self::Validating,
            StakingStatus::Consensus => Self::Consensus,
            StakingStatus::Unstaked => Self::Unstaked,
        }
    }
}

impl common::StakingStatus {
    pub const fn into_model(self) -> Option<StakingStatus> {
        match self {
            common::StakingStatus::Unspecified => None,
            common::StakingStatus::Follower => Some(StakingStatus::Follower),
            common::StakingStatus::Staked => Some(StakingStatus::Staked),
            common::StakingStatus::Staking => Some(StakingStatus::Staking),
            common::StakingStatus::Validating => Some(StakingStatus::Validating),
            common::StakingStatus::Consensus => Some(StakingStatus::Consensus),
            common::StakingStatus::Unstaked => Some(StakingStatus::Unstaked),
        }
    }
}
