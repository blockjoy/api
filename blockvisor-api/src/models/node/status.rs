use diesel_derive_enum::DbEnum;

use crate::grpc::common;
use crate::models::schema::sql_types;

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
        }
    }
}

impl From<common::NodeStatus> for NodeStatus {
    fn from(status: common::NodeStatus) -> Self {
        match status {
            common::NodeStatus::Unspecified => NodeStatus::Unknown,
            common::NodeStatus::ProvisioningPending => NodeStatus::ProvisioningPending,
            common::NodeStatus::Provisioning => NodeStatus::Provisioning,
            common::NodeStatus::Broadcasting => NodeStatus::Broadcasting,
            common::NodeStatus::Cancelled => NodeStatus::Cancelled,
            common::NodeStatus::Delegating => NodeStatus::Delegating,
            common::NodeStatus::Delinquent => NodeStatus::Delinquent,
            common::NodeStatus::Disabled => NodeStatus::Disabled,
            common::NodeStatus::Earning => NodeStatus::Earning,
            common::NodeStatus::Electing => NodeStatus::Electing,
            common::NodeStatus::Elected => NodeStatus::Elected,
            common::NodeStatus::Exported => NodeStatus::Exported,
            common::NodeStatus::Ingesting => NodeStatus::Ingesting,
            common::NodeStatus::Mining => NodeStatus::Mining,
            common::NodeStatus::Minting => NodeStatus::Minting,
            common::NodeStatus::Processing => NodeStatus::Processing,
            common::NodeStatus::Relaying => NodeStatus::Relaying,
            common::NodeStatus::DeletePending => NodeStatus::DeletePending,
            common::NodeStatus::Deleting => NodeStatus::Deleting,
            common::NodeStatus::Deleted => NodeStatus::Deleted,
            common::NodeStatus::UpdatePending => NodeStatus::UpdatePending,
            common::NodeStatus::Updating => NodeStatus::Updating,
            common::NodeStatus::Initializing => NodeStatus::Initializing,
            common::NodeStatus::Downloading => NodeStatus::Downloading,
            common::NodeStatus::Uploading => NodeStatus::Uploading,
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

impl From<common::SyncStatus> for SyncStatus {
    fn from(status: common::SyncStatus) -> Self {
        match status {
            common::SyncStatus::Unspecified => SyncStatus::Unknown,
            common::SyncStatus::Syncing => SyncStatus::Syncing,
            common::SyncStatus::Synced => SyncStatus::Synced,
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

impl From<common::StakingStatus> for StakingStatus {
    fn from(status: common::StakingStatus) -> StakingStatus {
        match status {
            common::StakingStatus::Unspecified => StakingStatus::Unknown,
            common::StakingStatus::Follower => StakingStatus::Follower,
            common::StakingStatus::Staked => StakingStatus::Staked,
            common::StakingStatus::Staking => StakingStatus::Staking,
            common::StakingStatus::Validating => StakingStatus::Validating,
            common::StakingStatus::Consensus => StakingStatus::Consensus,
            common::StakingStatus::Unstaked => StakingStatus::Unstaked,
        }
    }
}
