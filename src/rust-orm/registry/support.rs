use std::time::SystemTime;

use sea_orm::{
    ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter, Statement,
    prelude::{DateTimeUtc, DateTimeWithTimeZone, Uuid},
};

use crate::entities::package;

pub(crate) async fn package_in_org<C>(
    conn: &C,
    org_id: Uuid,
    package_name: &str,
) -> Result<package::Model, DbErr>
where
    C: ConnectionTrait,
{
    package::Entity::find()
        .filter(package::Column::OrgId.eq(org_id))
        .filter(package::Column::Name.eq(package_name))
        .one(conn)
        .await?
        .ok_or_else(|| DbErr::Custom("package not found".into()))
}

pub(crate) async fn new_uuid<C>(conn: &C) -> Result<Uuid, DbErr>
where
    C: ConnectionTrait,
{
    let row = conn
        .query_one(Statement::from_string(
            conn.get_database_backend(),
            "SELECT gen_random_uuid() AS id",
        ))
        .await?
        .ok_or_else(|| DbErr::Custom("Postgres returned no UUID".into()))?;
    row.try_get("", "id")
}

pub(crate) fn now() -> DateTimeWithTimeZone {
    let utc: DateTimeUtc = SystemTime::now().into();
    utc.fixed_offset()
}

pub(crate) fn validate_package_name(name: &str) -> Result<(), DbErr> {
    if !name.is_empty()
        && name.len() <= 128
        && name.trim() == name
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        Ok(())
    } else {
        Err(DbErr::Custom("invalid package name".into()))
    }
}

pub(crate) fn validate_nonempty(label: &str, value: &str, max_len: usize) -> Result<(), DbErr> {
    if !value.trim().is_empty() && value.len() <= max_len {
        Ok(())
    } else {
        Err(DbErr::Custom(format!("invalid {label}")))
    }
}

pub(crate) fn validate_archive_format(format: &str) -> Result<(), DbErr> {
    if matches!(format, "zip" | "tar" | "tar_gz" | "tar_zst") {
        Ok(())
    } else {
        Err(DbErr::Custom("invalid archive format".into()))
    }
}

pub(crate) fn validate_upload_state(state: &str) -> Result<(), DbErr> {
    if matches!(
        state,
        "pending" | "uploaded" | "verified" | "published" | "rejected" | "deleted"
    ) {
        Ok(())
    } else {
        Err(DbErr::Custom("invalid upload state".into()))
    }
}

pub(crate) fn validate_download_source(source: &str) -> Result<(), DbErr> {
    if matches!(source, "cli" | "web" | "api" | "mirror") {
        Ok(())
    } else {
        Err(DbErr::Custom("invalid download source".into()))
    }
}

pub(crate) fn validate_entity_type(entity_type: &str) -> Result<(), DbErr> {
    if matches!(entity_type, "org" | "project" | "package") {
        Ok(())
    } else {
        Err(DbErr::Custom("invalid embedding entity type".into()))
    }
}

pub(crate) fn validate_sha256(checksum: Option<&str>) -> Result<(), DbErr> {
    let Some(checksum) = checksum else {
        return Ok(());
    };
    if checksum.len() == 64
        && checksum
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(DbErr::Custom(
            "SHA-256 checksum must be 64 lowercase hexadecimal characters".into(),
        ))
    }
}

pub(crate) fn vector_literal(embedding: &[f32]) -> Result<String, DbErr> {
    if embedding.len() != 1536 {
        return Err(DbErr::Custom(format!(
            "embedding must contain 1536 dimensions, got {}",
            embedding.len()
        )));
    }
    if embedding.iter().any(|value| !value.is_finite()) {
        return Err(DbErr::Custom(
            "embedding dimensions must all be finite".into(),
        ));
    }
    Ok(format!(
        "[{}]",
        embedding
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
            .join(",")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_formats_match_upload_and_download_contracts() {
        for format in ["zip", "tar", "tar_gz", "tar_zst"] {
            assert!(validate_archive_format(format).is_ok());
        }
        assert!(validate_archive_format("tgz.exe").is_err());
    }

    #[test]
    fn vector_literals_are_dimensioned_and_finite() {
        assert!(vector_literal(&vec![0.0; 1536]).is_ok());
        assert!(vector_literal(&vec![0.0; 1535]).is_err());
        let mut invalid = vec![0.0; 1536];
        invalid[20] = f32::NAN;
        assert!(vector_literal(&invalid).is_err());
    }

    #[test]
    fn package_names_cannot_escape_registry_coordinates() {
        assert!(validate_package_name("http-kit").is_ok());
        assert!(validate_package_name("../admin").is_err());
        assert!(validate_package_name("org/name").is_err());
    }

    #[test]
    fn sha256_is_strict_lowercase_hex() {
        assert!(validate_sha256(Some(&"a".repeat(64))).is_ok());
        assert!(validate_sha256(Some(&"A".repeat(64))).is_err());
        assert!(validate_sha256(Some("deadbeef")).is_err());
    }
}
