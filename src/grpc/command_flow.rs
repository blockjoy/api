use crate::auth::TokenType;
use crate::errors::{ApiError, Result as ApiResult};
use crate::grpc::blockjoy::{
    command_flow_server::CommandFlow, info_update::Info as GrpcInfo, Command as GrpcCommand,
    Command, InfoUpdate, NodeInfo,
};
use crate::grpc::notification::{ChannelNotification, ChannelNotifier, NotificationPayload};
use crate::models::{Command as DbCommand, Host, Token};
use crate::models::{Node, UpdateInfo};
use crate::server::DbPool;
use anyhow::anyhow;
use crossbeam_channel::Receiver;
use sqlx::PgPool;
use std::pin::Pin;
use std::sync::Arc;
use std::{env, error::Error};
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tonic::codegen::futures_core::Stream;
use tonic::{Request, Response, Status, Streaming};
use uuid::Uuid;

#[allow(dead_code)]
fn match_for_io_error(err_status: &Status) -> Option<&std::io::Error> {
    let mut err: &(dyn Error + 'static) = err_status;

    loop {
        if let Some(io_err) = err.downcast_ref::<std::io::Error>() {
            return Some(io_err);
        }

        // h2::Error do not expose std::io::Error with `source()`
        // https://github.com/hyperium/h2/pull/462
        if let Some(h2_err) = err.downcast_ref::<h2::Error>() {
            if let Some(io_err) = h2_err.get_io() {
                return Some(io_err);
            }
        }

        err = match err.source() {
            Some(err) => err,
            None => return None,
        };
    }
}

pub struct CommandFlowServerImpl {
    db: DbPool,
    buffer_size: usize,
    notifier: ChannelNotifier,
}

impl CommandFlowServerImpl {
    pub fn new(db: DbPool, notifier: ChannelNotifier) -> Self {
        let buffer_size: usize = env::var("BIDI_BUFFER_SIZE")
            .map(|bs| bs.parse::<usize>())
            .unwrap()
            .unwrap_or(128);

        Self {
            db,
            buffer_size,
            notifier,
        }
    }

    /// Actually perform info update on an identified resource
    async fn handle_info_update<T, R>(info: T, db: DbPool) -> ApiResult<R>
    where
        R: UpdateInfo<T, R>,
    {
        // TODO: check ownership
        R::update_info(info, db).await
    }

    async fn process_info_update(
        db: Arc<PgPool>,
        update_sender: Sender<Result<Command, Status>>,
        update: InfoUpdate,
    ) -> ApiResult<()> {
        let update_result = match update.info.unwrap() {
            GrpcInfo::Command(_cmd_info) => unimplemented!(), // Self::handle_info_update::<CommandInfo, Command>(cmd_info, db),
            GrpcInfo::Host(_host_info) => unimplemented!(), // Self::handle_info_update::<HostInfo, Host>(host_info, db),
            GrpcInfo::Node(node_info) => Self::handle_info_update::<NodeInfo, Node>(node_info, db),
        }
        .await;

        match update_result {
            // send status info if error occurred
            Err(e) => match update_sender.send(Err(Status::from(e))).await {
                Ok(_) => Ok(()),
                Err(e) => Err(ApiError::UnexpectedError(anyhow!("Sender error: {}", e))),
            },
            _ => Ok(()), // just return unit type if all went well
        }
    }

    /// Received notification about new command row, sending corresponding message
    async fn process_notification(
        notification: NotificationPayload,
        db: DbPool,
        sender: Sender<Result<Command, Status>>,
    ) -> ApiResult<()> {
        let cmd_id = notification.get_id();
        let command = DbCommand::find_by_id(cmd_id, &db).await;

        match command {
            Ok(command) => {
                let msg = GrpcCommand::from(command);
                match sender.send(Ok(msg)).await {
                    Err(e) => Err(ApiError::UnexpectedError(anyhow!("Sender error: {}", e))),
                    _ => Ok(()), // just return unit type if all went well
                }
            }
            Err(e) => {
                let msg = Status::from(e);

                match sender.send(Err(msg)).await {
                    Err(e) => Err(ApiError::UnexpectedError(anyhow!("Sender error: {}", e))),
                    _ => Ok(()), // just return unit type if all went well
                }
            }
        }
    }

    async fn handle_notifications(
        host_id: Uuid,
        db: Arc<PgPool>,
        notifications: Receiver<ChannelNotification>,
        stream_sender: Sender<Result<Command, Status>>,
    ) -> Result<(), Status> {
        while let Ok(notification) = notifications.recv() {
            match notification {
                ChannelNotification::Command(cmd) => {
                    Self::process_notification(cmd, db.clone(), stream_sender.clone()).await?
                }
                _ => tracing::error!("received non Command notification"),
            }
        }

        // Connection broke
        match Host::toggle_online(host_id, false, &db.clone()).await {
            Ok(_) => Ok(()),
            Err(e) => Err(Status::from(e)),
        }
    }
}

#[tonic::async_trait]
impl CommandFlow for CommandFlowServerImpl {
    type CommandsStream = Pin<Box<dyn Stream<Item = Result<GrpcCommand, Status>> + Send + 'static>>;

    async fn commands(
        &self,
        request: Request<Streaming<InfoUpdate>>,
    ) -> Result<Response<Self::CommandsStream>, Status> {
        // DB token must be added by middleware beforehand
        let db_token = request.extensions().get::<Token>().unwrap();
        let host_id =
            match Token::get_host_for_token(&db_token.token, TokenType::Login, &self.db).await {
                Ok(host) => host.id,
                Err(e) => return Err(Status::from(e)),
            };

        // Host::toggle_online(host_id, true, &self.db).await?;

        let (tx, rx) = mpsc::channel(self.buffer_size);
        let mut update_stream = request.into_inner();

        // Clones intended to be moved inside async closures
        let db = self.db.clone();
        let sender = tx.clone();

        // Create task handling incoming updates
        let handle_updates = tokio::spawn(async move {
            while let Some(Ok(update)) = update_stream.next().await {
                Self::process_info_update(db.clone(), sender.clone(), update).await?
            }

            // Connection broke
            match Host::toggle_online(host_id, false, &db.clone()).await {
                Ok(_) => Ok(()),
                Err(e) => Err(Status::from(e)),
            }
        });

        let db = self.db.clone();
        let sender = tx.clone();

        // Create task handling incoming notifications
        let handle_notifications = tokio::spawn(Self::handle_notifications(
            host_id,
            db.clone(),
            self.notifier.commands_receiver().clone(),
            sender,
        ));

        // Join handles to ensure max. concurrency
        match tokio::try_join!(handle_updates, handle_notifications) {
            Ok(_) => tracing::info!("All tasks finished"),
            Err(e) => tracing::error!("Error in some task: {}", e),
        }

        let commands_stream = ReceiverStream::new(rx);

        Ok(Response::new(
            Box::pin(commands_stream) as Self::CommandsStream
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::models::{
        ConnectionStatus, Host, HostCmd, HostRequest, TokenRole, User, UserRequest,
    };
    use http::Uri;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::PgPool;
    use std::convert::TryFrom;
    use std::future::Future;
    use std::sync::Arc;

    use crate::auth::TokenIdentifyable;
    use crate::grpc::blockjoy::info_update::Info;
    use crate::grpc::blockjoy::{command_flow_client::CommandFlowClient, Uuid as GrpcUuid};
    use crate::grpc::blockjoy::{InfoUpdate, NodeInfo};
    use crate::models::validator::{
        StakeStatus, Validator, ValidatorStatus, ValidatorStatusRequest,
    };
    use tempfile::NamedTempFile;
    use test_macros::before;
    use tokio::net::{UnixListener, UnixStream};
    use tokio::time::{self, Duration};
    use tokio_stream::wrappers::UnixListenerStream;
    use tokio_stream::{Stream, StreamExt};
    use tonic::transport::{Channel, Endpoint};
    use tonic::{IntoStreamingRequest, Request};
    use tower::service_fn;
    use uuid::Uuid;

    async fn server_and_client_stub(
        db: Arc<PgPool>,
    ) -> (impl Future<Output = ()>, CommandFlowClient<Channel>) {
        let socket = NamedTempFile::new().unwrap();
        let socket = Arc::new(socket.into_temp_path());
        std::fs::remove_file(&*socket).unwrap();

        let uds = UnixListener::bind(&*socket).unwrap();
        let stream = UnixListenerStream::new(uds);

        let serve_future = async {
            let result = crate::grpc::server(db)
                .await
                .serve_with_incoming(stream)
                .await;

            assert!(result.is_ok());
            dbg!("Server is running");
        };

        let socket = Arc::clone(&socket);
        // Connect to the server over a Unix socket
        // The URL will be ignored.
        let channel = Endpoint::try_from("http://any.url")
            .unwrap()
            .connect_with_connector(service_fn(move |_: Uri| {
                let socket = Arc::clone(&socket);
                async move { UnixStream::connect(&*socket).await }
            }))
            .await
            .unwrap();

        let client = CommandFlowClient::new(channel);

        (serve_future, client)
    }

    pub async fn reset_db(pool: &PgPool) {
        sqlx::query("DELETE FROM payments")
            .execute(pool)
            .await
            .expect("Error deleting payments");
        sqlx::query("DELETE FROM rewards")
            .execute(pool)
            .await
            .expect("Error deleting rewards");
        sqlx::query("DELETE FROM validators")
            .execute(pool)
            .await
            .expect("Error deleting validators");
        sqlx::query("DELETE FROM tokens")
            .execute(pool)
            .await
            .expect("Error deleting tokens");
        sqlx::query("DELETE FROM hosts")
            .execute(pool)
            .await
            .expect("Error deleting hosts");
        sqlx::query("DELETE FROM users")
            .execute(pool)
            .await
            .expect("Error deleting users");
        sqlx::query("DELETE FROM orgs")
            .execute(pool)
            .await
            .expect("Error deleting orgs");
        sqlx::query("DELETE FROM info")
            .execute(pool)
            .await
            .expect("Error deleting info");
        sqlx::query("DELETE FROM invoices")
            .execute(pool)
            .await
            .expect("Error deleting invoices");
        sqlx::query("DELETE FROM blockchains")
            .execute(pool)
            .await
            .expect("Error deleting blockchains");
        sqlx::query("DELETE FROM host_provisions")
            .execute(pool)
            .await
            .expect("Error deleting host_provisions");
        sqlx::query("INSERT INTO info (block_height) VALUES (99)")
            .execute(pool)
            .await
            .expect("could not update info in test setup");
        sqlx::query("INSERT INTO blockchains (name,status) values ('Helium', 'production')")
            .execute(pool)
            .await
            .expect("Error inserting blockchains");
        sqlx::query("DELETE FROM broadcast_filters")
            .execute(pool)
            .await
            .expect("Error deleting broadcast_filters");

        let user = UserRequest {
            email: "test@here.com".into(),
            first_name: "Luuk".into(),
            last_name: "Tester".into(),
            password: "abc12345".into(),
            password_confirm: "abc12345".into(),
        };

        let user = User::create(user, pool, None)
            .await
            .expect("Could not create test user in db.");

        sqlx::query(
            "UPDATE users set pay_address = '123456', staking_quota = 3 where email = 'test@here.com'",
        )
            .execute(pool)
            .await
            .expect("could not set user's pay address for user test user in sql");

        sqlx::query("INSERT INTO invoices (user_id, earnings, fee_bps, validators_count, amount, starts_at, ends_at, is_paid) values ($1, 99, 200, 1, 1000000000, now(), now(), false)")
            .bind(user.id)
            .execute(pool)
            .await
            .expect("could insert test invoice into db");

        let user = UserRequest {
            email: "admin@here.com".into(),
            first_name: "Mister".into(),
            last_name: "Sister".into(),
            password: "abc12345".into(),
            password_confirm: "abc12345".into(),
        };

        User::create(user, pool, Some(TokenRole::Admin))
            .await
            .expect("Could not create test user in db.");

        let host = HostRequest {
            org_id: None,
            name: "Host-1".into(),
            version: Some("0.1.0".into()),
            location: Some("Virgina".into()),
            cpu_count: None,
            mem_size: None,
            disk_size: None,
            os: None,
            os_version: None,
            ip_addr: "192.168.1.1".into(),
            val_ip_addrs: Some(
                "192.168.0.1, 192.168.0.2, 192.168.0.3, 192.168.0.4, 192.168.0.5".into(),
            ),
            status: ConnectionStatus::Online,
        };

        let host = Host::create(host, pool)
            .await
            .expect("Could not create test host in db.");

        let status = ValidatorStatusRequest {
            version: None,
            block_height: None,
            status: ValidatorStatus::Synced,
        };

        for v in host.validators.expect("No validators.") {
            let _ = Validator::update_status(v.id, status.clone(), pool)
                .await
                .expect("Error updating validator status in db during setup.");
            let _ = Validator::update_stake_status(v.id, StakeStatus::Available, pool)
                .await
                .expect("Error updating validator stake status in db during setup.");
        }

        let host = HostRequest {
            org_id: None,
            name: "Host-2".into(),
            version: Some("0.1.0".into()),
            location: Some("Ohio".into()),
            cpu_count: None,
            mem_size: None,
            disk_size: None,
            os: None,
            os_version: None,
            ip_addr: "192.168.2.1".into(),
            val_ip_addrs: Some(
                "192.168.3.1, 192.168.3.2, 192.168.3.3, 192.168.3.4, 192.168.3.5".into(),
            ),
            status: ConnectionStatus::Online,
        };

        let host = Host::create(host, pool)
            .await
            .expect("Could not create test host in db.");

        let status = ValidatorStatusRequest {
            version: None,
            block_height: None,
            status: ValidatorStatus::Synced,
        };

        for v in host.validators.expect("No validators.") {
            let _ = Validator::update_status(v.id, status.clone(), pool)
                .await
                .expect("Error updating validator status in db during setup.");
            let _ = Validator::update_stake_status(v.id, StakeStatus::Available, pool)
                .await
                .expect("Error updating validator stake status in db during setup.");
        }
    }

    async fn setup() -> PgPool {
        dotenv::dotenv().ok();

        let db_url = std::env::var("DATABASE_URL").expect("Missing DATABASE_URL");
        if db_url.contains("digitalocean") {
            panic!("Attempting to use production db?");
        }
        let db_max_conn = std::env::var("DB_MAX_CONN")
            .unwrap_or_else(|_| "10".to_string())
            .parse()
            .unwrap();

        let pool = PgPoolOptions::new()
            .max_connections(db_max_conn)
            .connect(&db_url)
            .await
            .expect("Could not create db connection pool.");

        reset_db(&pool.clone()).await;

        pool
    }

    pub async fn get_test_host(db: &PgPool) -> Host {
        sqlx::query("select h.*, t.token, t.role from hosts h right join tokens t on h.id = t.host_id where name = 'Host-1'")
            .map(Host::from)
            .fetch_one(db)
            .await
            .unwrap()
    }

    fn node_info_requests_iter() -> impl Stream<Item = InfoUpdate> {
        tokio_stream::iter(1..=10).map(|i| InfoUpdate {
            info: Some(Info::Node(NodeInfo {
                id: Some(GrpcUuid::from(Uuid::new_v4())),
                name: Some("strizzi".into()),
                ip: Some("123.456.789.0".into()),
                block_height: Some(i as i64),
                onchain_name: Some("strizzi-asdfasdf".into()),
                app_status: None,
                container_status: None,
                sync_status: None,
                staking_status: None,
            })),
        })
    }

    /// TODO: Doesn't look like the test is really working
    #[before(call = "setup")]
    #[tokio::test]
    async fn responds_ok_with_valid_token_for_node_command() {
        let db = Arc::new(_before_values.await);
        let (serve_future, mut client) = server_and_client_stub(db.clone()).await;
        let host = get_test_host(&db.clone()).await;
        let token = host.get_token(&db.clone()).await.unwrap();

        let request_future = async move {
            println!("creating request");
            let in_stream = node_info_requests_iter().take(10);

            let mut request = Request::new(in_stream).into_streaming_request();

            println!("setting request metadata");

            request.metadata_mut().insert(
                "authorization",
                format!("Bearer {}", token.to_base64()).parse().unwrap(),
            );

            match client.commands(request).await {
                Ok(response) => {
                    println!("got response");
                    let mut response_stream = response.into_inner();

                    while let Some(received) = response_stream.next().await {
                        let received = received.unwrap();
                        println!("\treceived message: `{:?}`", received);
                    }
                }
                Err(s) => {
                    panic!("didn't work: {:?}", s)
                }
            }
        };

        let db_clone = db.clone();

        let create_commands = async move {
            println!("creating commands");

            let hosts = Host::find_all(&db).await.unwrap();

            // create new command so the DB can notify our server about it
            for host in hosts {
                println!("creating command for host {}", host.id);
                let mut tx = db.begin().await.unwrap();

                sqlx::query("insert into commands (host_id, cmd, sub_cmd) values ($1, $2, $3)")
                    .bind(host.id)
                    .bind(HostCmd::GetNodeVersion)
                    // Using host id again, just to have some valid uuid
                    .bind(host.id)
                    .execute(&mut tx)
                    .await
                    .expect("Some error at inserting command");

                tx.commit().await.expect("Some error at committing tx");
            }
        };

        let sleep = time::sleep(Duration::from_secs(1));
        tokio::pin!(sleep);

        // For whatever reason I need to wait for 1 sec here to make the test work
        time::sleep(Duration::from_secs(1)).await;

        // Wait for completion, when the client request future completes
        tokio::select! {
            _ = &mut sleep => {
                    create_commands.await;
                    db_clone.close().await;
                }
            _ = request_future => (),
            _ = serve_future => panic!("server returned first"),
        }
    }

    #[before(call = "setup")]
    #[tokio::test]
    async fn responds_unauthenticated_without_valid_token_for_node_command() {
        let db = Arc::new(_before_values.await);
        let (serve_future, mut client) = server_and_client_stub(db.clone()).await;

        let request_future = async {
            let in_stream = node_info_requests_iter().take(10);

            match client.commands(in_stream).await {
                Ok(response) => {
                    let mut response_stream = response.into_inner();

                    while let Some(received) = response_stream.next().await {
                        let received = received.unwrap();
                        println!("\treceived message: `{:?}`", received);
                    }
                }
                Err(s) => {
                    assert_eq!(tonic::Code::Unauthenticated, s.code());
                }
            }
        };

        // Wait for completion, when the client request future completes
        tokio::select! {
            _ = serve_future => panic!("server returned first"),
            _ = request_future => (),
        }
    }
}
