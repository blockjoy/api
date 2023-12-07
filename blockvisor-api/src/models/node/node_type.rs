use std::fmt;
use std::str::FromStr;

use derive_more::{AsRef, Deref, Display, From, Into};
use diesel_derive_enum::DbEnum;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display as DisplayDoc;
use semver::Version;
use thiserror::Error;
use tonic::Status;

use crate::grpc::common;
use crate::models::schema::sql_types;

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Failed to parse semantic Version: {0}
    ParseVersion(semver::Error),
    /// Unknown NodeType: {0}
    UnknownNodeType(String),
    /// Unknown NodeType value: {0}
    UnknownNodeTypeValue(i32),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            UnknownNodeType(_) | UnknownNodeTypeValue(_) => Status::internal("Internal error."),
            ParseVersion(_) => Status::invalid_argument("version"),
        }
    }
}

#[derive(Clone, Debug, Display, DieselNewType, AsRef, Deref, From, Into)]
pub struct NodeNetwork(String);

#[derive(Clone, Debug, Display, Hash, PartialEq, Eq, DieselNewType, AsRef, Deref, From, Into)]
pub struct NodeVersion(String);

impl NodeVersion {
    pub fn new(version: &str) -> Result<Self, Error> {
        version
            .parse::<Version>()
            .map(|version| Self(version.to_string().to_lowercase()))
            .map_err(Error::ParseVersion)
    }

    pub fn semver(&self) -> Result<Version, Error> {
        self.0.parse().map_err(Error::ParseVersion)
    }
}

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
    Archive = 13,
}

impl From<common::NodeType> for NodeType {
    fn from(node_type: common::NodeType) -> Self {
        match node_type {
            common::NodeType::Unspecified => NodeType::Unknown,
            common::NodeType::Miner => NodeType::Miner,
            common::NodeType::Etl => NodeType::Etl,
            common::NodeType::Validator => NodeType::Validator,
            common::NodeType::Api => NodeType::Api,
            common::NodeType::Oracle => NodeType::Oracle,
            common::NodeType::Relay => NodeType::Relay,
            common::NodeType::Execution => NodeType::Execution,
            common::NodeType::Beacon => NodeType::Beacon,
            common::NodeType::Mevboost => NodeType::MevBoost,
            common::NodeType::Node => NodeType::Node,
            common::NodeType::Fullnode => NodeType::FullNode,
            common::NodeType::Lightnode => NodeType::LightNode,
            common::NodeType::Archive => NodeType::Archive,
        }
    }
}

impl From<NodeType> for common::NodeType {
    fn from(node_type: NodeType) -> Self {
        match node_type {
            NodeType::Unknown => common::NodeType::Unspecified,
            NodeType::Miner => common::NodeType::Miner,
            NodeType::Etl => common::NodeType::Etl,
            NodeType::Validator => common::NodeType::Validator,
            NodeType::Api => common::NodeType::Api,
            NodeType::Oracle => common::NodeType::Oracle,
            NodeType::Relay => common::NodeType::Relay,
            NodeType::Execution => common::NodeType::Execution,
            NodeType::Beacon => common::NodeType::Beacon,
            NodeType::MevBoost => common::NodeType::Mevboost,
            NodeType::Node => common::NodeType::Node,
            NodeType::FullNode => common::NodeType::Fullnode,
            NodeType::LightNode => common::NodeType::Lightnode,
            NodeType::Archive => common::NodeType::Archive,
        }
    }
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
            Self::Archive => "Archive",
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
            "archive" => Ok(Self::Archive),
            _ => Err(Error::UnknownNodeType(s.into())),
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
        assert_eq!(NodeType::Archive.to_string(), "Archive");
    }
}
