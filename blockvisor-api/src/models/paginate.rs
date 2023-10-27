//! This fragment is modified from
//! [here](https://github.com/diesel-rs/diesel/blob/master/examples/postgres/advanced-blog-cli/src/pagination.rs)
//! to work with `diesel_async`.

use diesel::pg::Pg;
use diesel::query_builder::{AstPass, Query, QueryFragment, QueryId};
use diesel::sql_types::BigInt;
use diesel::QueryResult;
use diesel_async::methods::LoadQuery;
use diesel_async::{AsyncPgConnection, RunQueryDsl};

pub trait Paginate: Sized {
    fn paginate(self, limit: i64, offset: i64) -> Paginated<Self>;
}

impl<T> Paginate for T {
    fn paginate(self, limit: i64, offset: i64) -> Paginated<Self> {
        Paginated {
            query: self,
            // If someone requests 0 items, i.e. `self.limit == 0`, we still want to return the
            // correct count, so we need to return at least one row. That's why in this function we
            // change the limit into 1, and then correct in `Paginated::get_results_counted`.
            limit: if limit == 0 { 1 } else { limit },
            offset,
            no_results: limit == 0,
        }
    }
}

#[derive(Debug, Clone, Copy, QueryId)]
pub struct Paginated<T> {
    query: T,
    limit: i64,
    offset: i64,
    no_results: bool,
}

impl<T> Paginated<T> {
    #[allow(clippy::manual_async_fn)] // clippy lies
    pub fn get_results_counted<'a, U>(
        self,
        conn: &mut AsyncPgConnection,
    ) -> impl std::future::Future<Output = QueryResult<(i64, Vec<U>)>> + Send + '_
    where
        Self: LoadQuery<'a, AsyncPgConnection, (U, i64)> + 'static,
        U: Send,
        T: Send,
    {
        // It is not possible to write this function as an async function, because we need to
        // restrict the lifetime of the resulting `Future` to `'_`. If we wrote
        // `async fn get_results_counted<...>(...) -> ... + '_`, then we would not restrict the
        // lifetime of the resulting Future, but rather the lifetime of `Future::Output` to `'_`.
        async move {
            let no_results = self.no_results;
            let results = self.load::<(U, i64)>(conn).await?;
            let total = results.get(0).map_or(0, |x| x.1);
            let records = if no_results {
                vec![]
            } else {
                results.into_iter().map(|x| x.0).collect()
            };
            Ok((total, records))
        }
    }
}

impl<T: Query> Query for Paginated<T> {
    type SqlType = (T::SqlType, BigInt);
}

impl<T> diesel::RunQueryDsl<AsyncPgConnection> for Paginated<T> {}

impl<T> QueryFragment<Pg> for Paginated<T>
where
    T: QueryFragment<Pg>,
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
