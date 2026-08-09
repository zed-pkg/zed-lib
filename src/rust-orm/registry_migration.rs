//! Append-only migration for registry artifacts, search, and visibility policy.

use sea_orm::{ConnectionTrait, DbErr, Statement};

/// Immutable migration identifier stored in `zed_schema_migrations`.
pub const REGISTRY_FEATURES_MIGRATION: &str =
    "20260809_000002_registry_artifacts_search_visibility";

pub(crate) const REGISTRY_FEATURES_SQL: &str = r#"
CREATE EXTENSION IF NOT EXISTS vector;

ALTER TABLE users
    ADD COLUMN IF NOT EXISTS auth_issuer TEXT NOT NULL DEFAULT 'shared-auth';
ALTER TABLE users
    ADD COLUMN IF NOT EXISTS supabase_user_id UUID;

CREATE UNIQUE INDEX IF NOT EXISTS idx_users_auth_principal
    ON users(auth_issuer, shared_auth_subject);
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_supabase_user_id
    ON users(supabase_user_id)
    WHERE supabase_user_id IS NOT NULL;

ALTER TABLE package
    ADD COLUMN IF NOT EXISTS created_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL;
ALTER TABLE package
    ADD COLUMN IF NOT EXISTS default_archive_format TEXT NOT NULL DEFAULT 'tar_gz';
ALTER TABLE package
    ADD COLUMN IF NOT EXISTS download_count BIGINT NOT NULL DEFAULT 0;
ALTER TABLE package
    ADD COLUMN IF NOT EXISTS upload_count BIGINT NOT NULL DEFAULT 0;
ALTER TABLE package
    ADD COLUMN IF NOT EXISTS first_public_at TIMESTAMPTZ;
ALTER TABLE package
    ADD COLUMN IF NOT EXISTS visibility_changed_at TIMESTAMPTZ NOT NULL DEFAULT now();
ALTER TABLE package
    ADD COLUMN IF NOT EXISTS visibility_changed_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL;

-- Existing registry packages were public before account-console support. Keep
-- those rows public, record their original publication boundary, and make only
-- newly inserted packages private by default.
UPDATE package
SET first_public_at = created_at
WHERE visibility = 'public' AND first_public_at IS NULL;
ALTER TABLE package ALTER COLUMN visibility SET DEFAULT 'private';

DO $migration$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'package'::regclass
          AND conname = 'package_visibility_check'
    ) THEN
        ALTER TABLE package
            ADD CONSTRAINT package_visibility_check
            CHECK (visibility IN ('private', 'public', 'internal')) NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'package'::regclass
          AND conname = 'package_download_count_nonnegative'
    ) THEN
        ALTER TABLE package
            ADD CONSTRAINT package_download_count_nonnegative
            CHECK (download_count >= 0) NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'package'::regclass
          AND conname = 'package_upload_count_nonnegative'
    ) THEN
        ALTER TABLE package
            ADD CONSTRAINT package_upload_count_nonnegative
            CHECK (upload_count >= 0) NOT VALID;
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'package'::regclass
          AND conname = 'package_default_archive_format_check'
    ) THEN
        ALTER TABLE package
            ADD CONSTRAINT package_default_archive_format_check
            CHECK (default_archive_format IN ('zip', 'tar', 'tar_gz', 'tar_zst')) NOT VALID;
    END IF;
END
$migration$;

CREATE TABLE IF NOT EXISTS package_licenses (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    package_id UUID NOT NULL REFERENCES package(id) ON DELETE CASCADE,
    spdx_expression TEXT,
    license_name TEXT,
    license_url TEXT,
    license_text TEXT,
    checksum_sha256 CHAR(64),
    is_primary BOOLEAN NOT NULL DEFAULT false,
    created_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT package_licenses_has_identity CHECK (
        nullif(btrim(coalesce(spdx_expression, '')), '') IS NOT NULL
        OR nullif(btrim(coalesce(license_name, '')), '') IS NOT NULL
    ),
    CONSTRAINT package_licenses_checksum_shape CHECK (
        checksum_sha256 IS NULL OR checksum_sha256 ~ '^[0-9a-f]{64}$'
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_package_licenses_primary
    ON package_licenses(package_id)
    WHERE is_primary;
CREATE INDEX IF NOT EXISTS idx_package_licenses_package
    ON package_licenses(package_id, created_at DESC);

CREATE TABLE IF NOT EXISTS package_uploads (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    package_id UUID NOT NULL REFERENCES package(id) ON DELETE CASCADE,
    source_upload_id UUID,
    version TEXT NOT NULL,
    archive_format TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'pending',
    uploader_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    storage_backend TEXT NOT NULL DEFAULT 's3',
    storage_bucket TEXT NOT NULL,
    storage_key TEXT NOT NULL,
    original_filename TEXT,
    size_bytes BIGINT NOT NULL,
    sha256 CHAR(64) NOT NULL,
    vcs_tag TEXT,
    vcs_commit TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    uploaded_at TIMESTAMPTZ,
    verified_at TIMESTAMPTZ,
    published_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT package_uploads_artifact_key UNIQUE (
        storage_backend, storage_bucket, storage_key
    ),
    CONSTRAINT package_uploads_package_id_id_key UNIQUE (package_id, id),
    CONSTRAINT package_uploads_package_id_id_format_key UNIQUE (
        package_id, id, archive_format
    ),
    CONSTRAINT package_uploads_source_same_package_fk
        FOREIGN KEY (package_id, source_upload_id)
        REFERENCES package_uploads(package_id, id)
        ON DELETE NO ACTION,
    CONSTRAINT package_uploads_version_format_key UNIQUE (
        package_id, version, archive_format
    ),
    CONSTRAINT package_uploads_version_nonempty CHECK (btrim(version) <> ''),
    CONSTRAINT package_uploads_archive_format_check CHECK (
        archive_format IN ('zip', 'tar', 'tar_gz', 'tar_zst')
    ),
    CONSTRAINT package_uploads_state_check CHECK (
        state IN ('pending', 'uploaded', 'verified', 'published', 'rejected', 'deleted')
    ),
    CONSTRAINT package_uploads_size_positive CHECK (size_bytes > 0),
    CONSTRAINT package_uploads_sha256_shape CHECK (sha256 ~ '^[0-9a-f]{64}$'),
    CONSTRAINT package_uploads_metadata_object CHECK (jsonb_typeof(metadata) = 'object'),
    CONSTRAINT package_uploads_publication_timestamp CHECK (
        state <> 'published' OR published_at IS NOT NULL
    )
);

CREATE INDEX IF NOT EXISTS idx_package_uploads_package_published
    ON package_uploads(package_id, published_at DESC)
    WHERE state = 'published';

CREATE TABLE IF NOT EXISTS package_downloads (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    package_id UUID NOT NULL REFERENCES package(id) ON DELETE CASCADE,
    upload_id UUID NOT NULL,
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    archive_format TEXT NOT NULL,
    source TEXT NOT NULL DEFAULT 'api',
    request_id UUID NOT NULL DEFAULT gen_random_uuid(),
    bytes_served BIGINT NOT NULL,
    completed BOOLEAN NOT NULL DEFAULT true,
    -- Privacy-preserving keyed hashes only. Raw IP addresses, session ids, and
    -- bearer tokens never belong in the registry database.
    client_fingerprint_hash BYTEA,
    user_agent_hash BYTEA,
    downloaded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT package_downloads_request_key UNIQUE (request_id),
    CONSTRAINT package_downloads_upload_same_package_fk
        FOREIGN KEY (package_id, upload_id, archive_format)
        REFERENCES package_uploads(package_id, id, archive_format)
        ON DELETE RESTRICT,
    CONSTRAINT package_downloads_source_check CHECK (
        source IN ('cli', 'web', 'api', 'mirror')
    ),
    CONSTRAINT package_downloads_bytes_nonnegative CHECK (bytes_served >= 0)
);

CREATE INDEX IF NOT EXISTS idx_package_downloads_package_time
    ON package_downloads(package_id, downloaded_at DESC)
    WHERE completed;
CREATE INDEX IF NOT EXISTS idx_package_downloads_upload_time
    ON package_downloads(upload_id, downloaded_at DESC)
    WHERE completed;

ALTER TABLE org
    ADD COLUMN IF NOT EXISTS search_document TSVECTOR
    GENERATED ALWAYS AS (
        to_tsvector(
            'simple',
            coalesce(slug, '') || ' ' || coalesce(name, '') || ' ' || coalesce(description, '')
        )
    ) STORED;
ALTER TABLE projects
    ADD COLUMN IF NOT EXISTS search_document TSVECTOR
    GENERATED ALWAYS AS (
        to_tsvector(
            'simple',
            coalesce(slug, '') || ' ' || coalesce(name, '') || ' ' || coalesce(description, '')
        )
    ) STORED;
ALTER TABLE package
    ADD COLUMN IF NOT EXISTS search_document TSVECTOR
    GENERATED ALWAYS AS (
        to_tsvector(
            'simple',
            coalesce(name, '') || ' ' || coalesce(description, '') || ' ' || coalesce(repo_url, '')
        )
    ) STORED;

CREATE INDEX IF NOT EXISTS idx_org_search_document
    ON org USING gin(search_document);
CREATE INDEX IF NOT EXISTS idx_projects_search_document
    ON projects USING gin(search_document);
CREATE INDEX IF NOT EXISTS idx_package_search_document
    ON package USING gin(search_document);
CREATE INDEX IF NOT EXISTS idx_package_public_recent
    ON package(created_at DESC)
    WHERE visibility = 'public';

CREATE TABLE IF NOT EXISTS entity_embeddings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    entity_type TEXT NOT NULL,
    org_id UUID REFERENCES org(id) ON DELETE CASCADE,
    project_id UUID REFERENCES projects(id) ON DELETE CASCADE,
    package_id UUID REFERENCES package(id) ON DELETE CASCADE,
    entity_id UUID GENERATED ALWAYS AS (coalesce(org_id, project_id, package_id)) STORED,
    model TEXT NOT NULL,
    dimensions INTEGER NOT NULL DEFAULT 1536,
    embedding vector(1536) NOT NULL,
    source_text TEXT NOT NULL,
    source_hash CHAR(64) NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    search_document TSVECTOR GENERATED ALWAYS AS (
        to_tsvector('simple', coalesce(source_text, ''))
    ) STORED,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT entity_embeddings_exactly_one_entity CHECK (
        num_nonnulls(org_id, project_id, package_id) = 1
    ),
    CONSTRAINT entity_embeddings_type_check CHECK (
        entity_type IN ('org', 'project', 'package')
    ),
    CONSTRAINT entity_embeddings_type_matches_fk CHECK (
        (entity_type = 'org' AND org_id IS NOT NULL AND project_id IS NULL AND package_id IS NULL)
        OR (entity_type = 'project' AND org_id IS NULL AND project_id IS NOT NULL AND package_id IS NULL)
        OR (entity_type = 'package' AND org_id IS NULL AND project_id IS NULL AND package_id IS NOT NULL)
    ),
    CONSTRAINT entity_embeddings_dimensions_check CHECK (dimensions = 1536),
    CONSTRAINT entity_embeddings_model_nonempty CHECK (btrim(model) <> ''),
    CONSTRAINT entity_embeddings_source_hash_shape CHECK (
        source_hash ~ '^[0-9a-f]{64}$'
    ),
    CONSTRAINT entity_embeddings_metadata_object CHECK (
        jsonb_typeof(metadata) = 'object'
    ),
    CONSTRAINT entity_embeddings_version_key UNIQUE (
        entity_type, entity_id, model, source_hash
    )
);

CREATE INDEX IF NOT EXISTS idx_entity_embeddings_entity
    ON entity_embeddings(entity_type, entity_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_entity_embeddings_search_document
    ON entity_embeddings USING gin(search_document);
CREATE INDEX IF NOT EXISTS idx_entity_embeddings_cosine_hnsw
    ON entity_embeddings USING hnsw(embedding vector_cosine_ops);

CREATE OR REPLACE FUNCTION zed_enforce_package_project_org()
RETURNS trigger
LANGUAGE plpgsql
AS $function$
BEGIN
    IF NEW.project_id IS NOT NULL AND NOT EXISTS (
        SELECT 1
        FROM projects
        WHERE id = NEW.project_id AND org_id = NEW.org_id
    ) THEN
        RAISE EXCEPTION USING
            ERRCODE = 'foreign_key_violation',
            MESSAGE = 'package project must belong to the same organization';
    END IF;
    RETURN NEW;
END;
$function$;

DROP TRIGGER IF EXISTS package_project_org_guard ON package;
CREATE TRIGGER package_project_org_guard
BEFORE INSERT OR UPDATE OF org_id, project_id ON package
FOR EACH ROW EXECUTE FUNCTION zed_enforce_package_project_org();

CREATE OR REPLACE FUNCTION zed_enforce_package_visibility_transition()
RETURNS trigger
LANGUAGE plpgsql
AS $function$
BEGIN
    IF OLD.visibility = 'private' AND NEW.visibility = 'public' THEN
        -- UPDATE already owns the package row lock. Completed-download inserts
        -- update the same row, so the fifty-first download and this transition
        -- serialize into one deterministic order.
        IF clock_timestamp() > OLD.created_at + interval '10 days' THEN
            RAISE EXCEPTION USING
                ERRCODE = 'check_violation',
                MESSAGE = 'package cannot become public after it is more than 10 days old';
        END IF;
        IF OLD.download_count > 50 THEN
            RAISE EXCEPTION USING
                ERRCODE = 'check_violation',
                MESSAGE = 'package cannot become public after more than 50 downloads';
        END IF;
        NEW.first_public_at := coalesce(OLD.first_public_at, clock_timestamp());
        NEW.visibility_changed_at := clock_timestamp();
    ELSIF OLD.visibility IS DISTINCT FROM NEW.visibility THEN
        NEW.visibility_changed_at := clock_timestamp();
    END IF;
    RETURN NEW;
END;
$function$;

DROP TRIGGER IF EXISTS package_visibility_guard ON package;
CREATE TRIGGER package_visibility_guard
BEFORE UPDATE OF visibility ON package
FOR EACH ROW EXECUTE FUNCTION zed_enforce_package_visibility_transition();

CREATE OR REPLACE FUNCTION zed_record_completed_download()
RETURNS trigger
LANGUAGE plpgsql
AS $function$
BEGIN
    IF TG_OP = 'INSERT' THEN
        IF NEW.completed THEN
            UPDATE package
            SET download_count = download_count + 1
            WHERE id = NEW.package_id;
        END IF;
    ELSIF NEW.completed AND NOT OLD.completed THEN
        UPDATE package
        SET download_count = download_count + 1
        WHERE id = NEW.package_id;
    END IF;
    RETURN NEW;
END;
$function$;

DROP TRIGGER IF EXISTS package_downloads_record_completed ON package_downloads;
CREATE TRIGGER package_downloads_record_completed
AFTER INSERT OR UPDATE OF completed ON package_downloads
FOR EACH ROW EXECUTE FUNCTION zed_record_completed_download();

CREATE OR REPLACE FUNCTION zed_record_published_upload()
RETURNS trigger
LANGUAGE plpgsql
AS $function$
BEGIN
    IF TG_OP = 'INSERT' THEN
        IF NEW.state = 'published' THEN
            UPDATE package
            SET upload_count = upload_count + 1
            WHERE id = NEW.package_id;
        END IF;
    ELSIF NEW.state = 'published' AND OLD.state <> 'published' THEN
        UPDATE package
        SET upload_count = upload_count + 1
        WHERE id = NEW.package_id;
    END IF;
    RETURN NEW;
END;
$function$;

DROP TRIGGER IF EXISTS package_uploads_record_published ON package_uploads;
CREATE TRIGGER package_uploads_record_published
AFTER INSERT OR UPDATE OF state ON package_uploads
FOR EACH ROW EXECUTE FUNCTION zed_record_published_upload();

COMMENT ON TABLE users IS
    'Registry-local projection of externally authenticated principals; never a password or session store.';
COMMENT ON TABLE package_downloads IS
    'Append-only artifact download facts; raw IP, bearer token, and session data are prohibited.';
COMMENT ON FUNCTION zed_enforce_package_visibility_transition() IS
    'Allows private-to-public only while age <= 10 days and completed downloads <= 50.';
"#;

/// Apply the registry feature migration once. The caller owns the surrounding
/// transaction and advisory lock used by the main migrator.
pub(crate) async fn apply<C>(conn: &C) -> Result<bool, DbErr>
where
    C: ConnectionTrait,
{
    let already_applied = conn
        .query_one(Statement::from_string(
            conn.get_database_backend(),
            format!(
                "SELECT version FROM zed_schema_migrations WHERE version = '{}'",
                REGISTRY_FEATURES_MIGRATION
            ),
        ))
        .await?
        .is_some();

    if already_applied {
        return Ok(false);
    }

    conn.execute_unprepared(REGISTRY_FEATURES_SQL).await?;
    conn.execute_unprepared(&format!(
        "INSERT INTO zed_schema_migrations(version) VALUES ('{}')",
        REGISTRY_FEATURES_MIGRATION
    ))
    .await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_contains_every_requested_registry_entity() {
        for table in [
            "users",
            "org",
            "projects",
            "package",
            "package_licenses",
            "entity_embeddings",
            "package_uploads",
            "package_downloads",
        ] {
            assert!(REGISTRY_FEATURES_SQL.contains(table), "missing {table}");
        }
    }

    #[test]
    fn migration_keeps_exact_visibility_boundaries() {
        assert!(REGISTRY_FEATURES_SQL.contains("interval '10 days'"));
        assert!(REGISTRY_FEATURES_SQL.contains("OLD.download_count > 50"));
        assert!(!REGISTRY_FEATURES_SQL.contains("OLD.download_count >= 50"));
    }

    #[test]
    fn migration_serializes_downloads_with_visibility_updates() {
        assert!(
            REGISTRY_FEATURES_SQL
                .contains("UPDATE package\n            SET download_count = download_count + 1")
        );
        assert!(REGISTRY_FEATURES_SQL.contains("package_visibility_guard"));
    }

    #[test]
    fn sessions_and_passwords_are_not_registry_columns() {
        let lowered = REGISTRY_FEATURES_SQL.to_lowercase();
        assert!(!lowered.contains("password_hash"));
        assert!(!lowered.contains("refresh_token"));
        assert!(!lowered.contains("session_token"));
    }
}
