//! Shared SeaORM data-access crate for the `zed-pkg` org.
//!
//! This crate is the single place where zed-pkg services touch the shared
//! Postgres schema (`zed_pkg`). It enforces the org's write/read split, defined
//! in `SERVICE_AND_DATA_ARCHITECTURE.md` (in the org's `.github` repository):
//!
//! - The Rust **API server** performs ALL writes to the shared schema. It
//!   connects with [`DbRole::ReadWrite`].
//! - The **web server** may read the shared schema but never write it. It
//!   connects with [`DbRole::ReadOnly`], which forces
//!   `default_transaction_read_only=on` at the Postgres session level, and
//!   calls [`assert_read_only`] at startup to verify the setting took effect.
//!   (A SELECT-only DB role is the second layer of the same defense; grants
//!   are ops work, not this crate's.)
//! - Consumers call **named query functions** from [`queries`]; the crate
//!   never hands out a raw ORM session as its public contract. Web tiers may
//!   only call functions in [`queries::read`].
//!
//! Schema access is strictly namespaced: every table lives under the
//! [`ORG_SCHEMA`] (`zed_pkg`) schema, and connections set their search path
//! accordingly.

mod connect;
pub mod queries;
mod schema;

pub use connect::{DbRole, apply_role, assert_read_only, connect};
pub use schema::{ORG_SCHEMA, qualified};

// Re-export SeaORM so consumers depend on one data layer, resolved once.
pub use sea_orm;
