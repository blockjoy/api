#![allow(dead_code)]

mod dummy_token;
mod helper_traits;

use blockvisor_api::auth;
use blockvisor_api::models;
use blockvisor_api::TestDb;
use diesel_async::pooled_connection::bb8::PooledConnection;
use diesel_async::AsyncPgConnection;
pub use dummy_token::*;
use helper_traits::GrpcClient;
use mockito::ServerGuard;
use rand::Rng;
use std::convert::TryFrom;
use std::env::{remove_var, set_var};
use std::fmt::Debug;
use std::future::Future;
use std::sync::Arc;
use tempfile::{NamedTempFile, TempPath};
use tokio::net::{UnixListener, UnixStream};
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::{Channel, Endpoint, Uri};
use tonic::Response;

use blockvisor_api::grpc::api::auth_service_client::AuthServiceClient;
type AuthService = AuthServiceClient<tonic::transport::Channel>;

/// Our integration testing helper struct. Can be created cheaply with `new`, and is able to
/// receive requests and return responses. Exposes lots of helpers too to make creating new
/// integration tests easy. Re-exports some of the functionality from `TestDb` (a helper used
/// internally for more unit test-like tests).
pub struct Tester {
    db: TestDb,
    server_input: Arc<TempPath>,
    cloudflare: Arc<Option<ServerGuard>>,
}

impl std::ops::Deref for Tester {
    type Target = TestDb;

    fn deref(&self) -> &Self::Target {
        &self.db
    }
}

impl std::ops::DerefMut for Tester {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.db
    }
}

impl Tester {
    /// Creates a new tester, with the cloudflare API mocked.
    pub async fn new() -> Self {
        Self::new_with(true).await
    }

    /// Creates a new tester, but with the cloudflare API mocked if `cloudflare_mocked` is `true`.
    /// WARN: If `false`, the cloudflare API will be called
    pub async fn new_with(cloudflare_mocked: bool) -> Self {
        let db = TestDb::setup().await;
        let pool = db.pool.clone();
        let socket = NamedTempFile::new().unwrap();
        let socket = Arc::new(socket.into_temp_path());
        std::fs::remove_file(&*socket).unwrap();

        let uds = UnixListener::bind(&*socket).unwrap();
        let stream = UnixListenerStream::new(uds);
        let cloudflare_server = if cloudflare_mocked {
            Some(Self::mock_cloudflare_api().await)
        } else {
            None
        };
        tokio::spawn(async {
            blockvisor_api::grpc::server(pool)
                .await
                .serve_with_incoming(stream)
                .await
                .unwrap()
        });

        let socket = Arc::clone(&socket);

        Tester {
            db,
            server_input: socket,
            cloudflare: Arc::new(cloudflare_server),
        }
    }

    pub async fn conn(&self) -> PooledConnection<'_, AsyncPgConnection> {
        self.db.conn().await
    }

    /// Returns an admin user, so a user that has maximal permissions.
    pub async fn user(&self) -> models::User {
        self.db.user().await
    }

    /// Returns a pleb user, that has the same permissions but it is not confirmed.
    pub async fn unconfirmed_user(&self) -> models::User {
        self.db.unconfirmed_user().await
    }

    /// Returns a auth token for the admin user in the database.
    pub async fn admin_token(&self) -> auth::Jwt {
        let admin = self.user().await;
        self.user_token(&admin).await
    }

    pub async fn admin_refresh(&self) -> auth::Refresh {
        let admin = self.user().await;
        let iat = chrono::Utc::now();
        let exp = chrono::Duration::minutes(15);
        auth::Refresh::new(admin.id, iat, exp).unwrap()
    }

    pub async fn hosts(&self) -> Vec<models::Host> {
        let mut conn = self.conn().await;
        models::Host::find_all(&mut conn).await.unwrap()
    }

    pub async fn host(&self) -> models::Host {
        self.hosts().await.pop().unwrap()
    }

    pub async fn host2(&self) -> models::Host {
        let mut hosts = self.hosts().await;
        hosts.pop().unwrap();
        hosts.pop().unwrap()
    }

    pub async fn org(&self) -> models::Org {
        self.db.org().await
    }

    pub async fn org_for(&self, user: &models::User) -> models::Org {
        let mut conn = self.conn().await;
        models::Org::find_all_by_user(user.id, &mut conn)
            .await
            .unwrap()
            .into_iter()
            .find(|o| !o.is_personal)
            .unwrap()
    }

    pub async fn user_token(&self, user: &models::User) -> auth::Jwt {
        let req = blockvisor_api::grpc::api::AuthServiceLoginRequest {
            email: user.email.clone(),
            password: "abc12345".to_string(),
        };
        let resp = self.send(AuthService::login, req).await.unwrap();
        auth::Jwt::decode(&resp.token).unwrap()
    }

    pub fn host_token(&self, host: &models::Host) -> auth::Jwt {
        let iat = chrono::Utc::now();
        let claims = auth::Claims {
            resource_type: auth::ResourceType::Host,
            resource_id: host.id,
            iat,
            exp: (iat + chrono::Duration::minutes(15)),
            endpoints: auth::Endpoints::Wildcard,
            data: Default::default(),
        };
        auth::Jwt { claims }
    }

    // pub fn refresh_for(&self, token: &impl JwtToken) -> impl JwtToken + Clone {
    //     self.db.user_refresh_token(token.get_id())
    // }

    pub async fn node(&self) -> models::Node {
        let mut conn = self.conn().await;
        let node_id = "cdbbc736-f399-42ab-86cf-617ce983011d".parse().unwrap();
        models::Node::find_by_id(node_id, &mut conn).await.unwrap()
    }

    pub async fn blockchain(&self) -> models::Blockchain {
        self.db.blockchain().await
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
        token: auth::Jwt,
    ) -> Result<Resp, tonic::Status>
    where
        F: for<'any> TestableFunction<'any, In, tonic::Request<In>, Response<Resp>, Client>,
        Req: tonic::IntoRequest<In>,
        Client: GrpcClient<Channel> + Debug + 'static,
    {
        let mut req = req.into_request();
        let auth = format!("Bearer {}", token.encode().unwrap());
        req.metadata_mut()
            .insert("authorization", auth.parse().unwrap());
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
        let token = self.admin_token().await;
        self.send_with(f, req, token).await
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

    pub async fn mock_cloudflare_api() -> ServerGuard {
        let mut cloudfare_server = mockito::Server::new_async().await;
        let cloudfare_url = cloudfare_server.url();

        remove_var("CF_BASE_URL");
        set_var("CF_BASE_URL", cloudfare_url);

        let mut rng = rand::thread_rng();
        let id_dns = rng.gen_range(200000..5000000);
        cloudfare_server
            .mock(
                "POST",
                mockito::Matcher::Regex(r"^/zones/.*/dns_records$".to_string()),
            )
            .with_status(200)
            .with_body(format!("{{\"result\":{{\"id\":\"{:x}\"}}}}", id_dns))
            .create_async()
            .await;
        cloudfare_server
            .mock(
                "DELETE",
                mockito::Matcher::Regex(r"^/zones/.*/dns_records/.*$".to_string()),
            )
            .with_status(200)
            .create_async()
            .await;
        cloudfare_server
    }
}

/// This is a client function that we can run through the test machinery. This contains a _lot_ of
/// generics so lets break it down:
///
/// 1. `'a`: This is the lifetime of the client. We restrict the lifetime of the generated by the
///    tested function to be at most `'a`, because that future must borrow the client to make
///    progress.
/// 2. `In`: This is the type of the data that goes into the tested function, usually a struct
///    implementing `Deserialize`.
/// 3. `Req`: This is some type that implements `IntoRequest<In>`, meaning that it can be converted
///    into a request containing the `In` structure.
/// 4. `Resp`: This is the type of data that the function returns. Usually a struct (sometimes an
///    enum) that implements `Serialize`.
/// 5. `Client`: This is the client struct that is used to query the server. These are generated by
///    `tonic` from the proto files, and are generic over the transport layer. An example of what
///    could go here is `AuthenticationServiceClient<Channel>`. The `send` functions require that
///    this type implements `GrpcClient`.
pub trait TestableFunction<'a, In, Req, Resp, Client>:
    Fn(&'a mut Client, Req) -> Self::Fut
where
    Client: 'static,
{
    type Fut: 'a + Future<Output = Result<Resp, tonic::Status>>;
}

/// Implement our test function trait for all functions of the right signature.
impl<'a, F, Fut, In, Req, Resp, Client> TestableFunction<'a, In, Req, Resp, Client> for F
where
    F: Fn(&'a mut Client, Req) -> Fut,
    Fut: 'a + Future<Output = Result<Resp, tonic::Status>>,
    Client: 'static,
{
    type Fut = Fut;
}
