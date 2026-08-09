use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "package_downloads")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub package_id: Uuid,
    pub upload_id: Uuid,
    pub user_id: Option<Uuid>,
    pub archive_format: String,
    pub source: String,
    pub request_id: Uuid,
    pub bytes_served: i64,
    pub completed: bool,
    pub client_fingerprint_hash: Option<Vec<u8>>,
    pub user_agent_hash: Option<Vec<u8>>,
    pub downloaded_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
