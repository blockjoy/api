use crate::errors::{ApiError, Result as ApiResult};
use crate::grpc::helpers::required;
use anyhow::anyhow;
use ipnet::{IpAddrRange, Ipv4AddrRange};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::net::{IpAddr, Ipv4Addr};
use std::str::FromStr;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct IpAddress {
    pub(crate) id: Uuid,
    // Type IpAddr is required by sqlx
    pub(crate) ip: IpAddr,
    pub(crate) host_id: Option<Uuid>,
    pub(crate) is_assigned: bool,
}

pub struct IpAddressRequest {
    pub(crate) ip: IpAddr,
    pub(crate) host_id: Option<Uuid>,
}

pub struct IpAddressRangeRequest {
    pub(crate) from: IpAddr,
    pub(crate) to: IpAddr,
    pub(crate) host_id: Option<Uuid>,
}

impl IpAddressRangeRequest {
    pub fn try_new(from: IpAddr, to: IpAddr, host_id: Option<Uuid>) -> ApiResult<Self> {
        if to < from {
            Err(ApiError::UnexpectedError(anyhow!(
                "TO IP can't be smaller as FROM IP"
            )))
        } else {
            Ok(Self { from, to, host_id })
        }
    }
}

pub struct IpAddressSelectiveUpdate {
    pub(crate) id: Uuid,
    pub(crate) host_id: Option<Uuid>,
    pub(crate) assigned: Option<bool>,
}

impl IpAddress {
    pub async fn create(req: IpAddressRequest, db: &PgPool) -> ApiResult<Self> {
        sqlx::query_as::<_, Self>(
            r#"INSERT INTO ip_addresses (ip, host_id) 
                   values ($1, $2, $3) RETURNING *"#,
        )
        .bind(req.ip)
        .bind(req.host_id)
        .fetch_one(db)
        .await
        .map_err(ApiError::from)
    }

    pub async fn create_range(req: IpAddressRangeRequest, db: &PgPool) -> ApiResult<Vec<Self>> {
        // Type IpAddr is required by sqlx, so we have to convert to Ipv4Addr forth/back
        let start_range = Ipv4Addr::from_str(req.from.to_string().as_str())
            .map_err(|e| ApiError::UnexpectedError(anyhow!(e)))?;
        let stop_range = Ipv4Addr::from_str(req.to.to_string().as_str())
            .map_err(|e| ApiError::UnexpectedError(anyhow!(e)))?;
        let ip_addrs = IpAddrRange::from(Ipv4AddrRange::new(start_range, stop_range));
        let mut created: Vec<Self> = vec![];
        let mut tx = db.begin().await?;

        for ip in ip_addrs {
            tracing::debug!("creating ip {} for host {:?}", ip, req.host_id);

            created.push(
                sqlx::query_as::<_, Self>(
                    r#"INSERT INTO ip_addresses (ip, host_id) 
                   values ($1, $2) RETURNING *"#,
                )
                .bind(ip)
                .bind(req.host_id)
                .fetch_one(&mut tx)
                .await
                .map_err(ApiError::from)?,
            );
        }

        tx.commit().await?;

        Ok(created)
    }

    pub async fn update(update: IpAddressSelectiveUpdate, db: &PgPool) -> ApiResult<Self> {
        sqlx::query_as::<_, Self>(
            r#"UPDATE ip_addresses SET 
                    host_id = COALESCE($1, host_id),
                    is_assigned = COALESCE($2, is_assigned)
                WHERE id = $3 RETURNING *"#,
        )
        .bind(update.host_id)
        .bind(update.assigned)
        .bind(update.id)
        .fetch_one(db)
        .await
        .map_err(ApiError::from)
    }

    /// Helper returning the next valid IP address for host identified by `host_id`
    pub async fn next_for_host(host_id: Uuid, db: &PgPool) -> ApiResult<Self> {
        let ip = sqlx::query_as::<_, Self>(
            r#"SELECT * from ip_addresses
                    WHERE host_id = $1 and is_assigned = false
                    ORDER BY ip ASC LIMIT 1"#,
        )
        .bind(host_id)
        .fetch_one(db)
        .await
        .map_err(ApiError::IpAssignmentError)?;

        Self::assign(ip.id, ip.host_id.ok_or_else(required("host.id"))?, db).await
    }

    /// Helper assigned IP address identified by `ìd` to host identified by `host_id`
    pub async fn assign(id: Uuid, host_id: Uuid, db: &PgPool) -> ApiResult<Self> {
        let fields = IpAddressSelectiveUpdate {
            id,
            host_id: Some(host_id),
            assigned: Some(true),
        };

        Self::update(fields, db).await
    }

    /// Helper assigned IP address identified by `ìd` to host identified by `host_id`
    pub async fn unassign(id: Uuid, host_id: Uuid, db: &PgPool) -> ApiResult<Self> {
        let fields = IpAddressSelectiveUpdate {
            id,
            host_id: Some(host_id),
            assigned: Some(false),
        };

        Self::update(fields, db).await
    }

    pub fn in_range(ip: IpAddr, from: IpAddr, to: IpAddr) -> bool {
        !(ip < from || ip > to)
    }

    pub async fn assigned(ip: IpAddr, db: &PgPool) -> ApiResult<bool> {
        let ip_count: i32 = sqlx::query_scalar(
            r#"SELECT count(id)::int from ip_addresses
                    WHERE ip = $1"#,
        )
        .bind(ip)
        .fetch_one(db)
        .await?;

        Ok(ip_count > 0)
    }

    pub async fn delete(id: Uuid, db: &PgPool) -> ApiResult<Self> {
        sqlx::query_as::<_, Self>("delete from ip_addresses where id = $1 returning *")
            .bind(id)
            .fetch_one(db)
            .await
            .map_err(ApiError::from)
    }

    pub async fn find_by_node(node_ip: String, db: &PgPool) -> ApiResult<Self> {
        let node_ip: IpAddr = node_ip.parse()?;

        sqlx::query_as::<_, Self>("select * from ip_addresses where ip = $1 limit 1")
            .bind(node_ip)
            .fetch_one(db)
            .await
            .map_err(ApiError::from)
    }
}
