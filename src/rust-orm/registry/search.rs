use sea_orm::{
    ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr, Statement,
    prelude::{Json, Uuid},
};

use super::identity::FederatedIdentity;
use super::support::{validate_entity_type, validate_nonempty, validate_sha256, vector_literal};

#[derive(Clone, Debug, PartialEq)]
pub struct RegistrySearchHit {
    pub entity_type: String,
    pub entity_id: Uuid,
    pub label: String,
    pub description: Option<String>,
    pub score: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmbeddingInput {
    pub entity_type: String,
    pub entity_id: Uuid,
    pub model: String,
    pub source_text: String,
    pub source_hash: String,
    pub embedding: Vec<f32>,
    pub metadata: Json,
}

/// Visibility-aware text search. Anonymous callers see public packages only;
/// authenticated callers additionally see entities in their organizations.
pub async fn search_registry(
    conn: &DatabaseConnection,
    identity: Option<&FederatedIdentity>,
    query: &str,
    limit: u64,
) -> Result<Vec<RegistrySearchHit>, DbErr> {
    let (issuer, subject) = identity
        .map(|identity| (identity.issuer.clone(), identity.subject.clone()))
        .unwrap_or_else(|| (String::new(), String::new()));
    let query = query.trim().chars().take(200).collect::<String>();
    let limit = i64::try_from(limit.clamp(1, 100))
        .map_err(|_| DbErr::Custom("invalid search limit".into()))?;

    let statement = Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        r#"
WITH actor AS (
    SELECT id
    FROM users
    WHERE auth_issuer = $1 AND shared_auth_subject = $2
), visible_orgs AS (
    SELECT org_id
    FROM org_members
    WHERE user_id = (SELECT id FROM actor)
), hits AS (
    SELECT
        'org'::TEXT AS entity_type,
        o.id AS entity_id,
        o.slug AS label,
        o.description,
        ts_rank_cd(o.search_document, websearch_to_tsquery('simple', $3))::DOUBLE PRECISION AS score
    FROM org o
    WHERE o.id IN (SELECT org_id FROM visible_orgs)
      AND (
          $3 = '' OR o.search_document @@ websearch_to_tsquery('simple', $3)
          OR o.slug ILIKE '%' || $3 || '%'
          OR o.name ILIKE '%' || $3 || '%'
      )

    UNION ALL

    SELECT
        'project'::TEXT,
        p.id,
        o.slug || '/' || p.slug,
        p.description,
        ts_rank_cd(p.search_document, websearch_to_tsquery('simple', $3))::DOUBLE PRECISION
    FROM projects p
    JOIN org o ON o.id = p.org_id
    WHERE p.org_id IN (SELECT org_id FROM visible_orgs)
      AND (
          $3 = '' OR p.search_document @@ websearch_to_tsquery('simple', $3)
          OR p.slug ILIKE '%' || $3 || '%'
          OR p.name ILIKE '%' || $3 || '%'
      )

    UNION ALL

    SELECT
        'package'::TEXT,
        p.id,
        o.slug || '/' || p.name,
        p.description,
        ts_rank_cd(p.search_document, websearch_to_tsquery('simple', $3))::DOUBLE PRECISION
    FROM package p
    JOIN org o ON o.id = p.org_id
    WHERE (p.visibility = 'public' OR p.org_id IN (SELECT org_id FROM visible_orgs))
      AND (
          $3 = '' OR p.search_document @@ websearch_to_tsquery('simple', $3)
          OR p.name ILIKE '%' || $3 || '%'
          OR p.repo_url ILIKE '%' || $3 || '%'
      )
)
SELECT entity_type, entity_id, label, description, score
FROM hits
ORDER BY score DESC, label ASC
LIMIT $4
"#,
        [issuer.into(), subject.into(), query.into(), limit.into()],
    );

    let rows = conn.query_all(statement).await?;
    rows.into_iter()
        .map(|row| {
            Ok(RegistrySearchHit {
                entity_type: row.try_get("", "entity_type")?,
                entity_id: row.try_get("", "entity_id")?,
                label: row.try_get("", "label")?,
                description: row.try_get("", "description")?,
                score: row.try_get("", "score")?,
            })
        })
        .collect()
}

/// Visibility-aware cosine search over 1536-dimensional pgvector embeddings.
pub async fn semantic_search(
    conn: &DatabaseConnection,
    identity: Option<&FederatedIdentity>,
    model: &str,
    embedding: &[f32],
    limit: u64,
) -> Result<Vec<RegistrySearchHit>, DbErr> {
    validate_nonempty("embedding model", model, 255)?;
    let vector = vector_literal(embedding)?;
    let (issuer, subject) = identity
        .map(|identity| (identity.issuer.clone(), identity.subject.clone()))
        .unwrap_or_else(|| (String::new(), String::new()));
    let limit = i64::try_from(limit.clamp(1, 100))
        .map_err(|_| DbErr::Custom("invalid search limit".into()))?;

    let statement = Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        r#"
WITH actor AS (
    SELECT id
    FROM users
    WHERE auth_issuer = $1 AND shared_auth_subject = $2
), visible_orgs AS (
    SELECT org_id
    FROM org_members
    WHERE user_id = (SELECT id FROM actor)
)
SELECT
    e.entity_type,
    e.entity_id,
    CASE e.entity_type
        WHEN 'org' THEN embedded_org.slug
        WHEN 'project' THEN project_org.slug || '/' || embedded_project.slug
        WHEN 'package' THEN package_org.slug || '/' || embedded_package.name
    END AS label,
    CASE e.entity_type
        WHEN 'org' THEN embedded_org.description
        WHEN 'project' THEN embedded_project.description
        WHEN 'package' THEN embedded_package.description
    END AS description,
    (1 - (e.embedding <=> $3::vector))::DOUBLE PRECISION AS score
FROM entity_embeddings e
LEFT JOIN org embedded_org ON embedded_org.id = e.org_id
LEFT JOIN projects embedded_project ON embedded_project.id = e.project_id
LEFT JOIN org project_org ON project_org.id = embedded_project.org_id
LEFT JOIN package embedded_package ON embedded_package.id = e.package_id
LEFT JOIN org package_org ON package_org.id = embedded_package.org_id
WHERE e.model = $4
  AND (
      (e.entity_type = 'org' AND e.org_id IN (SELECT org_id FROM visible_orgs))
      OR (e.entity_type = 'project' AND embedded_project.org_id IN (SELECT org_id FROM visible_orgs))
      OR (
          e.entity_type = 'package'
          AND (
              embedded_package.visibility = 'public'
              OR embedded_package.org_id IN (SELECT org_id FROM visible_orgs)
          )
      )
  )
ORDER BY e.embedding <=> $3::vector
LIMIT $5
"#,
        [
            issuer.into(),
            subject.into(),
            vector.into(),
            model.to_owned().into(),
            limit.into(),
        ],
    );

    let rows = conn.query_all(statement).await?;
    rows.into_iter()
        .map(|row| {
            Ok(RegistrySearchHit {
                entity_type: row.try_get("", "entity_type")?,
                entity_id: row.try_get("", "entity_id")?,
                label: row.try_get("", "label")?,
                description: row.try_get("", "description")?,
                score: row.try_get("", "score")?,
            })
        })
        .collect()
}

/// Store an embedding from a trusted indexing worker. User-facing handlers must
/// authorize the source entity before enqueueing this operation.
pub async fn upsert_embedding(
    conn: &DatabaseConnection,
    input: &EmbeddingInput,
) -> Result<Uuid, DbErr> {
    validate_entity_type(&input.entity_type)?;
    validate_nonempty("embedding model", &input.model, 255)?;
    validate_sha256(Some(&input.source_hash))?;
    let vector = vector_literal(&input.embedding)?;
    let metadata = input.metadata.to_string();

    let row = conn
        .query_one(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            r#"
INSERT INTO entity_embeddings (
    entity_type,
    org_id,
    project_id,
    package_id,
    model,
    embedding,
    source_text,
    source_hash,
    metadata
) VALUES (
    $1,
    CASE WHEN $1 = 'org' THEN $2::UUID END,
    CASE WHEN $1 = 'project' THEN $2::UUID END,
    CASE WHEN $1 = 'package' THEN $2::UUID END,
    $3,
    $4::vector,
    $5,
    $6,
    $7::jsonb
)
ON CONFLICT (entity_type, entity_id, model, source_hash)
DO UPDATE SET
    embedding = EXCLUDED.embedding,
    source_text = EXCLUDED.source_text,
    metadata = EXCLUDED.metadata,
    updated_at = clock_timestamp()
RETURNING id
"#,
            [
                input.entity_type.clone().into(),
                input.entity_id.into(),
                input.model.clone().into(),
                vector.into(),
                input.source_text.clone().into(),
                input.source_hash.clone().into(),
                metadata.into(),
            ],
        ))
        .await?
        .ok_or_else(|| DbErr::Custom("embedding upsert returned no id".into()))?;
    row.try_get("", "id")
}
