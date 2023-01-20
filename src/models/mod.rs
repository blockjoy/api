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
//! connection argument as  `tx: &mut super::DbTrx<'_>`, whereas functions that do not mutate take
//! `db: &mut sqlx::PgConnection` as their database connection. This ensures that mutating
//! functions _must_ happen from within a transaction, and functions that not mutate may either be\
//! called from a transaction or from a 'bare' connection.

mod blockchain;
mod broadcast;
mod command;
mod host;
mod info;
mod invoice;
mod node;
mod org;
mod payment;
mod reward;
mod user;
// needs to be brought into namespace like this because of
// name ambiguities with another crate
mod blacklist_token;
mod invitation;
mod ip_address;
mod node_key_file;
mod node_property_value;
mod node_type;

use crate::errors::Result;
pub use blacklist_token::*;
pub use blockchain::*;
pub use broadcast::*;
pub use command::*;
use futures_util::{future::BoxFuture, stream::BoxStream};
pub use host::*;
pub use info::*;
pub use invitation::*;
pub use invoice::*;
pub use ip_address::*;
pub use node::*;
pub use node_key_file::*;
pub use node_property_value::*;
pub use node_type::*;
pub use org::*;
pub use payment::*;
pub use reward::*;
use sqlx::{
    postgres::{PgQueryResult, PgRow, PgStatement, PgTypeInfo},
    Describe, Either, Error, Execute, Postgres,
};
pub use user::*;

pub const STAKE_QUOTA_DEFAULT: i64 = 5;
pub const FEE_BPS_DEFAULT: i64 = 300;

pub type PgQuery<'a> = sqlx::query::Query<'a, Postgres, sqlx::postgres::PgArguments>;
#[tonic::async_trait]
pub trait UpdateInfo<T, R> {
    async fn update_info(info: T, tx: &mut DbTrx<'_>) -> Result<R>;
}

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
    /// We do not do any refcounting like wrapping `pool` in an `Arc`, because `sqlx::PgPool`
    /// internally already does refcounting:
    /// https://docs.rs/sqlx-core/0.6.2/src/sqlx_core/pool/mod.rs.html#244
    pool: sqlx::PgPool,
}

/// This is a wrapper type for a database connection that is in a transaction-state, i.e. `BEGIN;`
/// has been ran. The same justification as above applies to why we use a wrapper type here.
#[derive(Debug)]
pub struct DbTrx<'a>(sqlx::Transaction<'a, Postgres>);

impl DbPool {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Begins a new new database connnection. This means that the queries performed using this as
    /// a connection will not be flushed unless `commit` is called on the transaction. This
    /// function should be used in controllers that perform writes or deletes on the database.
    pub async fn begin(&self) -> Result<DbTrx<'_>> {
        Ok(DbTrx(self.pool.begin().await?))
    }

    /// Returns a database connection that is not in a transition state. Use this for read-only
    /// endpoints.
    pub async fn conn(&self) -> Result<sqlx::pool::PoolConnection<Postgres>> {
        Ok(self.pool.acquire().await?)
    }

    pub fn is_closed(&self) -> bool {
        self.pool.is_closed()
    }

    pub fn inner(&self) -> &sqlx::PgPool {
        &self.pool
    }

    #[cfg(test)]
    pub async fn close(&self) {
        self.pool.close().await
    }
}

impl<'a> DbTrx<'a> {
    pub async fn commit(self) -> Result<()> {
        Ok(self.0.commit().await?)
    }
}

impl<'a> std::ops::Deref for DbTrx<'a> {
    type Target = sqlx::PgConnection;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> std::ops::DerefMut for DbTrx<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Implementation of Executor for our custom database transaction type. Mostly just lifetime magic
/// and can safely be ignored wrt understanding the codebase.
impl<'c, 't> sqlx::Executor<'t> for &'t mut DbTrx<'c> {
    type Database = Postgres;

    fn fetch_many<'e, 'q: 'e, E: 'q>(
        self,
        query: E,
    ) -> BoxStream<'e, Result<Either<PgQueryResult, PgRow>, Error>>
    where
        't: 'e,
        E: Execute<'q, Postgres>,
    {
        self.0.fetch_many(query)
    }

    fn fetch_optional<'e, 'q: 'e, E: 'q>(
        self,
        query: E,
    ) -> BoxFuture<'e, Result<Option<PgRow>, Error>>
    where
        't: 'e,
        E: Execute<'q, Postgres>,
    {
        self.0.fetch_optional(query)
    }

    fn prepare_with<'e, 'q: 'e>(
        self,
        sql: &'q str,
        parameters: &'e [PgTypeInfo],
    ) -> BoxFuture<'e, Result<PgStatement<'q>, Error>>
    where
        't: 'e,
    {
        self.0.prepare_with(sql, parameters)
    }

    fn describe<'e, 'q: 'e>(self, sql: &'q str) -> BoxFuture<'e, Result<Describe<Postgres>, Error>>
    where
        't: 'e,
    {
        self.0.describe(sql)
    }
}
