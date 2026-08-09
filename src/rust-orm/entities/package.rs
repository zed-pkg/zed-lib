use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "package")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub org_id: Uuid,
    pub project_id: Option<Uuid>,
    pub name: String,
    pub description: Option<String>,
    pub vcs: String,
    pub repo_url: String,
    pub visibility: String,
    #[sea_orm(column_type = "JsonBinary")]
    pub config: Json,
    pub created_by_user_id: Option<Uuid>,
    pub default_archive_format: String,
    pub download_count: i64,
    pub upload_count: i64,
    pub first_public_at: Option<DateTimeWithTimeZone>,
    pub visibility_changed_at: DateTimeWithTimeZone,
    pub visibility_changed_by_user_id: Option<Uuid>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
