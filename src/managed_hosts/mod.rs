use crate::errors::{ApiError, Result as ApiResult};
use crate::models::Host;
use anyhow::anyhow;
use sqlx::PgPool;
use std::str::FromStr;

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

    /// Get the ID of the organization used to hold all managed hosts
    async fn get_managed_org(_db: &PgPool) -> ApiResult<uuid::Uuid> {
        let id =
            std::env::var("MANAGED_ORG_ID").map_err(|e| ApiError::UnexpectedError(anyhow!(e)))?;
        Ok(uuid::Uuid::from_str(id.as_str())?)
    }
}
