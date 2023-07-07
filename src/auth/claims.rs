use std::collections::HashMap;

use displaydoc::Display;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tonic::Status;
use tracing::error;

use crate::models::{Conn, Host, Node, Org};
use crate::timestamp::SecondsUtc;

use super::endpoint::{Endpoint, Endpoints};
use super::resource::{HostId, NodeId, OrgId, Resource, ResourceEntry, UserId};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Host does not have permission to access the requested HostId.
    EnsureHost,
    /// Node does not have permission to access the requested NodeId.
    EnsureNode,
    /// Org does not have permission to access the requested OrgId.
    EnsureOrg,
    /// User does not have permission to access the requested UserId.
    EnsureUser,
    /// Expiration time is before issue time.
    ExpiresBeforeIssued,
    /// Failed to lookup permissions for host {0}: {1},
    LookupHost(HostId, Box<crate::Error>),
    /// Failed to lookup permissions for node {0}: {1},
    LookupNode(NodeId, Box<crate::Error>),
    /// Failed to lookup permissions for org {0}: {1},
    LookupOrg(OrgId, Box<crate::Error>),
    /// Failed to lookup permissions for user {0}: {1},
    LookupUser(UserId, Box<crate::Error>),
    /// User is not an admin of the org.
    NotOrgAdmin,
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        error!("{}: {err}", std::any::type_name::<Error>());

        use Error::*;
        match err {
            EnsureHost | EnsureNode | EnsureOrg | EnsureUser | NotOrgAdmin => {
                Status::permission_denied("Access denied.")
            }
            ExpiresBeforeIssued | LookupHost(..) | LookupNode(..) | LookupOrg(..)
            | LookupUser(..) => Status::internal("Internal error."),
        }
    }
}

/// A serializable representation of the auth token claims.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claims {
    #[serde(flatten)]
    pub resource_entry: ResourceEntry,
    #[serde(flatten)]
    pub expirable: Expirable,
    pub endpoints: Endpoints,
    pub data: HashMap<String, String>,
}

impl Claims {
    pub fn new(resource: Resource, expirable: Expirable, endpoints: Endpoints) -> Self {
        Claims {
            resource_entry: resource.into(),
            expirable,
            endpoints,
            data: Default::default(),
        }
    }

    pub fn with_data(mut self, data: HashMap<String, String>) -> Self {
        self.data = data;
        self
    }

    pub fn from_now<I>(expires: chrono::Duration, resource: Resource, endpoints: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<Endpoint>,
    {
        let expirable = Expirable::from_now(expires);
        let endpoints = endpoints.into_iter().map(Into::into).collect();
        Self::new(resource, expirable, endpoints)
    }

    pub fn user_from_now<I>(expires: chrono::Duration, user_id: UserId, endpoints: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<Endpoint>,
    {
        Self::from_now(expires, Resource::User(user_id), endpoints)
    }

    pub fn resource(&self) -> Resource {
        self.resource_entry.into()
    }

    pub fn expires_at(&self) -> SecondsUtc {
        self.expirable.expires_at
    }

    /// Ensure that `Claims` can access the requested `Resource` as an Org member.
    pub async fn ensure(self, resource: Resource, conn: &mut Conn) -> Result<Ensure, Error> {
        let claims = match resource {
            Resource::User(id) => self.ensure_user(id).map(Into::into),
            Resource::Org(id) => self.ensure_org(id, false, conn).await.map(Into::into),
            Resource::Host(id) => self.ensure_host(id, false, conn).await.map(Into::into),
            Resource::Node(id) => self.ensure_node(id, false, conn).await.map(Into::into),
        }?;

        Ok(Ensure { resource, claims })
    }

    /// Ensure that `Claims` can access the requested `Resource` as an Org admin.
    pub async fn ensure_admin(self, resource: Resource, conn: &mut Conn) -> Result<Ensure, Error> {
        let claims = match resource {
            Resource::User(id) => self.ensure_user(id).map(Into::into),
            Resource::Org(id) => self.ensure_org(id, true, conn).await.map(Into::into),
            Resource::Host(id) => self.ensure_host(id, true, conn).await.map(Into::into),
            Resource::Node(id) => self.ensure_node(id, true, conn).await.map(Into::into),
        }?;

        Ok(Ensure { resource, claims })
    }

    /// Ensure that `Claims` can access the requested `UserId`.
    pub fn ensure_user(self, user_id: UserId) -> Result<UserClaims, Error> {
        let claims = self;
        match claims.resource() {
            Resource::User(id) if id == user_id => Ok(UserClaims { claims, user_id }),
            _ => Err(Error::EnsureUser),
        }
    }

    /// Ensure that `Claims` can access the requested `OrgId`.
    pub async fn ensure_org(
        self,
        org_id: OrgId,
        org_admin: bool,
        conn: &mut Conn,
    ) -> Result<OrgClaims, Error> {
        let claims = self;
        match claims.resource() {
            Resource::User(user_id) => {
                let has_org = if org_admin {
                    Org::is_admin(user_id, org_id, conn).await
                } else {
                    Org::is_member(user_id, org_id, conn).await
                };

                has_org
                    .map_err(|err| Error::LookupOrg(org_id, Box::new(err)))?
                    .then_some(OrgClaims { claims, org_id })
                    .ok_or(Error::EnsureOrg)
            }

            Resource::Org(id) if id == org_id => Ok(OrgClaims { claims, org_id }),

            _ => Err(Error::EnsureOrg),
        }
    }

    /// Ensure that `Claims` can access the requested `HostId`.
    pub async fn ensure_host(
        self,
        host_id: HostId,
        org_admin: bool,
        conn: &mut Conn,
    ) -> Result<HostClaims, Error> {
        let claims = self;
        match claims.resource() {
            Resource::User(user_id) => {
                let host = Host::find_by_id(host_id, conn)
                    .await
                    .map_err(|err| Error::LookupHost(host_id, Box::new(err)))?;

                let has_org = if org_admin {
                    Org::is_admin(user_id, host.org_id, conn).await
                } else {
                    Org::is_member(user_id, host.org_id, conn).await
                };

                has_org
                    .map_err(|err| Error::LookupOrg(host.org_id, Box::new(err)))?
                    .then_some(HostClaims { claims, host_id })
                    .ok_or(Error::EnsureHost)
            }

            Resource::Org(org_id) => {
                let host = Host::find_by_id(host_id, conn)
                    .await
                    .map_err(|err| Error::LookupHost(host_id, Box::new(err)))?;

                if host.org_id == org_id {
                    Ok(HostClaims { claims, host_id })
                } else {
                    Err(Error::EnsureHost)
                }
            }

            Resource::Host(id) if id == host_id => Ok(HostClaims { claims, host_id }),

            _ => Err(Error::EnsureHost),
        }
    }

    /// Ensure that `Claims` can access the requested `NodeId`.
    pub async fn ensure_node(
        self,
        node_id: NodeId,
        org_admin: bool,
        conn: &mut Conn,
    ) -> Result<NodeClaims, Error> {
        let claims = self;
        match claims.resource() {
            Resource::User(user_id) => {
                let node = Node::find_by_id(node_id, conn)
                    .await
                    .map_err(|err| Error::LookupNode(node_id, Box::new(err)))?;

                let has_org = if org_admin {
                    Org::is_admin(user_id, node.org_id, conn).await
                } else {
                    Org::is_member(user_id, node.org_id, conn).await
                };

                has_org
                    .map_err(|err| Error::LookupOrg(node.org_id, Box::new(err)))?
                    .then_some(NodeClaims { claims, node_id })
                    .ok_or(Error::EnsureNode)
            }

            Resource::Org(org_id) => {
                let node = Node::find_by_id(node_id, conn)
                    .await
                    .map_err(|err| Error::LookupNode(node_id, Box::new(err)))?;

                if node.org_id == org_id {
                    Ok(NodeClaims { claims, node_id })
                } else {
                    Err(Error::EnsureNode)
                }
            }

            Resource::Host(host_id) => {
                let node = Node::find_by_id(node_id, conn)
                    .await
                    .map_err(|err| Error::LookupNode(node_id, Box::new(err)))?;

                if node.host_id == host_id {
                    Ok(NodeClaims { claims, node_id })
                } else {
                    Err(Error::EnsureNode)
                }
            }

            Resource::Node(id) if id == node_id => Ok(NodeClaims { claims, node_id }),

            _ => Err(Error::EnsureNode),
        }
    }
}

pub enum EnsureClaims {
    User(UserClaims),
    Org(OrgClaims),
    Host(HostClaims),
    Node(NodeClaims),
}

/// Validated `Claims` for some `Resource`.
pub struct Ensure {
    resource: Resource,
    claims: EnsureClaims,
}

impl Ensure {
    pub fn resource(&self) -> Resource {
        self.resource
    }

    pub fn user(&self) -> Option<&UserClaims> {
        if let EnsureClaims::User(claims) = &self.claims {
            Some(claims)
        } else {
            None
        }
    }

    pub fn org(&self) -> Option<&OrgClaims> {
        if let EnsureClaims::Org(claims) = &self.claims {
            Some(claims)
        } else {
            None
        }
    }

    pub fn host(&self) -> Option<&HostClaims> {
        if let EnsureClaims::Host(claims) = &self.claims {
            Some(claims)
        } else {
            None
        }
    }

    pub fn node(&self) -> Option<&NodeClaims> {
        if let EnsureClaims::Node(claims) = &self.claims {
            Some(claims)
        } else {
            None
        }
    }
}

/// Validated `Claims` for some `UserId`.
pub struct UserClaims {
    claims: Claims,
    user_id: UserId,
}

impl UserClaims {
    pub fn claims(&self) -> &Claims {
        &self.claims
    }

    pub fn user_id(&self) -> UserId {
        self.user_id
    }
}

impl From<UserClaims> for EnsureClaims {
    fn from(claims: UserClaims) -> Self {
        EnsureClaims::User(claims)
    }
}

/// Validated `Claims` for some `OrgId`.
pub struct OrgClaims {
    claims: Claims,
    org_id: OrgId,
}

impl OrgClaims {
    pub fn claims(&self) -> &Claims {
        &self.claims
    }

    pub fn org_id(&self) -> OrgId {
        self.org_id
    }
}

impl From<OrgClaims> for EnsureClaims {
    fn from(claims: OrgClaims) -> Self {
        EnsureClaims::Org(claims)
    }
}

/// Validated `Claims` for some `HostId`.
pub struct HostClaims {
    claims: Claims,
    host_id: HostId,
}

impl HostClaims {
    pub fn claims(&self) -> &Claims {
        &self.claims
    }

    pub fn host_id(&self) -> HostId {
        self.host_id
    }
}

impl From<HostClaims> for EnsureClaims {
    fn from(claims: HostClaims) -> Self {
        EnsureClaims::Host(claims)
    }
}

/// Validated `Claims` for some `NodeId`.
pub struct NodeClaims {
    claims: Claims,
    node_id: NodeId,
}

impl NodeClaims {
    pub fn claims(&self) -> &Claims {
        &self.claims
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id
    }
}

impl From<NodeClaims> for EnsureClaims {
    fn from(claims: NodeClaims) -> Self {
        EnsureClaims::Node(claims)
    }
}

/// `Expirable` ensures that `issued_at` is not after `expires_at`.
///
/// It also serializes and deserializes with second precision since JWTs don't
/// support nanoseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Expirable {
    #[serde(rename = "iat")]
    pub issued_at: SecondsUtc,
    #[serde(rename = "exp")]
    pub expires_at: SecondsUtc,
}

impl Expirable {
    pub fn new(issued_at: SecondsUtc, expires_at: SecondsUtc) -> Result<Self, Error> {
        if expires_at < issued_at {
            return Err(Error::ExpiresBeforeIssued);
        }

        Ok(Self {
            issued_at,
            expires_at,
        })
    }

    pub fn from_now(expires: chrono::Duration) -> Self {
        let issued_at = SecondsUtc::now();
        let expires_at = issued_at + expires;

        Self {
            issued_at,
            expires_at,
        }
    }

    pub fn duration(&self) -> chrono::Duration {
        self.expires_at - self.issued_at
    }
}
