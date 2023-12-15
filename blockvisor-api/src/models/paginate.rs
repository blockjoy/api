//! This fragment is modified from
//! [here](https://github.com/diesel-rs/diesel/blob/master/examples/postgres/advanced-blog-cli/src/pagination.rs)
//! to work with `diesel_async`.

use std::future::Future;

use diesel::pg::Pg;
use diesel::query_builder::{AstPass, Query, QueryFragment, QueryId};
use diesel::sql_types::BigInt;
use diesel::QueryResult;
use diesel_async::methods::LoadQuery;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use displaydoc::Display;
use thiserror::Error;
use tonic::Status;

pub trait Paginate: Sized {
    fn paginate(self, limit: u64, offset: u64) -> Result<Paginated<Self>, Error>;
}

impl<Q> Paginate for Q {
    fn paginate(self, limit: u64, offset: u64) -> Result<Paginated<Self>, Error> {
        let (limit, empty) = match i64::try_from(limit).map_err(Error::Limit)? {
            // at least 1 row is needed for the correct count,
            0 => (1, true),
            n => (n, false),
        };
        let offset = i64::try_from(offset).map_err(Error::Offset)?;

        Ok(Paginated {
            query: self,
            limit,
            offset,
            empty,
        })
    }
}

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Failed to parse row count as u64: {0}
    Count(std::num::TryFromIntError),
    /// Failed to parse row limit as i64: {0}
    Limit(std::num::TryFromIntError),
    /// Failed to parse row offset as i64: {0}
    Offset(std::num::TryFromIntError),
    /// Failed to run paginated query: {0}
    Query(diesel::result::Error),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        use Error::*;
        match err {
            Limit(_) => Status::invalid_argument("limit"),
            Offset(_) => Status::invalid_argument("offset"),
            Query(diesel::result::Error::NotFound) => Status::not_found("Not found."),
            Query(_) | Count(_) => Status::internal("Internal error."),
        }
    }
}

#[derive(Clone, Copy, Debug, QueryId)]
pub struct Paginated<Q> {
    query: Q,
    limit: i64,
    offset: i64,
    empty: bool,
}

impl<Q> Paginated<Q> {
    // manual async to restrict Future to `'_` (instead of Future::Output)
    #[allow(clippy::manual_async_fn)]
    pub fn count_results<T>(
        self,
        conn: &mut AsyncPgConnection,
    ) -> impl Future<Output = Result<(Vec<T>, u64), Error>> + Send + '_
    where
        Self: for<'a> LoadQuery<'a, AsyncPgConnection, (T, i64)> + 'static,
        Q: Send,
        T: Send,
    {
        async move {
            let empty = self.empty;

            let data = self.load::<(T, i64)>(conn).await.map_err(Error::Query)?;
            let count = data.get(0).map_or(0, |x| x.1);
            let count = u64::try_from(count).map_err(Error::Count)?;

            let rows = if empty {
                vec![]
            } else {
                data.into_iter().map(|x| x.0).collect()
            };

            Ok((rows, count))
        }
    }
}

impl<Q: Query> Query for Paginated<Q> {
    type SqlType = (Q::SqlType, BigInt);
}

impl<Q> diesel::RunQueryDsl<AsyncPgConnection> for Paginated<Q> {}

impl<Q> QueryFragment<Pg> for Paginated<Q>
where
    Q: QueryFragment<Pg>,
{
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, Pg>) -> QueryResult<()> {
        out.push_sql("SELECT *, COUNT(*) OVER () FROM (");
        self.query.walk_ast(out.reborrow())?;
        out.push_sql(") t LIMIT ");
        out.push_bind_param::<BigInt, _>(&self.limit)?;
        out.push_sql(" OFFSET ");
        out.push_bind_param::<BigInt, _>(&self.offset)?;
        Ok(())
    }
}
