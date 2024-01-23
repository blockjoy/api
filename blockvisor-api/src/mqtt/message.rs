use derive_more::From;
use displaydoc::Display;
use prost::Message as _;
use thiserror::Error;

use crate::auth::resource::{HostId, NodeId, OrgId};
use crate::grpc::{api, common};
use crate::models::{Host, Node, Org, User};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Missing `host_id`. This should not happen.
    MissingHostId,
    /// Missing `node_id`. This should not happen.
    MissingNodeId,
    /// Missing `org_id`. This should not happen.
    MissingOrgId,
}

#[derive(From)]
pub enum Message {
    Command(api::Command),
    OrgMessage(api::OrgMessage),
    HostMessage(api::HostMessage),
    NodeMessage(Box<api::NodeMessage>),
}

impl From<api::NodeMessage> for Message {
    fn from(value: api::NodeMessage) -> Self {
        Self::NodeMessage(Box::new(value))
    }
}

impl Message {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Message::Command(msg) => msg.encode_to_vec(),
            Message::OrgMessage(msg) => msg.encode_to_vec(),
            Message::HostMessage(msg) => msg.encode_to_vec(),
            Message::NodeMessage(msg) => msg.encode_to_vec(),
        }
    }

    pub fn channels(&self) -> Result<Vec<String>, Error> {
        match self {
            Message::Command(msg) => msg.channels(),
            Message::OrgMessage(msg) => msg.channels(),
            Message::HostMessage(msg) => msg.channels(),
            Message::NodeMessage(msg) => msg.channels(),
        }
    }
}

impl api::Command {
    fn channels(&self) -> Result<Vec<String>, Error> {
        let host_id = self.host_id().ok_or(Error::MissingHostId)?;
        let mut channels = vec![format!("/hosts/{host_id}/commands")];

        if let Some(node_id) = self.node_id() {
            channels.push(format!("/hosts/{host_id}/nodes/{node_id}/commands"));
            channels.push(format!("/nodes/{node_id}/commands"));
        }

        Ok(channels)
    }

    fn host_id(&self) -> Option<&str> {
        match self.command.as_ref()? {
            api::command::Command::Node(cmd) => Some(&cmd.host_id),
            api::command::Command::Host(cmd) => Some(&cmd.host_id),
        }
    }

    fn node_id(&self) -> Option<&str> {
        match self.command.as_ref()? {
            api::command::Command::Node(cmd) => Some(&cmd.node_id),
            api::command::Command::Host(_) => None,
        }
    }
}

impl api::OrgMessage {
    fn channels(&self) -> Result<Vec<String>, Error> {
        let org_id = self.org_id().ok_or(Error::MissingOrgId)?;

        Ok(vec![format!("/orgs/{org_id}")])
    }

    fn org_id(&self) -> Option<OrgId> {
        use api::org_message::Message::*;
        match self.message.as_ref()? {
            Created(api::OrgCreated { org, .. }) => org.as_ref()?.id.parse().ok(),
            Updated(api::OrgUpdated { org, .. }) => org.as_ref()?.id.parse().ok(),
            Deleted(api::OrgDeleted { org_id, .. }) => org_id.parse().ok(),
            InvitationCreated(api::InvitationCreated { org_id, .. }) => org_id.parse().ok(),
            InvitationAccepted(api::InvitationAccepted { org_id, .. }) => org_id.parse().ok(),
            InvitationDeclined(api::InvitationDeclined { org_id, .. }) => org_id.parse().ok(),
        }
    }

    pub const fn created(org: api::Org, created_by: common::EntityUpdate) -> Self {
        api::OrgMessage {
            message: Some(api::org_message::Message::Created(api::OrgCreated {
                org: Some(org),
                created_by: Some(created_by),
            })),
        }
    }

    pub const fn updated(org: api::Org, updated_by: common::EntityUpdate) -> Self {
        api::OrgMessage {
            message: Some(api::org_message::Message::Updated(api::OrgUpdated {
                org: Some(org),
                updated_by: Some(updated_by),
            })),
        }
    }

    pub fn deleted(org: &Org, deleted_by: common::EntityUpdate) -> Self {
        api::OrgMessage {
            message: Some(api::org_message::Message::Deleted(api::OrgDeleted {
                org_id: org.id.to_string(),
                deleted_by: Some(deleted_by),
            })),
        }
    }

    pub fn invitation_created(invitation: api::Invitation, org: &Org) -> Self {
        api::OrgMessage {
            message: Some(api::org_message::Message::InvitationCreated(
                api::InvitationCreated {
                    org_id: org.id.to_string(),
                    invitation: Some(invitation),
                },
            )),
        }
    }

    pub fn invitation_accepted(invitation: api::Invitation, org: &Org, user: User) -> Self {
        api::OrgMessage {
            message: Some(api::org_message::Message::InvitationAccepted(
                api::InvitationAccepted {
                    org_id: org.id.to_string(),
                    invitation: Some(invitation),
                    user: Some(api::User::from_model(user)),
                },
            )),
        }
    }

    pub fn invitation_declined(invitation: api::Invitation, org: &Org) -> Self {
        api::OrgMessage {
            message: Some(api::org_message::Message::InvitationDeclined(
                api::InvitationDeclined {
                    org_id: org.id.to_string(),
                    invitation: Some(invitation),
                },
            )),
        }
    }
}

impl api::HostMessage {
    fn channels(&self) -> Result<Vec<String>, Error> {
        let host_id = self.host_id().ok_or(Error::MissingHostId)?;

        Ok(vec![format!("/hosts/{host_id}")])
    }

    fn host_id(&self) -> Option<HostId> {
        use api::host_message::Message::*;
        match self.message.as_ref()? {
            Created(api::HostCreated { host, .. }) => host.as_ref()?.id.parse().ok(),
            Updated(api::HostUpdated { host, .. }) => host.as_ref()?.id.parse().ok(),
            Deleted(api::HostDeleted { host_id, .. }) => host_id.parse().ok(),
        }
    }

    pub const fn created(host: api::Host, created_by: common::EntityUpdate) -> Self {
        api::HostMessage {
            message: Some(api::host_message::Message::Created(api::HostCreated {
                host: Some(host),
                created_by: Some(created_by),
            })),
        }
    }

    pub const fn updated(host: api::Host, updated_by: common::EntityUpdate) -> Self {
        api::HostMessage {
            message: Some(api::host_message::Message::Updated(api::HostUpdated {
                host: Some(host),
                updated_by: Some(updated_by),
            })),
        }
    }

    pub fn updated_many(hosts: Vec<api::Host>, updated_by: &common::EntityUpdate) -> Vec<Self> {
        hosts
            .into_iter()
            .map(|host| api::HostMessage {
                message: Some(api::host_message::Message::Updated(api::HostUpdated {
                    host: Some(host),
                    updated_by: Some(updated_by.clone()),
                })),
            })
            .collect()
    }

    pub fn deleted(host: &Host, deleted_by: common::EntityUpdate) -> Self {
        Self {
            message: Some(api::host_message::Message::Deleted(api::HostDeleted {
                host_id: host.id.to_string(),
                deleted_by: Some(deleted_by),
            })),
        }
    }
}

impl api::NodeMessage {
    fn channels(&self) -> Result<Vec<String>, Error> {
        let org_id = self.org_id().ok_or(Error::MissingOrgId)?;
        let host_id = self.host_id().ok_or(Error::MissingHostId)?;
        let node_id = self.node_id().ok_or(Error::MissingNodeId)?;

        Ok(vec![
            format!("/orgs/{org_id}/nodes"),
            format!("/hosts/{host_id}/nodes"),
            format!("/nodes/{node_id}"),
        ])
    }

    fn node_id(&self) -> Option<NodeId> {
        use api::node_message::Message::*;
        match self.message.as_ref()? {
            Created(api::NodeCreated { node, .. }) => node.as_ref()?.id.parse().ok(),
            Updated(api::NodeUpdated { node, .. }) => node.as_ref()?.id.parse().ok(),
            Deleted(api::NodeDeleted { node_id, .. }) => node_id.parse().ok(),
        }
    }

    fn host_id(&self) -> Option<HostId> {
        use api::node_message::Message::*;
        match self.message.as_ref()? {
            Created(api::NodeCreated { node, .. }) => node.as_ref()?.host_id.parse().ok(),
            Updated(api::NodeUpdated { node, .. }) => node.as_ref()?.host_id.parse().ok(),
            Deleted(api::NodeDeleted { host_id, .. }) => host_id.parse().ok(),
        }
    }

    fn org_id(&self) -> Option<OrgId> {
        use api::node_message::Message::*;
        match self.message.as_ref()? {
            Created(api::NodeCreated { node, .. }) => node.as_ref()?.org_id.parse().ok(),
            Updated(api::NodeUpdated { node, .. }) => node.as_ref()?.org_id.parse().ok(),
            Deleted(api::NodeDeleted { org_id, .. }) => org_id.parse().ok(),
        }
    }

    pub const fn created(node: api::Node, created_by: common::EntityUpdate) -> Self {
        api::NodeMessage {
            message: Some(api::node_message::Message::Created(api::NodeCreated {
                node: Some(node),
                created_by: Some(created_by),
            })),
        }
    }

    pub const fn updated(node: api::Node, updated_by: common::EntityUpdate) -> Self {
        api::NodeMessage {
            message: Some(api::node_message::Message::Updated(api::NodeUpdated {
                node: Some(node),
                updated_by: Some(updated_by),
            })),
        }
    }

    pub fn updated_many(nodes: Vec<api::Node>, updated_by: &common::EntityUpdate) -> Vec<Self> {
        nodes
            .into_iter()
            .map(|node| api::NodeMessage {
                message: Some(api::node_message::Message::Updated(api::NodeUpdated {
                    node: Some(node),
                    updated_by: Some(updated_by.clone()),
                })),
            })
            .collect()
    }

    pub fn deleted(node: &Node, deleted_by: Option<common::EntityUpdate>) -> Self {
        api::NodeMessage {
            message: Some(api::node_message::Message::Deleted(api::NodeDeleted {
                node_id: node.id.to_string(),
                host_id: node.host_id.to_string(),
                org_id: node.org_id.to_string(),
                deleted_by,
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::auth::rbac::access::tests::view_authz;
    use crate::config::Context;
    use crate::models::{Command, CommandType};

    use super::*;

    fn user_update(user: &User) -> common::EntityUpdate {
        common::EntityUpdate {
            resource: common::Resource::User.into(),
            resource_id: Some(user.id.to_string()),
            name: Some(user.name()),
            email: Some(user.email.clone()),
        }
    }

    #[tokio::test]
    async fn test_send_command() {
        let (ctx, db) = Context::with_mocked().await.unwrap();
        let command = Command {
            id: Uuid::new_v4().into(),
            host_id: db.seed.host.id,
            exit_message: None,
            created_at: chrono::Utc::now(),
            completed_at: None,
            node_id: Some(db.seed.node.id),
            acked_at: None,
            retry_hint_seconds: None,
            exit_code: None,
            command_type: CommandType::NodeDelete,
        };

        let command = crate::grpc::command::node_delete(&command).unwrap();
        ctx.notifier.send(command).await.unwrap();
    }

    #[tokio::test]
    async fn test_send_hosts() {
        let (ctx, db) = Context::with_mocked().await.unwrap();
        let mut conn = db.conn().await;

        let host = db.seed.host.clone();
        let user = db.seed.user.clone();

        let api_host = api::Host::from_host(host.clone(), None, &mut conn)
            .await
            .unwrap();
        let resource = user_update(&user);

        let msg = api::HostMessage::created(api_host.clone(), resource.clone());
        ctx.notifier.send(msg).await.unwrap();

        let msg = api::HostMessage::updated(api_host, resource.clone());
        ctx.notifier.send(msg).await.unwrap();

        let msg = api::HostMessage::deleted(&host, resource);
        ctx.notifier.send(msg).await.unwrap();
    }

    #[tokio::test]
    async fn test_send_nodes() {
        let (ctx, db) = Context::with_mocked().await.unwrap();
        let mut conn = db.conn().await;

        let authz = view_authz(&ctx, db.seed.node.id, &mut conn).await;
        let node = db.seed.node.clone();
        let user = db.seed.user.clone();

        let api_node = api::Node::from_model(node.clone(), &authz, &mut conn)
            .await
            .unwrap();
        let resource = user_update(&user);

        let msg = api::NodeMessage::created(api_node.clone(), resource.clone());
        ctx.notifier.send(msg).await.unwrap();

        let msg = api::NodeMessage::updated(api_node, resource.clone());
        ctx.notifier.send(msg).await.unwrap();

        let msg = api::NodeMessage::deleted(&node, Some(resource));
        ctx.notifier.send(msg).await.unwrap();
    }
}
