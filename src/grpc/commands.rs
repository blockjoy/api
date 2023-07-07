mod recover;
mod success;

use anyhow::anyhow;
use diesel_async::scoped_futures::ScopedFutureExt;

use crate::auth::claims::Claims;
use crate::auth::endpoint::Endpoint;
use crate::auth::resource::Resource;
use crate::models;
use crate::timestamp::NanosUtc;

use super::api::{self, command_service_server};
use super::helpers::required;

#[tonic::async_trait]
impl command_service_server::CommandService for super::Grpc {
    async fn update(
        &self,
        req: tonic::Request<api::CommandServiceUpdateRequest>,
    ) -> super::Resp<api::CommandServiceUpdateResponse> {
        self.trx(|c| update(req, c).scope_boxed())
            .await?
            .into_resp(&self.notifier)
            .await
    }

    async fn ack(
        &self,
        req: tonic::Request<api::CommandServiceAckRequest>,
    ) -> super::Resp<api::CommandServiceAckResponse> {
        self.trx(|c| ack(req, c).scope_boxed()).await
    }

    async fn pending(
        &self,
        req: tonic::Request<api::CommandServicePendingRequest>,
    ) -> super::Resp<api::CommandServicePendingResponse> {
        let mut conn = self.conn().await?;
        let resp = pending(req, &mut conn).await?;
        Ok(resp)
    }
}

async fn update(
    req: tonic::Request<api::CommandServiceUpdateRequest>,
    conn: &mut models::Conn,
) -> crate::Result<super::Outcome<api::CommandServiceUpdateResponse>> {
    let claims = conn.claims(&req, Endpoint::CommandUpdate).await?;
    let req = req.into_inner();
    let command = models::Command::find_by_id(req.id.parse()?, conn).await?;
    let host = command.host(conn).await?;
    let node = command.node(conn).await?;
    let is_allowed = access_allowed(claims, node.as_ref(), &host, conn).await?;
    if !is_allowed {
        super::forbidden!("Access denied");
    }
    let update_cmd = req.as_update()?;
    let cmd = update_cmd.update(conn).await?;
    let commands = match cmd.exit_status {
        Some(0) => {
            // Some responses require us to register success.
            success::register(&cmd, conn).await;
            vec![]
        }
        // Will match any integer other than 0.
        Some(_) => {
            // We got back an error status code. In practice, blockvisord sends 0 for
            // success and 1 for failure, but we treat every non-zero exit code as an
            // error, not just 1.
            recover::recover(&cmd, conn).await.unwrap_or_default()
        }
        None => vec![],
    };
    let command = api::Command::from_model(&cmd, conn).await?;
    let resp = api::CommandServiceUpdateResponse {
        command: Some(command.clone()),
    };
    Ok(super::Outcome::new(resp)
        .with_msg(command)
        .with_msgs(commands))
}

async fn ack(
    req: tonic::Request<api::CommandServiceAckRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::CommandServiceAckResponse> {
    let claims = conn.claims(&req, Endpoint::CommandAck).await?;
    let req = req.into_inner();
    let command = models::Command::find_by_id(req.id.parse()?, conn).await?;
    let host = command.host(conn).await?;
    let node = command.node(conn).await?;
    let is_allowed = access_allowed(claims, node.as_ref(), &host, conn).await?;
    if !is_allowed {
        super::forbidden!("Access denied");
    }
    if command.acked_at.is_none() {
        command.ack(conn).await?;
    }
    let resp = api::CommandServiceAckResponse {};
    Ok(tonic::Response::new(resp))
}

async fn pending(
    req: tonic::Request<api::CommandServicePendingRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::CommandServicePendingResponse> {
    let claims = conn.claims(&req, Endpoint::CommandPending).await?;
    let req = req.into_inner();
    let host_id = req.host_id.parse()?;
    let host = models::Host::find_by_id(host_id, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => models::Org::is_member(user_id, host.org_id, conn).await?,
        Resource::Org(org_id) => host.org_id == org_id,
        Resource::Host(host_id) => host_id == host.id,
        Resource::Node(_) => false,
    };
    if !is_allowed {
        super::forbidden!("Access denied");
    }
    let cmds = models::Command::find_pending_by_host(host_id, conn).await?;
    let mut commands = Vec::with_capacity(cmds.len());
    for cmd in cmds {
        let grpc_cmd = api::Command::from_model(&cmd, conn).await?;
        commands.push(grpc_cmd);
    }
    let resp = api::CommandServicePendingResponse { commands };
    Ok(tonic::Response::new(resp))
}

async fn access_allowed(
    claims: Claims,
    node: Option<&models::Node>,
    host: &models::Host,
    conn: &mut models::Conn,
) -> crate::Result<bool> {
    let allowed = match claims.resource() {
        Resource::User(user_id) => {
            if let Some(node) = &node {
                models::Org::is_member(user_id, node.org_id, conn).await?
            } else {
                models::Org::is_member(user_id, host.org_id, conn).await?
            }
        }
        Resource::Org(org_id) => {
            if let Some(node) = &node {
                org_id == node.org_id
            } else {
                org_id == host.org_id
            }
        }
        Resource::Host(host_id) => {
            if let Some(node) = &node {
                host_id == node.host_id
            } else {
                host_id == host.id
            }
        }
        Resource::Node(node_id) => {
            if let Some(node) = &node {
                node_id == node.id
            } else {
                false
            }
        }
    };
    Ok(allowed)
}

impl api::Command {
    pub async fn from_model(
        model: &models::Command,
        conn: &mut models::Conn,
    ) -> crate::Result<api::Command> {
        use api::command;
        use api::node_command::Command;
        use models::CommandType::*;

        // Extract the node id from the model, if there is one.
        let node_id = || model.node_id.ok_or_else(required("command.node_id"));
        // Closure to conveniently construct a api:: from the data that we need to have.
        let node_cmd = |command, node_id| -> Result<api::Command, crate::error::Error> {
            Ok(api::Command {
                id: model.id.to_string(),
                response: model.response.clone(),
                exit_code: model.exit_status,
                acked_at: model.acked_at.map(NanosUtc::from).map(Into::into),
                command: Some(command::Command::Node(api::NodeCommand {
                    node_id,
                    host_id: model.host_id.to_string(),
                    command: Some(command),
                    api_command_id: model.id.to_string(),
                    created_at: Some(NanosUtc::from(model.created_at).into()),
                })),
            })
        };
        // Construct a api::Command with the node id extracted from the `node.node_id` field.
        // Only `DeleteNode` does not use this method.
        let node_cmd_default_id = |command| node_cmd(command, node_id()?.to_string());

        let host_cmd = |host_id| {
            Ok(api::Command {
                id: model.id.to_string(),
                response: model.response.clone(),
                exit_code: model.exit_status,
                acked_at: model.acked_at.map(NanosUtc::from).map(Into::into),
                command: Some(command::Command::Host(api::HostCommand { host_id })),
            })
        };

        match model.cmd {
            RestartNode => node_cmd_default_id(Command::Restart(api::NodeRestart {})),
            KillNode => node_cmd_default_id(Command::Stop(api::NodeStop {})),
            ShutdownNode => node_cmd_default_id(Command::Stop(api::NodeStop {})),
            UpdateNode => {
                let node = models::Node::find_by_id(node_id()?, conn).await?;
                let cmd = Command::Update(api::NodeUpdate {
                    rules: Self::rules(&node)?,
                });
                node_cmd_default_id(cmd)
            }
            UpgradeNode => {
                let node = models::Node::find_by_id(node_id()?, conn).await?;
                let blockchain = models::Blockchain::find_by_id(node.blockchain_id, conn).await?;
                let mut image = api::ContainerImage {
                    protocol: blockchain.name,
                    node_version: node.version.to_lowercase(),
                    node_type: 0, // We use the setter to set this field for type-safety
                };
                image.set_node_type(api::NodeType::from_model(node.node_type));
                let cmd = Command::Upgrade(api::NodeUpgrade { image: Some(image) });
                node_cmd_default_id(cmd)
            }
            MigrateNode => Err(crate::Error::UnexpectedError(anyhow!("Not implemented"))),
            GetNodeVersion => node_cmd_default_id(Command::InfoGet(api::NodeGet {})),

            // The following should be HostCommands
            CreateNode => {
                let node = models::Node::find_by_id(node_id()?, conn).await?;
                let blockchain = models::Blockchain::find_by_id(node.blockchain_id, conn).await?;
                let id_to_name_map = models::BlockchainProperty::id_to_name_map(
                    &blockchain,
                    node.node_type,
                    &node.version,
                    conn,
                )
                .await?;
                let mut image = api::ContainerImage {
                    protocol: blockchain.name,
                    node_version: node.version.to_lowercase(),
                    node_type: 0, // We use the setter to set this field for type-safety
                };
                image.set_node_type(api::NodeType::from_model(node.node_type));
                let properties = node
                    .properties(conn)
                    .await?
                    .into_iter()
                    .map(|p| (&id_to_name_map[&p.blockchain_property_id], p.value))
                    .map(|(name, value)| api::Parameter::new(name, &value))
                    .collect();
                let mut node_create = api::NodeCreate {
                    name: node.name.clone(),
                    blockchain: node.blockchain_id.to_string(),
                    image: Some(image),
                    node_type: 0, // We use the setter to set this field for type-safety
                    ip: node.ip_addr.clone(),
                    gateway: node.ip_gateway.clone(),
                    properties,
                    rules: Self::rules(&node)?,
                    network: node.network,
                };
                node_create.set_node_type(api::NodeType::from_model(node.node_type));
                let cmd = Command::Create(node_create);

                node_cmd_default_id(cmd)
            }
            DeleteNode => {
                let node_id = model
                    .sub_cmd
                    .clone()
                    .ok_or_else(required("command.node_id"))?;
                let cmd = Command::Delete(api::NodeDelete {});
                node_cmd(cmd, node_id)
            }
            GetBVSVersion => host_cmd(model.host_id.to_string()),
            UpdateBVS => host_cmd(model.host_id.to_string()),
            RestartBVS => host_cmd(model.host_id.to_string()),
            RemoveBVS => host_cmd(model.host_id.to_string()),
            CreateBVS => host_cmd(model.host_id.to_string()),
            StopBVS => host_cmd(model.host_id.to_string()),
        }
    }

    pub fn rules(node: &models::Node) -> crate::Result<Vec<api::Rule>> {
        fn firewall_rules(
            // I'll leave the Vec for now, maybe we need it later
            denied_or_allowed_ips: Vec<models::FilteredIpAddr>,
            action: api::Action,
        ) -> crate::Result<Vec<api::Rule>> {
            let mut rules = vec![];
            for ip in denied_or_allowed_ips {
                // Validate IP
                if !cidr_utils::cidr::IpCidr::is_ip_cidr(&ip.ip) {
                    return Err(crate::Error::Cidr);
                }

                rules.push(api::Rule {
                    name: "".to_string(),
                    action: action as i32,
                    direction: api::Direction::In as i32,
                    protocol: api::Protocol::Both as i32,
                    ips: Some(ip.ip),
                    ports: vec![],
                });
            }

            Ok(rules)
        }

        let rules = firewall_rules(node.allow_ips()?, api::Action::Allow)?
            .into_iter()
            .chain(firewall_rules(node.deny_ips()?, api::Action::Deny)?)
            .collect();
        Ok(rules)
    }
}

impl api::CommandServiceUpdateRequest {
    fn as_update(&self) -> crate::Result<models::UpdateCommand<'_>> {
        Ok(models::UpdateCommand {
            id: self.id.parse()?,
            response: self.response.as_deref(),
            exit_status: self.exit_code,
            completed_at: self.exit_code.map(|_| chrono::Utc::now()),
        })
    }
}

impl api::Parameter {
    fn new(name: &str, val: &str) -> Self {
        Self {
            name: name.to_owned(),
            value: val.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Context;
    use crate::tests::TestDb;

    #[tokio::test]
    async fn test_create_firewall_rules() {
        let context = Context::from_default_toml().await.unwrap();
        let db = TestDb::setup(context).await;
        let node = db.node().await;
        api::Command::rules(&node).unwrap();
    }
}
