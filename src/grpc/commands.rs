mod recover;
mod success;

use anyhow::anyhow;
use diesel_async::scoped_futures::ScopedFutureExt;

use super::api::{self, command_service_server};
use super::helpers::required;
use crate::auth::token::{Claims, Endpoint, Resource};
use crate::{auth, models};

struct CommandResult<T> {
    commands: Vec<api::Command>,
    resp: tonic::Response<T>,
}

#[tonic::async_trait]
impl command_service_server::CommandService for super::GrpcImpl {
    async fn create(
        &self,
        req: tonic::Request<api::CommandServiceCreateRequest>,
    ) -> super::Resp<api::CommandServiceCreateResponse> {
        let result = self.trx(|c| create(req, c).scope_boxed()).await?;
        for command in &result.commands {
            self.notifier.commands_sender().send(command).await?;
        }
        Ok(result.resp)
    }

    async fn get(
        &self,
        req: tonic::Request<api::CommandServiceGetRequest>,
    ) -> super::Resp<api::CommandServiceGetResponse> {
        let mut conn = self.conn().await?;
        let resp = get(req, &mut conn).await?;
        Ok(resp)
    }

    async fn update(
        &self,
        req: tonic::Request<api::CommandServiceUpdateRequest>,
    ) -> super::Resp<api::CommandServiceUpdateResponse> {
        let result = self.trx(|c| update(self, req, c).scope_boxed()).await?;
        for command in &result.commands {
            self.notifier.commands_sender().send(command).await?;
        }
        Ok(result.resp)
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

async fn create(
    req: tonic::Request<api::CommandServiceCreateRequest>,
    conn: &mut models::Conn,
) -> crate::Result<CommandResult<api::CommandServiceCreateResponse>> {
    let claims = auth::get_claims(&req, Endpoint::CommandCreate, conn).await?;
    let req = req.into_inner();
    let node = req.node(conn).await?;
    let host = req.host(conn).await?;
    let is_allowed = access_allowed(claims, node.as_ref(), &host, conn).await?;
    if !is_allowed {
        super::forbidden!("Access denied");
    }
    let command_type = req.command_type()?;
    let command = req
        .as_new(host.id, node.map(|n| n.id), command_type)?
        .create(conn)
        .await?;
    let command = api::Command::from_model(&command, conn).await?;
    let resp = tonic::Response::new(api::CommandServiceCreateResponse {
        command: Some(command.clone()),
    });
    Ok(CommandResult {
        commands: vec![command],
        resp,
    })
}

async fn get(
    req: tonic::Request<api::CommandServiceGetRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::CommandServiceGetResponse> {
    let claims = auth::get_claims(&req, Endpoint::CommandGet, conn).await?;
    let req = req.into_inner();
    let id = req.id.parse()?;
    let command = models::Command::find_by_id(id, conn).await?;
    let host = command.host(conn).await?;
    let node = command.node(conn).await?;
    let is_allowed = access_allowed(claims, node.as_ref(), &host, conn).await?;
    if !is_allowed {
        super::forbidden!("Access denied");
    }
    let command = api::Command::from_model(&command, conn).await?;
    let resp = api::CommandServiceGetResponse {
        command: Some(command),
    };
    Ok(tonic::Response::new(resp))
}

async fn update(
    impler: &super::GrpcImpl,
    req: tonic::Request<api::CommandServiceUpdateRequest>,
    conn: &mut models::Conn,
) -> crate::Result<CommandResult<api::CommandServiceUpdateResponse>> {
    let claims = auth::get_claims(&req, Endpoint::CommandUpdate, conn).await?;
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
    let mut commands = vec![];
    match cmd.exit_status {
        Some(0) => {
            // Some responses require us to register success.
            success::register(&cmd, conn).await;
        }
        // Will match any integer other than 0.
        Some(_) => {
            // We got back an error status code. In practice, blockvisord sends 0 for
            // success and 1 for failure, but we treat every non-zero exit code as an
            // error, not just 1.
            if let Ok(cmds) = recover::recover(impler, &cmd, conn).await {
                commands.extend(cmds);
            }
        }
        None => {}
    }
    let command = api::Command::from_model(&cmd, conn).await?;
    commands.push(command.clone());
    let resp = api::CommandServiceUpdateResponse {
        command: Some(command),
    };
    Ok(CommandResult {
        commands,
        resp: tonic::Response::new(resp),
    })
}

async fn pending(
    req: tonic::Request<api::CommandServicePendingRequest>,
    conn: &mut models::Conn,
) -> super::Result<api::CommandServicePendingResponse> {
    let claims = auth::get_claims(&req, Endpoint::CommandPending, conn).await?;
    let req = req.into_inner();
    let host_id = req.host_id.parse()?;
    let host = models::Host::find_by_id(host_id, conn).await?;
    let is_allowed = match claims.resource() {
        Resource::User(user_id) => {
            if let Some(org_id) = host.org_id {
                models::Org::is_member(user_id, org_id, conn).await?
            } else {
                false
            }
        }
        Resource::Org(org_id) => host.org_id == Some(org_id),
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
            } else if let Some(host_org_id) = host.org_id {
                models::Org::is_member(user_id, host_org_id, conn).await?
            } else {
                false
            }
        }
        Resource::Org(org_id) => {
            if let Some(node) = &node {
                org_id == node.org_id
            } else if let Some(host_org_id) = host.org_id {
                org_id == host_org_id
            } else {
                false
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
        let node_cmd = |command, node_id| {
            Ok(api::Command {
                id: model.id.to_string(),
                response: model.response.clone(),
                exit_code: model.exit_status,
                command: Some(command::Command::Node(api::NodeCommand {
                    node_id,
                    host_id: model.host_id.to_string(),
                    command: Some(command),
                    api_command_id: model.id.to_string(),
                    created_at: Some(super::try_dt_to_ts(model.created_at)?),
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
                    self_update: Some(node.self_update),
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
                    self_update: node.self_update,
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

impl api::CommandServiceCreateRequest {
    fn as_new(
        &self,
        host_id: uuid::Uuid,
        node_id: Option<uuid::Uuid>,
        command_type: models::CommandType,
    ) -> crate::Result<models::NewCommand<'_>> {
        Ok(models::NewCommand {
            host_id,
            cmd: command_type,
            sub_cmd: None,
            node_id,
        })
    }

    async fn host(&self, conn: &mut models::Conn) -> crate::Result<models::Host> {
        use api::command_service_create_request::Command::*;

        let command = self.command.as_ref().ok_or_else(required("command"))?;
        let host_id = match command {
            StartNode(api::StartNodeCommand { node_id, .. })
            | StopNode(api::StopNodeCommand { node_id, .. })
            | RestartNode(api::RestartNodeCommand { node_id, .. }) => {
                let node = models::Node::find_by_id(node_id.parse()?, conn).await?;
                node.host_id
            }
            StartHost(api::StartHostCommand { host_id, .. })
            | StopHost(api::StopHostCommand { host_id, .. })
            | RestartHost(api::RestartHostCommand { host_id, .. }) => host_id.parse()?,
        };
        let host = models::Host::find_by_id(host_id, conn).await?;
        Ok(host)
    }

    async fn node(&self, conn: &mut models::Conn) -> crate::Result<Option<models::Node>> {
        use api::command_service_create_request::Command::*;

        let command = self.command.as_ref().ok_or_else(required("command"))?;
        let node_id = match command {
            StartNode(start) => start.node_id.parse()?,
            StopNode(stop) => stop.node_id.parse()?,
            RestartNode(restart) => restart.node_id.parse()?,
            StartHost(_) => return Ok(None),
            StopHost(_) => return Ok(None),
            RestartHost(_) => return Ok(None),
        };
        let node = models::Node::find_by_id(node_id, conn).await?;
        Ok(Some(node))
    }

    fn command_type(&self) -> crate::Result<models::CommandType> {
        use api::command_service_create_request::Command::*;

        let command = self.command.as_ref().ok_or_else(required("command"))?;
        let command_type = match command {
            StartNode(_) => models::CommandType::RestartNode,
            StopNode(_) => models::CommandType::KillNode,
            RestartNode(_) => models::CommandType::RestartNode,
            StartHost(_) => models::CommandType::RestartBVS,
            StopHost(_) => models::CommandType::StopBVS,
            RestartHost(_) => models::CommandType::RestartBVS,
        };
        Ok(command_type)
    }
}

impl api::CommandServiceUpdateRequest {
    fn as_update(&self) -> crate::Result<models::UpdateCommand<'_>> {
        Ok(models::UpdateCommand {
            id: self.id.parse()?,
            response: self.response.as_deref(),
            exit_status: self.exit_code,
            completed_at: chrono::Utc::now(),
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

    #[tokio::test]
    async fn test_create_firewall_rules() {
        let context = crate::config::Context::new_with_default_toml().unwrap();
        let db = crate::TestDb::setup(context).await;
        let node = db.node().await;
        api::Command::rules(&node).unwrap();
    }
}
