use std::fmt;
use std::str::FromStr;

use derive_more::{AsRef, Deref, Display, From, Into};
use diesel_derive_enum::DbEnum;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display as DisplayDoc;
use thiserror::Error;

use crate::grpc::api;
use crate::models::schema::sql_types;

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Unknown NodeType: {0}
    UnknownNodeType(String),
    /// Unknown NodeType value: {0}
    UnknownNodeTypeValue(i32),
}

#[derive(Clone, Debug, Display, DieselNewType, AsRef, Deref, From, Into)]
pub struct NodeNetwork(String);

#[derive(Clone, Debug, Display, DieselNewType, AsRef, Deref, From, Into)]
pub struct NodeVersion(String);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, DbEnum)]
#[ExistingTypePath = "sql_types::EnumNodeType"]
pub enum NodeType {
    Unknown = 0,
    Miner = 1,
    Etl = 2,
    Validator = 3,
    Api = 4,
    Oracle = 5,
    Relay = 6,
    Execution = 7,
    Beacon = 8,
    MevBoost = 9,
    Node = 10,
    FullNode = 11,
    LightNode = 12,
}

impl fmt::Display for NodeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Unknown => "Unknown",
            Self::Miner => "Miner",
            Self::Etl => "ETL",
            Self::Validator => "Validator",
            Self::Api => "API",
            Self::Oracle => "Oracle",
            Self::Relay => "Relay",
            Self::Execution => "Execution",
            Self::Beacon => "Beacon",
            Self::MevBoost => "MEVBoost",
            Self::Node => "Node",
            Self::FullNode => "FullNode",
            Self::LightNode => "LightNode",
        };
        write!(f, "{s}")
    }
}

impl FromStr for NodeType {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "unknown" => Ok(Self::Unknown),
            "miner" => Ok(Self::Miner),
            "etl" => Ok(Self::Etl),
            "validator" => Ok(Self::Validator),
            "api" => Ok(Self::Api),
            "oracle" => Ok(Self::Oracle),
            "relay" => Ok(Self::Relay),
            "execution" => Ok(Self::Execution),
            "beacon" => Ok(Self::Beacon),
            "mevboost" => Ok(Self::MevBoost),
            "node" => Ok(Self::Node),
            "fullnode" => Ok(Self::FullNode),
            "lightnode" => Ok(Self::LightNode),
            _ => Err(Error::UnknownNodeType(s.into())),
        }
    }
}

impl TryFrom<i32> for NodeType {
    type Error = Error;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(NodeType::Unknown),
            1 => Ok(NodeType::Miner),
            2 => Ok(NodeType::Etl),
            3 => Ok(NodeType::Validator),
            4 => Ok(NodeType::Api),
            5 => Ok(NodeType::Oracle),
            6 => Ok(NodeType::Relay),
            7 => Ok(NodeType::Execution),
            8 => Ok(NodeType::Beacon),
            9 => Ok(NodeType::MevBoost),
            10 => Ok(NodeType::Node),
            11 => Ok(NodeType::FullNode),
            12 => Ok(NodeType::LightNode),
            _ => Err(Error::UnknownNodeTypeValue(value)),
        }
    }
}

impl From<NodeType> for i32 {
    fn from(value: NodeType) -> Self {
        match value {
            NodeType::Unknown => 0,
            NodeType::Miner => 1,
            NodeType::Etl => 2,
            NodeType::Validator => 3,
            NodeType::Api => 4,
            NodeType::Oracle => 5,
            NodeType::Relay => 6,
            NodeType::Execution => 7,
            NodeType::Beacon => 8,
            NodeType::MevBoost => 9,
            NodeType::Node => 10,
            NodeType::FullNode => 11,
            NodeType::LightNode => 12,
        }
    }
}

impl api::NodeType {
    pub fn from_model(model: NodeType) -> Self {
        match model {
            NodeType::Unknown => Self::Unspecified,
            NodeType::Miner => Self::Miner,
            NodeType::Etl => Self::Etl,
            NodeType::Validator => Self::Validator,
            NodeType::Api => Self::Api,
            NodeType::Oracle => Self::Oracle,
            NodeType::Relay => Self::Relay,
            NodeType::Execution => Self::Execution,
            NodeType::Beacon => Self::Beacon,
            NodeType::MevBoost => Self::Mevboost,
            NodeType::Node => Self::Node,
            NodeType::FullNode => Self::Fullnode,
            NodeType::LightNode => Self::Lightnode,
        }
    }

    pub fn into_model(self) -> NodeType {
        match self {
            Self::Unspecified => NodeType::Unknown,
            Self::Miner => NodeType::Miner,
            Self::Etl => NodeType::Etl,
            Self::Validator => NodeType::Validator,
            Self::Api => NodeType::Api,
            Self::Oracle => NodeType::Oracle,
            Self::Relay => NodeType::Relay,
            Self::Execution => NodeType::Execution,
            Self::Beacon => NodeType::Beacon,
            Self::Mevboost => NodeType::MevBoost,
            Self::Node => NodeType::Node,
            Self::Fullnode => NodeType::FullNode,
            Self::Lightnode => NodeType::LightNode,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_valid_string_for_value() {
        assert_eq!(NodeType::Unknown.to_string(), "Unknown");
        assert_eq!(NodeType::Miner.to_string(), "Miner");
        assert_eq!(NodeType::Etl.to_string(), "ETL");
        assert_eq!(NodeType::Validator.to_string(), "Validator");
        assert_eq!(NodeType::Api.to_string(), "API");
        assert_eq!(NodeType::Oracle.to_string(), "Oracle");
        assert_eq!(NodeType::Relay.to_string(), "Relay");
        assert_eq!(NodeType::Execution.to_string(), "Execution");
        assert_eq!(NodeType::Beacon.to_string(), "Beacon");
        assert_eq!(NodeType::MevBoost.to_string(), "MEVBoost");
        assert_eq!(NodeType::Node.to_string(), "Node");
        assert_eq!(NodeType::FullNode.to_string(), "FullNode");
        assert_eq!(NodeType::LightNode.to_string(), "LightNode");
    }
}
