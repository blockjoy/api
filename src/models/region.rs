use super::schema::regions;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;

#[derive(Debug, Clone, Queryable)]
pub struct Region {
    pub id: uuid::Uuid,
    pub name: String,
}

impl Region {
    pub async fn by_ids(
        mut region_ids: Vec<uuid::Uuid>,
        conn: &mut super::Conn,
    ) -> crate::Result<Vec<Self>> {
        region_ids.sort();
        region_ids.dedup();
        let ip = regions::table
            .filter(regions::id.eq_any(region_ids))
            .get_results(conn)
            .await?;
        Ok(ip)
    }

    pub async fn by_id(region_id: uuid::Uuid, conn: &mut super::Conn) -> crate::Result<Self> {
        let ip = regions::table
            .filter(regions::id.eq(region_id))
            .get_result(conn)
            .await?;
        Ok(ip)
    }

    pub async fn get_or_create(name: &str, conn: &mut super::Conn) -> crate::Result<Self> {
        let region = diesel::insert_into(regions::table)
            .values(regions::name.eq(name.to_lowercase()))
            .on_conflict(regions::name)
            .do_nothing()
            .get_result(conn)
            .await?;
        Ok(region)
    }
}
