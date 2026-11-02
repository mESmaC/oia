//! `oia-domain` — Core domain identifiers and schema version primitives for Open Intelligence Agency (`#9`).
//!
//! This crate has zero dependencies on UI toolkits, HTTP clients, database engines, model
//! services, or vendor SDKs (`docs/architecture.md` Section 1.2). Versioned evidence records,
//! provenance graphs, and content hashes follow in `#10`.

pub mod ids;

pub use ids::{ActorId, IdError, MAX_ID_LEN, OpaqueId, SchemaVersion, VersionId};

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies bounded identifier and schema version validation (`docs/architecture.md` §2.1, `#9`).
    #[test]
    fn identifier_and_schema_version_contracts() -> Result<(), IdError> {
        let case_id = OpaqueId::new("case:phoenix-1997_01")?;
        assert_eq!(case_id.as_str(), "case:phoenix-1997_01");
        assert_eq!(Err(IdError::Empty), OpaqueId::new(""));
        assert_eq!(
            Err(IdError::TooLong(MAX_ID_LEN + 1)),
            OpaqueId::new("a".repeat(MAX_ID_LEN + 1))
        );
        assert_eq!(
            Err(IdError::InvalidCharacter('/')),
            OpaqueId::new("case/../traversal")
        );
        assert_eq!(Err(IdError::InvalidCharacter('.')), OpaqueId::new(".."));
        assert_eq!(SchemaVersion::V1.as_u32(), 1);
        assert_eq!(Err(IdError::ZeroSchemaVersion), SchemaVersion::new(0));
        Ok(())
    }
}
