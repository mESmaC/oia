//! Application boundary and local desktop workbench for Open Intelligence Agency.
//!
//! `local_tools` connects the merged document/privacy kernels to bounded local operations.
//! The optional (default-enabled) Tauri adapter draws results and collects user intents.
//! Persistent cases, evidence envelopes, durable jobs and role-based services remain in #14.

use oia_domain::SchemaVersion;

pub mod collection;

#[cfg(feature = "desktop")]
pub mod desktop;
pub(crate) mod globe;
pub mod local_tools;
pub mod news;
pub(crate) mod recreation;
pub mod recreation_composer;
pub(crate) mod witness;

/// Canonical application binary name.
pub const APP_NAME: &str = "oia";

/// Workspace crate version.
pub const WORKSPACE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Reports the implemented workbench scope without opening a window.
#[must_use]
pub fn status_banner() -> String {
    format!(
        "{APP_NAME} v{WORKSPACE_VERSION} | schema_v{} | status=local-workbench (session-only tools; cases pending #14)",
        SchemaVersion::V1.as_u32()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that the banner reports the application version and schema v1
    /// without claiming unimplemented runtime or UI capabilities.
    #[test]
    fn status_banner_reports_workbench_version_and_schema() {
        let banner = status_banner();
        assert!(banner.starts_with("oia v0.1.0 | schema_v1 |"));
        assert!(banner.contains("local-workbench"));
    }
}
