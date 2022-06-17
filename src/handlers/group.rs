use crate::errors::{ApiError, Result as ApiResult};
use crate::models::*;
use crate::server::DbPool;
use axum::extract::{Extension, Json, Path};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use uuid::Uuid;

async fn check_org_access(auth: Authentication, org_id: Uuid, db: &DbPool) -> ApiResult<()> {
    let user_id = auth.get_user(db.as_ref()).await?.id;
    if Org::find_org_user(&user_id, &org_id, db.as_ref())
        .await?
        .role
        == OrgRole::Member
    {
        Err(ApiError::InsufficientPermissionsError)
    } else {
        Ok(())
    }
}

pub async fn create_group(
    Extension(db): Extension<DbPool>,
    Json(group): Json<GroupRequest>,
    auth: Authentication,
) -> ApiResult<impl IntoResponse> {
    check_org_access(auth, group.org_id, &db).await?;
    let group = Group::create(&group, db.as_ref()).await?;
    Ok((StatusCode::OK, Json(group)))
}

pub async fn add_to_group(
    Extension(db): Extension<DbPool>,
    Json(req): Json<GroupAddRequest>,
    auth: Authentication,
) -> ApiResult<impl IntoResponse> {
    let group = Group::find_by_id(req.group_id, db.as_ref()).await?;
    check_org_access(auth, group.org_id, &db).await?;
    let group = Group::add(&req, db.as_ref()).await?;
    Ok((StatusCode::OK, Json(group)))
}

pub async fn get_group(
    Extension(db): Extension<DbPool>,
    Path(id): Path<Uuid>,
    auth: Authentication,
) -> ApiResult<impl IntoResponse> {
    let group = Group::find_by_id(id, db.as_ref()).await?;
    check_org_access(auth, group.org_id, &db).await?;
    Ok((StatusCode::OK, Json(group)))
}

pub async fn get_group_items(
    Extension(db): Extension<DbPool>,
    Path(id): Path<Uuid>,
    auth: Authentication,
) -> ApiResult<impl IntoResponse> {
    let group = Group::find_by_id(id, db.as_ref()).await?;
    check_org_access(auth, group.org_id, &db).await?;
    let items = Group::get_members(id, db.as_ref()).await?;
    Ok((StatusCode::OK, Json(items)))
}
