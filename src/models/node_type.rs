use derive_getters::Getters;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgHasArrayType;

use crate::{errors::ApiError, grpc::helpers::required};

pub enum NodeTypeKey {
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

impl NodeTypeKey {
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

impl From<NodeTypeKey> for i32 {
    fn from(id: NodeTypeKey) -> Self {
        match id {
            NodeTypeKey::Unknown => 0,
            NodeTypeKey::Miner => 1,
            NodeTypeKey::Etl => 2,
            NodeTypeKey::Validator => 3,
            NodeTypeKey::Api => 4,
            NodeTypeKey::Oracle => 5,
            NodeTypeKey::Relay => 6,
            NodeTypeKey::Execution => 7,
            NodeTypeKey::Beacon => 8,
            NodeTypeKey::MevBoost => 9,
            NodeTypeKey::Node => 10,
            NodeTypeKey::FullNode => 11,
            NodeTypeKey::LightNode => 12,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeTypeProperty {
    name: String,
    ui_type: String,
    default: Option<String>,
    disabled: bool,
    required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Getters)]
pub struct NodeType {
    id: i32,
    version: String,
    properties: Option<Vec<NodeTypeProperty>>,
}

impl PgHasArrayType for NodeType {
    fn array_type_info() -> sqlx::postgres::PgTypeInfo {
        sqlx::postgres::PgTypeInfo::with_name("JSONB[]")
    }
}

impl TryFrom<String> for NodeType {
    type Error = ApiError;

    fn try_from(json: String) -> Result<Self, Self::Error> {
        serde_json::from_str(&json).map_err(Into::into)
    }
}

impl NodeTypeProperty {
    pub fn to_json(&self) -> Result<String, ApiError> {
        if self.default.is_none() {
            return Err(required("default")().into());
        }
        let json_str = serde_json::to_string(self)?;
        Ok(json_str)
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_label(&self) -> &str {
        &self.name
    }

    pub fn get_default(&self) -> Option<&str> {
        self.default.as_deref()
    }

    pub fn get_property_type(&self) -> &str {
        &self.ui_type
    }
}

impl NodeType {
    pub fn to_json(&self) -> Result<String, ApiError> {
        // TO DECIDE: Thomas in the earlier version we didn't include the `version` field here,
        // was that by design?
        let json_str = serde_json::to_string(self)?;
        Ok(json_str)
    }

    pub fn special_type(id: NodeTypeKey) -> Self {
        Self {
            id: id.into(),
            version: "".into(),
            properties: Some(vec![]),
        }
    }

    pub fn get_id(&self) -> i32 {
        self.id
    }

    pub fn get_properties(&self) -> Option<&[NodeTypeProperty]> {
        self.properties.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use crate::models::NodeTypeKey;

    #[test]
    fn returns_valid_string_for_value() {
        assert_eq!(NodeTypeKey::str_from_value(0), "Unknown".to_string());
        assert_eq!(NodeTypeKey::str_from_value(1), "Miner".to_string());
        assert_eq!(NodeTypeKey::str_from_value(2), "ETL".to_string());
        assert_eq!(NodeTypeKey::str_from_value(3), "Validator".to_string());
        assert_eq!(NodeTypeKey::str_from_value(4), "API".to_string());
        assert_eq!(NodeTypeKey::str_from_value(5), "Oracle".to_string());
        assert_eq!(NodeTypeKey::str_from_value(6), "Relay".to_string());
        assert_eq!(NodeTypeKey::str_from_value(7), "Execution".to_string());
        assert_eq!(NodeTypeKey::str_from_value(8), "Beacon".to_string());
        assert_eq!(NodeTypeKey::str_from_value(9), "MEVBoost".to_string());
        assert_eq!(NodeTypeKey::str_from_value(10), "Node".to_string());
        assert_eq!(NodeTypeKey::str_from_value(11), "FullNode".to_string());
        assert_eq!(NodeTypeKey::str_from_value(12), "LightNode".to_string());
        assert_eq!(NodeTypeKey::str_from_value(100), "Unknown".to_string());
    }
}
