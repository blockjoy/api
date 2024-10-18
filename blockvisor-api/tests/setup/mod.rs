pub mod helper;

use std::net::SocketAddr;
use std::sync::Arc;

use rand::distributions::{Alphanumeric, DistString};
use rand::rngs::OsRng;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;

use blockvisor_api::auth::claims::{Claims, Expirable};
use blockvisor_api::auth::rbac::{ApiKeyRole, GrpcRole, Roles};
use blockvisor_api::auth::resource::{HostId, NodeId, ResourceEntry};
use blockvisor_api::auth::token::jwt::Jwt;
use blockvisor_api::auth::token::refresh::{Encoded, Refresh};
use blockvisor_api::auth::token::Cipher;
use blockvisor_api::config::Context;
use blockvisor_api::database::seed::{self, Seed};
use blockvisor_api::database::tests::TestDb;
use blockvisor_api::database::Conn;
use blockvisor_api::model::{Host, Org, User};

use self::helper::rpc;
use self::helper::traits::SocketRpc;

/// Spawns an instance of blockvisor-api for running integration tests.
///
/// Implements `SocketRpc` for making RPC requests to the running instance.
pub struct TestServer {
    db: TestDb,
    context: Arc<Context>,
    addr: SocketAddr,
}

#[allow(dead_code)]
impl TestServer {
    pub async fn new() -> Self {
        let (context, db) = Context::with_mocked().await.unwrap();
        // let _ = context.config.log.try_start();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let stream = TcpListenerStream::new(listener);

        let server_context = context.clone();
        tokio::spawn(async move {
            blockvisor_api::grpc::server(&server_context)
                .serve_with_incoming(stream)
                .await
                .unwrap()
        });

        TestServer { db, context, addr }
    }

    pub async fn conn(&self) -> Conn<'_> {
        self.db.conn().await
    }

    pub fn context(&self) -> &Context {
        &self.context
    }

    pub fn cipher(&self) -> &Cipher {
        &self.context.auth.cipher
    }

    pub fn seed(&self) -> &Seed {
        &self.db.seed
    }

    pub async fn root_claims(&self) -> Claims {
        rpc::login(self, seed::ROOT_EMAIL).await
    }

    pub async fn root_jwt(&self) -> Jwt {
        let claims = self.root_claims().await;
        self.cipher().jwt.encode(&claims).unwrap()
    }

    pub async fn admin_claims(&self) -> Claims {
        rpc::login(self, seed::ADMIN_EMAIL).await
    }

    pub async fn admin_jwt(&self) -> Jwt {
        let claims = self.admin_claims().await;
        self.cipher().jwt.encode(&claims).unwrap()
    }

    pub fn admin_refresh(&self) -> Refresh {
        let admin_id = self.seed().user.id;
        Refresh::from_now(chrono::Duration::minutes(15), admin_id)
    }

    pub fn admin_encoded(&self) -> Encoded {
        let refresh = self.admin_refresh();
        self.cipher().refresh.encode(&refresh).unwrap()
    }

    pub async fn member_claims(&self) -> Claims {
        rpc::login(self, seed::MEMBER_EMAIL).await
    }

    pub async fn member_jwt(&self) -> Jwt {
        let claims = self.member_claims().await;
        self.cipher().jwt.encode(&claims).unwrap()
    }

    pub async fn unconfirmed_user(&self) -> User {
        let email = seed::UNCONFIRMED_EMAIL;
        let mut conn = self.conn().await;
        User::by_email(email, &mut conn).await.unwrap()
    }

    pub fn host_claims_for(&self, host_id: HostId) -> Claims {
        let roles = Roles::One(GrpcRole::NewHost.into());
        let resource = ResourceEntry::new_host(host_id).into();
        let expirable = Expirable::from_now(chrono::Duration::minutes(15));
        Claims::new(resource, expirable, roles.into())
    }

    pub fn host_claims(&self) -> Claims {
        self.host_claims_for(self.seed().host.id)
    }

    pub fn host_jwt(&self) -> Jwt {
        let claims = self.host_claims();
        self.cipher().jwt.encode(&claims).unwrap()
    }

    pub fn node_claims_for(&self, node_id: NodeId) -> Claims {
        let roles = Roles::One(ApiKeyRole::Node.into());
        let resource = ResourceEntry::new_node(node_id).into();
        let expirable = Expirable::from_now(chrono::Duration::minutes(15));
        Claims::new(resource, expirable, roles.into())
    }

    pub fn node_claims(&self) -> Claims {
        self.node_claims_for(self.seed().node.id)
    }

    pub fn node_jwt(&self) -> Jwt {
        let claims = self.node_claims();
        self.cipher().jwt.encode(&claims).unwrap()
    }

    pub async fn host1(&self) -> Host {
        let mut conn = self.conn().await;
        Host::by_name(seed::HOST_1, &mut conn).await.unwrap()
    }

    pub async fn host2(&self) -> Host {
        let mut conn = self.conn().await;
        Host::by_name(seed::HOST_2, &mut conn).await.unwrap()
    }

    pub async fn org(&self) -> Org {
        let mut conn = self.conn().await;
        let org_id = seed::ORG_ID.parse().unwrap();
        Org::by_id(org_id, &mut conn).await.unwrap()
    }

    pub async fn rng(&mut self) -> OsRng {
        *self.context.rng.lock().await
    }

    pub async fn rand_string(&mut self, len: usize) -> String {
        let mut rng = self.rng().await;
        Alphanumeric.sample_string(&mut rng, len)
    }

    pub async fn rand_email(&mut self) -> String {
        let user = self.rand_string(8).await;
        let domain = self.rand_string(8).await;
        format!("{user}@{domain}.com")
    }
}

impl SocketRpc for TestServer {
    fn socket_addr(&self) -> SocketAddr {
        self.addr
    }

    async fn root_jwt(&self) -> Jwt {
        self.root_jwt().await
    }

    async fn admin_jwt(&self) -> Jwt {
        self.admin_jwt().await
    }

    async fn member_jwt(&self) -> Jwt {
        self.member_jwt().await
    }
}
