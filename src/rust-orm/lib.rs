//! Canonical SeaORM data plane for the `zed-pkg` registry.
//!
//! `zed-lib` owns the durable registry schema, migrations, entities, and named
//! operations used by the API server, the MASH web server, migration jobs, and
//! background workers. Services import this crate instead of copying SeaORM
//! entities or constructing ad-hoc queries.
//!
//! The registry and Shared Auth remain separate data planes:
//!
//! - this crate owns registry authorization and product data (`users`, `org`,
//!   `projects`, `package`, memberships, invitations, package artifacts, and
//!   search data);
//! - Shared Auth owns authentication ceremonies and revocable sessions in its
//!   customer-auth RDS instance;
//! - a verified Shared Auth subject is mapped to one registry user and may also
//!   retain its corresponding Supabase user identifier.

mod connect;
pub mod entities;
pub mod migrations;
pub mod models;
pub mod policy;
pub mod queries;
pub mod registry;
mod registry_migration;
mod schema;

pub use connect::{DbRole, apply_role, assert_read_only, connect};
pub use migrations::{
    ACCOUNT_CONSOLE_MIGRATION, LATEST_MIGRATION, MigrationReport, ORG_NAME_COMPAT_MIGRATION,
    migrate,
};
pub use registry_migration::REGISTRY_FEATURES_MIGRATION;
pub use schema::{REGISTRY_SCHEMA, qualified};

pub use sea_orm;
