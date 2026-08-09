use std::time::Duration;

use sea_orm::{
    ActiveModelTrait,
    ActiveValue::Set,
    ColumnTrait, ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr, EntityTrait,
    QueryFilter, Statement, TransactionTrait,
    prelude::{Json, Uuid},
};

use crate::entities::{package, package_license, project};
use crate::policy::validate_private_to_public;

use super::identity::{FederatedIdentity, actor_and_org, require_admin, require_writer};
use super::support::{
    new_uuid, now, package_in_org, validate_archive_format, validate_nonempty,
    validate_package_name, validate_sha256,
};

#[derive(Clone, Debug, PartialEq)]
pub struct CreatePackageInput {
    pub project_id: Option<Uuid>,
    pub name: String,
    pub description: Option<String>,
    pub vcs: String,
    pub repo_url: String,
    pub config: Json,
    pub default_archive_format: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PackageLicenseInput {
    pub spdx_expression: Option<String>,
    pub license_name: Option<String>,
    pub license_url: Option<String>,
    pub license_text: Option<String>,
    pub checksum_sha256: Option<String>,
    pub is_primary: bool,
}

/// Create a private package. Public visibility is available only through
/// `make_package_public`, which owns the row lock and policy transition.
pub async fn create_package(
    conn: &DatabaseConnection,
    identity: &FederatedIdentity,
    org_slug: &str,
    input: CreatePackageInput,
) -> Result<package::Model, DbErr> {
    validate_package_name(&input.name)?;
    validate_archive_format(&input.default_archive_format)?;
    validate_nonempty("vcs", &input.vcs, 32)?;
    validate_nonempty("repository URL", &input.repo_url, 2048)?;

    let (actor, org_model, role) = actor_and_org(conn, identity, org_slug).await?;
    require_writer(&role)?;

    if let Some(project_id) = input.project_id {
        let project_in_org = project::Entity::find_by_id(project_id)
            .filter(project::Column::OrgId.eq(org_model.id))
            .one(conn)
            .await?
            .is_some();
        if !project_in_org {
            return Err(DbErr::Custom(
                "package project must belong to the same organization".into(),
            ));
        }
    }

    let now = now();
    package::ActiveModel {
        id: Set(new_uuid(conn).await?),
        org_id: Set(org_model.id),
        project_id: Set(input.project_id),
        name: Set(input.name),
        description: Set(input.description),
        vcs: Set(input.vcs),
        repo_url: Set(input.repo_url),
        visibility: Set("private".into()),
        config: Set(input.config),
        created_by_user_id: Set(Some(actor.id)),
        default_archive_format: Set(input.default_archive_format),
        download_count: Set(0),
        upload_count: Set(0),
        first_public_at: Set(None),
        visibility_changed_at: Set(now),
        visibility_changed_by_user_id: Set(Some(actor.id)),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(conn)
    .await
}

/// Convert a private package to public while holding the package row lock.
pub async fn make_package_public(
    conn: &DatabaseConnection,
    identity: &FederatedIdentity,
    org_slug: &str,
    package_name: &str,
) -> Result<package::Model, DbErr> {
    let (actor, org_model, role) = actor_and_org(conn, identity, org_slug).await?;
    require_admin(&role)?;

    let txn = conn.begin().await?;
    let row = txn
        .query_one(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "SELECT id, visibility, download_count, \
                    GREATEST(0, floor(EXTRACT(EPOCH FROM (clock_timestamp() - created_at)) * 1000000))::BIGINT AS age_micros \
             FROM package \
             WHERE org_id = $1 AND name = $2 \
             FOR UPDATE",
            [org_model.id.into(), package_name.to_owned().into()],
        ))
        .await?
        .ok_or_else(|| DbErr::Custom("package not found".into()))?;

    let package_id: Uuid = row.try_get("", "id")?;
    let visibility: String = row.try_get("", "visibility")?;
    let completed_downloads: i64 = row.try_get("", "download_count")?;
    let age_micros: i64 = row.try_get("", "age_micros")?;

    if visibility == "public" {
        txn.commit().await?;
        return package::Entity::find_by_id(package_id)
            .one(conn)
            .await?
            .ok_or_else(|| DbErr::Custom("package disappeared after visibility check".into()));
    }
    if visibility != "private" {
        return Err(DbErr::Custom(
            "only private packages can be converted to public".into(),
        ));
    }

    let age = Duration::from_micros(
        u64::try_from(age_micros)
            .map_err(|_| DbErr::Custom("database returned a negative package age".into()))?,
    );
    validate_private_to_public(age, completed_downloads)
        .map_err(|error| DbErr::Custom(error.to_string()))?;

    txn.execute(Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "UPDATE package \
         SET visibility = 'public', \
             first_public_at = coalesce(first_public_at, clock_timestamp()), \
             visibility_changed_at = clock_timestamp(), \
             visibility_changed_by_user_id = $2, \
             updated_at = clock_timestamp() \
         WHERE id = $1",
        [package_id.into(), actor.id.into()],
    ))
    .await?;
    txn.commit().await?;

    package::Entity::find_by_id(package_id)
        .one(conn)
        .await?
        .ok_or_else(|| DbErr::Custom("package disappeared after visibility update".into()))
}

pub async fn add_package_license(
    conn: &DatabaseConnection,
    identity: &FederatedIdentity,
    org_slug: &str,
    package_name: &str,
    input: PackageLicenseInput,
) -> Result<package_license::Model, DbErr> {
    if input
        .spdx_expression
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
        && input
            .license_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
    {
        return Err(DbErr::Custom(
            "a SPDX expression or license name is required".into(),
        ));
    }
    validate_sha256(input.checksum_sha256.as_deref())?;

    let (actor, org_model, role) = actor_and_org(conn, identity, org_slug).await?;
    require_writer(&role)?;
    let package_model = package_in_org(conn, org_model.id, package_name).await?;
    let txn = conn.begin().await?;

    if input.is_primary {
        txn.execute(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            "UPDATE package_licenses \
             SET is_primary = false, updated_at = clock_timestamp() \
             WHERE package_id = $1 AND is_primary",
            [package_model.id.into()],
        ))
        .await?;
    }

    let now = now();
    let model = package_license::ActiveModel {
        id: Set(new_uuid(&txn).await?),
        package_id: Set(package_model.id),
        spdx_expression: Set(input.spdx_expression),
        license_name: Set(input.license_name),
        license_url: Set(input.license_url),
        license_text: Set(input.license_text),
        checksum_sha256: Set(input.checksum_sha256),
        is_primary: Set(input.is_primary),
        created_by_user_id: Set(Some(actor.id)),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&txn)
    .await?;
    txn.commit().await?;
    Ok(model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_create_input_has_no_public_visibility_escape_hatch() {
        let input = CreatePackageInput {
            project_id: None,
            name: "http-kit".into(),
            description: None,
            vcs: "git".into(),
            repo_url: "https://example.test/http-kit".into(),
            config: Json::Object(Default::default()),
            default_archive_format: "tar_gz".into(),
        };
        assert_eq!(input.name, "http-kit");
    }
}
