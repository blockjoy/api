//! This module contains the code related to database tables. Each table is represented by one or
//! more models, which are structs that have a field for the columns of the queries that they
//! interact with. There may exist multiple models for a given table, for example models that are
//! used for updating rows often do not contain all of the columns, whereas models that are used
//! for selecting usually do.
//!
//! ### Database Connection Type
//! We want our endpoints to be atomic, meaning that if an api call succeeds, it will succeed in
//! its entirety. When it fails, it must not have modified the state of the database. To this end
//! we distinguish between two different types of function that interact with the database. Those
//! that mutate, i.e. perform queries using INSERT, UPDATE or DELETE, and those that do not, i.e.
//! those that only do SELECT. To ensure atomicity, the functions that mutate _must_ be called from
//! within a transaction. There is no easy way to ensure that this happens automatically, but we
//! can at least keep the responsibility of verifying this contained to this module. The way we
//! enforce this is by having the functions that mutate accept a database connection of a different
//! type than those that do  not mutate. Functions that mutate state will accept their database
//! connection argument as  `conn: &mut AsyncPgConnection`, whereas functions that do not mutate take
//! `conn: &mut AsyncPgConnection` as their database connection. This ensures that mutating
//! functions _must_ happen from within a transaction, and functions that not mutate may either be\
//! called from a transaction or from a 'bare' connection.

mod blacklist_token;
mod blockchain;
mod broadcast;
mod command;
mod host;
mod host_provision;
mod invitation;
mod invoice;
mod ip_address;
mod node;
mod node_key_file;
mod node_property;
mod node_type;
mod org;
mod payment;
pub mod schema;
mod user;

use crate::errors::Result;
use diesel_async::pooled_connection::bb8::{Pool, PooledConnection};
use diesel_async::scoped_futures::{ScopedBoxFuture, ScopedFutureExt};
use diesel_async::{AsyncConnection, AsyncPgConnection};

pub use blacklist_token::*;
pub use blockchain::*;
pub use broadcast::*;
pub use command::*;
pub use host::*;
pub use host_provision::*;
pub use invitation::*;
pub use invoice::*;
pub use ip_address::*;
pub use node::*;
pub use node_key_file::*;
pub use node_property::*;
pub use node_type::*;
pub use org::*;
pub use payment::*;
pub use user::*;

pub const STAKE_QUOTA_DEFAULT: i64 = 3;
pub const FEE_BPS_DEFAULT: i64 = 300;

#[tonic::async_trait]
pub trait UpdateInfo<T, R> {
    async fn update_info(info: T, conn: &mut AsyncPgConnection) -> Result<R>;
}

diesel::sql_function!(fn lower(x: diesel::sql_types::Text) -> diesel::sql_types::Text);

/// Our wrapper type for a ref counted postgres pool. We use a wrapper type because the functions
/// `begin` and `conn` return a `Result<_, sqlx::Error>`. From our controllers we must return a
/// `Result<_, tonic::Status>`, but there is no `impl From<sqlx::Error> for tonic::Status`, nor can
/// we create one. This means that we cannot use the `?`-operator in our controllers when we
/// retrieve a connection or when we begin a transaction. Since we do this in virtually every
/// controller, it is worth creating this wrapper type that also has the functions `begin` and
/// `conn`, but altered such that they return a `Result<_, errors::Error>`. With this Err-variant
/// we _can_ use the `?`-operator in our controllers.
#[derive(Debug, Clone)]
pub struct DbPool {
    pool: Pool<diesel_async::AsyncPgConnection>,
}

// /// This is a wrapper type for a database connection that is in a transaction-state, i.e. `BEGIN;`
// /// has been ran. The same justification as above applies to why we use a wrapper type here.
// pub struct DbTrx<'a, 'b>(&'a mut PooledConnection<'b, AsyncPgConnection>);

impl DbPool {
    pub fn new(pool: Pool<diesel_async::AsyncPgConnection>) -> Self {
        Self { pool }
    }

    pub async fn trx<'a, F, T>(&self, f: F) -> Result<T>
    where
        F: for<'r> FnOnce(
                &'r mut diesel_async::AsyncPgConnection,
            ) -> ScopedBoxFuture<'a, 'r, crate::Result<T>>
            + Send
            + 'a,
        T: Send + 'a,
    {
        self.pool
            .get()
            .await?
            .transaction(|c| {
                async move {
                    let ok = f(c).await?;
                    Ok(ok)
                }
                .scope_boxed()
            })
            .await
    }

    /// Returns a database connection that is not in a transition state. Use this for read-only
    /// endpoints.
    pub async fn conn(&self) -> Result<PooledConnection<'_, AsyncPgConnection>> {
        Ok(self.pool.get().await?)
    }

    pub fn is_closed(&self) -> bool {
        self.pool.state().connections == 0
    }

    pub fn inner(&self) -> &Pool<diesel_async::AsyncPgConnection> {
        &self.pool
    }
}

// pub trait TrxFn<'a, Ret>: FnOnce(&'a mut diesel_async::AsyncPgConnection) -> Self::Fut {
//     type Fut: 'a + futures_util::Future<Output = crate::Result<Ret>> + Send;
// }

// /// Implement our test function trait for all functions of the right signature.
// impl<'a: 'b, 'b, F, Fut, Ret> TrxFn<'a, Ret> for F
// where
//     F: FnOnce(&'a mut diesel_async::AsyncPgConnection) -> Fut,
//     Fut: 'b + futures_util::Future<Output = crate::Result<Ret>> + Send,
//     Ret: 'static
// {
//     type Fut = Fut;
// }

// impl std::ops::Deref for DbTrx<'_, '_> {
//     type Target = diesel_async::AsyncPgConnection;

//     fn deref(&self) -> &Self::Target {
//         &self.0
//     }
// }

// impl std::ops::DerefMut for DbTrx<'_, '_> {
//     fn deref_mut(&mut self) -> &mut Self::Target {
//         &mut self.0
//     }
// }
