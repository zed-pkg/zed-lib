//! The org's shared-schema namespace.
//!
//! Shared definitions come from `oresoftware/k8s-libs-and-shared-defs`
//! (imported as a zed package via `.zpkg.toml`); the schema is strictly
//! namespaced per org, and for zed-pkg that namespace is [`ORG_SCHEMA`].

/// The Postgres schema every shared table lives under.
pub const ORG_SCHEMA: &str = "zed_pkg";

/// Return `"<ORG_SCHEMA>.<table>"` for a bare table name.
///
/// # Panics
///
/// Panics if `table` is empty, contains whitespace, or contains a quote
/// character — a table name is an identifier chosen by this org's code, never
/// runtime input, so a bad one is a programming error and fails loudly.
pub fn qualified(table: &str) -> String {
    assert!(!table.is_empty(), "table name must not be empty");
    assert!(
        !table.chars().any(char::is_whitespace),
        "table name must not contain whitespace: {table:?}"
    );
    assert!(
        !table.contains(['"', '\'', '`']),
        "table name must not contain quote characters: {table:?}"
    );
    format!("{ORG_SCHEMA}.{table}")
}

#[cfg(test)]
mod tests {
    use super::{ORG_SCHEMA, qualified};

    #[test]
    fn qualifies_with_org_schema() {
        assert_eq!(qualified("packages"), "zed_pkg.packages");
        assert!(qualified("packages").starts_with(ORG_SCHEMA));
    }

    #[test]
    #[should_panic(expected = "must not be empty")]
    fn rejects_empty() {
        qualified("");
    }

    #[test]
    #[should_panic(expected = "whitespace")]
    fn rejects_whitespace() {
        qualified("pack ages");
    }

    #[test]
    #[should_panic(expected = "quote")]
    fn rejects_quotes() {
        qualified("packages\";drop_table_users;--");
    }
}
