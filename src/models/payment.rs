use crate::Result;
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel_async::{AsyncPgConnection, RunQueryDsl};

use super::schema::payments;

#[derive(Debug, Clone, Insertable, Queryable)]
#[diesel(table_name = payments)]
pub struct Payment {
    pub hash: String,
    pub user_id: uuid::Uuid,
    pub block: i64,
    pub payer: String,
    pub payee: String,
    pub amount: i64,
    pub oracle_price: i64,
    pub created_at: Option<DateTime<Utc>>,
}

impl Payment {
    pub async fn create(payments: &[Payment], conn: &mut AsyncPgConnection) -> Result<()> {
        diesel::insert_into(payments::table)
            .values(payments)
            .execute(conn)
            .await?;

        Ok(())
    }

    pub async fn find_all_by_user(
        user_id: uuid::Uuid,
        conn: &mut AsyncPgConnection,
    ) -> Result<Vec<Payment>> {
        let payments = payments::table
            .filter(payments::user_id.eq(user_id))
            .get_results(conn)
            .await?;
        Ok(payments)
    }
}

#[derive(Debug, Clone, QueryableByName)]
pub struct PaymentDue {
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    pub pay_address: Option<String>,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub amount: i64,
    #[diesel(sql_type = diesel::sql_types::Timestamptz)]
    pub due_date: DateTime<Utc>,
}
