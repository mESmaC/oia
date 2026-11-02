//! `oia-app` — Application boundary crate for Open Intelligence Agency (`#9`).
//!
//! In `#9`, this crate establishes the application boundary above `oia-domain` and provides the
//! bootstrap `oia` CLI version and workspace status banner. Durable jobs, command/query execution,
//! role-based case services, and workspace shell / renderer integration follow in `#14` and `#16`
//! once `D01` (`docs/adr/0001-d01-deployment-and-ui-boundary.md`,
//! `docs/adr/0008-d01-workspace-renderer.md`) is decided.

use oia_domain::SchemaVersion;

/// Canonical application binary name.
pub const APP_NAME: &str = "oia";

/// Workspace crate version.
pub const WORKSPACE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Returns the deterministic `#9` workspace bootstrap status banner.
#[must_use]
pub fn status_banner() -> String {
    format!(
        "{APP_NAME} v{WORKSPACE_VERSION} | schema_v{} | status=workspace-bootstrap (#9; D01 renderer pending #16)",
        SchemaVersion::V1.as_u32()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that the `#9` bootstrap banner reports the application version and schema v1
    /// without claiming unimplemented runtime or UI capabilities.
    #[test]
    fn status_banner_reports_bootstrap_version_and_schema() {
        let banner = status_banner();
        assert!(banner.starts_with("oia v0.1.0 | schema_v1 |"));
        assert!(banner.contains("workspace-bootstrap"));
    }
}
