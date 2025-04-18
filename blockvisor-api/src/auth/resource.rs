use std::collections::HashSet;
use std::fmt;

use derive_more::{Deref, Display, From, FromStr};
use diesel_derive_enum::DbEnum;
use diesel_derive_newtype::DieselNewType;
use displaydoc::Display as DisplayDoc;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::database::Conn;
use crate::grpc::{Status, common};
use crate::model::schema::sql_types;
use crate::model::{Host, Node, Org, User};

#[derive(Debug, DisplayDoc, Error)]
pub enum Error {
    /// Resource host error: {0}
    Host(#[from] crate::model::host::Error),
    /// Resource node error: {0}
    Node(#[from] crate::model::node::Error),
    /// Resource org error: {0}
    Org(#[from] crate::model::org::Error),
    /// Failed to parse ResourceId: {0}
    ParseResourceId(uuid::Error),
    /// No org id for public host.
    PublicHost,
    /// Unknown resource type.
    UnknownResourceType,
    /// Resource user error: {0}
    User(#[from] crate::model::user::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            ParseResourceId(_) => Status::invalid_argument("resource_id"),
            PublicHost => Status::failed_precondition("no org for public host"),
            UnknownResourceType => Status::invalid_argument("resource_type"),
            Host(err) => err.into(),
            Node(err) => err.into(),
            Org(err) => err.into(),
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
    #[allow(clippy::missing_const_for_fn)]
    pub fn new(typ: ResourceType, id: ResourceId) -> Self {
        match typ {
            ResourceType::User => Resource::User(UserId(*id)),
            ResourceType::Org => Resource::Org(OrgId(*id)),
            ResourceType::Host => Resource::Host(HostId(*id)),
            ResourceType::Node => Resource::Node(NodeId(*id)),
        }
    }

    pub fn typ(&self) -> ResourceType {
        self.into()
    }

    pub fn id(&self) -> ResourceId {
        self.into()
    }

    pub fn user(self) -> Option<UserId> {
        matches!(self, Resource::User(_)).then_some(UserId(*self.id()))
    }

    pub fn org(self) -> Option<OrgId> {
        matches!(self, Resource::Org(_)).then_some(OrgId(*self.id()))
    }

    pub fn host(self) -> Option<HostId> {
        matches!(self, Resource::Host(_)).then_some(HostId(*self.id()))
    }

    pub fn node(self) -> Option<NodeId> {
        matches!(self, Resource::Node(_)).then_some(NodeId(*self.id()))
    }

    pub async fn id_exists(self, conn: &mut Conn<'_>) -> Result<ResourceId, Error> {
        match self {
            Resource::User(id) => Ok(User::by_id(id, conn).await.map(|_| id.into())?),
            Resource::Org(id) => Ok(Org::by_id(id, conn).await.map(|_| id.into())?),
            Resource::Host(id) => Ok(Host::org_id(id, conn).await.map(|_| id.into())?),
            Resource::Node(id) => Ok(Node::org_id(id, conn).await.map(|_| id.into())?),
        }
    }

    pub async fn org_id(self, conn: &mut Conn<'_>) -> Result<OrgId, Error> {
        match self {
            Resource::User(id) => Ok(Org::find_personal(id, conn).await.map(|org| org.id)?),
            Resource::Org(id) => Ok(id),
            Resource::Host(id) => Ok(Host::org_id(id, conn).await?.ok_or(Error::PublicHost)?),
            Resource::Node(id) => Ok(Node::org_id(id, conn).await?),
        }
    }
}

impl fmt::Display for Resource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.typ(), self.id())
    }
}

impl From<UserId> for Resource {
    fn from(id: UserId) -> Self {
        Resource::User(id)
    }
}

impl From<&UserId> for Resource {
    fn from(id: &UserId) -> Self {
        Resource::User(*id)
    }
}

impl From<OrgId> for Resource {
    fn from(id: OrgId) -> Self {
        Resource::Org(id)
    }
}

impl From<&OrgId> for Resource {
    fn from(id: &OrgId) -> Self {
        Resource::Org(*id)
    }
}

impl From<NodeId> for Resource {
    fn from(id: NodeId) -> Self {
        Resource::Node(id)
    }
}

impl From<&NodeId> for Resource {
    fn from(id: &NodeId) -> Self {
        Resource::Node(*id)
    }
}

impl From<HostId> for Resource {
    fn from(id: HostId) -> Self {
        Resource::Host(id)
    }
}

impl From<&HostId> for Resource {
    fn from(id: &HostId) -> Self {
        Resource::Host(*id)
    }
}

impl From<&Resource> for ResourceId {
    fn from(resource: &Resource) -> Self {
        match resource {
            Resource::User(UserId(id)) => ResourceId(*id),
            Resource::Org(OrgId(id)) => ResourceId(*id),
            Resource::Host(HostId(id)) => ResourceId(*id),
            Resource::Node(NodeId(id)) => ResourceId(*id),
        }
    }
}

impl From<&Resource> for ResourceType {
    fn from(resource: &Resource) -> Self {
        match resource {
            Resource::User(_) => ResourceType::User,
            Resource::Org(_) => ResourceType::Org,
            Resource::Host(_) => ResourceType::Host,
            Resource::Node(_) => ResourceType::Node,
        }
    }
}

impl<R> From<R> for common::Resource
where
    R: Into<Resource> + Send,
{
    fn from(resource: R) -> Self {
        let resource = resource.into();
        common::Resource {
            resource_type: common::ResourceType::from(resource.typ()).into(),
            resource_id: resource.id().to_string(),
        }
    }
}

impl TryFrom<&common::Resource> for Resource {
    type Error = Error;

    fn try_from(resource: &common::Resource) -> Result<Self, Self::Error> {
        let typ: ResourceType = resource.resource_type().try_into()?;
        let id: ResourceId = resource
            .resource_id
            .parse()
            .map_err(Error::ParseResourceId)?;

        Ok(Resource::new(typ, id))
    }
}

/// The types of resources that can grant authorization.
///
/// These are in hierarchial order, where a user has access to multiple orgs,
/// while an org has multiple hosts, and a host has multiple nodes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, DbEnum)]
#[ExistingTypePath = "sql_types::EnumResourceType"]
pub enum ResourceType {
    User,
    Org,
    Host,
    Node,
}

impl fmt::Display for ResourceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResourceType::User => write!(f, "user"),
            ResourceType::Org => write!(f, "org"),
            ResourceType::Host => write!(f, "host"),
            ResourceType::Node => write!(f, "node"),
        }
    }
}

impl From<ResourceType> for common::ResourceType {
    fn from(resource_type: ResourceType) -> Self {
        match resource_type {
            ResourceType::User => common::ResourceType::User,
            ResourceType::Org => common::ResourceType::Org,
            ResourceType::Host => common::ResourceType::Host,
            ResourceType::Node => common::ResourceType::Node,
        }
    }
}

impl TryFrom<common::ResourceType> for ResourceType {
    type Error = Error;

    fn try_from(resource_type: common::ResourceType) -> Result<Self, Self::Error> {
        match resource_type {
            common::ResourceType::Unspecified => Err(Error::UnknownResourceType),
            common::ResourceType::User => Ok(ResourceType::User),
            common::ResourceType::Org => Ok(ResourceType::Org),
            common::ResourceType::Node => Ok(ResourceType::Node),
            common::ResourceType::Host => Ok(ResourceType::Host),
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
    Serialize,
    Deserialize,
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
    All,
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

impl<T> From<&[T]> for Resources
where
    T: Into<Resource> + Copy,
{
    fn from(items: &[T]) -> Self {
        Resources::Many(items.iter().map(|i| (*i).into()).collect())
    }
}

impl<T> From<&Vec<T>> for Resources
where
    T: Into<Resource> + Copy,
{
    fn from(items: &Vec<T>) -> Self {
        Resources::Many(items.iter().map(|i| (*i).into()).collect())
    }
}

impl<const N: usize, T> From<[T; N]> for Resources
where
    T: Into<Resource> + Copy,
{
    fn from(items: [T; N]) -> Self {
        match N {
            0 => Resources::All,
            1 => Resources::One(items[0].into()),
            _ => items.as_ref().into(),
        }
    }
}

/// A serializable representation for storing inside JWTs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimsResource {
    pub resource_type: ResourceType,
    pub resource_id: ResourceId,
}

impl From<Resource> for ClaimsResource {
    fn from(resource: Resource) -> Self {
        ClaimsResource {
            resource_type: resource.typ(),
            resource_id: resource.id(),
        }
    }
}

impl From<ClaimsResource> for Resource {
    fn from(claims: ClaimsResource) -> Resource {
        Resource::new(claims.resource_type, claims.resource_id)
    }
}
