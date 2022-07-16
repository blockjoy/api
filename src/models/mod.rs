use crate::errors::{ApiError, Result};
use angry_purple_tiger::AnimalName;
use chrono::{DateTime, Utc};
use log::{debug, error};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use sqlx::{FromRow, PgPool};
use std::convert::From;
use std::sync::Arc;
use uuid::Uuid;

mod blockchain;
pub use blockchain::*;
mod broadcast;
pub use broadcast::*;
mod host;
pub use host::*;
mod node;
pub use node::*;
mod org;
pub use org::*;
mod user;
pub use user::*;

pub const STAKE_QUOTA_DEFAULT: i64 = 5;
pub const FEE_BPS_DEFAULT: i64 = 300;

pub type DbPool = Arc<PgPool>;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "enum_validator_status", rename_all = "snake_case")]
pub enum ValidatorStatus {
    Provisioning,
    Syncing,
    Upgrading,
    Migrating,
    Synced,
    Consensus,
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "enum_stake_status", rename_all = "snake_case")]
pub enum StakeStatus {
    Available,
    Staking,
    Staked,
    Delinquent,
    Disabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "enum_host_cmd", rename_all = "snake_case")]
pub enum HostCmd {
    CreateNode,
    RestartNode,
    KillNode,
    ShutdownNode,
    DeleteNode,
    UpdateNode,
    MigrateNode,
    GetNodeVersion,
    GetBVSVersion,
    UpdateBVS,
    RestartBVS,
    RemoveBVS,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Command {
    pub id: Uuid,
    pub host_id: Uuid,
    pub cmd: HostCmd,
    pub sub_cmd: Option<String>,
    pub response: Option<String>,
    pub exit_status: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl Command {
    pub async fn find_by_id(id: Uuid, db: &PgPool) -> Result<Self> {
        sqlx::query_as::<_, Self>("SELECT * FROM commands where id = $1")
            .bind(id)
            .fetch_one(db)
            .await
            .map_err(ApiError::from)
    }

    pub async fn find_all_by_host(host_id: Uuid, db: &PgPool) -> Result<Vec<Command>> {
        sqlx::query_as::<_, Self>(
            "SELECT * FROM commands where host_id = $1 ORDER BY created_at DESC",
        )
        .bind(host_id)
        .fetch_all(db)
        .await
        .map_err(ApiError::from)
    }

    pub async fn find_pending_by_host(host_id: Uuid, db: &PgPool) -> Result<Vec<Command>> {
        sqlx::query_as::<_, Self>("SELECT * FROM commands where host_id = $1 AND completed_at IS NULL ORDER BY created_at DESC")
        .bind(host_id)
        .fetch_all(db)
            .await.map_err(ApiError::from)
    }

    pub async fn create(host_id: Uuid, command: CommandRequest, db: &PgPool) -> Result<Command> {
        sqlx::query_as::<_, Self>(
            "INSERT INTO commands (host_id, cmd, sub_cmd) VALUES ($1, $2, $3) RETURNING *",
        )
        .bind(host_id)
        .bind(command.cmd)
        .bind(command.sub_cmd)
        .fetch_one(db)
        .await
        .map_err(ApiError::from)
    }

    pub async fn update_response(
        id: Uuid,
        response: CommandResponseRequest,
        db: &PgPool,
    ) -> Result<Command> {
        sqlx::query_as::<_, Self>("UPDATE commands SET response = $1, exit_status = $2, completed_at = now() WHERE id = $3 RETURNING *")
        .bind(response.response)
        .bind(response.exit_status)
        .bind(id)
        .fetch_one(db)
        .await.map_err(ApiError::from)
    }

    pub async fn delete(id: Uuid, db: &PgPool) -> Result<u64> {
        let mut tx = db.begin().await?;
        let deleted = sqlx::query("DELETE FROM commands WHERE id = $1")
            .bind(id)
            .execute(&mut tx)
            .await?;

        tx.commit().await?;
        Ok(deleted.rows_affected())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandRequest {
    pub cmd: HostCmd,
    pub sub_cmd: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResponseRequest {
    pub response: Option<String>,
    pub exit_status: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Validator {
    pub id: Uuid,
    pub name: String,
    pub version: Option<String>,
    pub ip_addr: String,
    pub host_id: Uuid,
    pub user_id: Option<Uuid>,
    pub owner_address: Option<String>,
    pub address: Option<String>,
    pub address_name: Option<String>,
    pub swarm_key: Option<String>,
    pub block_height: Option<i64>,
    pub stake_status: StakeStatus,
    pub staking_height: Option<i64>,
    pub status: ValidatorStatus,
    pub tenure_penalty: f64,
    pub dkg_penalty: f64,
    pub performance_penalty: f64,
    pub total_penalty: f64,
    pub transferred_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Validator {
    pub async fn find_all(db: &PgPool) -> Result<Vec<Self>> {
        sqlx::query_as::<_, Self>("SELECT * FROM validators")
            .fetch_all(db)
            .await
            .map_err(ApiError::from)
    }

    pub async fn find_all_by_host(host_id: Uuid, db: &PgPool) -> Result<Vec<Self>> {
        sqlx::query_as::<_, Self>(
            "SELECT * FROM validators WHERE host_id = $1 order by status DESC, stake_status, name",
        )
        .bind(host_id)
        .fetch_all(db)
        .await
        .map_err(ApiError::from)
    }

    pub async fn find_all_by_user(user_id: Uuid, db: &PgPool) -> Result<Vec<Self>> {
        sqlx::query_as::<_, Self>(
            "SELECT * FROM validators WHERE user_id = $1 order by status DESC, stake_status, name",
        )
        .bind(user_id)
        .fetch_all(db)
        .await
        .map_err(ApiError::from)
    }

    pub async fn find_by_id(id: Uuid, db: &PgPool) -> Result<Self> {
        sqlx::query_as::<_, Self>("SELECT * FROM validators WHERE id = $1")
            .bind(id)
            .fetch_one(db)
            .await
            .map_err(ApiError::from)
    }

    pub async fn find_all_by_stake_status(
        stake_status: StakeStatus,
        db: &PgPool,
    ) -> Result<Vec<Self>> {
        sqlx::query_as::<_, Self>(
            "SELECT * FROM validators WHERE stake_status = $1 order by status DESC, stake_status, name",
        )
        .bind(stake_status)
        .fetch_all(db)
        .await
        .map_err(ApiError::from)
    }

    pub async fn find_all_by_status(
        status: ValidatorStatus,
        db: &PgPool,
    ) -> Result<Vec<Validator>> {
        sqlx::query_as::<_, Self>(
            "SELECT * FROM validators where status = $1 order by status DESC, stake_status, name",
        )
        .bind(status)
        .fetch_all(db)
        .await
        .map_err(ApiError::from)
    }

    pub async fn list_staking_export(user_id: &Uuid, db: &PgPool) -> Result<Vec<ValidatorStaking>> {
        sqlx::query_as::<_, ValidatorStaking>(
            "SELECT address, 10000::BIGINT as stake FROM validators where user_id=$1 and stake_status=$2",
        )
        .bind(user_id)
        .bind(StakeStatus::Staking)
        .fetch_all(db)
        .await
        .map_err(ApiError::from)
    }

    pub async fn create_tx(validator: ValidatorRequest, tx: &mut PgConnection) -> Result<Self> {
        let validator = sqlx::query_as::<_, Self>("INSERT INTO validators (name, version, ip_addr, host_id, user_id, address, swarm_key, block_height, stake_status, status, tenure_penalty, dkg_penalty, performance_penalty, total_penalty) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14) RETURNING *")
        .bind(validator.name)
        .bind(validator.version)
        .bind(validator.ip_addr)
        .bind(validator.host_id)
        .bind(validator.user_id)
        .bind(validator.address)
        .bind(validator.swarm_key)
        .bind(validator.block_height)
        .bind(validator.stake_status)
        .bind(validator.status)
        .bind(validator.tenure_penalty)
        .bind(validator.dkg_penalty)
        .bind(validator.performance_penalty)
        .bind(validator.total_penalty)
        .fetch_one(tx)
        .await?;

        Ok(validator)
    }

    pub async fn update_status(
        id: Uuid,
        validator: ValidatorStatusRequest,
        db: &PgPool,
    ) -> Result<Self> {
        let mut tx = db.begin().await.unwrap();
        let validator = sqlx::query_as::<_, Self>(
            r#"UPDATE validators SET version=$1, block_height=$2, status=$3, updated_at=now()  WHERE id = $4 RETURNING *"#
        )
        .bind(validator.version)
        .bind(validator.block_height)
        .bind(validator.status)
        .bind(id)
        .fetch_one(&mut tx)
        .await?;

        tx.commit().await.unwrap();
        Ok(validator)
    }

    pub async fn update_stake_status(id: Uuid, status: StakeStatus, db: &PgPool) -> Result<Self> {
        let query = match status {
            StakeStatus::Available => {
                r#"UPDATE validators SET stake_status=$1, owner_address=NULL, user_id=NULL, staking_height=NULL, updated_at=now()  WHERE id = $2 RETURNING *"#
            }
            StakeStatus::Staking => {
                r#"UPDATE validators SET stake_status=$1, staking_height=(SELECT block_height FROM info ORDER BY block_height DESC LIMIT 1), updated_at=now() WHERE id = $2 RETURNING *"#
            }
            _ => {
                r#"UPDATE validators SET stake_status=$1, staking_height=NULL, updated_at=now()  WHERE id = $2 RETURNING *"#
            }
        };

        Ok(sqlx::query_as::<_, Self>(query)
            .bind(status)
            .bind(id)
            .fetch_one(db)
            .await?)
    }

    pub async fn update_owner_address(
        id: Uuid,
        owner_address: Option<String>,
        db: &PgPool,
    ) -> Result<Self> {
        let mut tx = db.begin().await.unwrap();
        let validator = sqlx::query_as::<_, Self>(
            r#"UPDATE validators SET owner_address=$1, updated_at=now()  WHERE id = $2 RETURNING *"#,
        )
        .bind(owner_address)
        .bind(id)
        .fetch_one(&mut tx)
        .await?;

        tx.commit().await.unwrap();
        Ok(validator)
    }

    pub async fn update_penalty(
        id: Uuid,
        penalty: ValidatorPenaltyRequest,
        db: &PgPool,
    ) -> Result<Validator> {
        Ok(sqlx::query_as::<_, Self>("UPDATE validators SET tenure_penalty=$1, dkg_penalty=$2, performance_penalty=$3, total_penalty=$4 where id = $5 RETURNING *")
        .bind(penalty.tenure_penalty)
        .bind(penalty.dkg_penalty)
        .bind(penalty.performance_penalty)
        .bind(penalty.total_penalty)
        .bind(id)
        .fetch_one(db)
        .await?)
    }

    pub async fn update_identity(
        id: Uuid,
        validator: ValidatorIdentityRequest,
        db: &PgPool,
    ) -> Result<Self> {
        let mut address_name = None;
        if let Some(val_addr) = &validator.address {
            address_name = match val_addr.parse::<AnimalName>() {
                Ok(name) => Some(name.to_string()),
                Err(_) => None,
            }
        };

        let mut tx = db.begin().await.unwrap();
        let validator = sqlx::query_as::<_, Self>(
            r#"UPDATE validators SET version=$1, address=$2, swarm_key=$3, address_name=$4, updated_at=now() WHERE id = $5 RETURNING *"#
        )
        .bind(validator.version)
        .bind(validator.address)
        .bind(validator.swarm_key)
        .bind(address_name)
        .bind(id)
        .fetch_one(&mut tx)
        .await?;

        tx.commit().await.unwrap();
        Ok(validator)
    }

    pub async fn migrate(db: &PgPool, id: Uuid) -> Result<Validator> {
        let mut tx = db.begin().await?;
        let val = sqlx::query_as::<_, Self>("SELECT * FROM validators where id = $1")
            .bind(id)
            .fetch_one(&mut tx)
            .await?;

        //TODO: This could just select id
        let new_val = sqlx::query_as::<_, Self>("SELECT * FROM validators WHERE (status = 'synced' OR status = 'syncing') AND stake_status = 'available' and host_id <> $1 ORDER BY random() LIMIT 1")
        .bind(val.host_id)
        .fetch_one(&mut tx)
        .await?;

        let _ = sqlx::query("UPDATE validators SET address = NULL, address_name = NULL, owner_address = NULL, user_id = NULL, swarm_key = NULL, status='stopped', stake_status = 'disabled' WHERE id = $1")
         .bind(val.id)
         .execute(&mut tx)
         .await?;

        let new_val = sqlx::query_as::<_, Self>("UPDATE validators SET address=$1, address_name=$2, owner_address=$3, user_id=$4, swarm_key=$5,status='migrating', stake_status=$6, staking_height=$7 where id=$8 RETURNING *")
         .bind(val.address)
         .bind(val.address_name)
         .bind(val.owner_address)
         .bind(val.user_id)
         .bind(val.swarm_key)
         .bind(val.stake_status)
         .bind(val.staking_height)
         .bind(new_val.id)
         .fetch_one(&mut tx)
         .await?;

        tx.commit().await?;

        Ok(new_val)
    }

    pub async fn inventory_count(db: &PgPool) -> Result<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) AS available FROM validators where stake_status = 'available' and (status = 'synced' OR status = 'syncing')",
        )
        .fetch_one(db)
        .await?;

        Ok(row.0)
    }

    pub async fn stake(db: &PgPool, user: &User, count: i64) -> Result<Vec<Validator>> {
        if user.can_stake(db, count).await? {
            let mut tx = db.begin().await?;
            let res = sqlx::query_as::<_, Self>(
                r#"
            WITH inv AS (
                SELECT id FROM validators
                WHERE (status = 'synced' OR status = 'syncing') AND stake_status = 'available'
                ORDER BY random()
                LIMIT $1
            ) 
            UPDATE validators SET 
                user_id = $2, 
                stake_status = $3,
                staking_height = (SELECT block_height FROM info LIMIT 1)
            FROM inv
            WHERE validators.id = inv.id
            RETURNING *;
            "#,
            )
            .bind(count)
            .bind(user.id)
            .bind(StakeStatus::Staking)
            .fetch_all(&mut tx)
            .await?;

            tx.commit().await?;
            return Ok(res);
        }

        Err(ApiError::ValidationError(
            "User's staking quota over limit.".to_string(),
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ValidatorDetail {
    pub id: Uuid,
    pub name: String,
    pub version: Option<String>,
    pub host_name: String,
    pub host_id: Uuid,
    pub user_id: Option<Uuid>,
    pub user_email: Option<String>,
    pub address: Option<String>,
    pub address_name: Option<String>,
    pub block_height: Option<i64>,
    pub stake_status: StakeStatus,
    pub status: ValidatorStatus,
    pub tenure_penalty: f64,
    pub dkg_penalty: f64,
    pub performance_penalty: f64,
    pub total_penalty: f64,
    pub staking_height: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ValidatorDetail {
    pub async fn list_needs_attention(db: &PgPool) -> Result<Vec<ValidatorDetail>> {
        sqlx::query_as::<_, ValidatorDetail> ("SELECT hosts.name as host_name, users.email as user_email, validators.* FROM validators inner join hosts on hosts.id = validators.host_id left join users on users.id = validators.user_id where (validators.status <> 'synced' OR validators.stake_status = 'staking' OR validators.status = 'migrating' OR validators.status = 'upgrading') order by status DESC, stake_status, name")
        .fetch_all(db)
        .await
        .map_err(ApiError::from)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorRequest {
    pub name: String,
    pub version: Option<String>,
    pub ip_addr: String,
    pub host_id: Uuid,
    pub user_id: Option<Uuid>,
    pub address: Option<String>,
    pub swarm_key: Option<String>,
    pub block_height: Option<i64>,
    pub stake_status: StakeStatus,
    pub status: ValidatorStatus,
    pub tenure_penalty: f64,
    pub dkg_penalty: f64,
    pub performance_penalty: f64,
    pub total_penalty: f64,
}

impl ValidatorRequest {
    pub fn new(host_id: Uuid, ip_addr: &str) -> Self {
        Self {
            name: petname::petname(2, "_"),
            version: None,
            ip_addr: ip_addr.to_owned(),
            host_id,
            user_id: None,
            address: None,
            swarm_key: None,
            block_height: None,
            stake_status: StakeStatus::Available,
            status: ValidatorStatus::Provisioning,
            tenure_penalty: 0.0,
            dkg_penalty: 0.0,
            performance_penalty: 0.0,
            total_penalty: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorStatusRequest {
    pub version: Option<String>,
    pub block_height: Option<i64>,
    pub status: ValidatorStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorPenaltyRequest {
    pub tenure_penalty: f64,
    pub dkg_penalty: f64,
    pub performance_penalty: f64,
    pub total_penalty: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorIdentityRequest {
    pub version: Option<String>,
    pub address: Option<String>,
    pub swarm_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorStakeRequest {
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ValidatorStaking {
    pub address: String,
    pub stake: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reward {
    pub id: Uuid,
    pub block: i64,
    pub hash: String,
    pub txn_time: DateTime<Utc>,
    pub validator_id: Uuid,
    pub user_id: Option<Uuid>,
    pub account: String,
    pub validator: String,
    pub amount: i64,
    pub created_at: DateTime<Utc>,
}

impl Reward {
    pub async fn summary_by_user(db: &PgPool, user_id: &Uuid) -> Result<RewardSummary> {
        let row: RewardSummary = sqlx::query_as(
            r##"SELECT 
                        COALESCE(SUM(amount) FILTER (WHERE txn_time BETWEEN now() - '30 day'::interval AND now()), 0)::BIGINT as last_30,
                        COALESCE(SUM(amount) FILTER (WHERE txn_time BETWEEN now() - '14 day'::interval AND now()), 0)::BIGINT as last_14,
                        COALESCE(SUM(amount) FILTER (WHERE txn_time BETWEEN now() - '7 day'::interval AND now()), 0)::BIGINT as last_7,
                        COALESCE(SUM(amount) FILTER (WHERE txn_time BETWEEN now() - '1 day'::interval AND now()), 0)::BIGINT as last_1,
                        COALESCE(SUM(amount), 0)::BIGINT as total
                    FROM rewards 
                    WHERE user_id=$1"##
            )
            .bind(user_id)
            .fetch_one(db)
            .await?;

        Ok(row)
    }

    pub async fn create(db: &PgPool, rewards: &[RewardRequest]) -> Result<()> {
        for reward in rewards {
            if reward.amount < 1 {
                error!("Reward has zero amount. {:?}", reward);
            }
            let res = sqlx::query("INSERT INTO rewards (block, hash, txn_time, validator_id, user_id, account, validator, amount) values ($1,$2,$3,$4,$5,$6,$7,$8)")
                .bind(&reward.block)
                .bind(&reward.hash)
                .bind(&reward.txn_time)
                .bind(&reward.validator_id)
                .bind(&reward.user_id)
                .bind(&reward.account)
                .bind(&reward.validator)
                .bind(&reward.amount)
                .execute(db)
                .await;

            if let Err(e) = res {
                debug!("Creating rewards (duplicate violations expected): {}", e);
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardRequest {
    pub block: i64,
    pub hash: String,
    pub txn_time: DateTime<Utc>,
    pub validator_id: Uuid,
    pub user_id: Option<Uuid>,
    pub account: String,
    pub validator: String,
    pub amount: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RewardSummary {
    pub total: i64,
    pub last_30: i64,
    pub last_14: i64,
    pub last_7: i64,
    pub last_1: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct InfoRequest {
    pub block_height: i64,
    /// Divide by 100000000 to get USD value
    pub oracle_price: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Info {
    pub block_height: i64,
    pub staked_count: i64,
    /// Divide by 100000000 to get USD value
    pub oracle_price: i64,
    pub total_rewards: i64,
}

impl Info {
    pub async fn update_info(db: &PgPool, info: &InfoRequest) -> Result<Info> {
        sqlx::query_as::<_, Info>(
            "UPDATE info SET block_height = $1, oracle_price = $2, total_rewards = COALESCE((SELECT SUM(amount) FROM rewards), 0), staked_count = (SELECT count(*) FROM validators where stake_status = 'staked') WHERE block_height <> $1 RETURNING *",
        )
        .bind(info.block_height)
        .bind(info.oracle_price)
        .fetch_one(db)
        .await
        .map_err(ApiError::from)
    }

    pub async fn get_info(db: &PgPool) -> Result<Info> {
        sqlx::query_as::<_, Info>("SELECT * FROM info LIMIT 1")
            .fetch_one(db)
            .await
            .map_err(ApiError::from)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Invoice {
    pub id: i32,
    pub user_id: Uuid,
    pub pay_address: String,
    pub earnings: i64,
    pub fee_bps: i64,
    pub amount: i64,
    pub validators_count: i64,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub is_paid: bool,
}

impl Invoice {
    pub async fn find_all_by_user(db: &PgPool, user_id: &Uuid) -> Result<Vec<Invoice>> {
        sqlx::query_as::<_, Invoice>(
            r##"SELECT
                        invoices.*,
                        users.pay_address
                    FROM
                        invoices
                    INNER JOIN
                        users on users.id = invoices.user_id
                    WHERE
                        user_id = $1 
                    ORDER BY 
                        ends_at DESC
                    "##,
        )
        .bind(user_id)
        .fetch_all(db)
        .await
        .map_err(ApiError::from)
    }

    /// Gets all wallets addresses with a due amount.
    pub async fn find_all_payments_due(db: &PgPool) -> Result<Vec<PaymentDue>> {
        sqlx::query_as::<_, PaymentDue>("SELECT users.pay_address, sum(amount), min(ends_at) FROM invoices INNER JOIN users on users.id = invoices.user_id WHERE is_paid = false GROUP BY address")
        .fetch_all(db)
        .await
        .map_err(ApiError::from)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PaymentDue {
    pub pay_address: String,
    pub amount: i64,
    pub due_date: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Payment {
    pub hash: String,
    pub user_id: Uuid,
    pub block: i64,
    pub payer: String,
    pub payee: String,
    pub amount: i64,
    pub oracle_price: i64,
    pub created_at: Option<DateTime<Utc>>,
}

impl Payment {
    pub async fn create(db: &PgPool, payments: &[Payment]) -> Result<()> {
        for payment in payments {
            let res = sqlx::query(
                r##"
                INSERT INTO payments (
                    hash,
                    user_id,
                    block, 
                    payer,
                    payee,
                    amount,
                    oracle_price
                ) values ($1,$2,$3,$4,$5,$6,$7)"##,
            )
            .bind(&payment.block)
            .bind(&payment.user_id)
            .bind(&payment.hash)
            .bind(&payment.payer)
            .bind(&payment.payee)
            .bind(&payment.amount)
            .bind(&payment.oracle_price)
            .execute(db)
            .await;

            if let Err(e) = res {
                debug!("Creating payments (duplicate violations expected): {}", e);
            }
        }

        Ok(())
    }

    pub async fn find_all_by_user(db: &PgPool, user_id: Uuid) -> Result<Vec<Payment>> {
        Ok(sqlx::query_as::<_, Payment>(
            "SELECT * FROM payments WHERE user_id = $1 ORDER BY created_at DESC",
        )
        .bind(user_id)
        .fetch_all(db)
        .await?)
    }
}
