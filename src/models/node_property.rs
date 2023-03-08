use crate::errors::ApiError;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NodePropertyValue {
    pub name: String,
    label: String,
    description: String,
    ui_type: String,
    disabled: bool,
    required: bool,
    pub value: Option<String>,
}

/// A list of properties that goes into the `node_type` field of a `node`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NodeProperties {
    pub version: Option<String>,
    pub properties: Option<Vec<NodePropertyValue>>,
}

impl TryFrom<String> for NodeProperties {
    type Error = ApiError;

    fn try_from(json: String) -> Result<Self, Self::Error> {
        let json = serde_json::from_str(&json)?;
        Ok(json)
    }
}

impl NodeProperties {
    pub fn iter_props(&self) -> impl Iterator<Item = &NodePropertyValue> {
        self.properties.iter().flat_map(|p| p.iter())
    }
}

/// This is a list of properties, but with the type of node stored as an integer in the `id` field.
/// This is the same way it happens for `BlockchainProperties`, and this representation is used
/// over the gRPC.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NodePropertiesWithId {
    pub id: i32,
    #[serde(flatten)]
    pub props: NodeProperties,
}

/// A list of properties that goes into the `node_type` field of a `node`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BlockchainProperties {
    pub id: i32,
    pub version: String,
    pub properties: Option<Vec<NodePropertyValue>>,
}
