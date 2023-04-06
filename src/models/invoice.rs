use super::schema::{invoices, users};
use crate::models::payment::PaymentDue;
use crate::Result;
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel_async::{AsyncPgConnection, RunQueryDsl};

#[derive(Debug, Queryable)]
pub struct Invoice {
    pub id: i32,
    pub user_id: uuid::Uuid,
    pub amount: i64,
    pub validators_count: i64,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub is_paid: bool,
    pub earnings: i64,
    pub fee_bps: i64,
}

#[derive(Debug, Queryable)]
pub struct InvoiceWithPayAdress {
    pub invoice: Invoice,
    pub pay_address: Option<String>,
}

impl Invoice {
    pub async fn find_all_by_user(
        user_id: uuid::Uuid,
        conn: &mut AsyncPgConnection,
    ) -> Result<Vec<InvoiceWithPayAdress>> {
        let invoices = invoices::table
            .filter(invoices::user_id.eq(user_id))
            .inner_join(users::table)
            .order_by(invoices::ends_at.desc())
            .select((invoices::all_columns, users::pay_address))
            .get_results(conn)
            .await?;
        Ok(invoices)
    }

    /// Gets all wallets addresses with a due amount.
    pub async fn find_all_payments_due(conn: &mut AsyncPgConnection) -> Result<Vec<PaymentDue>> {
        let payments = diesel::sql_query(
            "
            SELECT
                users.pay_address,
                COALESCE(sum(amount), 0),
                min(ends_at)
            FROM invoices
            INNER JOIN users on users.id = invoices.user_id
            WHERE is_paid = false
            GROUP BY address;",
        )
        .get_results(conn)
        .await?;
        Ok(payments)
    }
}
