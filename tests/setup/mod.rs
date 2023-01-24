#![allow(dead_code)]
mod dummy_token;

mod helper_traits;

use api::auth::{self, JwtToken, TokenRole, TokenType};
use api::models;
use api::{grpc::blockjoy_ui, TestDb};
pub use dummy_token::*;
use futures_util::{Stream, StreamExt};
use helper_traits::GrpcClient;
use std::convert::TryFrom;
use std::fmt::Debug;
use std::future::Future;
use std::sync::Arc;
use tempfile::{NamedTempFile, TempPath};
use tokio::net::{UnixListener, UnixStream};
use tokio_stream::wrappers::UnixListenerStream;
use tonic::codec::Streaming;
use tonic::transport::{Channel, Endpoint, Uri};
use tonic::Response;

/// Our integration testing helper struct. Can be created cheaply with `new`, and is able to
/// receive requests and return responses. Exposes lots of helpers too to make creating new
/// integration tests easy. Re-exports some of the functionality from `TestDb` (a helper used
/// internally for more unit test-like tests).
pub struct Tester {
    db: TestDb,
    server_input: Arc<TempPath>,
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
    pub async fn new() -> Self {
        let db = TestDb::setup().await;
        let pool = db.pool.clone();
        let socket = NamedTempFile::new().unwrap();
        let socket = Arc::new(socket.into_temp_path());
        std::fs::remove_file(&*socket).unwrap();

        let uds = UnixListener::bind(&*socket).unwrap();
        let stream = UnixListenerStream::new(uds);
        tokio::spawn(async {
            api::grpc::server(pool)
                .await
                .serve_with_incoming(stream)
                .await
                .unwrap()
        });

        let socket = Arc::clone(&socket);

        Tester {
            db,
            server_input: socket,
        }
    }

    pub async fn begin(&self) -> api::models::DbTrx {
        self.db.pool.begin().await.unwrap()
    }

    pub async fn conn(&self) -> sqlx::pool::PoolConnection<sqlx::Postgres> {
        self.db.pool.conn().await.unwrap()
    }

    pub fn meta(&self) -> blockjoy_ui::RequestMeta {
        blockjoy_ui::RequestMeta {
            id: Some(uuid::Uuid::new_v4().to_string()),
            token: None,
            fields: vec![],
            pagination: None,
        }
    }

    pub fn pagination(&self) -> blockjoy_ui::Pagination {
        blockjoy_ui::Pagination {
            current_page: 0,
            items_per_page: 10,
            total_items: None,
        }
    }

    /// Returns an admin user, so a user that has maximal permissions.
    pub async fn admin_user(&self) -> models::User {
        self.db.admin_user().await
    }

    /// Returns a (auth, refresh) token pair.
    pub async fn admin_token(&self) -> (impl JwtToken, impl JwtToken) {
        let auth = self.user_token(&self.admin_user().await);
        let refresh = self.db.user_refresh_token(auth.get_id());
        (auth, refresh)
    }

    pub async fn hosts(&self) -> Vec<models::Host> {
        models::Host::find_all(&mut *self.conn().await)
            .await
            .unwrap()
    }

    pub async fn host(&self) -> models::Host {
        self.hosts().await.first().unwrap().clone()
    }

    pub async fn host2(&self) -> models::Host {
        self.hosts().await.pop().unwrap()
    }

    pub async fn org(&self) -> models::Org {
        models::Org::find_all(&mut *self.conn().await)
            .await
            .unwrap()
            .pop()
            .unwrap()
    }

    pub async fn org_for(&self, user: &models::User) -> models::Org {
        models::Org::find_all_by_user(user.id, &mut *self.conn().await)
            .await
            .unwrap()
            .first()
            .unwrap()
            .clone()
    }

    pub fn user_token(&self, user: &models::User) -> impl JwtToken + Clone {
        auth::UserAuthToken::create_token_for(user, TokenType::UserAuth, TokenRole::User, None)
            .unwrap()
    }

    pub fn host_token(&self, host: &models::Host) -> impl JwtToken + Clone {
        auth::HostAuthToken::create_token_for(host, TokenType::HostAuth, TokenRole::User, None)
            .unwrap()
    }

    pub fn refresh_for(&self, token: &impl JwtToken) -> impl JwtToken + Clone {
        self.db.user_refresh_token(token.get_id())
    }

    pub async fn node(&self) -> models::Node {
        sqlx::query_as(r#"
            INSERT INTO
                nodes (id, org_id, host_id, node_type, blockchain_id)
            VALUES
                ('59edfb35-bbf1-460f-bd3d-e4c86ba73e0d', (SELECT id FROM orgs LIMIT 1), (SELECT id FROM hosts LIMIT 1), '{"id":404}', (SELECT id FROM blockchains LIMIT 1))
            ON CONFLICT (id) DO UPDATE
            SET id = '59edfb35-bbf1-460f-bd3d-e4c86ba73e0d'
            RETURNING *;
        "#)
        .fetch_one(&mut self.conn().await)
        .await
        .unwrap()
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
        self._send(f, req.into_request()).await
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
    pub async fn send_with<F, In, Req, Resp, Client, AuthTkn, RefreshTkn>(
        &self,
        f: F,
        req: Req,
        auth: AuthTkn,
        refresh: RefreshTkn,
    ) -> Result<Resp, tonic::Status>
    where
        F: for<'any> TestableFunction<'any, In, tonic::Request<In>, Response<Resp>, Client>,
        Req: tonic::IntoRequest<In>,
        Client: GrpcClient<Channel> + Debug + 'static,
        AuthTkn: JwtToken,
        RefreshTkn: JwtToken,
    {
        let mut req = req.into_request();

        let auth = format!("Bearer {}", auth.to_base64().unwrap());
        req.metadata_mut()
            .insert("authorization", auth.parse().unwrap());

        let refresh = format!("refresh={}", refresh.to_base64().unwrap());
        req.metadata_mut()
            .insert("cookie", refresh.parse().unwrap());

        self._send(f, req).await
    }

    /// Sends a request with authentication as though the user were an admin. This is the same as
    /// creating an admin token manually and then calling `tester.send_with(_, _, ...admin_tokens)`.
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
        let (auth, refresh) = self.admin_token().await;
        self.send_with(f, req, auth, refresh).await
    }

    async fn _send<F, In, Resp, Client>(
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

    /// This endpoint is used to talk to streaming endpoints (which is only CommandFlow for now).
    /// The types that are used here are a little different compared to the types for the normal
    /// endpoints, because `tonic` uses different types too. The main difference in api is
    /// illustrated by this example:
    ///
    /// ## Example
    /// ```rs,ignore
    /// let stream = tester
    ///     .open_stream_with(Service::endpoint, tokio_stream::once(data), "token")
    ///     .await
    ///     .unwrap();
    /// let data = stream.assert_receives().await;
    /// assert_eq!(data, expected);
    /// ```
    pub async fn open_stream_with<F, In, Req, Resp, Client, S, AuthTkn, RefreshTkn>(
        &self,
        f: F,
        req: Req,
        auth: AuthTkn,
        refresh: RefreshTkn,
    ) -> Result<Streaming<Resp>, tonic::Status>
    where
        F: for<'any> TestableFunction<
            'any,
            In,
            tonic::Request<S>,
            Response<Streaming<Resp>>,
            Client,
        >,
        Req: tonic::IntoStreamingRequest<Message = In, Stream = S>,
        Client: GrpcClient<Channel> + Debug + 'static,
        AuthTkn: JwtToken,
        RefreshTkn: JwtToken,
    {
        let mut req = req.into_streaming_request();

        let auth = format!("Bearer {}", auth.to_base64().unwrap());
        req.metadata_mut()
            .insert("authorization", auth.parse().unwrap());

        let refresh = format!("refresh={}", refresh.to_base64().unwrap());
        req.metadata_mut()
            .insert("cookie", refresh.parse().unwrap());

        self._open_stream(f, req).await
    }

    /// This endpoint is used to talk to streaming endpoints (which is only CommandFlow for now).
    /// The types that are used here are a little different compared to the types for the normal
    /// endpoints, because `tonic` uses different types too. The main difference in api is
    /// illustrated by this example:
    ///
    /// ## Example
    /// ```rs,ignore
    ///
    /// ```
    pub async fn open_stream_admin<F, In, Req, Resp, Client, S>(
        &self,
        f: F,
        req: Req,
    ) -> Result<Streaming<Resp>, tonic::Status>
    where
        F: for<'any> TestableFunction<
            'any,
            In,
            tonic::Request<S>,
            Response<Streaming<Resp>>,
            Client,
        >,
        Req: tonic::IntoStreamingRequest<Message = In, Stream = S>,
        Client: GrpcClient<Channel> + Debug + 'static,
    {
        let (auth, refresh) = self.admin_token().await;
        self.open_stream_with(f, req, auth, refresh).await
    }

    pub async fn _open_stream<F, In, S, Resp, Client>(
        &self,
        f: F,
        req: tonic::Request<S>,
    ) -> Result<Streaming<Resp>, tonic::Status>
    where
        F: for<'any> TestableFunction<
            'any,
            In,
            tonic::Request<S>,
            Response<Streaming<Resp>>,
            Client,
        >,
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
        let resp: Response<Streaming<Resp>> = f(&mut client, req).await?;
        Ok(resp.into_inner())
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

/// A extension trait for streams that we can use to do quick and easy assertions.
#[tonic::async_trait]
pub trait TestStream: Stream {
    /// Panics if the stream does not receive the provided element
    async fn assert_receives(&mut self) -> Self::Item
    where
        Self: Unpin,
    {
        tokio::select! {
            elem = self.next() => {
                match elem {
                    Some(elem) => elem,
                    None => panic!("Stream returned None!"),
                }
            },
            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                panic!("Stream did not receive any elements!");
            },
        }
    }

    /// Panics if the stream receives an element.
    async fn assert_empty(&mut self)
    where
        Self: Unpin,
        Self::Item: Debug,
    {
        tokio::select! {
            elem = self.next() => {
                if let Some(elem) = elem {
                    panic!("Stream received data! `{elem:?}`");
                }
            },
            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => { },
        }
    }
}

impl<T> TestStream for T where T: Stream {}
