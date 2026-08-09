use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "package_licenses")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub package_id: Uuid,
    pub spdx_expression: Option<String>,
    pub license_name: Option<String>,
    pub license_url: Option<String>,
    pub license_text: Option<String>,
    pub checksum_sha256: Option<String>,
    pub is_primary: bool,
    pub created_by_user_id: Option<Uuid>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
