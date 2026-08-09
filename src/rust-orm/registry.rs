//! Transport-neutral registry operations for API, web, CLI, and workers.
//!
//! Callers verify JWT/session material before constructing a federated identity.
//! These modules map verified principals to registry users and enforce product
//! authorization; they never validate passwords or own browser sessions.

mod artifacts;
mod identity;
mod packages;
mod search;
mod support;

pub use artifacts::{
    PackageDownloadInput, PackageUploadInput, record_package_download, register_package_upload,
};
pub use identity::{FederatedIdentity, ensure_federated_user};
pub use packages::{
    CreatePackageInput, PackageLicenseInput, add_package_license, create_package,
    make_package_public,
};
pub use search::{
    EmbeddingInput, RegistrySearchHit, search_registry, semantic_search, upsert_embedding,
};
