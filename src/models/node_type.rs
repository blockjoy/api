use crate::errors::ApiError;

// TODO: This should not have to implement serialize anymore.
#[derive(Debug, Clone, Copy, diesel_derive_enum::DbEnum, serde::Serialize, serde::Deserialize)]
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

impl NodeType {
    pub fn str_from_value(value: i32) -> String {
        match value {
            0 => "Unknown".to_string(),
            1 => "Miner".to_string(),
            2 => "ETL".to_string(),
            3 => "Validator".to_string(),
            4 => "API".to_string(),
            5 => "Oracle".to_string(),
            6 => "Relay".to_string(),
            7 => "Execution".to_string(),
            8 => "Beacon".to_string(),
            9 => "MEVBoost".to_string(),
            10 => "Node".to_string(),
            11 => "FullNode".to_string(),
            12 => "LightNode".to_string(),
            _ => "Unknown".to_string(),
        }
    }
}

impl std::fmt::Display for NodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let i: i32 = (*self).into();
        write!(f, "{}", Self::str_from_value(i))
    }
}

impl std::str::FromStr for NodeType {
    type Err = ApiError;

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
    type Error = ApiError;

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

impl Into<i32> for NodeType {
    fn into(self) -> i32 {
        match self {
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
    use crate::models::NodeType;

    #[test]
    fn returns_valid_string_for_value() {
        assert_eq!(NodeType::str_from_value(0), "Unknown");
        assert_eq!(NodeType::str_from_value(1), "Miner");
        assert_eq!(NodeType::str_from_value(2), "ETL");
        assert_eq!(NodeType::str_from_value(3), "Validator");
        assert_eq!(NodeType::str_from_value(4), "API");
        assert_eq!(NodeType::str_from_value(5), "Oracle");
        assert_eq!(NodeType::str_from_value(6), "Relay");
        assert_eq!(NodeType::str_from_value(7), "Execution");
        assert_eq!(NodeType::str_from_value(8), "Beacon");
        assert_eq!(NodeType::str_from_value(9), "MEVBoost");
        assert_eq!(NodeType::str_from_value(10), "Node");
        assert_eq!(NodeType::str_from_value(11), "FullNode");
        assert_eq!(NodeType::str_from_value(12), "LightNode");
        assert_eq!(NodeType::str_from_value(100), "Unknown");
    }
}
