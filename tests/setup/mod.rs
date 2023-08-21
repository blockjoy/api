#![allow(dead_code)]

pub mod helper;

use std::convert::TryFrom;
use std::fmt::Debug;
use std::sync::Arc;

use blockvisor_api::auth::claims::{Claims, Expirable};
use blockvisor_api::auth::endpoint::Endpoints;
use blockvisor_api::auth::resource::{HostId, ResourceEntry};
use blockvisor_api::auth::token::refresh::Refresh;
use blockvisor_api::auth::token::{Cipher, RequestToken};
use blockvisor_api::config::Context;
use blockvisor_api::database::tests::TestDb;
use blockvisor_api::grpc::api::auth_service_client::AuthServiceClient;
use blockvisor_api::models::node::NewNode;
use blockvisor_api::models::{Host, Org, User};
use derive_more::{Deref, DerefMut};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use rand::distributions::{Alphanumeric, DistString};
use rand::rngs::OsRng;
use tempfile::{NamedTempFile, TempPath};
use tokio::net::{UnixListener, UnixStream};
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::{Channel, Endpoint, Uri};
use tonic::Response;

use self::helper::traits::{GrpcClient, TestableFunction};

type AuthService = AuthServiceClient<tonic::transport::Channel>;

/// Our integration testing helper struct. Can be created cheaply with `new`, and is able to
/// receive requests and return responses. Exposes lots of helpers too to make creating new
/// integration tests easy. Re-exports some of the functionality from `TestDb` (a helper used
/// internally for more unit test-like tests).
#[derive(Deref, DerefMut)]
pub struct Tester {
    #[deref]
    #[deref_mut]
    db: TestDb,
    context: Arc<Context>,
    server_input: Arc<TempPath>,
    rng: OsRng,
}

impl Tester {
    pub async fn new() -> Self {
        let (context, db) = Context::with_mocked().await.unwrap();
        let _ = context.config.log.try_start();

        let socket = NamedTempFile::new().unwrap();
        let socket = Arc::new(socket.into_temp_path());
        std::fs::remove_file(&*socket).unwrap();

        let uds = UnixListener::bind(&*socket).unwrap();
        let stream = UnixListenerStream::new(uds);

        let server_context = context.clone();
        tokio::spawn(async move {
            blockvisor_api::grpc::server(server_context)
                .await
                .serve_with_incoming(stream)
                .await
                .unwrap()
        });

        Tester {
            db,
            context,
            server_input: Arc::clone(&socket),
            rng: OsRng {},
        }
    }

    pub fn context(&self) -> &Context {
        &self.context
    }

    pub fn cipher(&self) -> &Cipher {
        &self.context.auth.cipher
    }

    /// Returns a auth token for the admin user in the database.
    pub async fn admin_token(&self) -> Claims {
        let admin = self.db.user().await;
        self.user_token(&admin).await
    }

    pub async fn admin_refresh(&self) -> Refresh {
        let admin = self.db.user().await;
        let expires = chrono::Duration::minutes(15);
        Refresh::from_now(expires, admin.id)
    }

    pub async fn user_token(&self, user: &User) -> Claims {
        let req = blockvisor_api::grpc::api::AuthServiceLoginRequest {
            email: user.email.clone(),
            password: "abc12345".to_string(),
        };

        let resp = self.send(AuthService::login, req).await.unwrap();
        let token = match resp.token.parse().unwrap() {
            RequestToken::Bearer(token) => token,
            _ => panic!("Unexpected RequestToken type"),
        };

        self.cipher().jwt.decode(&token).unwrap()
    }

    pub fn host_token(&self, host: &Host) -> Claims {
        let resource = ResourceEntry::new_host(host.id).into();
        let expirable = Expirable::from_now(chrono::Duration::minutes(15));

        Claims::new(resource, expirable, Endpoints::Wildcard)
    }

    pub async fn hosts(&self) -> Vec<Host> {
        use blockvisor_api::models::schema::hosts;

        let mut conn = self.conn().await;
        hosts::table.get_results(&mut conn).await.unwrap()
    }

    pub async fn org_for(&self, user: &User) -> Org {
        use blockvisor_api::models::schema::{orgs, orgs_users};

        let mut conn = self.conn().await;
        orgs::table
            .filter(orgs::is_personal.eq(false))
            .filter(orgs_users::user_id.eq(user.id))
            .inner_join(orgs_users::table)
            .select(Org::as_select())
            .get_result(&mut conn)
            .await
            .unwrap()
    }

    pub async fn create_node(
        &self,
        node: &NewNode<'_>,
        host_id_param: &HostId,
        ip_add_param: &str,
        dns_id: &str,
    ) {
        use blockvisor_api::models::schema::nodes;

        let mut conn = self.conn().await;
        diesel::insert_into(nodes::table)
            .values((
                node,
                nodes::host_id.eq(host_id_param),
                nodes::ip_addr.eq(ip_add_param),
                nodes::dns_record_id.eq(dns_id),
            ))
            .execute(&mut conn)
            .await
            .unwrap();
    }

    /// Send a request without any authentication to the test server.  All the functions that we
    /// want to test are of a similar type, because they are all generated by tonic.
    /// ## Examples
    /// Some examples in a central place here:
    /// ### Simple test
    /// ```rs
    /// type Service = AuthenticationService<Channel>;
    /// let tester = setup::Tester::new().await;
    /// tester.send(Service::login, your_login_request).await.unwrap();
    /// let status = tester.send(Service::login, bad_login_request).await.unwrap_err();
    /// assert_eq!(status.code(), tonic::Code::Unauthenticated);
    /// ```
    /// ### Test for success
    /// ```rs
    /// type Service = AuthenticationService<Channel>;
    /// let tester = setup::Tester::new().await;
    /// tester.send(Service::refresh, req).await.unwrap();
    /// ```
    ///
    /// ### Generic params
    /// We have some generics going on here so lets break it down.
    /// The function that we want to test is of type `F`. Its signature is required to be
    /// `(&mut Client, Req) -> impl Future<Output = Result<Response<Resp>, tonic::Status>>`.
    /// We further restrict that `Req` must satisfy `impl tonic::IntoRequest<In>`. This means that
    /// `In` is the JSON structure that the requests take, `Req` is the type that the function
    /// takes that can be constructed from the `In` type, and `Resp` is the type that is returned
    /// on success.
    pub async fn send<F, In, Req, Resp, Client>(
        &self,
        f: F,
        req: Req,
    ) -> Result<Resp, tonic::Status>
    where
        F: for<'any> TestableFunction<'any, In, tonic::Request<In>, Response<Resp>, Client>,
        Req: tonic::IntoRequest<In>,
        Client: GrpcClient<Channel> + Debug + 'static,
    {
        self.send_(f, req.into_request()).await
    }

    /// Sends the provided request to the provided function, just as `send` would do, but adds the
    /// provided token to the metadata of the request. The token is base64 encoded and prefixed
    /// with `"Bearer "`. This allows you to send custom authentication through the testing
    /// machinery, which is needed for stuff like testing auth.
    ///
    /// ## Examples
    /// Some examples to demonstrate how to make tests with this:
    /// ### Empty token
    /// ```rs
    /// type Service = SomeService<Channel>;
    /// let tester = setup::Tester::new().await;
    /// let status = tester.send(Service::some_endpoint, some_data, "").await.unwrap_err();
    /// assert_eq!(status.code(), tonic::Code::Unauthorized);
    /// ```
    pub async fn send_with<F, In, Req, Resp, Client>(
        &self,
        f: F,
        req: Req,
        token: &str,
    ) -> Result<Resp, tonic::Status>
    where
        F: for<'any> TestableFunction<'any, In, tonic::Request<In>, Response<Resp>, Client>,
        Req: tonic::IntoRequest<In>,
        Client: GrpcClient<Channel> + Debug + 'static,
    {
        let mut req = req.into_request();
        let auth_header = format!("Bearer {}", token).parse().unwrap();
        req.metadata_mut().insert("authorization", auth_header);

        self.send_(f, req).await
    }

    /// Sends a request with authentication as though the user were an admin. This is the same as
    /// creating an admin token manually and then calling `tester.send_with(_, _, admin_token)`.
    pub async fn send_admin<F, In, Req, Resp, Client>(
        &self,
        f: F,
        req: Req,
    ) -> Result<Resp, tonic::Status>
    where
        F: for<'any> TestableFunction<'any, In, tonic::Request<In>, Response<Resp>, Client>,
        Req: tonic::IntoRequest<In>,
        Client: GrpcClient<Channel> + Debug + 'static,
    {
        let claims = self.admin_token().await;
        let jwt = self.cipher().jwt.encode(&claims).unwrap();

        self.send_with(f, req, &jwt).await
    }

    async fn send_<F, In, Resp, Client>(
        &self,
        f: F,
        req: tonic::Request<In>,
    ) -> Result<Resp, tonic::Status>
    where
        F: for<'any> TestableFunction<'any, In, tonic::Request<In>, Response<Resp>, Client>,
        Client: GrpcClient<Channel> + Debug + 'static,
    {
        let socket = Arc::clone(&self.server_input);
        let channel = Endpoint::try_from("http://any.url")
            .unwrap()
            .connect_with_connector(tower::service_fn(move |_: Uri| {
                let socket = Arc::clone(&socket);
                async move { UnixStream::connect(&*socket).await }
            }))
            .await
            .unwrap();
        let mut client = Client::create(channel);
        let resp: Response<Resp> = f(&mut client, req).await?;
        Ok(resp.into_inner())
    }

    pub fn rng(&mut self) -> &mut OsRng {
        &mut self.rng
    }

    pub fn rand_string(&mut self, len: usize) -> String {
        Alphanumeric.sample_string(&mut self.rng, len)
    }

    pub fn rand_email(&mut self) -> String {
        format!("{}@{}.com", self.rand_string(8), self.rand_string(8))
    }
}
