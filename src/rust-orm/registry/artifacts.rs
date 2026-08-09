use sea_orm::{
    ActiveModelTrait,
    ActiveValue::Set,
    ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter,
    prelude::{Json, Uuid},
};

use crate::entities::{org_member, package, package_download, package_upload};

use super::identity::{FederatedIdentity, actor_and_org, ensure_federated_user, require_writer};
use super::support::{
    new_uuid, now, package_in_org, validate_archive_format, validate_download_source,
    validate_nonempty, validate_sha256, validate_upload_state,
};

#[derive(Clone, Debug, PartialEq)]
pub struct PackageUploadInput {
    pub source_upload_id: Option<Uuid>,
    pub version: String,
    pub archive_format: String,
    pub state: String,
    pub storage_backend: String,
    pub storage_bucket: String,
    pub storage_key: String,
    pub original_filename: Option<String>,
    pub size_bytes: i64,
    pub sha256: String,
    pub vcs_tag: Option<String>,
    pub vcs_commit: Option<String>,
    pub metadata: Json,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageDownloadInput {
    pub upload_id: Uuid,
    pub request_id: Option<Uuid>,
    pub source: String,
    pub bytes_served: i64,
    pub completed: bool,
    pub client_fingerprint_hash: Option<Vec<u8>>,
    pub user_agent_hash: Option<Vec<u8>>,
}

pub async fn register_package_upload(
    conn: &DatabaseConnection,
    identity: &FederatedIdentity,
    org_slug: &str,
    package_name: &str,
    input: PackageUploadInput,
) -> Result<package_upload::Model, DbErr> {
    validate_nonempty("version", &input.version, 255)?;
    validate_archive_format(&input.archive_format)?;
    validate_upload_state(&input.state)?;
    validate_nonempty("storage backend", &input.storage_backend, 64)?;
    validate_nonempty("storage bucket", &input.storage_bucket, 255)?;
    validate_nonempty("storage key", &input.storage_key, 2048)?;
    validate_sha256(Some(&input.sha256))?;
    if input.size_bytes <= 0 {
        return Err(DbErr::Custom("upload size must be positive".into()));
    }

    let (actor, org_model, role) = actor_and_org(conn, identity, org_slug).await?;
    require_writer(&role)?;
    let package_model = package_in_org(conn, org_model.id, package_name).await?;
    let now = now();
    let published_at = (input.state == "published").then_some(now);
    let verified_at = matches!(input.state.as_str(), "verified" | "published").then_some(now);
    let uploaded_at = (input.state != "pending").then_some(now);

    package_upload::ActiveModel {
        id: Set(new_uuid(conn).await?),
        package_id: Set(package_model.id),
        source_upload_id: Set(input.source_upload_id),
        version: Set(input.version),
        archive_format: Set(input.archive_format),
        state: Set(input.state),
        uploader_user_id: Set(Some(actor.id)),
        storage_backend: Set(input.storage_backend),
        storage_bucket: Set(input.storage_bucket),
        storage_key: Set(input.storage_key),
        original_filename: Set(input.original_filename),
        size_bytes: Set(input.size_bytes),
        sha256: Set(input.sha256),
        vcs_tag: Set(input.vcs_tag),
        vcs_commit: Set(input.vcs_commit),
        metadata: Set(input.metadata),
        uploaded_at: Set(uploaded_at),
        verified_at: Set(verified_at),
        published_at: Set(published_at),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(conn)
    .await
}

/// Record one artifact delivery. The database trigger increments the package
/// counter only for completed downloads and locks the same row used by the
/// private-to-public transition.
pub async fn record_package_download(
    conn: &DatabaseConnection,
    identity: Option<&FederatedIdentity>,
    input: PackageDownloadInput,
) -> Result<package_download::Model, DbErr> {
    validate_download_source(&input.source)?;
    if input.bytes_served < 0 {
        return Err(DbErr::Custom("bytes served cannot be negative".into()));
    }

    let upload = package_upload::Entity::find_by_id(input.upload_id)
        .one(conn)
        .await?
        .ok_or_else(|| DbErr::Custom("package upload not found".into()))?;
    let package_model = package::Entity::find_by_id(upload.package_id)
        .one(conn)
        .await?
        .ok_or_else(|| DbErr::Custom("package not found".into()))?;

    let user_id = match identity {
        Some(identity) => Some(ensure_federated_user(conn, identity).await?.id),
        None => None,
    };

    if package_model.visibility != "public" {
        let user_id = user_id.ok_or_else(|| {
            DbErr::Custom("authentication is required for a non-public package".into())
        })?;
        let member = org_member::Entity::find()
            .filter(org_member::Column::OrgId.eq(package_model.org_id))
            .filter(org_member::Column::UserId.eq(user_id))
            .one(conn)
            .await?
            .is_some();
        if !member {
            return Err(DbErr::Custom(
                "organization membership is required for this package".into(),
            ));
        }
    }

    package_download::ActiveModel {
        id: Set(new_uuid(conn).await?),
        package_id: Set(package_model.id),
        upload_id: Set(upload.id),
        user_id: Set(user_id),
        archive_format: Set(upload.archive_format),
        source: Set(input.source),
        request_id: Set(match input.request_id {
            Some(request_id) => request_id,
            None => new_uuid(conn).await?,
        }),
        bytes_served: Set(input.bytes_served),
        completed: Set(input.completed),
        client_fingerprint_hash: Set(input.client_fingerprint_hash),
        user_agent_hash: Set(input.user_agent_hash),
        downloaded_at: Set(now()),
    }
    .insert(conn)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_input_cannot_carry_raw_network_or_session_data() {
        let input = PackageDownloadInput {
            upload_id: Uuid::nil(),
            request_id: None,
            source: "cli".into(),
            bytes_served: 0,
            completed: false,
            client_fingerprint_hash: None,
            user_agent_hash: None,
        };
        assert_eq!(input.source, "cli");
    }
}
