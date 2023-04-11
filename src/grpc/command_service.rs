use super::blockjoy::{self, commands_server::Commands};
use super::convert;
use super::helpers::required;
use crate::auth::FindableById;
use crate::models;
use anyhow::anyhow;
use diesel_async::scoped_futures::ScopedFutureExt;
use std::str::FromStr;
use tonic::{Request, Response, Status};

impl blockjoy::CommandInfo {
    fn as_update(&self) -> crate::Result<models::UpdateCommand<'_>> {
        Ok(models::UpdateCommand {
            id: self.id.parse()?,
            response: self.response.as_deref(),
            exit_status: self.exit_code,
            completed_at: chrono::Utc::now(),
        })
    }
}

impl blockjoy::Parameter {
    fn new(name: &str, val: &str) -> Self {
        Self {
            name: name.to_owned(),
            value: val.to_owned(),
        }
    }
}

impl blockjoy::Command {
    pub async fn from_model(
        model: &models::Command,
        conn: &mut diesel_async::AsyncPgConnection,
    ) -> crate::Result<blockjoy::Command> {
        use blockjoy::command::Type;
        use blockjoy::node_command::Command;
        use models::HostCmd::*;

        // Extract the node id from the model, if there is one.
        let node_id = || model.node_id.ok_or_else(required("command.node_id"));
        // Closure to conveniently construct a blockjoy:: from the data that we need to have.
        let node_cmd = |command, node_id| {
            Ok(blockjoy::Command {
                r#type: Some(Type::Node(blockjoy::NodeCommand {
                    node_id,
                    host_id: model.host_id.to_string(),
                    command: Some(command),
                    api_command_id: model.id.to_string(),
                    created_at: Some(convert::try_dt_to_ts(model.created_at)?),
                })),
            })
        };
        // Construct a blockjoy::Command with the node id extracted from the `node.node_id` field.
        // Only `DeleteNode` does not use this method.
        let node_cmd_default_id = |command| node_cmd(command, node_id()?.to_string());

        match model.cmd {
            RestartNode => node_cmd_default_id(Command::Restart(blockjoy::NodeRestart {})),
            KillNode => node_cmd_default_id(Command::Stop(blockjoy::NodeStop {})),
            ShutdownNode => node_cmd_default_id(Command::Stop(blockjoy::NodeStop {})),
            UpdateNode => {
                let node = models::Node::find_by_id(node_id()?, conn).await?;
                let cmd = Command::Update(blockjoy::NodeUpdate {
                    self_update: Some(node.self_update),
                });
                node_cmd_default_id(cmd)
            }
            MigrateNode => Err(crate::Error::UnexpectedError(anyhow!("Not implemented"))),
            GetNodeVersion => node_cmd_default_id(Command::InfoGet(blockjoy::NodeGet {})),

            // The following should be HostCommands
            CreateNode => {
                let node = models::Node::find_by_id(node_id()?, conn).await?;
                let blockchain = models::Blockchain::find_by_id(node.blockchain_id, conn).await?;
                let image = blockjoy::ContainerImage {
                    protocol: blockchain.name,
                    node_type: node.node_type.to_string().to_lowercase(),
                    node_version: node.version.as_deref().unwrap_or("latest").to_lowercase(),
                    status: blockjoy::container_image::StatusName::Development.into(),
                };
                let network = blockjoy::Parameter::new("network", &node.network);
                let r#type = models::NodePropertiesWithId {
                    id: node.node_type.into(),
                    props: node.properties()?,
                };
                let properties = node
                    .properties()?
                    .iter_props()
                    .flat_map(|p| p.value.as_ref().map(|v| (&p.name, v)))
                    .map(|(name, value)| blockjoy::Parameter::new(name, value))
                    .chain([network])
                    .collect();
                let cmd = Command::Create(blockjoy::NodeCreate {
                    name: node.name,
                    blockchain: node.blockchain_id.to_string(),
                    image: Some(image),
                    r#type: serde_json::to_string(&r#type)?,
                    ip: node.ip_addr,
                    gateway: node.ip_gateway,
                    self_update: node.self_update,
                    properties,
                });

                node_cmd_default_id(cmd)
            }
            DeleteNode => {
                let node_id = model
                    .sub_cmd
                    .clone()
                    .ok_or_else(required("command.node_id"))?;
                let cmd = Command::Delete(blockjoy::NodeDelete {});
                node_cmd(cmd, node_id)
            }
            GetBVSVersion => Err(crate::Error::UnexpectedError(anyhow!("Not implemented"))),
            UpdateBVS => Err(crate::Error::UnexpectedError(anyhow!("Not implemented"))),
            RestartBVS => Err(crate::Error::UnexpectedError(anyhow!("Not implemented"))),
            RemoveBVS => Err(crate::Error::UnexpectedError(anyhow!("Not implemented"))),
            CreateBVS => Err(crate::Error::UnexpectedError(anyhow!("Not implemented"))),
            StopBVS => Err(crate::Error::UnexpectedError(anyhow!("Not implemented"))),
        }
    }
}

#[tonic::async_trait]
impl Commands for super::GrpcImpl {
    async fn get(
        &self,
        request: Request<blockjoy::CommandInfo>,
    ) -> Result<Response<blockjoy::Command>, Status> {
        let inner = request.into_inner();
        let cmd_id = uuid::Uuid::from_str(inner.id.as_str()).map_err(crate::Error::from)?;
        let mut db_conn = self.conn().await?;
        let cmd = models::Command::find_by_id(cmd_id, &mut db_conn).await?;
        let grpc_cmd = blockjoy::Command::from_model(&cmd, &mut db_conn).await?;
        let response = Response::new(grpc_cmd);

        Ok(response)
    }

    async fn update(
        &self,
        request: Request<blockjoy::CommandInfo>,
    ) -> Result<Response<()>, Status> {
        let inner = request.into_inner();
        self.trx(|c| {
            async move {
                let update_cmd = inner.as_update()?;
                update_cmd.update(c).await?;
                Ok(())
            }
            .scope_boxed()
        })
        .await?;

        Ok(Response::new(()))
    }

    async fn pending(
        &self,
        request: Request<blockjoy::PendingCommandsRequest>,
    ) -> Result<Response<blockjoy::CommandResponse>, Status> {
        let inner = request.into_inner();
        let host_id = inner.host_id.parse().map_err(crate::Error::from)?;
        let mut db_conn = self.conn().await?;
        let cmds = models::Command::find_pending_by_host(host_id, &mut db_conn).await?;
        let mut response = blockjoy::CommandResponse { commands: vec![] };

        for cmd in cmds {
            let grpc_cmd = blockjoy::Command::from_model(&cmd, &mut db_conn).await?;
            response.commands.push(grpc_cmd);
        }

        Ok(Response::new(response))
    }
}
