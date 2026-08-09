use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr, EntityTrait,
    QueryFilter, Statement, prelude::Uuid,
};

use crate::entities::{org, org_member, user};
use crate::models::SessionIdentity;

use super::support::{new_uuid, validate_nonempty};

#[derive(Clone, Debug, PartialEq)]
pub struct FederatedIdentity {
    pub issuer: String,
    pub subject: String,
    pub supabase_user_id: Option<Uuid>,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

impl FederatedIdentity {
    /// Preserve the deployed Shared Auth mapping while allowing direct
    /// Supabase Auth sessions to supply an explicit issuer and user id.
    pub fn from_shared_auth(identity: &SessionIdentity) -> Self {
        Self {
            issuer: "shared-auth".into(),
            subject: identity.subject.clone(),
            supabase_user_id: None,
            email: identity.email.clone(),
            display_name: identity.display_name.clone(),
            avatar_url: identity.avatar_url.clone(),
        }
    }
}

/// Upsert the registry-local projection of a verified principal.
///
/// The compatibility columns are written through centralized SQL so the
/// original Shared Auth SeaORM entity stays source-compatible for existing
/// consumers during the expand migration.
pub async fn ensure_federated_user(
    conn: &DatabaseConnection,
    identity: &FederatedIdentity,
) -> Result<user::Model, DbErr> {
    validate_identity(identity)?;
    let proposed_id = new_uuid(conn).await?;
    let supabase_user_id = identity.supabase_user_id.map(|id| id.to_string());

    let row = conn
        .query_one(Statement::from_sql_and_values(
            DatabaseBackend::Postgres,
            r#"
INSERT INTO users (
    id,
    auth_issuer,
    shared_auth_subject,
    supabase_user_id,
    email,
    display_name,
    avatar_url,
    settings,
    created_at,
    updated_at
) VALUES (
    $1,
    $2,
    $3,
    $4::UUID,
    $5,
    $6,
    $7,
    '{}'::jsonb,
    clock_timestamp(),
    clock_timestamp()
)
ON CONFLICT (shared_auth_subject)
DO UPDATE SET
    auth_issuer = EXCLUDED.auth_issuer,
    supabase_user_id = coalesce(EXCLUDED.supabase_user_id, users.supabase_user_id),
    email = coalesce(EXCLUDED.email, users.email),
    display_name = coalesce(EXCLUDED.display_name, users.display_name),
    avatar_url = coalesce(EXCLUDED.avatar_url, users.avatar_url),
    updated_at = clock_timestamp()
RETURNING id
"#,
            [
                proposed_id.into(),
                identity.issuer.clone().into(),
                identity.subject.clone().into(),
                supabase_user_id.into(),
                identity.email.clone().into(),
                identity.display_name.clone().into(),
                identity.avatar_url.clone().into(),
            ],
        ))
        .await?
        .ok_or_else(|| DbErr::Custom("identity projection returned no user id".into()))?;
    let user_id: Uuid = row.try_get("", "id")?;

    user::Entity::find_by_id(user_id)
        .one(conn)
        .await?
        .ok_or_else(|| DbErr::Custom("identity projection returned an unknown user".into()))
}

pub(crate) async fn actor_and_org(
    conn: &DatabaseConnection,
    identity: &FederatedIdentity,
    org_slug: &str,
) -> Result<(user::Model, org::Model, String), DbErr> {
    let actor = ensure_federated_user(conn, identity).await?;
    let org_model = org::Entity::find()
        .filter(org::Column::Slug.eq(org_slug))
        .one(conn)
        .await?
        .ok_or_else(|| DbErr::Custom("organization not found".into()))?;
    let membership = org_member::Entity::find()
        .filter(org_member::Column::OrgId.eq(org_model.id))
        .filter(org_member::Column::UserId.eq(actor.id))
        .one(conn)
        .await?
        .ok_or_else(|| DbErr::Custom("organization membership required".into()))?;
    Ok((actor, org_model, membership.role))
}

pub(crate) fn require_admin(role: &str) -> Result<(), DbErr> {
    if matches!(role, "owner" | "admin") {
        Ok(())
    } else {
        Err(DbErr::Custom("administrator role required".into()))
    }
}

pub(crate) fn require_writer(role: &str) -> Result<(), DbErr> {
    if matches!(role, "owner" | "admin" | "member") {
        Ok(())
    } else {
        Err(DbErr::Custom("write-capable membership required".into()))
    }
}

fn validate_identity(identity: &FederatedIdentity) -> Result<(), DbErr> {
    validate_nonempty("authentication issuer", &identity.issuer, 255)?;
    validate_nonempty("authentication subject", &identity.subject, 1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_auth_adapter_keeps_the_deployed_issuer() {
        let identity = SessionIdentity {
            subject: "subject-1".into(),
            email: Some("user@example.test".into()),
            display_name: None,
            avatar_url: None,
        };
        let federated = FederatedIdentity::from_shared_auth(&identity);
        assert_eq!(federated.issuer, "shared-auth");
        assert_eq!(federated.subject, "subject-1");
        assert_eq!(federated.supabase_user_id, None);
    }

    #[test]
    fn role_checks_are_fail_closed() {
        assert!(require_admin("owner").is_ok());
        assert!(require_admin("member").is_err());
        assert!(require_writer("member").is_ok());
        assert!(require_writer("reader").is_err());
    }
}
