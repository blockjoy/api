use std::collections::HashSet;
use std::fmt;

use derive_more::{Deref, Display, From, FromStr};
use diesel_derive_enum::DbEnum;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display as DisplayDoc;
use serde::{Deserialize, Serialize};
use strum::{EnumString, IntoStaticStr};
use thiserror::Error;
use tonic::Status;
use uuid::Uuid;

use crate::database::Conn;
use crate::grpc::common;
use crate::models::schema::sql_types;
use crate::models::User;

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Missing `resource_id`.
    MissingResourceId,
    /// Failed to parse ResourceId: {0}
    ParseResourceId(uuid::Error),
    /// Unknown resource type.
    UnknownResourceType,
    /// Resource user error: {0}
    User(#[from] crate::models::user::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            MissingResourceId | ParseResourceId(_) => Status::invalid_argument("resource_id"),
            UnknownResourceType => Status::invalid_argument("resource"),
            User(err) => err.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    User(UserId),
    Org(OrgId),
    Host(HostId),
    Node(NodeId),
}

impl Resource {
    pub fn id(self) -> ResourceId {
        self.into()
    }

    pub const fn user(self) -> Option<UserId> {
        if let Resource::User(id) = self {
            Some(id)
        } else {
            None
        }
    }

    pub const fn org(self) -> Option<OrgId> {
        if let Resource::Org(id) = self {
            Some(id)
        } else {
            None
        }
    }

    pub const fn host(self) -> Option<HostId> {
        if let Resource::Host(id) = self {
            Some(id)
        } else {
            None
        }
    }

    pub const fn node(self) -> Option<NodeId> {
        if let Resource::Node(id) = self {
            Some(id)
        } else {
            None
        }
    }
}

impl From<UserId> for Resource {
    fn from(id: UserId) -> Self {
        Resource::User(id)
    }
}

impl From<OrgId> for Resource {
    fn from(id: OrgId) -> Self {
        Resource::Org(id)
    }
}

impl From<NodeId> for Resource {
    fn from(id: NodeId) -> Self {
        Resource::Node(id)
    }
}

impl From<HostId> for Resource {
    fn from(id: HostId) -> Self {
        Resource::Host(id)
    }
}

impl fmt::Display for Resource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (resource_name, resource_id): (&str, ResourceId) = match *self {
            Resource::User(id) => ("user", id.into()),
            Resource::Org(id) => ("org", id.into()),
            Resource::Host(id) => ("host", id.into()),
            Resource::Node(id) => ("node", id.into()),
        };
        write!(f, "{resource_name} resource {resource_id}")
    }
}

impl From<Resource> for ResourceId {
    fn from(resource: Resource) -> Self {
        match resource {
            Resource::User(UserId(id)) => ResourceId(id),
            Resource::Org(OrgId(id)) => ResourceId(id),
            Resource::Host(HostId(id)) => ResourceId(id),
            Resource::Node(NodeId(id)) => ResourceId(id),
        }
    }
}

impl From<Resource> for ResourceType {
    fn from(resource: Resource) -> Self {
        match resource {
            Resource::User(_) => ResourceType::User,
            Resource::Org(_) => ResourceType::Org,
            Resource::Host(_) => ResourceType::Host,
            Resource::Node(_) => ResourceType::Node,
        }
    }
}

/// The types of resources that can grant authorization.
///
/// These are in hierarchial order, where a user has access to multiple orgs,
/// while an org has multiple hosts, and a host has multiple nodes.
#[derive(
    Clone,
    Copy,
    Debug,
    Display,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    EnumString,
    IntoStaticStr,
    DbEnum,
)]
#[ExistingTypePath = "sql_types::EnumResourceType"]
pub enum ResourceType {
    User,
    Org,
    Host,
    Node,
}

impl From<ResourceType> for common::Resource {
    fn from(ty: ResourceType) -> Self {
        match ty {
            ResourceType::User => common::Resource::User,
            ResourceType::Org => common::Resource::Org,
            ResourceType::Host => common::Resource::Host,
            ResourceType::Node => common::Resource::Node,
        }
    }
}

impl TryFrom<common::Resource> for ResourceType {
    type Error = Error;

    fn try_from(proto: common::Resource) -> Result<Self, Self::Error> {
        match proto {
            common::Resource::Unspecified => Err(Error::UnknownResourceType),
            common::Resource::User => Ok(ResourceType::User),
            common::Resource::Org => Ok(ResourceType::Org),
            common::Resource::Node => Ok(ResourceType::Node),
            common::Resource::Host => Ok(ResourceType::Host),
        }
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    Display,
    Hash,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    DieselNewType,
    Deref,
    From,
    FromStr,
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

#[derive(
    Clone,
    Copy,
    Debug,
    Display,
    Hash,
    PartialEq,
    Eq,
    Deref,
    From,
    FromStr,
    PartialOrd,
    Ord,
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
    Deref,
    From,
    FromStr,
    PartialOrd,
    Ord,
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
    Deref,
    From,
    FromStr,
    PartialOrd,
    Ord,
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
    Deref,
    From,
    FromStr,
    PartialOrd,
    Ord,
    DieselNewType,
)]
pub struct NodeId(Uuid);

#[derive(Clone, Debug)]
pub enum Resources {
    One(Resource),
    Many(Vec<Resource>),
}

impl<T> From<T> for Resources
where
    T: Into<Resource>,
{
    fn from(item: T) -> Self {
        Resources::One(item.into())
    }
}

impl<T> From<&HashSet<T>> for Resources
where
    T: Into<Resource> + Copy,
{
    fn from(items: &HashSet<T>) -> Self {
        Resources::Many(items.iter().map(|i| (*i).into()).collect())
    }
}

/// A serializable representation of the resource type and id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceEntry {
    pub resource_type: ResourceType,
    pub resource_id: ResourceId,
}

impl ResourceEntry {
    pub const fn new(resource_type: ResourceType, resource_id: ResourceId) -> Self {
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

impl TryFrom<&common::EntityUpdate> for Resource {
    type Error = Error;

    fn try_from(update: &common::EntityUpdate) -> Result<Self, Self::Error> {
        let id: Uuid = update
            .resource_id
            .as_ref()
            .ok_or(Error::MissingResourceId)?
            .parse()
            .map_err(Error::ParseResourceId)?;

        match update.resource() {
            common::Resource::Unspecified => Err(Error::UnknownResourceType),
            common::Resource::User => Ok(Resource::User(UserId(id))),
            common::Resource::Org => Ok(Resource::Org(OrgId(id))),
            common::Resource::Host => Ok(Resource::Host(HostId(id))),
            common::Resource::Node => Ok(Resource::Node(NodeId(id))),
        }
    }
}

impl common::EntityUpdate {
    pub async fn from_resource<R>(resource: R, conn: &mut Conn<'_>) -> Result<Self, Error>
    where
        R: Into<Resource> + Send,
    {
        let entry = ResourceEntry::from(resource.into());
        let user = if entry.resource_type == ResourceType::User {
            Some(User::find_by_id((*entry.resource_id).into(), conn).await?)
        } else {
            None
        };

        Ok(common::EntityUpdate {
            resource: common::Resource::from(entry.resource_type).into(),
            resource_id: Some(entry.resource_id.to_string()),
            name: user.as_ref().map(User::name),
            email: user.as_ref().map(|u| u.email.clone()),
        })
    }

    pub fn from_user(user: &User) -> Self {
        common::EntityUpdate {
            resource: common::Resource::User.into(),
            resource_id: Some(user.id.to_string()),
            name: Some(user.name()),
            email: Some(user.email.clone()),
        }
    }

    pub fn from_org(org_id: OrgId) -> Self {
        common::EntityUpdate {
            resource: common::Resource::Org.into(),
            resource_id: Some(org_id.to_string()),
            name: None,
            email: None,
        }
    }
}
