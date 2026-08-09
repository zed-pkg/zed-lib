use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "package_uploads")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub package_id: Uuid,
    pub source_upload_id: Option<Uuid>,
    pub version: String,
    pub archive_format: String,
    pub state: String,
    pub uploader_user_id: Option<Uuid>,
    pub storage_backend: String,
    pub storage_bucket: String,
    pub storage_key: String,
    pub original_filename: Option<String>,
    pub size_bytes: i64,
    pub sha256: String,
    pub vcs_tag: Option<String>,
    pub vcs_commit: Option<String>,
    #[sea_orm(column_type = "JsonBinary")]
    pub metadata: Json,
    pub uploaded_at: Option<DateTimeWithTimeZone>,
    pub verified_at: Option<DateTimeWithTimeZone>,
    pub published_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
