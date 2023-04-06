use crate::Error;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NodePropertyValue {
    pub name: String,
    pub label: String,
    pub description: String,
    pub ui_type: String,
    pub disabled: bool,
    pub required: bool,
    pub value: Option<String>,
}

/// A list of properties that goes into the `node_type` field of a `node`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NodeProperties {
    pub version: Option<String>,
    pub properties: Option<Vec<NodePropertyValue>>,
}

impl TryFrom<String> for NodeProperties {
    type Error = Error;

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
    pub properties: Option<Vec<BlockchainPropertyValue>>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BlockchainPropertyValue {
    pub name: String,
    ui_type: String,
    disabled: bool,
    required: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parsing_node_properties() {
        let props = [
            serde_json::json!({"properties": [{"name": "self-hosted", "label": "", "value": "false", "ui_type": "switch", "disabled": true, "required": true, "description": ""}]}),
            serde_json::json!({"properties": [{"name": "self-hosted", "label": "", "value": "false", "ui_type": "switch", "disabled": true, "required": true, "description": ""}]}),
            serde_json::json!({"properties": [{"name": "self-hosted", "label": "", "value": "false", "default": "false", "ui_type": "switch", "disabled": true, "required": true, "description": ""}]}),
            serde_json::json!({"properties": [{"name": "self-hosted", "label": "", "value": "false", "ui_type": "switch", "disabled": true, "required": true, "description": ""}]}),
            serde_json::json!({"properties": [{"name": "self-hosted", "label": "", "value": "false", "default": "false", "ui_type": "switch", "disabled": true, "required": true, "description": ""}]}),
            serde_json::json!({"properties": [{"name": "self-hosted", "label": "", "value": "false", "ui_type": "switch", "disabled": true, "required": true, "description": ""}]}),
            serde_json::json!({"properties": [{"name": "self-hosted", "label": "", "value": "false", "ui_type": "switch", "disabled": true, "required": true, "description": ""}]}),
            serde_json::json!({"properties": [{"name": "self-hosted", "label": "", "value": "false", "ui_type": "switch", "disabled": true, "required": true, "description": ""}]}),
        ];
        for prop in props {
            let _: NodeProperties = serde_json::from_value(prop).unwrap();
        }
    }

    #[test]
    fn test_parsing_blockchain_properties() {
        let props = [
            serde_json::json!([{"id": 10, "version": "3.4.0-build.1", "properties": [{"name": "self-hosted", "default": "false", "ui_type": "switch", "disabled": true, "required": true}]}, {"id": 3, "version": "3.4.0-build.6", "properties": [{"name": "keystore-file", "default": "", "ui_type": "key-upload", "disabled": false, "required": true}, {"name": "voting-pwd", "default": "", "ui_type": "voting_key_pwd", "disabled": false, "required": false}, {"name": "fee-recipient", "default": "", "ui_type": "wallet_address", "disabled": false, "required": true}, {"name": "mev-boost", "default": "", "ui_type": "switch", "disabled": false, "required": false}, {"name": "self-hosted", "default": "false", "ui_type": "switch", "disabled": true, "required": true}]}]),
            serde_json::json!([{"id": 3, "version": "1.17.2-build.5", "properties": [{"name": "self-hosted", "default": "false", "ui_type": "switch", "disabled": true, "required": true}]}]),
            serde_json::json!([{"id": 3, "version": "0.0.3", "properties": [{"name": "self-hosted", "default": "false", "ui_type": "switch", "disabled": true, "required": true}]}]),
            serde_json::json!([{"id": 3, "version": "0.0.3", "properties": [{"name": "self-hosted", "default": "false", "ui_type": "switch", "disabled": true, "required": true}]}]),
            serde_json::json!([{"id": 3, "version": "0.0.3", "properties": [{"name": "self-hosted", "default": "false", "ui_type": "switch", "disabled": true, "required": true}]}]),
            serde_json::json!([{"id": 10, "version": "3.14.2", "properties": [{"name": "self-hosted", "default": "false", "ui_type": "switch", "disabled": true, "required": true}]}]),
            serde_json::json!([{"id": 3, "version": "3.3.0", "properties": [{"name": "keystore-file", "default": "", "ui_type": "key-upload", "disabled": false, "required": true}, {"name": "voting-pwd", "default": "", "ui_type": "voting_key_pwd", "disabled": false, "required": true}, {"name": "fee-recipient", "default": "", "ui_type": "wallet_address", "disabled": false, "required": true}, {"name": "mev-boost", "default": "", "ui_type": "switch", "disabled": false, "required": false}, {"name": "self-hosted", "default": "false", "ui_type": "switch", "disabled": true, "required": true}]}]),
            serde_json::json!([{"id": 3, "version": "0.0.3", "properties": [{"name": "self-hosted", "default": "false", "ui_type": "switch", "disabled": true, "required": true}]}]),
            serde_json::json!([{"id": 10, "version": "1.31.0-build.1", "properties": [{"name": "self-hosted", "default": "false", "ui_type": "switch", "disabled": true, "required": true}]}]),
            serde_json::json!([{"id": 10, "version": "15.1.0-build.1", "properties": [{"name": "self-hosted", "default": "false", "ui_type": "switch", "disabled": true, "required": true}]}]),
            serde_json::json!([{"id": 10, "version": "1.2.4-build.1", "properties": [{"name": "self-hosted", "default": "false", "ui_type": "switch", "disabled": true, "required": true}]}]),
            serde_json::json!([{"id": 3, "version": "0.0.3", "properties": [{"name": "self-hosted", "default": "false", "ui_type": "switch", "disabled": true, "required": true}]}]),
            serde_json::json!([{"id": 3, "version": "0.0.3", "properties": [{"name": "self-hosted", "default": "false", "ui_type": "switch", "disabled": true, "required": true}]}]),
            serde_json::json!([{"id": 10, "version": "2.0.2-build.1", "properties": [{"name": "self-hosted", "default": "false", "ui_type": "switch", "disabled": true, "required": true}]}]),
            serde_json::json!([{"id": 10, "version": "1.35.5-build.1", "properties": [{"name": "self-hosted", "default": "false", "ui_type": "switch", "disabled": true, "required": true}]}, {"id": 3, "version": "0.0.3", "properties": [{"name": "self-hosted", "default": "false", "ui_type": "switch", "disabled": true, "required": true}]}]),
        ];
        for prop in props {
            let _: Vec<BlockchainProperties> = serde_json::from_value(prop).unwrap();
        }
    }
}
