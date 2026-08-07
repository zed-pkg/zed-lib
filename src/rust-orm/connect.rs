//! Role-aware connection construction for the shared `zed_pkg` schema.

use std::time::Duration;

use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbErr};

use crate::schema::ORG_SCHEMA;

/// The role a service connects with, per `SERVICE_AND_DATA_ARCHITECTURE.md`.
///
/// - [`DbRole::ReadWrite`]: the API server, the sole writer of the shared
///   schema.
/// - [`DbRole::ReadOnly`]: web tiers. The connection URL gains the Postgres
///   startup option `default_transaction_read_only=on`, so every transaction
///   on the pool is read-only unless a session explicitly (and audibly)
///   overrides it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbRole {
    ReadWrite,
    ReadOnly,
}

/// Percent-encoded form of `-c default_transaction_read_only=on`.
///
/// Space and `=` are encoded so the value survives URL query parsing intact;
/// the driver percent-decodes it before handing it to Postgres as the
/// `options` startup parameter.
const READ_ONLY_OPTIONS: &str = "options=-c%20default_transaction_read_only%3Don";

/// Apply `role` to `database_url`.
///
/// [`DbRole::ReadWrite`] passes the URL through untouched. [`DbRole::ReadOnly`]
/// appends the Postgres startup option `options=-c
/// default_transaction_read_only=on` (percent-encoded), respecting any query
/// string already present. Pure function; see the unit tests below.
pub fn apply_role(database_url: &str, role: DbRole) -> String {
    match role {
        DbRole::ReadWrite => database_url.to_owned(),
        DbRole::ReadOnly => {
            let separator = if database_url.contains('?') { '&' } else { '?' };
            format!("{database_url}{separator}{READ_ONLY_OPTIONS}")
        }
    }
}

/// Connect to Postgres with `role` applied and the search path pinned to
/// [`ORG_SCHEMA`].
///
/// Pool defaults are deliberately modest; services with special needs should
/// still come through here and we widen the defaults, not fork them.
pub async fn connect(database_url: &str, role: DbRole) -> Result<DatabaseConnection, DbErr> {
    let mut options = ConnectOptions::new(apply_role(database_url, role));
    options
        .max_connections(10)
        .min_connections(1)
        .connect_timeout(Duration::from_secs(10))
        .acquire_timeout(Duration::from_secs(10))
        .idle_timeout(Duration::from_secs(300))
        .sqlx_logging(false)
        .set_schema_search_path(ORG_SCHEMA);
    Database::connect(options).await
}

/// Verify the connection really is read-only; call at web-server startup.
///
/// Returns an error unless `current_setting('default_transaction_read_only')`
/// is `on`. This turns a misconfigured URL (or a driver silently dropping the
/// startup option) into a startup failure instead of a latent write path.
pub async fn assert_read_only(conn: &DatabaseConnection) -> Result<(), DbErr> {
    use sea_orm::{ConnectionTrait, Statement};

    let statement = Statement::from_string(
        conn.get_database_backend(),
        "SELECT current_setting('default_transaction_read_only') AS setting",
    );
    let row = conn
        .query_one(statement)
        .await?
        .ok_or_else(|| DbErr::Custom("default_transaction_read_only returned no row".into()))?;
    let setting: String = row.try_get("", "setting")?;
    if setting == "on" {
        Ok(())
    } else {
        Err(DbErr::Custom(format!(
            "connection is not read-only: default_transaction_read_only = {setting:?} \
             (expected \"on\"; web tiers must connect with DbRole::ReadOnly)"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::{DbRole, apply_role};

    #[test]
    fn read_only_appends_options_to_plain_url() {
        assert_eq!(
            apply_role("postgres://app@db/zed", DbRole::ReadOnly),
            "postgres://app@db/zed?options=-c%20default_transaction_read_only%3Don"
        );
    }

    #[test]
    fn read_only_respects_existing_query_string() {
        assert_eq!(
            apply_role("postgres://app@db/zed?sslmode=require", DbRole::ReadOnly),
            "postgres://app@db/zed?sslmode=require&options=-c%20default_transaction_read_only%3Don"
        );
    }

    #[test]
    fn read_write_passes_url_through() {
        let url = "postgres://app@db/zed?sslmode=require";
        assert_eq!(apply_role(url, DbRole::ReadWrite), url);
    }
}
