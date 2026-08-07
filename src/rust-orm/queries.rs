//! Named query functions over the shared `zed_pkg` schema.
//!
//! # Contract
//!
//! This crate exports **named query functions**, never a raw ORM session. Each
//! data access a service needs gets a function here with a name, a signature,
//! and a home:
//!
//! - [`read`] — query functions with no side effects on the shared schema.
//!   **Web tiers may only call functions in this submodule.**
//! - [`write`] — mutations of the shared schema. Only the API server (the sole
//!   shared-schema writer, connecting with
//!   [`DbRole::ReadWrite`](crate::DbRole::ReadWrite)) may call these.
//!
//! Keeping the split at the module boundary makes a web-tier write reviewable
//! at a glance: any `queries::write::` path in web-server code is a bug, no
//! schema knowledge required.

/// Read-only queries. The only submodule web tiers may call.
pub mod read {
    use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr, Statement};

    /// Verify the connection can execute a query (`SELECT 1`).
    ///
    /// Suitable for liveness/readiness probes in any tier.
    pub async fn healthcheck(conn: &DatabaseConnection) -> Result<(), DbErr> {
        let statement = Statement::from_string(conn.get_database_backend(), "SELECT 1");
        conn.query_one(statement).await.map(|_| ())
    }
}

/// Shared-schema mutations. API server only; web tiers must never call in
/// here. Empty until the first shared-schema write moves behind the boundary.
pub mod write {}
