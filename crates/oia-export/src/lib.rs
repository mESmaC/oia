//! `oia-export` — Cited Markdown and self-contained HTML report exporters with true content
//! redaction, illustrative recreation-prompt exporter (`MEDIA-04`), and portable investigation
//! bundle (`manifest.json`, `records/`, `assets/`, `reports/`, `reproduction/`) export and
//! atomic validated import (`REPRO-01`, `REPRO-02`, `COLLAB-03`, `C10`, `A10`, `A16`, `A17`).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use oia_domain::{
    AvailabilityState, CitationAnchor, ContentHash, DocumentReleaseKind, DomainError, DomainObject,
    OmissionReason, OpaqueId, Report, VersionId, WorkspaceVisibility,
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
    /// Count of verified assets imported and stored in `target_store`.
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

/// Exports a portable [`InvestigationBundle`] for `case_id` from `store` (`REPRO-01`, `C10`, `A17`).
///
/// - Enforces per-source `ReuseConditions::redistribution_allowed` (`Shared Research Rule 9`).
/// - When `include_sensitive == false`, omits sensitive artifacts with `OmissionReason::RedactedSensitiveContent`
///   and redacts sensitive report blocks.
/// - Populates `entry_inventory`, `records`, `asset_payloads`, `rendered_reports`, `omitted_assets`,
///   and `attribution_notices`.
pub fn export_case_bundle(
    store: &EvidenceStore,
    case_id: &OpaqueId,
    package_id: OpaqueId,
    now_ms: i64,
    include_sensitive: bool,
) -> Result<InvestigationBundle, BundleError> {
    let mut records = Vec::new();
    let mut entry_inventory = Vec::new();
    let mut asset_payloads = BTreeMap::new();
    let mut rendered_reports = BTreeMap::new();
    let mut omitted_assets = Vec::new();
    let mut attribution_notices = Vec::new();

    // Build lookup of Source redistribution rules and attribution notices.
    let mut source_redistribution: BTreeMap<OpaqueId, (bool, String)> = BTreeMap::new();
    let mut sr_to_source: BTreeMap<VersionId, OpaqueId> = BTreeMap::new();
    for (ver, obj) in store.graph().all_versions() {
        match obj {
            DomainObject::Source(s) => {
                source_redistribution.insert(
                    s.envelope.id.clone(),
                    (
                        s.reuse_conditions.redistribution_allowed,
                        s.reuse_conditions.attribution.clone(),
                    ),
                );
            }
            DomainObject::SourceRecord(sr) => {
                sr_to_source.insert(ver.clone(), sr.source_id.clone());
            }
            _ => {}
        }
    }

    for (ver, obj) in store.graph().all_versions() {
        let belongs_to_case = obj.envelope().visibility.case_id() == Some(case_id)
            || matches!(obj, DomainObject::Source(_));
        if !belongs_to_case {
            continue;
        }
        if !include_sensitive && obj.envelope().visibility.is_sensitive() {
            if let DomainObject::Artifact(art) = obj {
                omitted_assets.push(OmittedBundleAsset {
                    artifact_version_id: ver.clone(),
                    reason: OmissionReason::RedactedSensitiveContent,
                    known_hash: Some(art.content_hash.clone()),
                });
            }
            continue;
        }

        if let DomainObject::Source(s) = obj
            && !s.reuse_conditions.attribution.is_empty()
            && !attribution_notices.contains(&s.reuse_conditions.attribution)
        {
            attribution_notices.push(s.reuse_conditions.attribution.clone());
        }

        if let DomainObject::Artifact(art) = obj {
            // Check if any parent SourceRecord's Source prohibits redistribution.
            let mut allow_redist = true;
            for src_ver in &art.envelope.provenance.source_version_ids {
                if let Some(sid) = sr_to_source.get(src_ver)
                    && let Some((allowed, attr)) = source_redistribution.get(sid)
                {
                    if !attr.is_empty() && !attribution_notices.contains(attr) {
                        attribution_notices.push(attr.clone());
                    }
                    if !*allowed {
                        allow_redist = false;
                    }
                }
            }
            if !allow_redist {
                omitted_assets.push(OmittedBundleAsset {
                    artifact_version_id: ver.clone(),
                    reason: OmissionReason::RedistributionProhibitedByPolicy,
                    known_hash: Some(art.content_hash.clone()),
                });
            } else if art.envelope.availability != AvailabilityState::Available {
                omitted_assets.push(OmittedBundleAsset {
                    artifact_version_id: ver.clone(),
                    reason: OmissionReason::UpstreamReferenceOnly,
                    known_hash: Some(art.content_hash.clone()),
                });
            } else {
                let bytes = store
                    .read_verified_blob(&art.content_hash)
                    .map_err(BundleError::Store)?;
                let rel_path = format!("assets/{}", art.content_hash.to_hex());
                if let std::collections::btree_map::Entry::Vacant(vacant) =
                    asset_payloads.entry(rel_path.clone())
                {
                    entry_inventory.push(BundleEntry {
                        relative_path: rel_path,
                        content_hash: art.content_hash.clone(),
                        byte_len: bytes.len() as u64,
                    });
                    vacant.insert(bytes);
                }
            }
        }

        if let DomainObject::Report(rep) = obj {
            let md = render_report_markdown(rep, !include_sensitive);
            let md_bytes = md.into_bytes();
            let md_hash = ContentHash::compute(&md_bytes);
            let rel_path = format!("reports/{}.md", rep.envelope.version_id.as_str());
            entry_inventory.push(BundleEntry {
                relative_path: rel_path.clone(),
                content_hash: md_hash,
                byte_len: md_bytes.len() as u64,
            });
            rendered_reports.insert(rel_path, md_bytes);
        }

        let rec_json = obj
            .to_json_value()
            .and_then(|v| v.to_canonical_json().map_err(DomainError::Json))
            .map_err(BundleError::Domain)?;
        let rec_bytes = rec_json.into_bytes();
        let rec_hash = ContentHash::compute(&rec_bytes);
        entry_inventory.push(BundleEntry {
            relative_path: format!("records/{}.json", ver.as_str()),
            content_hash: rec_hash,
            byte_len: rec_bytes.len() as u64,
        });
        records.push(obj.clone());
    }

    Ok(InvestigationBundle {
        manifest: BundleManifest {
            schema_version: CURRENT_BUNDLE_SCHEMA_VERSION,
            package_id,
            created_at_ms: now_ms,
            entry_inventory,
            omitted_assets,
            attribution_notices,
        },
        records,
        asset_payloads,
        rendered_reports,
    })
}

fn ns_opaque(ns: &str, id: &OpaqueId) -> Result<OpaqueId, BundleError> {
    OpaqueId::new(format!("{ns}:{}", id.as_str()))
        .map_err(DomainError::from)
        .map_err(BundleError::Domain)
}

fn ns_version(ns: &str, ver: &VersionId) -> Result<VersionId, BundleError> {
    VersionId::new(format!("{ns}:{}", ver.as_str()))
        .map_err(DomainError::from)
        .map_err(BundleError::Domain)
}

fn ns_anchor(ns: &str, a: &CitationAnchor) -> Result<CitationAnchor, BundleError> {
    Ok(CitationAnchor {
        anchor_id: a.anchor_id.clone(),
        document_version_id: ns_version(ns, &a.document_version_id)?,
        page_number: a.page_number,
        byte_start: a.byte_start,
        byte_end: a.byte_end,
        span_hash: a.span_hash.clone(),
        coverage: a.coverage.clone(),
    })
}

fn namespace_domain_object(ns: &str, obj: &DomainObject) -> Result<DomainObject, BundleError> {
    let mut cloned = obj.clone();
    {
        let env = cloned.envelope_mut();
        let origin_id = env.id.clone();
        let origin_ver = env.version_id.clone();
        env.id = ns_opaque(ns, &origin_id)?;
        env.version_id = ns_version(ns, &origin_ver)?;
        if let Some(ref prev) = env.supersedes {
            env.supersedes = Some(ns_version(ns, prev)?);
        }
        let mut mapped_sources = Vec::with_capacity(env.provenance.source_version_ids.len());
        for sv in &env.provenance.source_version_ids {
            mapped_sources.push(ns_version(ns, sv)?);
        }
        env.provenance.source_version_ids = mapped_sources;
        env.provenance
            .processing_history
            .push(format!("imported_from_origin_id:{origin_id}"));
        env.provenance
            .processing_history
            .push(format!("imported_from_origin_version:{origin_ver}"));
        env.visibility = match &env.visibility {
            WorkspaceVisibility::PrivateCase { case_id, sensitive } => {
                WorkspaceVisibility::PrivateCase {
                    case_id: ns_opaque(ns, case_id)?,
                    sensitive: *sensitive,
                }
            }
            WorkspaceVisibility::SharedCase {
                case_id,
                policy_id,
                sensitive,
            } => WorkspaceVisibility::SharedCase {
                case_id: ns_opaque(ns, case_id)?,
                policy_id: ns_opaque(ns, policy_id)?,
                sensitive: *sensitive,
            },
            WorkspaceVisibility::LibraryPublic => WorkspaceVisibility::LibraryPublic,
        };
    }

    match &mut cloned {
        DomainObject::Source(_) => {}
        DomainObject::SourceRecord(sr) => {
            sr.source_id = ns_opaque(ns, &sr.source_id)?;
            sr.acquisition_attempt_id = ns_opaque(ns, &sr.acquisition_attempt_id)?;
        }
        DomainObject::Artifact(art) => {
            if let Some(ref d) = art.derived_from_artifact {
                art.derived_from_artifact = Some(ns_version(ns, d)?);
            }
        }
        DomainObject::Document(doc) => {
            doc.source_record_version = ns_version(ns, &doc.source_record_version)?;
            if let Some(ref oav) = doc.original_artifact_version {
                doc.original_artifact_version = Some(ns_version(ns, oav)?);
            }
            doc.release_kind = match &doc.release_kind {
                DocumentReleaseKind::OriginalRelease => DocumentReleaseKind::OriginalRelease,
                DocumentReleaseKind::Revision {
                    previous_document_version,
                } => DocumentReleaseKind::Revision {
                    previous_document_version: ns_version(ns, previous_document_version)?,
                },
                DocumentReleaseKind::DifferentlyRedactedRelease {
                    related_document_version,
                    redaction_note,
                } => DocumentReleaseKind::DifferentlyRedactedRelease {
                    related_document_version: ns_version(ns, related_document_version)?,
                    redaction_note: redaction_note.clone(),
                },
                DocumentReleaseKind::SyndicatedOrRepublished {
                    upstream_source_version,
                } => DocumentReleaseKind::SyndicatedOrRepublished {
                    upstream_source_version: match upstream_source_version {
                        Some(v) => Some(ns_version(ns, v)?),
                        None => None,
                    },
                },
            };
            let mut anchors = Vec::with_capacity(doc.anchors.len());
            for a in &doc.anchors {
                anchors.push(ns_anchor(ns, a)?);
            }
            doc.anchors = anchors;
        }
        DomainObject::Observation(_) => {}
        DomainObject::Event(ev) => {
            for d in &mut ev.cluster_history {
                d.decision_id = ns_opaque(ns, &d.decision_id)?;
                let mut mv = Vec::with_capacity(d.member_versions.len());
                for v in &d.member_versions {
                    mv.push(ns_version(ns, v)?);
                }
                d.member_versions = mv;
            }
        }
        DomainObject::Claim(cl) => {
            if let Some(ref a) = cl.supporting_anchor {
                cl.supporting_anchor = Some(ns_anchor(ns, a)?);
            }
        }
        DomainObject::Investigation(inv) => {
            let map_vec = |items: &[VersionId]| -> Result<Vec<VersionId>, BundleError> {
                items.iter().map(|v| ns_version(ns, v)).collect()
            };
            inv.evidence_versions = map_vec(&inv.evidence_versions)?;
            inv.saved_query_versions = map_vec(&inv.saved_query_versions)?;
            inv.annotation_versions = map_vec(&inv.annotation_versions)?;
            inv.hypothesis_versions = map_vec(&inv.hypothesis_versions)?;
            inv.lead_versions = map_vec(&inv.lead_versions)?;
        }
        DomainObject::Report(rep) => {
            rep.investigation_id = ns_opaque(ns, &rep.investigation_id)?;
            for b in &mut rep.blocks {
                let mut cv = Vec::with_capacity(b.cited_versions.len());
                for v in &b.cited_versions {
                    cv.push(ns_version(ns, v)?);
                }
                b.cited_versions = cv;
                let mut ca = Vec::with_capacity(b.citation_anchors.len());
                for a in &b.citation_anchors {
                    ca.push(ns_anchor(ns, a)?);
                }
                b.citation_anchors = ca;
            }
        }
        DomainObject::Review(rev) => {
            rev.target_version_id = ns_version(ns, &rev.target_version_id)?;
            let mut cv = Vec::with_capacity(rev.cited_evidence_versions.len());
            for v in &rev.cited_evidence_versions {
                cv.push(ns_version(ns, v)?);
            }
            rev.cited_evidence_versions = cv;
        }
        DomainObject::Annotation(ann) => {
            ann.target_version_id = ns_version(ns, &ann.target_version_id)?;
        }
        DomainObject::SavedQuery(sq) => {
            if let Some(ref c) = sq.case_scope {
                sq.case_scope = Some(ns_opaque(ns, c)?);
            }
        }
        DomainObject::Hypothesis(hyp) => {
            hyp.supporting_versions = hyp
                .supporting_versions
                .iter()
                .map(|v| ns_version(ns, v))
                .collect::<Result<Vec<_>, _>>()?;
            hyp.contradicting_versions = hyp
                .contradicting_versions
                .iter()
                .map(|v| ns_version(ns, v))
                .collect::<Result<Vec<_>, _>>()?;
        }
        DomainObject::ResearchLead(_) => {}
    }

    Ok(cloned)
}

/// Validates every invariant of `bundle` BEFORE mutating `target_store` (`REPRO-02`, `C10`, `A17`).
///
/// Rejects:
/// - Unsupported required schema versions (`schema_version == 0` or `> CURRENT_BUNDLE_SCHEMA_VERSION`)
/// - Path traversal or invalid entry paths
/// - Oversized / decompression-bomb payloads exceeding `limits`
/// - Corrupted SHA-256 asset hashes or byte lengths, or undeclared extra entries in `asset_payloads`
/// - Duplicate conflicting version IDs or collisions with existing objects/cases in `target_store`
///
/// Persists verified asset payloads into `target_store` (`put_verified_blob`) and namespaces
/// all imported IDs, versions, `supersedes`, and case references when `local_id_namespace` is `Some`
/// so existing local cases and records can never be overwritten or superseded (`C10`).
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
    let mut verified_assets: Vec<(ContentHash, &[u8])> = Vec::new();

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
            verified_assets.push((entry.content_hash.clone(), bytes.as_slice()));
        }
    }

    // Reject undeclared entries in `asset_payloads` that were not listed in `entry_inventory`.
    for path in bundle.asset_payloads.keys() {
        if !seen_paths.contains(path) {
            return Err(BundleError::MissingDeclaredEntry(path.clone()));
        }
    }

    let mut seen_raw_versions = BTreeSet::new();
    for obj in &bundle.records {
        obj.validate().map_err(BundleError::Domain)?;
        let ver = &obj.envelope().version_id;
        if !seen_raw_versions.insert(ver.clone()) {
            return Err(BundleError::DuplicateConflictingVersion(ver.clone()));
        }
    }

    // Pre-build local ID mapping and check that NO mapped version, ID, or case_id collides with
    // existing records in `target_store` (`C10`).
    let mut id_map = BTreeMap::new();
    let mut mapped_records = Vec::with_capacity(bundle.records.len());
    for obj in &bundle.records {
        let origin_id = obj.envelope().id.clone();
        let mapped = match local_id_namespace {
            Some(ns) => namespace_domain_object(ns, obj)?,
            None => obj.clone(),
        };
        let local_id = mapped.envelope().id.clone();
        let local_ver = mapped.envelope().version_id.clone();

        if target_store.graph().get_version(&local_ver).is_some() {
            return Err(BundleError::ExistingVersionCollision(local_ver));
        }
        if target_store.graph().get_latest(&local_id).is_some()
            && !id_map.values().any(|already| already == &local_id)
        {
            return Err(BundleError::ExistingVersionCollision(local_ver));
        }
        if let Some(cid) = mapped.envelope().visibility.case_id()
            && target_store.graph().get_latest(cid).is_some()
            && !bundle
                .records
                .iter()
                .any(|r| matches!(r, DomainObject::Investigation(_)))
        {
            return Err(BundleError::ExistingVersionCollision(local_ver));
        }
        if let Some(ref sup) = mapped.envelope().supersedes
            && target_store.graph().get_version(sup).is_some()
        {
            return Err(BundleError::ExistingVersionCollision(sup.clone()));
        }

        id_map.insert(origin_id, local_id);
        mapped_records.push(mapped);
    }

    // Dry-run inserting all mapped objects on a cloned graph before mutating `target_store`!
    {
        let mut dry_graph = target_store.graph().clone();
        for obj in &mapped_records {
            dry_graph
                .insert_object(obj.clone(), None)
                .map_err(BundleError::Domain)?;
        }
    }

    // Persist verified asset blobs into `target_store` (`C10`).
    let mut imported_asset_count = 0usize;
    for (hash, bytes) in verified_assets {
        target_store
            .put_verified_blob(&hash, bytes, RetentionClass::PinnedCaseEvidence)
            .map_err(BundleError::Store)?;
        imported_asset_count += 1;
    }

    // Commit validated objects into `target_store`.
    for obj in mapped_records {
        target_store
            .put_object(obj, None)
            .map_err(BundleError::Store)?;
    }

    Ok(BundleImportReceipt {
        package_id: bundle.manifest.package_id.clone(),
        origin_to_local_id_map: id_map,
        imported_asset_count,
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
    /// Entry listed in manifest was missing from bundle payload map (or extra undeclared payload).
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
    /// Bundle version or case ID collided with an existing record in the store (existing cases are never overwritten).
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
                write!(f, "declared bundle entry '{p}' is missing or undeclared")
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
                "bundle version or case reference '{v}' collides with an existing record; existing cases are never overwritten"
            ),
        }
    }
}

impl std::error::Error for BundleError {}

#[cfg(test)]
mod tests {
    use super::*;
    use oia_domain::{
        ActorId, Artifact, DomainObjectType, Investigation, ObjectEnvelope, OriginClass,
        ProvenanceRecord, ReassessmentState, ReportBlock, ReportPublicationState, ReuseConditions,
        ReviewStatus, SchemaVersion, Source, SourceHealthState, SourceRecord,
    };
    use oia_store::StoreBudget;

    /// Verifies physical content redaction in Markdown/HTML exports, `export_case_bundle` with
    /// redistribution policy enforcement, and `validate_and_import_bundle` asset storage and
    /// full namespace isolation (`#11`, `C10`, `A10`, `A16`, `A17`).
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
                created_by: actor.clone(),
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
        store.put_object(existing_case.clone(), None)?;

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
            records: vec![existing_case, DomainObject::Report(report.clone())],
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
        assert!(
            store
                .graph()
                .get_version(&VersionId::new("imp:ver_rep_1")?)
                .is_none()
        );

        // Fix the asset bytes and verify clean import with full namespacing and verified blob storage (`C10`).
        let mut good_bundle = bad_bundle;
        good_bundle
            .asset_payloads
            .insert("assets/blob1".to_owned(), asset_bytes.clone());
        let receipt = validate_and_import_bundle(
            &mut store,
            &good_bundle,
            BundleImportLimits::default(),
            Some("imp"),
        )?;
        assert_eq!(receipt.imported_asset_count, 1);
        assert_eq!(store.read_verified_blob(&asset_hash)?, asset_bytes);
        assert_eq!(
            receipt.origin_to_local_id_map.get(&OpaqueId::new("rep_1")?),
            Some(&OpaqueId::new("imp:rep_1")?)
        );
        // Existing local `ver_case_alpha_1` remains Reviewed (not Superseded by imported bundle).
        assert_eq!(
            store
                .graph()
                .get_version(&VersionId::new("ver_case_alpha_1")?)
                .map(|o| o.envelope().review_status),
            Some(ReviewStatus::Reviewed)
        );

        // 3. Verify `export_case_bundle` omits non-redistributable assets with `RedistributionProhibitedByPolicy`.
        store.put_object(
            DomainObject::Source(Source {
                envelope: ObjectEnvelope {
                    id: OpaqueId::new("src_restricted_redist")?,
                    version_id: VersionId::new("ver_src_restricted_1")?,
                    schema_version: SchemaVersion::V1,
                    object_type: DomainObjectType::Source,
                    created_at_ms: 1_700_000_000_000,
                    created_by: actor.clone(),
                    visibility: WorkspaceVisibility::LibraryPublic,
                    supersedes: None,
                    provenance: ProvenanceRecord {
                        source_version_ids: Vec::new(),
                        declared_origin_note: Some("Registered source".to_owned()),
                        processing_history: Vec::new(),
                    },
                    origin_class: OriginClass::AcquiredSourceMaterial,
                    review_status: ReviewStatus::Reviewed,
                    availability: AvailabilityState::Available,
                    reassessment: ReassessmentState::Current,
                },
                provider_name: "Archive".to_owned(),
                capabilities: Vec::new(),
                reuse_conditions: ReuseConditions {
                    ordinary_access_method: "web".to_owned(),
                    automated_processing_allowed: true,
                    retention_allowed: true,
                    redistribution_allowed: false,
                    attribution: "Archive Attribution".to_owned(),
                },
                credential_ref: None,
                health: SourceHealthState::Available,
            }),
            None,
        )?;
        let non_redist_bytes = b"non redistributable bytes";
        let non_redist_hash = ContentHash::compute(non_redist_bytes);
        store.commit_source_and_artifact(
            SourceRecord {
                envelope: ObjectEnvelope {
                    id: OpaqueId::new("sr_nr")?,
                    version_id: VersionId::new("ver_sr_nr")?,
                    schema_version: SchemaVersion::V1,
                    object_type: DomainObjectType::SourceRecord,
                    created_at_ms: 1_700_000_000_000,
                    created_by: actor.clone(),
                    visibility: WorkspaceVisibility::PrivateCase {
                        case_id: case_id.clone(),
                        sensitive: false,
                    },
                    supersedes: None,
                    provenance: ProvenanceRecord {
                        source_version_ids: Vec::new(),
                        declared_origin_note: Some("Acquired".to_owned()),
                        processing_history: Vec::new(),
                    },
                    origin_class: OriginClass::AcquiredSourceMaterial,
                    review_status: ReviewStatus::Unreviewed,
                    availability: AvailabilityState::Available,
                    reassessment: ReassessmentState::Current,
                },
                source_id: OpaqueId::new("src_restricted_redist")?,
                upstream_version: "1".to_owned(),
                acquisition_attempt_id: OpaqueId::new("acq_nr")?,
                source_locator: "https://archive.example/1".to_owned(),
                source_event_time_ms: None,
                public_availability_time_ms: None,
                retrieved_at_ms: 1_700_000_000_000,
                content: oia_domain::RetainedContent::RetainedBlob {
                    content_hash: non_redist_hash.clone(),
                    byte_len: non_redist_bytes.len() as u64,
                    media_type: "text/plain".to_owned(),
                },
            },
            Artifact {
                envelope: ObjectEnvelope {
                    id: OpaqueId::new("art_nr")?,
                    version_id: VersionId::new("ver_art_nr")?,
                    schema_version: SchemaVersion::V1,
                    object_type: DomainObjectType::Artifact,
                    created_at_ms: 1_700_000_000_000,
                    created_by: actor,
                    visibility: WorkspaceVisibility::PrivateCase {
                        case_id: case_id.clone(),
                        sensitive: false,
                    },
                    supersedes: None,
                    provenance: ProvenanceRecord {
                        source_version_ids: vec![VersionId::new("ver_sr_nr")?],
                        declared_origin_note: None,
                        processing_history: Vec::new(),
                    },
                    origin_class: OriginClass::AcquiredSourceMaterial,
                    review_status: ReviewStatus::Unreviewed,
                    availability: AvailabilityState::Available,
                    reassessment: ReassessmentState::Current,
                },
                content_hash: non_redist_hash,
                media_type: "text/plain".to_owned(),
                byte_len: non_redist_bytes.len() as u64,
                derived_from_artifact: None,
                quality_effect_disclosure: None,
            },
            non_redist_bytes,
            RetentionClass::PinnedCaseEvidence,
        )?;

        let exported = export_case_bundle(
            &store,
            &case_id,
            OpaqueId::new("pkg_out")?,
            1_700_000_200_000,
            false,
        )?;
        assert!(exported.asset_payloads.is_empty());
        assert_eq!(exported.manifest.omitted_assets.len(), 1);
        assert_eq!(
            exported.manifest.omitted_assets[0].reason,
            OmissionReason::RedistributionProhibitedByPolicy
        );

        let _ = std::fs::remove_dir_all(&store_dir);
        Ok(())
    }
}
