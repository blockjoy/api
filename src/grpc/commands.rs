use super::api::{self, commands_server};
use super::helpers::required;
use crate::auth::FindableById;
use crate::firewall::create_rules_for_node;
use crate::models;
use anyhow::anyhow;
use diesel_async::scoped_futures::ScopedFutureExt;
use diesel_async::AsyncPgConnection;
use std::str::FromStr;
use tonic::Request;

#[tonic::async_trait]
impl commands_server::Commands for super::GrpcImpl {
    async fn create(
        &self,
        req: Request<api::CreateCommandRequest>,
    ) -> super::Result<api::CreateCommandResponse> {
        let refresh_token = super::get_refresh_token(&req);
        let req = req.into_inner();
        self.trx(|c| {
            async move {
                let host_id = req.host_id(c).await?;
                let node_id = req.node_id()?;
                let command_type = req.command_type()?;
                let command = req
                    .as_new(host_id, node_id, command_type)?
                    .create(c)
                    .await?;
                let command = api::Command::from_model(&command, c).await?;
                self.notifier.commands_sender().send(&command).await?;
                let response = api::CreateCommandResponse {
                    command: Some(command),
                };

                Ok(super::response_with_refresh_token(refresh_token, response)?)
            }
            .scope_boxed()
        })
        .await
    }

    async fn get(
        &self,
        request: Request<api::GetCommandRequest>,
    ) -> super::Result<api::GetCommandResponse> {
        let refresh_token = super::get_refresh_token(&request);
        let inner = request.into_inner();
        let cmd_id = uuid::Uuid::from_str(inner.id.as_str()).map_err(crate::Error::from)?;
        let mut db_conn = self.conn().await?;
        let cmd = models::Command::find_by_id(cmd_id, &mut db_conn).await?;
        let command = api::Command::from_model(&cmd, &mut db_conn).await?;
        let response = api::GetCommandResponse {
            command: Some(command),
        };
        super::response_with_refresh_token(refresh_token, response)
    }

    async fn update(
        &self,
        request: Request<api::UpdateCommandRequest>,
    ) -> super::Result<api::UpdateCommandResponse> {
        let refresh_token = super::get_refresh_token(&request);
        let inner = request.into_inner();
        let update_cmd = inner.as_update()?;
        self.trx(|c| {
            async move {
                let command = update_cmd.update(c).await?;
                let command = api::Command::from_model(&command, c).await?;
                let resp = api::UpdateCommandResponse {
                    command: Some(command),
                };
                Ok(super::response_with_refresh_token(refresh_token, resp)?)
            }
            .scope_boxed()
        })
        .await
    }

    async fn pending(
        &self,
        request: Request<api::PendingCommandsRequest>,
    ) -> super::Result<api::PendingCommandsResponse> {
        let refresh_token = super::get_refresh_token(&request);
        let inner = request.into_inner();
        let host_id = inner.host_id.parse().map_err(crate::Error::from)?;
        let mut db_conn = self.conn().await?;
        let cmds = models::Command::find_pending_by_host(host_id, &mut db_conn).await?;
        let mut commands = Vec::with_capacity(cmds.len());
        for cmd in cmds {
            let grpc_cmd = api::Command::from_model(&cmd, &mut db_conn).await?;
            commands.push(grpc_cmd);
        }
        let response = api::PendingCommandsResponse { commands };
        super::response_with_refresh_token(refresh_token, response)
    }
}

impl api::Command {
    pub async fn from_model(
        model: &models::Command,
        conn: &mut diesel_async::AsyncPgConnection,
    ) -> crate::Result<api::Command> {
        use api::command;
        use api::node_command::Command;
        use models::HostCmd::*;

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
                    rules: create_rules_for_node(&node)?,
                });
                node_cmd_default_id(cmd)
            }
            MigrateNode => Err(crate::Error::UnexpectedError(anyhow!("Not implemented"))),
            GetNodeVersion => node_cmd_default_id(Command::InfoGet(api::NodeGet {})),

            // The following should be HostCommands
            CreateNode => {
                let node = models::Node::find_by_id(node_id()?, conn).await?;
                let blockchain = models::Blockchain::find_by_id(node.blockchain_id, conn).await?;
                let mut image = api::ContainerImage {
                    protocol: blockchain.name,
                    node_version: node.version.as_deref().unwrap_or("latest").to_lowercase(),
                    node_type: 0, // We use the setter to set this field for type-safety
                    status: 0,    // We use the setter to set this field for type-safety
                };
                image.set_node_type(api::node::NodeType::from_model(node.node_type));
                image.set_status(api::container_image::StatusName::Development);
                let network = api::Parameter::new("network", &node.network);
                let properties = node
                    .properties()?
                    .iter_props()
                    .flat_map(|p| p.value.as_ref().map(|v| (&p.name, v)))
                    .map(|(name, value)| api::Parameter::new(name, value))
                    .chain([network])
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
                    rules: create_rules_for_node(&node)?,
                };
                node_create.set_node_type(api::node::NodeType::from_model(node.node_type));
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
}

impl api::CreateCommandRequest {
    fn as_new(
        &self,
        host_id: uuid::Uuid,
        node_id: Option<uuid::Uuid>,
        command_type: models::HostCmd,
    ) -> crate::Result<models::NewCommand<'_>> {
        Ok(models::NewCommand {
            host_id,
            cmd: command_type,
            sub_cmd: None,
            node_id,
        })
    }

    async fn host_id(&self, conn: &mut AsyncPgConnection) -> crate::Result<uuid::Uuid> {
        use api::create_command_request::Command::*;

        let command = self.command.as_ref().ok_or_else(required("command"))?;
        let node_id = match command {
            StartNode(start) => start.node_id.parse()?,
            StopNode(stop) => stop.node_id.parse()?,
            RestartNode(restart) => restart.node_id.parse()?,
            StartHost(start) => return Ok(start.host_id.parse()?),
            StopHost(stop) => return Ok(stop.host_id.parse()?),
            RestartHost(restart) => return Ok(restart.host_id.parse()?),
        };
        let node = models::Node::find_by_id(node_id, conn).await?;
        Ok(node.host_id)
    }

    fn node_id(&self) -> crate::Result<Option<uuid::Uuid>> {
        use api::create_command_request::Command::*;

        let command = self.command.as_ref().ok_or_else(required("command"))?;
        match command {
            StartNode(start) => Ok(Some(start.node_id.parse()?)),
            StopNode(stop) => Ok(Some(stop.node_id.parse()?)),
            RestartNode(restart) => Ok(Some(restart.node_id.parse()?)),
            StartHost(_) => Ok(None),
            StopHost(_) => Ok(None),
            RestartHost(_) => Ok(None),
        }
    }

    fn command_type(&self) -> crate::Result<models::HostCmd> {
        use api::create_command_request::Command::*;

        let command = self.command.as_ref().ok_or_else(required("command"))?;
        let command_type = match command {
            StartNode(_) => models::HostCmd::RestartNode,
            StopNode(_) => models::HostCmd::KillNode,
            RestartNode(_) => models::HostCmd::RestartNode,
            StartHost(_) => models::HostCmd::RestartBVS,
            StopHost(_) => models::HostCmd::StopBVS,
            RestartHost(_) => models::HostCmd::RestartBVS,
        };
        Ok(command_type)
    }
}

impl api::UpdateCommandRequest {
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
