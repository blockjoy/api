use serde::{Deserialize, Serialize};
use sqlx::postgres::PgHasArrayType;

use crate::errors::ApiError;
use crate::models::NodeTypeKey;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodePropertyValue {
    pub name: String,
    label: String,
    description: String,
    ui_type: String,
    disabled: bool,
    required: bool,
    pub value: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeProperties {
    id: i32,
    properties: Option<Vec<NodePropertyValue>>,
}

impl PgHasArrayType for NodeProperties {
    fn array_type_info() -> sqlx::postgres::PgTypeInfo {
        sqlx::postgres::PgTypeInfo::with_name("JSONB[]")
    }
}

impl TryFrom<String> for NodeProperties {
    type Error = ApiError;

    fn try_from(json: String) -> Result<Self, Self::Error> {
        let json = serde_json::from_str(&json)?;
        Ok(json)
    }
}

impl NodePropertyValue {
    pub fn to_json(&self) -> Result<String, ApiError> {
        let json_str = serde_json::to_string(self)?;
        Ok(json_str)
    }
}

impl NodeProperties {
    pub fn to_json(&self) -> Result<String, ApiError> {
        let json_str = serde_json::to_string(self)?;
        Ok(json_str)
    }

    pub fn special_type(id: NodeTypeKey) -> Self {
        Self {
            id: id.into(),
            properties: Some(vec![]),
        }
    }

    pub fn get_id(&self) -> i32 {
        self.id
    }

    pub fn get_properties(&self) -> Option<&[NodePropertyValue]> {
        self.properties.as_deref()
    }

    pub fn iter_props(&self) -> impl Iterator<Item = &NodePropertyValue> {
        self.properties.iter().flat_map(|p| p.iter())
    }
}
