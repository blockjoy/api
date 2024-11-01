use std::fmt;
use std::str::FromStr;

use derive_more::{AsRef, Deref, Display, From, Into};
use diesel_derive_enum::DbEnum;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display as DisplayDoc;
use semver::Version;
use thiserror::Error;

use crate::grpc::{common, Status};
use crate::model::schema::sql_types;

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Failed to parse semantic Version: {0}
    ParseVersion(semver::Error),
    /// Unknown NodeType: {0}
    UnknownNodeType(String),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            ParseVersion(_) => Status::invalid_argument("version"),
            UnknownNodeType(_) => Status::internal("Internal error."),
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
    Unknown,
    Miner,
    Etl,
    Validator,
    Api,
    Oracle,
    Relay,
    Execution,
    Beacon,
    MevBoost,
    Node,
    FullNode,
    LightNode,
    Archive,
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
            NodeType::Unknown => "Unknown",
            NodeType::Miner => "Miner",
            NodeType::Etl => "ETL",
            NodeType::Validator => "Validator",
            NodeType::Api => "API",
            NodeType::Oracle => "Oracle",
            NodeType::Relay => "Relay",
            NodeType::Execution => "Execution",
            NodeType::Beacon => "Beacon",
            NodeType::MevBoost => "MEVBoost",
            NodeType::Node => "Node",
            NodeType::FullNode => "FullNode",
            NodeType::LightNode => "LightNode",
            NodeType::Archive => "Archive",
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
