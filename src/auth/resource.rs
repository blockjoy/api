use derive_more::{Deref, Display, From, FromStr, Into};
use diesel_derive_newtype::DieselNewType;
use serde::{Deserialize, Serialize};
use strum::{EnumString, IntoStaticStr};
use uuid::Uuid;

#[derive(
    Clone,
    Copy,
    Debug,
    Display,
    Hash,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Deref,
    From,
    FromStr,
    Into,
    DieselNewType,
)]
pub struct UserId(Uuid);

#[derive(
    Clone,
    Copy,
    Debug,
    Display,
    Hash,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Deref,
    From,
    FromStr,
    Into,
    DieselNewType,
)]
pub struct OrgId(Uuid);

#[derive(
    Clone,
    Copy,
    Debug,
    Display,
    Hash,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Deref,
    From,
    FromStr,
    Into,
    DieselNewType,
)]
pub struct HostId(Uuid);

#[derive(
    Clone,
    Copy,
    Debug,
    Display,
    Hash,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Deref,
    From,
    FromStr,
    Into,
    DieselNewType,
)]
pub struct NodeId(Uuid);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    User(UserId),
    Org(OrgId),
    Host(HostId),
    Node(NodeId),
}

impl Resource {
    pub fn user(&self) -> Option<UserId> {
        if let Resource::User(id) = self {
            Some(*id)
        } else {
            None
        }
    }

    pub fn org(&self) -> Option<OrgId> {
        if let Resource::Org(id) = self {
            Some(*id)
        } else {
            None
        }
    }

    pub fn host(&self) -> Option<HostId> {
        if let Resource::Host(id) = self {
            Some(*id)
        } else {
            None
        }
    }

    pub fn node(&self) -> Option<NodeId> {
        if let Resource::Node(id) = self {
            Some(*id)
        } else {
            None
        }
    }
}

/// The types of resources that can grant authorization. For example, a user has access to all nodes
/// it has created, but a host also has access to all nodes that run on it. They are hierarchically
/// sorted here, which is to say that a user has multiple orgs, an org has multiple hosts and a host
/// has multiple nodes.
#[derive(
    Clone, Copy, Debug, Display, PartialEq, Eq, Serialize, Deserialize, EnumString, IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum ResourceType {
    User,
    Org,
    Host,
    Node,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    DieselNewType,
    Deref,
    From,
    FromStr,
    Into,
)]
pub struct ResourceId(Uuid);

impl From<UserId> for ResourceId {
    fn from(user_id: UserId) -> Self {
        ResourceId(*user_id)
    }
}

impl From<OrgId> for ResourceId {
    fn from(org_id: OrgId) -> Self {
        ResourceId(*org_id)
    }
}

impl From<NodeId> for ResourceId {
    fn from(node_id: NodeId) -> Self {
        ResourceId(*node_id)
    }
}

impl From<HostId> for ResourceId {
    fn from(host_id: HostId) -> Self {
        ResourceId(*host_id)
    }
}

/// A serializable representation of the resource type and id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceEntry {
    #[serde(rename = "resource")]
    pub resource_type: ResourceType,
    pub resource_id: ResourceId,
}

impl ResourceEntry {
    pub fn new(resource_type: ResourceType, resource_id: ResourceId) -> Self {
        ResourceEntry {
            resource_type,
            resource_id,
        }
    }

    pub fn new_user(user_id: UserId) -> Self {
        ResourceEntry {
            resource_type: ResourceType::User,
            resource_id: (*user_id).into(),
        }
    }

    pub fn new_org(org_id: OrgId) -> Self {
        ResourceEntry {
            resource_type: ResourceType::Org,
            resource_id: (*org_id).into(),
        }
    }

    pub fn new_host(host_id: HostId) -> Self {
        ResourceEntry {
            resource_type: ResourceType::Host,
            resource_id: (*host_id).into(),
        }
    }

    pub fn new_node(node_id: NodeId) -> Self {
        ResourceEntry {
            resource_type: ResourceType::Node,
            resource_id: (*node_id).into(),
        }
    }
}

impl From<Resource> for ResourceEntry {
    fn from(resource: Resource) -> Self {
        let (resource_type, resource_id) = match resource {
            Resource::User(id) => (ResourceType::User, ResourceId(*id)),
            Resource::Org(id) => (ResourceType::Org, ResourceId(*id)),
            Resource::Host(id) => (ResourceType::Host, ResourceId(*id)),
            Resource::Node(id) => (ResourceType::Node, ResourceId(*id)),
        };

        ResourceEntry {
            resource_type,
            resource_id,
        }
    }
}

impl From<ResourceEntry> for Resource {
    fn from(entry: ResourceEntry) -> Self {
        let id = *entry.resource_id;
        match entry.resource_type {
            ResourceType::User => Resource::User(UserId(id)),
            ResourceType::Org => Resource::Org(OrgId(id)),
            ResourceType::Host => Resource::Host(HostId(id)),
            ResourceType::Node => Resource::Node(NodeId(id)),
        }
    }
}
