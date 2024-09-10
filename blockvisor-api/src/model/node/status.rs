use diesel_derive_enum::DbEnum;
use displaydoc::Display;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::grpc::{common, Status};
use crate::model::schema::sql_types;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Unknown NextState.
    UnknownNextState,
    /// Unknown NodeHealth.
    UnknownNodeHealth,
    /// Unknown NodeState.
    UnknownNodeState,
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            UnknownNextState => Status::invalid_argument("next"),
            UnknownNodeHealth => Status::invalid_argument("protocol_state.health"),
            UnknownNodeState => Status::invalid_argument("state"),
        }
    }
}

pub struct NodeStatus {
    pub state: NodeState,
    pub next: Option<NextState>,
    pub protocol: Option<ProtocolStatus>,
}

impl From<NodeStatus> for common::NodeStatus {
    fn from(status: NodeStatus) -> Self {
        common::NodeStatus {
            state: common::NodeState::from(status.state) as i32,
            next: status.next.map(|next| common::NextState::from(next) as i32),
            protocol: status.protocol.map(Into::into),
        }
    }
}

impl TryFrom<common::NodeStatus> for NodeStatus {
    type Error = Error;

    fn try_from(status: common::NodeStatus) -> Result<Self, Self::Error> {
        Ok(NodeStatus {
            state: status.state().try_into()?,
            next: status.next.map(|_| status.next().try_into()).transpose()?,
            protocol: status.protocol.map(TryInto::try_into).transpose()?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumNodeState"]
pub enum NodeState {
    Starting,
    Running,
    Stopped,
    Failed,
    Upgrading,
    Deleting,
    Deleted,
}

impl From<NodeState> for common::NodeState {
    fn from(state: NodeState) -> Self {
        match state {
            NodeState::Starting => Self::Starting,
            NodeState::Running => Self::Running,
            NodeState::Stopped => Self::Stopped,
            NodeState::Failed => Self::Failed,
            NodeState::Upgrading => Self::Upgrading,
            NodeState::Deleting => Self::Deleting,
            NodeState::Deleted => Self::Deleted,
        }
    }
}

impl TryFrom<common::NodeState> for NodeState {
    type Error = Error;

    fn try_from(state: common::NodeState) -> Result<Self, Self::Error> {
        match state {
            common::NodeState::Unspecified => Err(Error::UnknownNodeState),
            common::NodeState::Starting => Ok(NodeState::Starting),
            common::NodeState::Running => Ok(NodeState::Running),
            common::NodeState::Stopped => Ok(NodeState::Stopped),
            common::NodeState::Failed => Ok(NodeState::Failed),
            common::NodeState::Upgrading => Ok(NodeState::Upgrading),
            common::NodeState::Deleting => Ok(NodeState::Deleting),
            common::NodeState::Deleted => Ok(NodeState::Deleted),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum)]
#[ExistingTypePath = "sql_types::EnumNextState"]
pub enum NextState {
    Stopping,
    Deleting,
    Upgrading,
}

impl From<NextState> for common::NextState {
    fn from(state: NextState) -> Self {
        match state {
            NextState::Stopping => Self::Stopping,
            NextState::Deleting => Self::Deleting,
            NextState::Upgrading => Self::Upgrading,
        }
    }
}

impl TryFrom<common::NextState> for NextState {
    type Error = Error;

    fn try_from(state: common::NextState) -> Result<Self, Self::Error> {
        match state {
            common::NextState::Unspecified => Err(Error::UnknownNextState),
            common::NextState::Stopping => Ok(NextState::Stopping),
            common::NextState::Deleting => Ok(NextState::Deleting),
            common::NextState::Upgrading => Ok(NextState::Upgrading),
        }
    }
}

pub struct ProtocolStatus {
    pub state: String,
    pub health: NodeHealth,
}

impl From<ProtocolStatus> for common::ProtocolStatus {
    fn from(status: ProtocolStatus) -> Self {
        common::ProtocolStatus {
            state: status.state,
            health: common::NodeHealth::from(status.health) as i32,
        }
    }
}

impl TryFrom<common::ProtocolStatus> for ProtocolStatus {
    type Error = Error;

    fn try_from(status: common::ProtocolStatus) -> Result<Self, Self::Error> {
        let health = status.health().try_into()?;
        Ok(ProtocolStatus {
            state: status.state,
            health,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, DbEnum, Serialize, Deserialize)]
#[ExistingTypePath = "sql_types::EnumHealth"]
pub enum NodeHealth {
    Healthy,
    Neutral,
    Unhealthy,
}

impl From<NodeHealth> for common::NodeHealth {
    fn from(health: NodeHealth) -> Self {
        match health {
            NodeHealth::Healthy => Self::Healthy,
            NodeHealth::Neutral => Self::Neutral,
            NodeHealth::Unhealthy => Self::Unhealthy,
        }
    }
}

impl TryFrom<common::NodeHealth> for NodeHealth {
    type Error = Error;

    fn try_from(health: common::NodeHealth) -> Result<Self, Self::Error> {
        match health {
            common::NodeHealth::Unspecified => Err(Error::UnknownNodeHealth),
            common::NodeHealth::Healthy => Ok(NodeHealth::Healthy),
            common::NodeHealth::Neutral => Ok(NodeHealth::Neutral),
            common::NodeHealth::Unhealthy => Ok(NodeHealth::Unhealthy),
        }
    }
}
