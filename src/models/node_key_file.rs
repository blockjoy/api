use crate::auth::FindableById;
use crate::errors::{ApiError, Result as ApiResult};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct CreateNodeKeyFileRequest {
    pub name: String,
    pub content: String,
    pub node_id: Uuid,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, FromRow)]
pub struct NodeKeyFile {
    pub id: Uuid,
    pub name: String,
    pub content: String,
    pub node_id: Uuid,
}

impl NodeKeyFile {
    pub async fn create(
        req: CreateNodeKeyFileRequest,
        db: &mut sqlx::PgConnection,
    ) -> ApiResult<Self> {
        sqlx::query_as(
            r#"
            INSERT INTO node_key_files 
                (name, content, node_id)
            VALUES
                ($1, $2, $3)
            RETURNING *
        "#,
        )
        .bind(req.name)
        .bind(req.content)
        .bind(req.node_id)
        .fetch_one(db)
        .await
        .map_err(ApiError::from)
    }

    pub async fn find_by_node(node_id: Uuid, db: &mut sqlx::PgConnection) -> ApiResult<Vec<Self>> {
        sqlx::query_as(
            r#"
            SELECT * FROM node_key_files WHERE node_id = $1
        "#,
        )
        .bind(node_id)
        .fetch_all(db)
        .await
        .map_err(ApiError::from)
    }

    pub async fn delete(node_id: Uuid, db: &mut sqlx::PgConnection) -> ApiResult<Self> {
        sqlx::query_as(
            r#"
            DELETE FROM node_key_files 
            WHERE node_id = $1
            RETURNING *
        "#,
        )
        .bind(node_id)
        .fetch_one(db)
        .await
        .map_err(ApiError::from)
    }
}

#[tonic::async_trait]
impl FindableById for NodeKeyFile {
    async fn find_by_id(id: Uuid, db: &mut sqlx::PgConnection) -> ApiResult<Self> {
        sqlx::query_as(
            r#"
            SELECT * FROM node_key_files WHERE id = $1
        "#,
        )
        .bind(id)
        .fetch_one(db)
        .await
        .map_err(ApiError::from)
    }
}
