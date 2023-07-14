//! This module contains the code related to database tables. Each table is represented by one or
//! more models, which are structs that have a field for the columns of the queries that they
//! interact with. There may exist multiple models for a given table, for example models that are
//! used for updating rows often do not contain all of the columns, whereas models that are used
//! for selecting usually do.

pub mod api_key;
mod blacklist_token;
mod blockchain;
mod command;
mod host;
mod invitation;
mod ip_address;
mod node;
mod node_key_file;
mod node_log;
mod node_scheduler;
mod node_type;
mod org;
mod paginate;
mod region;
pub mod schema;
pub mod subscription;
mod user;

pub use blacklist_token::*;
pub use blockchain::*;
pub use command::*;
pub use host::*;
pub use invitation::*;
pub use ip_address::*;
pub use node::*;
pub use node_key_file::*;
pub use node_log::*;
pub use node_scheduler::*;
pub use node_type::*;
pub use org::*;
use paginate::Paginate;
pub use region::*;
pub use user::*;

use std::cmp;
use std::sync::Arc;

use derive_more::{Deref, DerefMut};
use diesel_async::pooled_connection::bb8::{Pool, PooledConnection};
use diesel_async::scoped_futures::{ScopedBoxFuture, ScopedFutureExt};
use diesel_async::{AsyncConnection, AsyncPgConnection};

use crate::auth;
use crate::auth::claims::Claims;
use crate::auth::endpoint::Endpoint;
use crate::config::Context;

diesel::sql_function!(fn lower(x: diesel::sql_types::Text) -> diesel::sql_types::Text);
diesel::sql_function!(fn string_to_array(version: diesel::sql_types::Text, split: diesel::sql_types::Text) -> diesel::sql_types::Array<diesel::sql_types::Text>);

/// Our wrapper type for a ref counted postgres pool. We use a wrapper type because the functions
/// `begin` and `conn` return a `Result<_, sqlx::Error>`. From our controllers we must return a
/// `Result<_, tonic::Status>`, but there is no `impl From<sqlx::Error> for tonic::Status`, nor can
/// we create one. This means that we cannot use the `?`-operator in our controllers when we
/// retrieve a connection or when we begin a transaction. Since we do this in virtually every
/// controller, it is worth creating this wrapper type that also has the functions `begin` and
/// `conn`, but altered such that they return a `Result<_, error::Error>`. With this Err-variant
/// we _can_ use the `?`-operator in our controllers.
#[derive(Clone)]
pub struct DbPool {
    pool: Pool<AsyncPgConnection>,
    pub context: Arc<Context>,
}

impl DbPool {
    pub fn new(pool: Pool<AsyncPgConnection>, context: Arc<Context>) -> Self {
        Self { pool, context }
    }

    pub async fn conn(&self) -> crate::Result<Conn> {
        Ok(Conn {
            inner: self.pool.get_owned().await?,
            context: self.context.clone(),
        })
    }

    /// Run a closure within a transactional context.
    pub async fn trx<'a, F, T, E>(&self, f: F) -> Result<T, tonic::Status>
    where
        F: for<'r> FnOnce(&'r mut Conn) -> ScopedBoxFuture<'a, 'r, Result<T, E>> + Send + 'a,
        T: Send + 'a,
        E: From<diesel::result::Error> + Into<tonic::Status> + Send + 'a,
    {
        let mut conn = self.conn().await?;
        conn.transaction(|c| f(c).scope_boxed())
            .await
            .map_err(Into::into)
    }

    /// Run a closure without a transactional context.
    // Most of the constraints here (i.e. the ScopedBoxFuture and the Send and 'a bounds on T) are
    // not necessary here. They are here to ensure that any context where it is possible to call
    // `run`, it is also possible to call `trx`, making it very easy to switch between a
    // transactional and non-transactional context.
    pub async fn run<'a, F, T, E>(&self, f: F) -> Result<T, tonic::Status>
    where
        F: for<'r> FnOnce(&'r mut Conn) -> ScopedBoxFuture<'a, 'r, Result<T, E>> + Send + 'a,
        T: Send + 'a,
        E: From<diesel::result::Error> + Into<tonic::Status> + Send + 'a,
    {
        let mut conn = self.conn().await?;
        f(&mut conn).await.map_err(Into::into)
    }

    pub fn is_closed(&self) -> bool {
        self.pool.state().connections == 0
    }
}

/// A wrapper around an `AsyncPgConnection` together with some `Context` metadata.
#[derive(Deref, DerefMut)]
pub struct Conn {
    #[deref]
    #[deref_mut]
    inner: PooledConnection<'static, AsyncPgConnection>,
    pub context: Arc<Context>,
}

impl Conn {
    pub async fn claims<T>(
        &mut self,
        req: &tonic::Request<T>,
        endpoint: Endpoint,
    ) -> Result<Claims, auth::Error> {
        let auth = self.context.auth.clone();
        auth.claims(req, endpoint, self).await
    }
}

fn semver_cmp(s1: &str, s2: &str) -> Option<cmp::Ordering> {
    s1.split('.')
        .zip(s2.split('.'))
        .find_map(|(s1, s2)| cmp_str(s1, s2))
}

fn cmp_str(s1: &str, s2: &str) -> Option<cmp::Ordering> {
    let take_nums = |s: &str| s.chars().take_while(|c| c.is_numeric()).collect::<String>();
    let parse_nums = |s: String| s.parse().ok();
    parse_nums(take_nums(s1))
        .and_then(|n1: i64| parse_nums(take_nums(s2)).map(move |n2| (n1, n2)))
        .map(|(n1, n2)| n1.cmp(&n2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmp::Ordering::*;

    #[test]
    fn test_semver_cmp() {
        assert_eq!(semver_cmp("1.2.3", "1.2.3"), Some(Equal));
        assert_eq!(semver_cmp("3.2.3", "1.2.3"), Some(Greater));
        assert_eq!(semver_cmp("1.2.3", "3.2.3"), Some(Less));
        assert_eq!(semver_cmp("1.2.3-beta", "1.2.3"), Some(Equal));
        assert_eq!(semver_cmp("1.2.3-beta3", "1.2.3.4"), Some(Equal));
        assert_eq!(semver_cmp("1.2-beta.3", "1.2"), Some(Equal));
    }

    #[test]
    fn test_cmp_str() {
        assert_eq!(cmp_str("1", "1"), Some(Equal));
        assert_eq!(cmp_str("1", "2"), Some(Less));
        assert_eq!(cmp_str("3", "2"), Some(Greater));
        assert_eq!(cmp_str("3-beta", "2"), Some(Greater));
        assert_eq!(cmp_str("3", "2-beta"), Some(Greater));
    }
}
