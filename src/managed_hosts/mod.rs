use crate::errors::{ApiError, Result as ApiResult};
use crate::models::Host;
use sqlx::PgPool;

pub struct ManagedHosts {}

impl ManagedHosts {
    /// TODO: Return the next usable host for given Node
    /// For now we just return the only host we know works
    /// Real implementation needs to receive node-type resources in order to find a good fit
    pub async fn next_available_host(db: &PgPool) -> ApiResult<Host> {
        let org_id = ManagedHosts::get_managed_org(db).await?;
        let host = sqlx::query("select * from host where org_id = $1 limit 1")
            .bind(org_id)
            .fetch_one(db)
            .await
            .map(|row| Host::try_from(row).unwrap_or_default())
            .map_err(ApiError::from);

        match host {
            Ok(host) => Ok(host),
            Err(e) => {
                tracing::error!("didn't find managed host: {e}");
                Err(e)
            }
        }
    }

    async fn get_managed_org(_db: &PgPool) -> ApiResult<uuid::Uuid> {
        Ok(uuid::Uuid::parse_str(
            "ed0fa3a5-bac8-4d71-aca4-63d0afd1189c",
        )?)
    }
}
