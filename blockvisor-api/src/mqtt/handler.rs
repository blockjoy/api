use std::str::FromStr;

use displaydoc::Display;
use serde::{Deserialize, Deserializer};
use thiserror::Error;
use uuid::Uuid;

use crate::auth::resource::{HostId, NodeId, OrgId};

const WILDCARD_CHARS: &[char] = &['#', '+'];
const UUID_LEN: usize = 36;

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Claims check failed: {0}
    Claims(#[from] crate::auth::claims::Error),
    /// Failed to parse topic UUID: {0}
    ParseUuid(uuid::Error),
    /// Topic does not contain a valid UUID.
    TopicLen,
    /// Unknown Topic type: {0}
    UnknownTopic(String),
}

#[derive(Debug, Deserialize)]
pub struct AclRequest {
    pub operation: OperationType,
    pub username: String,
    pub topic: Topic,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub enum OperationType {
    #[serde(rename = "1")]
    Publish,
    #[serde(rename = "2")]
    Subscribe,
}

#[derive(Debug)]
pub enum Topic {
    /// `/orgs/<uuid>/...`
    Orgs(OrgId),
    /// `/hosts/<uuid>/...`
    Hosts(HostId),
    /// `/nodes/<uuid>/...`
    Nodes(NodeId),
    /// `/bv/hosts/<uuid>/status`
    BvHostsStatus(HostId),
    /// Any topic containing `#` or `+`.
    Wildcard(String),
}

impl FromStr for Topic {
    type Err = Error;

    fn from_str<'s>(s: &'s str) -> Result<Self, Self::Err> {
        let parse_uuid = |text: &'s str| {
            if text.len() < UUID_LEN {
                Err(Error::TopicLen)
            } else {
                let (id, rest) = text.split_at(UUID_LEN);
                id.parse::<Uuid>()
                    .map(|id| (id, rest))
                    .map_err(Error::ParseUuid)
            }
        };

        if s.contains(WILDCARD_CHARS) {
            Ok(Topic::Wildcard(s.into()))
        } else if let Some(suffix) = s.strip_prefix("/orgs/") {
            let (id, _) = parse_uuid(suffix)?;
            Ok(Topic::Orgs(id.into()))
        } else if let Some(suffix) = s.strip_prefix("/hosts/") {
            let (id, _) = parse_uuid(suffix)?;
            Ok(Topic::Hosts(id.into()))
        } else if let Some(suffix) = s.strip_prefix("/nodes/") {
            let (id, _) = parse_uuid(suffix)?;
            Ok(Topic::Nodes(id.into()))
        } else if let Some(suffix) = s.strip_prefix("/bv/hosts/") {
            match parse_uuid(suffix)? {
                (id, "/status") => Ok(Topic::BvHostsStatus(id.into())),
                _ => Err(Error::UnknownTopic(s.into())),
            }
        } else {
            Err(Error::UnknownTopic(s.into()))
        }
    }
}

impl<'de> Deserialize<'de> for Topic {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).and_then(|s| s.parse().map_err(serde::de::Error::custom))
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    #[test]
    fn parse_topic() {
        let uuid = Uuid::new_v4();
        let tests = [
            (format!("/org/{uuid}"), false),
            (format!("orgs/{uuid}"), false),
            (format!("/orgs/{uuid}"), true),
            (format!("/orgs/{uuid}/"), true),
            (format!("/orgs/{uuid}/stuff"), true),
            (format!("/hosts/{uuid}/"), true),
            (format!("/nodes/{uuid}/"), true),
            (format!("/bv/hosts/{uuid}/status"), true),
            (format!("/bv/hosts/{uuid}/status123"), false),
            (format!("/bv/hosts/{uuid}/stat"), false),
            ("/bv/hosts/#".to_string(), true),
        ];

        for (test, valid) in tests {
            let topic = test.parse::<Topic>();
            if topic.is_ok() && !valid {
                panic!("should not be valid: {topic:#?}");
            } else if topic.is_err() && valid {
                panic!("should be valid: {test}");
            }
        }
    }

    #[test]
    fn parse_acl_request() {
        let json = r#"{
            "operation": "1",
            "username": "jwt",
            "topic": "/bv/hosts/c1ce7b5c-fde1-40ab-afa5-06c8265b63f8/status"
        }"#;
        let _: AclRequest = serde_json::from_str(json).unwrap();
    }
}
