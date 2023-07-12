use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, diesel_derive_enum::DbEnum)]
#[ExistingTypePath = "crate::models::schema::sql_types::EnumNodeType"]
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

impl std::fmt::Display for NodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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

impl std::str::FromStr for NodeType {
    type Err = Error;

    fn from_str(s: &str) -> crate::Result<Self> {
        let res = match s.to_lowercase().as_str() {
            "unknown" => Self::Unknown,
            "miner" => Self::Miner,
            "etl" => Self::Etl,
            "validator" => Self::Validator,
            "api" => Self::Api,
            "oracle" => Self::Oracle,
            "relay" => Self::Relay,
            "execution" => Self::Execution,
            "beacon" => Self::Beacon,
            "mevboost" => Self::MevBoost,
            "node" => Self::Node,
            "fullnode" => Self::FullNode,
            "lightnode" => Self::LightNode,
            _ => return Err(anyhow::anyhow!("Cannot parse {s} as a valid NodeType").into()),
        };
        Ok(res)
    }
}

impl TryFrom<i32> for NodeType {
    type Error = Error;

    fn try_from(value: i32) -> crate::Result<Self> {
        let val = match value {
            0 => NodeType::Unknown,
            1 => NodeType::Miner,
            2 => NodeType::Etl,
            3 => NodeType::Validator,
            4 => NodeType::Api,
            5 => NodeType::Oracle,
            6 => NodeType::Relay,
            7 => NodeType::Execution,
            8 => NodeType::Beacon,
            9 => NodeType::MevBoost,
            10 => NodeType::Node,
            11 => NodeType::FullNode,
            12 => NodeType::LightNode,
            _ => return Err(anyhow::anyhow!("Cannot parse {value} as a NodeType").into()),
        };
        Ok(val)
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
