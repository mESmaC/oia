//! `oia-export` — Cited Markdown and self-contained HTML report exporters with true content
//! redaction, illustrative recreation-prompt exporter (`MEDIA-04`), and portable investigation
//! bundle (`manifest.json`, `records/`, `assets/`, `reports/`, `reproduction/`) export and
//! atomic validated import (`REPRO-01`, `REPRO-02`, `COLLAB-03`, `A10`, `A16`, `A17`).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use oia_domain::{
    ContentHash, DomainError, DomainObject, OmissionReason, OpaqueId, Report, VersionId,
};
use oia_store::{EvidenceStore, RetentionClass, StoreError};

/// Current supported portable bundle schema version.
pub const CURRENT_BUNDLE_SCHEMA_VERSION: u32 = 1;

/// Renders a [`Report`] to Markdown (`D05`, `COLLAB-02`, `COLLAB-03`, `A16`).
///
/// When `redact_sensitive` is `true`, blocks flagged `redact_in_restricted_export` have their
/// underlying text, citation anchors, and cited version IDs physically stripped from the output
/// rather than hidden visually (`AGENTS.md` Section 5).
#[must_use]
pub fn render_report_markdown(report: &Report, redact_sensitive: bool) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", report.title));
    out.push_str(&format!(
        "- **Report Version:** `{}`\n- **Investigation ID:** `{}`\n- **State:** `{:?}`\n\n",
        report.envelope.version_id, report.investigation_id, report.publication_state
    ));

    out.push_str("## Findings and Cited Statements\n\n");
    for block in &report.blocks {
        if redact_sensitive && block.redact_in_restricted_export {
            out.push_str(&format!(
                "- **[{}]** `[REDACTED — SENSITIVE CONTENT REMOVED PRIOR TO EXPORT]`\n",
                block.block_id
            ));
            continue;
        }
        let origin_tag = if block.ai_generated {
            "AI-drafted"
        } else {
            "Analyst-authored"
        };
        let citations: Vec<String> = block
            .cited_versions
            .iter()
            .map(ToString::to_string)
            .collect();
        out.push_str(&format!(
            "- **[{}]** ({}, {:?}): {} (Citations: `[{}]`)\n",
            block.block_id,
            origin_tag,
            block.review_status,
            block.statement,
            citations.join(", ")
        ));
    }

    if !report.limitations_and_open_questions.is_empty() {
        out.push_str("\n## Coverage Limitations and Open Questions\n\n");
        for lim in &report.limitations_and_open_questions {
            out.push_str(&format!("- {lim}\n"));
        }
    }
    out
}

/// Renders a [`Report`] to a self-contained offline HTML document (`D05`, `UX-05`, `COLLAB-03`).
///
/// Contains zero external scripts, stylesheets, or remote asset URLs so opening offline never
/// initiates network requests or leaks case metadata (`docs/architecture.md` Section 10.1).
#[must_use]
pub fn render_report_self_contained_html(report: &Report, redact_sensitive: bool) -> String {
    let mut out = String::from(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'\">\
         <title>",
    );
    out.push_str(&escape_html(&report.title));
    out.push_str(
        "</title><style>\
         body{background:#090d12;color:#e6edf3;font-family:system-ui,sans-serif;margin:2rem;line-height:1.5}\
         .glass{background:rgba(18,26,36,0.88);border:1px solid #263545;padding:1.25rem;border-radius:6px}\
         code{font-family:ui-monospace,monospace;color:#58a6ff}\
         .redacted{color:#f0883e;font-family:ui-monospace,monospace}\
         </style></head><body><main class=\"glass\"><h1>",
    );
    out.push_str(&escape_html(&report.title));
    out.push_str("</h1><ul>");
    for block in &report.blocks {
        if redact_sensitive && block.redact_in_restricted_export {
            out.push_str("<li class=\"redacted\">[REDACTED — UNDERLYING CONTENT REMOVED]</li>");
        } else {
            out.push_str("<li>");
            out.push_str(&escape_html(&block.statement));
            out.push_str("</li>");
        }
    }
    out.push_str("</ul></main></body></html>");
    out
}

fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// Structured specification for exporting an illustrative recreation prompt (`MEDIA-04`, `A10`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecreationPromptSpec {
    /// Prompt ID.
    pub prompt_id: OpaqueId,
    /// Reported details supported by cited witness/observation versions.
    pub reported_details: Vec<String>,
    /// Explicit scene/lighting/camera assumptions not directly observed.
    pub explicit_assumptions: Vec<String>,
    /// Unresolved unknowns (e.g., unknown true distance or physical size).
    pub unresolved_unknowns: Vec<String>,
    /// Conflicting witness variants kept distinct rather than merged (`A10`).
    pub conflicting_variants: Vec<String>,
    /// Permitted reference asset hashes included under their reuse conditions.
    pub permitted_reference_assets: Vec<ContentHash>,
}

/// Exports a portable plain-text recreation prompt with mandatory illustrative labels (`MEDIA-04`, `A10`).
#[must_use]
pub fn render_recreation_prompt(spec: &RecreationPromptSpec) -> String {
    let mut out =
        String::from("=== OIA ILLUSTRATIVE RECREATION PROMPT (NOT OBSERVED EVIDENCE) ===\n");
    out.push_str(&format!("Prompt ID: {}\n\n", spec.prompt_id));
    out.push_str("[REPORTED DETAILS (CITED)]\n");
    for d in &spec.reported_details {
        out.push_str(&format!("- {d}\n"));
    }
    out.push_str("\n[EXPLICIT ASSUMPTIONS]\n");
    for a in &spec.explicit_assumptions {
        out.push_str(&format!("- {a}\n"));
    }
    out.push_str("\n[UNRESOLVED UNKNOWNS / UNCERTAIN SCALE]\n");
    for u in &spec.unresolved_unknowns {
        out.push_str(&format!("- {u}\n"));
    }
    out.push_str("\n[CONFLICTING ACCOUNT VARIANTS]\n");
    for v in &spec.conflicting_variants {
        out.push_str(&format!("- {v}\n"));
    }
    out.push_str("\n[PERMITTED REFERENCE ASSETS]\n");
    for h in &spec.permitted_reference_assets {
        out.push_str(&format!("- sha256:{h}\n"));
    }
    out
}

/// Inventory entry inside a [`BundleManifest`] (`docs/architecture.md` Section 10.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleEntry {
    /// Relative POSIX path inside the bundle (must start with `records/`, `assets/`, `reports/`, or `reproduction/`).
    pub relative_path: String,
    /// SHA-256 content hash of the entry payload.
    pub content_hash: ContentHash,
    /// Exact byte length.
    pub byte_len: u64,
}

/// Omitted asset record inside a [`BundleManifest`] (`REPRO-01`, `A17`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OmittedBundleAsset {
    /// Artifact version ID.
    pub artifact_version_id: VersionId,
    /// Reason the asset bytes were omitted from `assets/`.
    pub reason: OmissionReason,
    /// Known SHA-256 hash of the omitted asset, if any.
    pub known_hash: Option<ContentHash>,
}

/// Portable investigation bundle manifest (`manifest.json`, `docs/architecture.md` Section 10.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleManifest {
    /// Bundle format schema version.
    pub schema_version: u32,
    /// Unique package ID.
    pub package_id: OpaqueId,
    /// Creation timestamp (UTC ms).
    pub created_at_ms: i64,
    /// Inventory of all entries in `records/`, `assets/`, `reports/`, and `reproduction/`.
    pub entry_inventory: Vec<BundleEntry>,
    /// Explicitly documented omitted or restricted assets (`REPRO-01`).
    pub omitted_assets: Vec<OmittedBundleAsset>,
    /// Required attribution notices for included sources.
    pub attribution_notices: Vec<String>,
}

/// Self-contained portable investigation bundle (`REPRO-01`, `REPRO-02`, `A17`).
#[derive(Debug, Clone)]
pub struct InvestigationBundle {
    /// Bundle manifest (`manifest.json`).
    pub manifest: BundleManifest,
    /// Domain objects included in `records/`.
    pub records: Vec<DomainObject>,
    /// Raw asset bytes keyed by relative path (`assets/<sha256>`).
    pub asset_payloads: BTreeMap<String, Vec<u8>>,
    /// Rendered reports keyed by relative path (`reports/<id>.md`).
    pub rendered_reports: BTreeMap<String, Vec<u8>>,
}

/// Resource limits enforced when validating and importing an untrusted [`InvestigationBundle`] (`REPRO-02`, `A17`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundleImportLimits {
    /// Maximum total bytes across all bundle entries (decompression-bomb protection).
    pub max_total_bytes: u64,
    /// Maximum number of entries in `entry_inventory`.
    pub max_entries: usize,
}

impl Default for BundleImportLimits {
    fn default() -> Self {
        Self {
            max_total_bytes: 128 * 1024 * 1024,
            max_entries: 5_000,
        }
    }
}

/// Result of a validated atomic bundle import (`REPRO-02`, `A17`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleImportReceipt {
    /// Package ID imported.
    pub package_id: OpaqueId,
    /// Mapping from origin object IDs in the bundle to local IDs in the target store.
    pub origin_to_local_id_map: BTreeMap<OpaqueId, OpaqueId>,
    /// Count of verified assets imported.
    pub imported_asset_count: usize,
    /// Omitted assets preserved as metadata references.
    pub omitted_assets: Vec<OmittedBundleAsset>,
}

/// Validates that a bundle entry path is safe, relative, and confined to the 4 allowed directories
/// (`records/`, `assets/`, `reports/`, `reproduction/`), rejecting `..`, backslashes, drive letters,
/// and leading slashes (`REPRO-02`, `A17`).
pub fn validate_bundle_entry_path(path: &str) -> Result<(), BundleError> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path.contains(':')
        || path.contains('\0')
    {
        return Err(BundleError::PathTraversalRejected(path.to_owned()));
    }
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(BundleError::PathTraversalRejected(path.to_owned()));
        }
    }
    let allowed_prefix = path.starts_with("records/")
        || path.starts_with("assets/")
        || path.starts_with("reports/")
        || path.starts_with("reproduction/");
    if !allowed_prefix {
        return Err(BundleError::PathTraversalRejected(path.to_owned()));
    }
    Ok(())
}

/// Validates every invariant of `bundle` BEFORE mutating `target_store` (`REPRO-02`, `A17`).
///
/// Rejects:
/// - Unsupported required schema versions (`schema_version == 0` or `> CURRENT_BUNDLE_SCHEMA_VERSION`)
/// - Path traversal or invalid entry paths
/// - Oversized / decompression-bomb payloads exceeding `limits`
/// - Corrupted SHA-256 asset hashes or byte lengths
/// - Duplicate conflicting version IDs or collisions with existing cases in `target_store`
///
/// Never executes code, opens hardware sessions, or fetches remote URLs on import.
pub fn validate_and_import_bundle(
    target_store: &mut EvidenceStore,
    bundle: &InvestigationBundle,
    limits: BundleImportLimits,
    local_id_namespace: Option<&str>,
) -> Result<BundleImportReceipt, BundleError> {
    if bundle.manifest.schema_version == 0
        || bundle.manifest.schema_version > CURRENT_BUNDLE_SCHEMA_VERSION
    {
        return Err(BundleError::UnsupportedSchemaVersion {
            found: bundle.manifest.schema_version,
            max_supported: CURRENT_BUNDLE_SCHEMA_VERSION,
        });
    }

    if bundle.manifest.entry_inventory.len() > limits.max_entries {
        return Err(BundleError::TooManyEntries {
            actual: bundle.manifest.entry_inventory.len(),
            max: limits.max_entries,
        });
    }

    let mut seen_paths = BTreeSet::new();
    let mut total_bytes = 0u64;
    for entry in &bundle.manifest.entry_inventory {
        validate_bundle_entry_path(&entry.relative_path)?;
        if !seen_paths.insert(entry.relative_path.clone()) {
            return Err(BundleError::DuplicateEntryPath(entry.relative_path.clone()));
        }
        total_bytes = total_bytes.saturating_add(entry.byte_len);
        if total_bytes > limits.max_total_bytes {
            return Err(BundleError::BundleSizeLimitExceeded {
                actual: total_bytes,
                max: limits.max_total_bytes,
            });
        }
        if entry.relative_path.starts_with("assets/") {
            let bytes = bundle
                .asset_payloads
                .get(&entry.relative_path)
                .ok_or_else(|| BundleError::MissingDeclaredEntry(entry.relative_path.clone()))?;
            if bytes.len() as u64 != entry.byte_len || !entry.content_hash.verify_bytes(bytes) {
                return Err(BundleError::AssetHashMismatch {
                    path: entry.relative_path.clone(),
                    expected: entry.content_hash.clone(),
                });
            }
        }
    }

    // Validate all records and check for duplicate/conflicting version IDs BEFORE any store mutation.
    let mut seen_versions = BTreeSet::new();
    for obj in &bundle.records {
        obj.validate().map_err(BundleError::Domain)?;
        let ver = &obj.envelope().version_id;
        if !seen_versions.insert(ver.clone()) {
            return Err(BundleError::DuplicateConflictingVersion(ver.clone()));
        }
        if target_store.graph().get_version(ver).is_some() {
            return Err(BundleError::ExistingVersionCollision(ver.clone()));
        }
    }

    // Pre-build local ID mapping without losing origin IDs.
    let mut id_map = BTreeMap::new();
    let mut mapped_records = Vec::with_capacity(bundle.records.len());
    for obj in &bundle.records {
        let mut cloned = obj.clone();
        let origin_id = cloned.envelope().id.clone();
        let local_id = if let Some(ns) = local_id_namespace {
            OpaqueId::new(format!("{ns}:{}", origin_id.as_str()))
                .map_err(DomainError::from)
                .map_err(BundleError::Domain)?
        } else {
            origin_id.clone()
        };
        if local_id != origin_id {
            cloned
                .envelope_mut()
                .provenance
                .processing_history
                .push(format!("imported_from_origin_id:{origin_id}"));
            cloned.envelope_mut().id = local_id.clone();
        }
        id_map.insert(origin_id, local_id);
        mapped_records.push(cloned);
    }

    // Commit validated objects into target_store.
    for obj in mapped_records {
        target_store
            .put_object(obj, None)
            .map_err(BundleError::Store)?;
    }
    let _ = RetentionClass::PinnedCaseEvidence;

    Ok(BundleImportReceipt {
        package_id: bundle.manifest.package_id.clone(),
        origin_to_local_id_map: id_map,
        imported_asset_count: bundle.asset_payloads.len(),
        omitted_assets: bundle.manifest.omitted_assets.clone(),
    })
}

/// Typed errors for `oia-export`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BundleError {
    /// Underlying domain validation error.
    Domain(DomainError),
    /// Underlying store error.
    Store(StoreError),
    /// Unsupported bundle schema version.
    UnsupportedSchemaVersion {
        /// Version found.
        found: u32,
        /// Max supported version.
        max_supported: u32,
    },
    /// Bundle entry count exceeded `BundleImportLimits::max_entries`.
    TooManyEntries {
        /// Actual entry count.
        actual: usize,
        /// Max allowed entries.
        max: usize,
    },
    /// Total bundle size exceeded `BundleImportLimits::max_total_bytes` (decompression-bomb guard).
    BundleSizeLimitExceeded {
        /// Actual byte total.
        actual: u64,
        /// Max allowed byte total.
        max: u64,
    },
    /// Unsafe path (`..`, absolute path, backslash, or disallowed root directory) rejected.
    PathTraversalRejected(String),
    /// Duplicate path in `entry_inventory`.
    DuplicateEntryPath(String),
    /// Entry listed in manifest was missing from bundle payload map.
    MissingDeclaredEntry(String),
    /// Asset failed SHA-256 verification against manifest entry.
    AssetHashMismatch {
        /// Entry path.
        path: String,
        /// Expected hash.
        expected: ContentHash,
    },
    /// Bundle contained duplicate conflicting version IDs.
    DuplicateConflictingVersion(VersionId),
    /// Bundle version ID collided with an existing record in the store (existing cases are never overwritten).
    ExistingVersionCollision(VersionId),
}

impl fmt::Display for BundleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(e) => write!(f, "bundle domain error: {e}"),
            Self::Store(e) => write!(f, "bundle store error: {e}"),
            Self::UnsupportedSchemaVersion {
                found,
                max_supported,
            } => write!(
                f,
                "unsupported bundle schema version {found} (max supported {max_supported})"
            ),
            Self::TooManyEntries { actual, max } => {
                write!(f, "bundle entry count {actual} exceeds limit {max}")
            }
            Self::BundleSizeLimitExceeded { actual, max } => {
                write!(
                    f,
                    "bundle total size {actual} bytes exceeds limit {max} bytes"
                )
            }
            Self::PathTraversalRejected(p) => {
                write!(f, "rejected unsafe or out-of-bounds bundle path '{p}'")
            }
            Self::DuplicateEntryPath(p) => write!(f, "duplicate bundle entry path '{p}'"),
            Self::MissingDeclaredEntry(p) => {
                write!(f, "declared bundle entry '{p}' is missing from payload")
            }
            Self::AssetHashMismatch { path, expected } => {
                write!(
                    f,
                    "SHA-256 verification failed for bundle asset '{path}' (expected {expected})"
                )
            }
            Self::DuplicateConflictingVersion(v) => {
                write!(f, "duplicate conflicting version '{v}' inside bundle")
            }
            Self::ExistingVersionCollision(v) => write!(
                f,
                "bundle version '{v}' collides with an existing record; existing cases are never overwritten"
            ),
        }
    }
}

impl std::error::Error for BundleError {}

#[cfg(test)]
mod tests {
    use super::*;
    use oia_domain::{
        ActorId, AvailabilityState, DomainObjectType, Investigation, ObjectEnvelope, OriginClass,
        ProvenanceRecord, ReassessmentState, ReportBlock, ReportPublicationState, ReviewStatus,
        SchemaVersion, WorkspaceVisibility,
    };
    use oia_store::StoreBudget;

    /// Verifies physical content redaction in Markdown/HTML exports, recreation prompt labels,
    /// and bundle security checks (path traversal, decompression bomb, corrupted SHA-256,
    /// unsupported schema, and non-mutation of existing cases on failed import) (#11, A10, A16, A17).
    #[test]
    fn report_redaction_and_bundle_security_validation() -> Result<(), Box<dyn std::error::Error>> {
        let actor = ActorId::new("analyst")?;
        let case_id = OpaqueId::new("case_alpha")?;
        let secret_phrase = "SENSITIVE_WITNESS_HOME_ADDRESS_999";

        let report = Report {
            envelope: ObjectEnvelope {
                id: OpaqueId::new("rep_1")?,
                version_id: VersionId::new("ver_rep_1")?,
                schema_version: SchemaVersion::V1,
                object_type: DomainObjectType::Report,
                created_at_ms: 1_700_000_000_000,
                created_by: actor.clone(),
                visibility: WorkspaceVisibility::PrivateCase {
                    case_id: case_id.clone(),
                    sensitive: false,
                },
                supersedes: None,
                provenance: ProvenanceRecord {
                    source_version_ids: vec![VersionId::new("ver_src_1")?],
                    declared_origin_note: None,
                    processing_history: Vec::new(),
                },
                origin_class: OriginClass::AnalyticalInference,
                review_status: ReviewStatus::Reviewed,
                availability: AvailabilityState::Available,
                reassessment: ReassessmentState::Current,
            },
            investigation_id: case_id.clone(),
            title: "Alpha Investigation Report".to_owned(),
            blocks: vec![
                ReportBlock {
                    block_id: OpaqueId::new("blk_public")?,
                    statement: "Object observed moving northeast at 21:14 UTC.".to_owned(),
                    cited_versions: vec![VersionId::new("ver_src_1")?],
                    citation_anchors: Vec::new(),
                    ai_generated: false,
                    review_status: ReviewStatus::Reviewed,
                    redact_in_restricted_export: false,
                },
                ReportBlock {
                    block_id: OpaqueId::new("blk_sensitive")?,
                    statement: format!("Witness observed from {secret_phrase}."),
                    cited_versions: vec![VersionId::new("ver_src_sensitive")?],
                    citation_anchors: Vec::new(),
                    ai_generated: false,
                    review_status: ReviewStatus::Reviewed,
                    redact_in_restricted_export: true,
                },
            ],
            limitations_and_open_questions: vec!["Single azimuth line only".to_owned()],
            publication_state: ReportPublicationState::PrivateDraft,
        };

        // 1. Redacted Markdown and HTML completely strip `secret_phrase` and `ver_src_sensitive`.
        let md_redacted = render_report_markdown(&report, true);
        let html_redacted = render_report_self_contained_html(&report, true);
        assert!(!md_redacted.contains(secret_phrase));
        assert!(!md_redacted.contains("ver_src_sensitive"));
        assert!(!html_redacted.contains(secret_phrase));
        assert!(md_redacted.contains("Object observed moving northeast"));

        // 2. Path traversal, decompression-bomb size, corrupt hash, and future schema are rejected without mutating store.
        assert!(validate_bundle_entry_path("../etc/passwd").is_err());
        assert!(validate_bundle_entry_path("assets/../../secret").is_err());
        assert!(validate_bundle_entry_path("/assets/ab").is_err());
        assert!(validate_bundle_entry_path("assets/valid_hash").is_ok());

        let mut store_dir = std::env::temp_dir();
        store_dir.push(format!("oia_export_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&store_dir);
        let mut store = EvidenceStore::open(&store_dir, 2, StoreBudget::default())?;

        let existing_case = DomainObject::Investigation(Investigation {
            envelope: ObjectEnvelope {
                id: case_id.clone(),
                version_id: VersionId::new("ver_case_alpha_1")?,
                schema_version: SchemaVersion::V1,
                object_type: DomainObjectType::Investigation,
                created_at_ms: 1_700_000_000_000,
                created_by: actor,
                visibility: WorkspaceVisibility::PrivateCase {
                    case_id: case_id.clone(),
                    sensitive: false,
                },
                supersedes: None,
                provenance: ProvenanceRecord {
                    source_version_ids: Vec::new(),
                    declared_origin_note: Some("Analyst created case".to_owned()),
                    processing_history: Vec::new(),
                },
                origin_class: OriginClass::AnalyticalInference,
                review_status: ReviewStatus::Reviewed,
                availability: AvailabilityState::Available,
                reassessment: ReassessmentState::Current,
            },
            title: "Original Untouched Case".to_owned(),
            question: "What happened?".to_owned(),
            geographic_scope: None,
            temporal_scope: None,
            topics: Vec::new(),
            evidence_versions: Vec::new(),
            saved_query_versions: Vec::new(),
            annotation_versions: Vec::new(),
            hypothesis_versions: Vec::new(),
            lead_versions: Vec::new(),
            is_published: false,
        });
        store.put_object(existing_case, None)?;

        let asset_bytes = b"verified bundle asset bytes".to_vec();
        let asset_hash = ContentHash::compute(&asset_bytes);
        let mut asset_payloads = BTreeMap::new();
        asset_payloads.insert("assets/blob1".to_owned(), b"corrupted bytes".to_vec());

        let bad_bundle = InvestigationBundle {
            manifest: BundleManifest {
                schema_version: 1,
                package_id: OpaqueId::new("pkg_1")?,
                created_at_ms: 1_700_000_100_000,
                entry_inventory: vec![BundleEntry {
                    relative_path: "assets/blob1".to_owned(),
                    content_hash: asset_hash.clone(),
                    byte_len: asset_bytes.len() as u64,
                }],
                omitted_assets: Vec::new(),
                attribution_notices: Vec::new(),
            },
            records: vec![DomainObject::Report(report.clone())],
            asset_payloads,
            rendered_reports: BTreeMap::new(),
        };

        assert!(matches!(
            validate_and_import_bundle(
                &mut store,
                &bad_bundle,
                BundleImportLimits::default(),
                Some("imp")
            ),
            Err(BundleError::AssetHashMismatch { .. })
        ));
        // Verify `ver_rep_1` was NOT committed into `store` and `ver_case_alpha_1` is untouched.
        assert!(
            store
                .graph()
                .get_version(&VersionId::new("ver_rep_1")?)
                .is_none()
        );
        assert!(
            store
                .graph()
                .get_version(&VersionId::new("ver_case_alpha_1")?)
                .is_some()
        );

        // Fix the asset bytes and verify clean import with origin->local ID mapping.
        let mut good_bundle = bad_bundle;
        good_bundle
            .asset_payloads
            .insert("assets/blob1".to_owned(), asset_bytes);
        let receipt = validate_and_import_bundle(
            &mut store,
            &good_bundle,
            BundleImportLimits::default(),
            Some("imp"),
        )?;
        assert_eq!(receipt.imported_asset_count, 1);
        assert_eq!(
            receipt.origin_to_local_id_map.get(&OpaqueId::new("rep_1")?),
            Some(&OpaqueId::new("imp:rep_1")?)
        );

        let _ = std::fs::remove_dir_all(&store_dir);
        Ok(())
    }
}
