//! One-time organization and project invitation acceptance.
//!
//! Invitation tokens are never persisted in plaintext. The caller presents the
//! one-time token, Postgres derives its SHA-256 digest, and acceptance succeeds
//! only when the verified Shared Auth email matches a live invitation. The
//! membership insert and invitation consumption occur in one transaction.

use std::time::SystemTime;

use sea_orm::{
    ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection, DbErr, EntityTrait,
    QueryFilter, Statement, TransactionTrait, Value,
    prelude::{DateTimeUtc, DateTimeWithTimeZone},
    sea_query::OnConflict,
};

use crate::entities::{
    org, org_invitation, org_member, project, project_invitation, project_member,
};
use crate::models::{InvitationAcceptance, InvitationTarget, SessionIdentity};
use crate::queries::write;

/// Accept exactly one unexpired invitation for the verified Shared Auth user.
pub async fn accept(
    conn: &DatabaseConnection,
    identity: &SessionIdentity,
    raw_token: &str,
) -> Result<InvitationAcceptance, DbErr> {
    validate_token(raw_token)?;
    let verified_email = normalized_identity_email(identity)?;
    let user = write::ensure_user(conn, identity).await?;
    let token_hash = hash_token(conn, raw_token).await?;
    let accepted_at = now();
    let txn = conn.begin().await?;

    let org_candidate = org_invitation::Entity::find()
        .filter(org_invitation::Column::TokenHash.eq(&token_hash))
        .filter(org_invitation::Column::AcceptedAt.is_null())
        .filter(org_invitation::Column::ExpiresAt.gt(accepted_at))
        .one(&txn)
        .await?;
    let project_candidate = project_invitation::Entity::find()
        .filter(project_invitation::Column::TokenHash.eq(&token_hash))
        .filter(project_invitation::Column::AcceptedAt.is_null())
        .filter(project_invitation::Column::ExpiresAt.gt(accepted_at))
        .one(&txn)
        .await?;

    let acceptance = match (org_candidate, project_candidate) {
        (Some(invitation), None) => {
            require_matching_email(&invitation.email, &verified_email)?;
            consume_org_invitation(&txn, invitation, user.id, accepted_at).await?
        }
        (None, Some(invitation)) => {
            require_matching_email(&invitation.email, &verified_email)?;
            consume_project_invitation(&txn, invitation, user.id, accepted_at).await?
        }
        (None, None) => return Err(invalid_invitation()),
        (Some(_), Some(_)) => {
            return Err(DbErr::Custom(
                "ambiguous invitation token; contact registry support".into(),
            ));
        }
    };

    txn.commit().await?;
    Ok(acceptance)
}

async fn consume_org_invitation<C>(
    conn: &C,
    invitation: org_invitation::Model,
    user_id: sea_orm::prelude::Uuid,
    accepted_at: DateTimeWithTimeZone,
) -> Result<InvitationAcceptance, DbErr>
where
    C: ConnectionTrait,
{
    consume_once(
        conn,
        "org_invitations",
        invitation.id,
        accepted_at,
    )
    .await?;

    org_member::Entity::insert(org_member::ActiveModel {
        org_id: Set(invitation.org_id),
        user_id: Set(user_id),
        role: Set(invitation.role.clone()),
        created_at: Set(accepted_at),
        updated_at: Set(accepted_at),
    })
    .on_conflict(
        OnConflict::columns([org_member::Column::OrgId, org_member::Column::UserId])
            .do_nothing()
            .to_owned(),
    )
    .exec(conn)
    .await?;

    let organization = org::Entity::find_by_id(invitation.org_id)
        .one(conn)
        .await?
        .ok_or_else(invalid_invitation)?;

    Ok(InvitationAcceptance {
        invitation_id: invitation.id,
        user_id,
        role: invitation.role,
        target: InvitationTarget::Organization {
            org_id: organization.id,
            org_slug: organization.slug,
        },
    })
}

async fn consume_project_invitation<C>(
    conn: &C,
    invitation: project_invitation::Model,
    user_id: sea_orm::prelude::Uuid,
    accepted_at: DateTimeWithTimeZone,
) -> Result<InvitationAcceptance, DbErr>
where
    C: ConnectionTrait,
{
    consume_once(
        conn,
        "project_invitations",
        invitation.id,
        accepted_at,
    )
    .await?;

    project_member::Entity::insert(project_member::ActiveModel {
        project_id: Set(invitation.project_id),
        user_id: Set(user_id),
        role: Set(invitation.role.clone()),
        created_at: Set(accepted_at),
        updated_at: Set(accepted_at),
    })
    .on_conflict(
        OnConflict::columns([
            project_member::Column::ProjectId,
            project_member::Column::UserId,
        ])
        .do_nothing()
        .to_owned(),
    )
    .exec(conn)
    .await?;

    let project = project::Entity::find_by_id(invitation.project_id)
        .one(conn)
        .await?
        .ok_or_else(invalid_invitation)?;
    let organization = org::Entity::find_by_id(project.org_id)
        .one(conn)
        .await?
        .ok_or_else(invalid_invitation)?;

    Ok(InvitationAcceptance {
        invitation_id: invitation.id,
        user_id,
        role: invitation.role,
        target: InvitationTarget::Project {
            org_id: organization.id,
            org_slug: organization.slug,
            project_id: project.id,
            project_slug: project.slug,
        },
    })
}

async fn consume_once<C>(
    conn: &C,
    table: &'static str,
    invitation_id: sea_orm::prelude::Uuid,
    accepted_at: DateTimeWithTimeZone,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    let statement = Statement::from_sql_and_values(
        conn.get_database_backend(),
        format!(
            "UPDATE {table} \
             SET accepted_at = $1 \
             WHERE id = $2 AND accepted_at IS NULL AND expires_at > $1"
        ),
        [
            Value::ChronoDateTimeWithTimeZone(Some(Box::new(accepted_at))),
            Value::Uuid(Some(Box::new(invitation_id))),
        ],
    );
    let result = conn.execute(statement).await?;
    if result.rows_affected() == 1 {
        Ok(())
    } else {
        Err(invalid_invitation())
    }
}

async fn hash_token<C>(conn: &C, raw_token: &str) -> Result<String, DbErr>
where
    C: ConnectionTrait,
{
    let statement = Statement::from_sql_and_values(
        conn.get_database_backend(),
        "SELECT encode(digest($1, 'sha256'), 'hex') AS token_hash",
        [Value::String(Some(Box::new(raw_token.to_owned())))],
    );
    conn.query_one(statement)
        .await?
        .ok_or_else(invalid_invitation)?
        .try_get("", "token_hash")
}

fn normalized_identity_email(identity: &SessionIdentity) -> Result<String, DbErr> {
    let email = identity
        .email
        .as_deref()
        .map(str::trim)
        .filter(|email| !email.is_empty())
        .ok_or_else(|| {
            DbErr::Custom("a verified email is required to accept an invitation".into())
        })?;
    Ok(email.to_lowercase())
}

fn require_matching_email(invited_email: &str, verified_email: &str) -> Result<(), DbErr> {
    if invited_email.trim().eq_ignore_ascii_case(verified_email) {
        Ok(())
    } else {
        Err(invalid_invitation())
    }
}

fn validate_token(token: &str) -> Result<(), DbErr> {
    if (32..=256).contains(&token.len())
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        Ok(())
    } else {
        Err(invalid_invitation())
    }
}

fn invalid_invitation() -> DbErr {
    DbErr::Custom("invitation is invalid, expired, already used, or belongs to another email".into())
}

fn now() -> DateTimeWithTimeZone {
    let utc: DateTimeUtc = SystemTime::now().into();
    utc.fixed_offset()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_shape_is_bounded_and_url_safe() {
        assert!(validate_token(&"a".repeat(64)).is_ok());
        assert!(validate_token(&"a".repeat(31)).is_err());
        assert!(validate_token(&"a".repeat(257)).is_err());
        assert!(validate_token(&format!("{}+", "a".repeat(63))).is_err());
    }

    #[test]
    fn invitation_email_matches_case_insensitively() {
        assert!(require_matching_email("User@Example.COM", "user@example.com").is_ok());
        assert!(require_matching_email("other@example.com", "user@example.com").is_err());
    }

    #[test]
    fn verified_email_is_required() {
        let identity = SessionIdentity {
            subject: "subject".into(),
            email: None,
            display_name: None,
            avatar_url: None,
        };
        assert!(normalized_identity_email(&identity).is_err());
    }
}
