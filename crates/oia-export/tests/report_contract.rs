#![forbid(unsafe_code)]
#![deny(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use oia_export::{
    AnchorValidationState, CitationAnchor, EvidenceInventoryEntry, Limit, OmissionReason,
    OriginClass, ReassessmentStatus, RecipientDisclosure, ReportBlock, ReportError, ReportLimits,
    ReportSection, ReportSnapshot, ReviewStatus, StatementCitation, StopReason, TargetAvailability,
    TimeDisplay, TimePrecision, preview_report, render_html, render_markdown,
};

const REPORT_V1: &[u8] = include_bytes!("../../../fixtures/documents/a03/originals/report-v1.txt");
const REPORT_V2: &[u8] = include_bytes!("../../../fixtures/documents/a03/originals/report-v2.txt");
const REDACTED_A: &[u8] =
    include_bytes!("../../../fixtures/documents/a03/originals/redacted-a.txt");
const CONFLICTING: &[u8] =
    include_bytes!("../../../fixtures/documents/a03/originals/conflicting.txt");
const UNTRUSTED: &[u8] = include_bytes!("../../../fixtures/documents/a03/originals/untrusted.txt");

fn default_limits() -> Result<ReportLimits, ReportError> {
    ReportLimits::new(32, 64, 32, 131_072, 131_072)
}

#[test]
fn preserves_a03_versions_labels_anchors_and_distinct_duplicate_byte_citations()
-> Result<(), ReportError> {
    let entries = [
        EvidenceInventoryEntry {
            entry_id: "entry_orig_a03",
            citation_label: "CIT-1",
            source_id: "publisher_main",
            source_record_id: "report-original",
            source_record_version_id: "report-original_version_1",
            artifact_id: Some("report-v1.txt"),
            artifact_version_id: Some("art_v1"),
            availability: TargetAvailability::Available,
            omission_reason: None,
            disclosure: RecipientDisclosure::FullTarget,
            retained_utf8_bytes: Some(REPORT_V1),
            external_reference_url: Some("https://publisher.invalid/a03"),
            redacted_from_entry_id: None,
        },
        EvidenceInventoryEntry {
            entry_id: "entry_mirror_a03",
            citation_label: "CIT-2",
            source_id: "mirror_node",
            source_record_id: "report-republication",
            source_record_version_id: "report-republication-version-1",
            artifact_id: Some("report-v1.txt"),
            artifact_version_id: Some("art_v1"),
            availability: TargetAvailability::Available,
            omission_reason: None,
            disclosure: RecipientDisclosure::FullTarget,
            retained_utf8_bytes: Some(REPORT_V1),
            external_reference_url: Some("https://mirror.invalid/a03"),
            redacted_from_entry_id: None,
        },
        EvidenceInventoryEntry {
            entry_id: "entry-corr",
            citation_label: "CIT-3",
            source_id: "publisher_main",
            source_record_id: "report-correction",
            source_record_version_id: "report-correction-version-1",
            artifact_id: Some("report-v2.txt"),
            artifact_version_id: Some("art_v2"),
            availability: TargetAvailability::Available,
            omission_reason: None,
            disclosure: RecipientDisclosure::FullTarget,
            retained_utf8_bytes: Some(REPORT_V2),
            external_reference_url: None,
            redacted_from_entry_id: None,
        },
        EvidenceInventoryEntry {
            entry_id: "entry-conflict",
            citation_label: "CIT-4",
            source_id: "independent",
            source_record_id: "conflicting-account",
            source_record_version_id: "conflicting-account-version-1",
            artifact_id: Some("conflicting.txt"),
            artifact_version_id: Some("art-conflict-v1"),
            availability: TargetAvailability::Available,
            omission_reason: None,
            disclosure: RecipientDisclosure::FullTarget,
            retained_utf8_bytes: Some(CONFLICTING),
            external_reference_url: None,
            redacted_from_entry_id: None,
        },
        EvidenceInventoryEntry {
            entry_id: "entry-scan",
            citation_label: "CIT-5",
            source_id: "scanner",
            source_record_id: "scanned-report",
            source_record_version_id: "scanned-report-version-1",
            artifact_id: Some("scan-page-1.png"),
            artifact_version_id: Some("art-scan-p1"),
            availability: TargetAvailability::Available,
            omission_reason: None,
            disclosure: RecipientDisclosure::FullTarget,
            retained_utf8_bytes: None,
            external_reference_url: None,
            redacted_from_entry_id: None,
        },
    ];

    let citations = [
        StatementCitation {
            entry_id: "entry_orig_a03",
            anchor: CitationAnchor::Utf8ByteRange {
                start: 85,
                end: 128,
                is_unadjusted_original_offset: false,
            },
        },
        StatementCitation {
            entry_id: "entry_mirror_a03",
            anchor: CitationAnchor::Utf8ByteRange {
                start: 85,
                end: 128,
                is_unadjusted_original_offset: false,
            },
        },
        StatementCitation {
            entry_id: "entry_mirror_a03",
            anchor: CitationAnchor::ExtractedPassage {
                extraction_version_id: "ext_mirror_v1",
                passage_line: 3,
                byte_start: 85,
                byte_end: 128,
            },
        },
        StatementCitation {
            entry_id: "entry-corr",
            anchor: CitationAnchor::Utf8ByteRange {
                start: 85,
                end: 142,
                is_unadjusted_original_offset: false,
            },
        },
        StatementCitation {
            entry_id: "entry-conflict",
            anchor: CitationAnchor::Utf8ByteRange {
                start: 46,
                end: 118,
                is_unadjusted_original_offset: false,
            },
        },
        StatementCitation {
            entry_id: "entry-scan",
            anchor: CitationAnchor::PageRegion {
                page_number: 1,
                region_label: "bbox(0.10,0.20,0.80,0.45)",
            },
        },
        StatementCitation {
            entry_id: "entry-scan",
            anchor: CitationAnchor::UnsupportedAnchor {
                kind: "temporal-audio-frame",
                limitation_note: "audio frame anchors are not verifiable on still PNG pages",
            },
        },
    ];

    let blocks = [ReportBlock {
        block_id: "blk_chron_1",
        statement_id: "stmt_chron_1",
        section: ReportSection::Chronology,
        actor_attribution: "model:grounded_drafter_v1",
        origin_class: OriginClass::Generated,
        review_status: ReviewStatus::Unreviewed,
        chronology_time: Some(TimeDisplay {
            text: "unknown",
            precision: TimePrecision::Unknown {
                reason: "not specified by this synthetic account",
            },
            time_reference: "unrecorded",
        }),
        uncertainty_note: Some(
            "Event and publication times are null across the synthetic A03 accounts.",
        ),
        source_limitation_note: Some("Scanned page 2 is unreadable and page 3 is missing."),
        text: "Synthetic accounts report two lights in edition 1, three in edition 2, and one in a conflicting account.",
        citations: &citations,
        sensitive_dependency_entry_ids: &[],
    }];

    let snapshot = ReportSnapshot {
        report_id: "rep_1",
        report_version_id: "rep_1_v1",
        case_id: "case_1",
        case_version_id: "case_1_v1",
        title: "A03 Multi-Source Chronology Report",
        snapshot_created_at: TimeDisplay {
            text: "2026-11-02T19:00:00Z",
            precision: TimePrecision::ExactInstant,
            time_reference: "UTC",
        },
        reassessment_status: ReassessmentStatus::NeedsReassessment {
            flagging_record_version_id: "report-correction-version-1",
            reason: "Publisher edition 2 superseded edition 1",
        },
        coverage_summary: "5 synthetic records included.",
        redaction_policy_id: "policy_private_v1",
        generic_omission_note: None,
        evidence_entries: &entries,
        blocks: &blocks,
    };

    let projection = preview_report(&snapshot, default_limits()?, || Ok(()))?;
    assert_eq!(projection.counts().included_full_targets, 5);
    assert_eq!(projection.counts().validated_citations, 4);
    assert_eq!(projection.counts().unvalidated_citations, 2);
    assert_eq!(projection.counts().unsupported_anchor_citations, 1);

    let first_citations = projection
        .blocks()
        .first()
        .map(|b| b.citations.as_slice())
        .unwrap_or(&[]);
    assert_eq!(
        first_citations.first().map(|c| &c.validation),
        Some(&AnchorValidationState::ValidatedUtf8Slice {
            start: 85,
            end: 128,
            quoted_text: "The fictional observer reported two lights.",
        })
    );
    assert_eq!(
        first_citations.get(1).map(|c| &c.validation),
        Some(&AnchorValidationState::ValidatedUtf8Slice {
            start: 85,
            end: 128,
            quoted_text: "The fictional observer reported two lights.",
        })
    );
    assert_eq!(
        first_citations.get(2).map(|c| &c.validation),
        Some(&AnchorValidationState::UnvalidatedExtractedPassage {
            extraction_version_id: "ext_mirror_v1",
            passage_line: 3,
            byte_start: 85,
            byte_end: 128,
            availability: TargetAvailability::Available,
        })
    );

    let md = render_markdown(&projection, || Ok(()))?;
    let html = render_html(&projection, || Ok(()))?;

    for output in [&md, &html] {
        assert!(output.contains("[ORIGIN: GENERATED]"));
        assert!(output.contains("[REVIEW: UNREVIEWED]"));
        assert!(output.contains("[REASSESSMENT REQUIRED]"));
        assert!(output.contains("report-correction-version-1"));
        assert!(output.contains("[UNCERTAINTY]:"));
        assert!(output.contains("[SOURCE LIMITATION]:"));
        assert!(output.contains("CIT-1"));
        assert!(output.contains("CIT-2"));
        // B1: IDs containing `_` must match between Markdown and HTML without literal `\_`.
        assert!(output.contains("entry_orig_a03"));
        assert!(output.contains("entry_mirror_a03"));
        assert!(output.contains("publisher_main"));
        assert!(output.contains("report-original@report-original_version_1"));
        assert!(output.contains("report-republication@report-republication-version-1"));
        assert!(output.contains("stmt_chron_1"));
        assert!(output.contains("model:grounded_drafter_v1"));
        assert!(output.contains("ext_mirror_v1"));
        assert!(!output.contains("entry\\_orig\\_a03"));
        assert!(output.contains("not specified by this synthetic account"));
        assert!(output.contains("ANCHOR UNVALIDATED (extracted passage)"));
        assert!(output.contains("No material selected for this section."));
        assert!(output.contains("UNSUPPORTED ANCHOR LIMITATION"));
    }
    assert!(html.contains("href=\"#cite-1\""));
    assert!(html.contains("href=\"#cite-2\""));
    assert!(html.contains("id=\"cite-1\""));
    assert!(html.contains("id=\"cite-2\""));
    assert!(html.contains("@media print"));
    Ok(())
}

#[test]
fn enforces_redaction_preview_existence_sensitive_isolation_and_conclusion_guards()
-> Result<(), ReportError> {
    let entry_redacted = EvidenceInventoryEntry {
        entry_id: "entry-redacted-a",
        citation_label: "CIT-RED-A",
        source_id: "archive",
        source_record_id: "release-a",
        source_record_version_id: "release-a-version-1",
        artifact_id: Some("redacted-a.txt"),
        artifact_version_id: Some("art-red-a-v1"),
        availability: TargetAvailability::Available,
        omission_reason: None,
        disclosure: RecipientDisclosure::FullTarget,
        retained_utf8_bytes: Some(REDACTED_A),
        external_reference_url: None,
        redacted_from_entry_id: Some("entry-secret-origin"),
    };
    let entry_ref_only = EvidenceInventoryEntry {
        entry_id: "entry-ref-only",
        citation_label: "CIT-REF",
        source_id: "restricted",
        source_record_id: "reference-only",
        source_record_version_id: "reference-only-version-1",
        artifact_id: None,
        artifact_version_id: None,
        availability: TargetAvailability::ReferenceOnly,
        omission_reason: Some(OmissionReason::RetentionProhibited),
        disclosure: RecipientDisclosure::MetadataOnly,
        retained_utf8_bytes: None,
        external_reference_url: None,
        redacted_from_entry_id: None,
    };
    let entry_generic = EvidenceInventoryEntry {
        entry_id: "entry-generic-omitted",
        citation_label: "CIT-GENERIC-HIDDEN",
        source_id: "classified-source-alpha",
        source_record_id: "record-alpha",
        source_record_version_id: "record-alpha-v1",
        artifact_id: None,
        artifact_version_id: None,
        availability: TargetAvailability::Restricted,
        omission_reason: Some(OmissionReason::Restricted),
        disclosure: RecipientDisclosure::GenericCoverageOnly,
        retained_utf8_bytes: None,
        external_reference_url: None,
        redacted_from_entry_id: None,
    };
    let entry_secret = EvidenceInventoryEntry {
        entry_id: "entry-secret-origin",
        citation_label: "CIT-SECRET-ID",
        source_id: "secret-compartment-compartment99",
        source_record_id: "secret-record-99",
        source_record_version_id: "secret-record-99-v1",
        artifact_id: None,
        artifact_version_id: None,
        availability: TargetAvailability::Restricted,
        omission_reason: Some(OmissionReason::Restricted),
        disclosure: RecipientDisclosure::ExistenceSensitive,
        retained_utf8_bytes: None,
        external_reference_url: None,
        redacted_from_entry_id: None,
    };
    let entries = [entry_redacted, entry_ref_only, entry_generic, entry_secret];

    let citations_ok = [
        StatementCitation {
            entry_id: "entry-redacted-a",
            anchor: CitationAnchor::Utf8ByteRange {
                start: 46,
                end: 109,
                is_unadjusted_original_offset: false,
            },
        },
        StatementCitation {
            entry_id: "entry-ref-only",
            anchor: CitationAnchor::Utf8ByteRange {
                start: 0,
                end: 20,
                is_unadjusted_original_offset: false,
            },
        },
    ];

    let base_block = ReportBlock {
        block_id: "blk-1",
        statement_id: "stmt-1",
        section: ReportSection::BoundedConclusions,
        actor_attribution: "analyst:cameron",
        origin_class: OriginClass::Inference,
        review_status: ReviewStatus::Reviewed,
        chronology_time: None,
        uncertainty_note: Some("Conclusion bounded to redacted Release A."),
        source_limitation_note: None,
        text: "Redacted Release A supports a two-light report without disclosing the redacted location.",
        citations: &citations_ok,
        sensitive_dependency_entry_ids: &["entry-redacted-a"],
    };
    let valid_blocks = [base_block];

    // B4: Even when generic_omission_note is None, GenericCoverageOnly omissions (> 0) render an explicit notice.
    let base_snapshot = ReportSnapshot {
        report_id: "rep-redact",
        report_version_id: "rep-redact-v1",
        case_id: "case-redact",
        case_version_id: "case-redact-v1",
        title: "Redaction Isolation Check",
        snapshot_created_at: TimeDisplay {
            text: "2026-11-02T19:10:00Z",
            precision: TimePrecision::ExactInstant,
            time_reference: "UTC",
        },
        reassessment_status: ReassessmentStatus::Current,
        coverage_summary: "Redacted export scope.",
        redaction_policy_id: "policy-external-v1",
        generic_omission_note: None,
        evidence_entries: &entries,
        blocks: &valid_blocks,
    };

    let projection = preview_report(&base_snapshot, default_limits()?, || Ok(()))?;
    assert_eq!(projection.visible_inventory().len(), 2);
    assert_eq!(projection.counts().included_full_targets, 1);
    assert_eq!(projection.counts().reference_only_targets, 1);
    assert_eq!(projection.counts().generic_coverage_omissions, 1);

    let md = render_markdown(&projection, || Ok(()))?;
    let html = render_html(&projection, || Ok(()))?;
    let debug_proj = format!("{projection:?}");
    for output in [&md, &html, &debug_proj] {
        assert!(!output.contains("entry-secret-origin"));
        assert!(!output.contains("CIT-SECRET-ID"));
        assert!(!output.contains("secret-compartment-compartment99"));
        assert!(!output.contains("secret-record-99"));
        assert!(!output.contains("entry-generic-omitted"));
        assert!(!output.contains("CIT-GENERIC-HIDDEN"));
        assert!(!output.contains("classified-source-alpha"));
    }
    for output in [&md, &html] {
        assert!(output.contains("redacted derivative"));
        assert!(output.contains(
            "1 additional record(s) omitted under recipient policy (generic coverage only)."
        ));
        assert!(
            output.contains("ANCHOR UNVALIDATED: target reference-only (retention prohibited)")
        );
    }

    // 1. Conclusion depending on ExistenceSensitive or MetadataOnly entry is rejected.
    let sensitive_conclusion_block = [ReportBlock {
        sensitive_dependency_entry_ids: &["entry-secret-origin"],
        ..base_block
    }];
    let sensitive_snapshot = ReportSnapshot {
        blocks: &sensitive_conclusion_block,
        ..base_snapshot
    };
    assert_eq!(
        preview_report(&sensitive_snapshot, default_limits()?, || Ok(())),
        Err(ReportError::SensitiveConclusionRequiresRedaction {
            statement_id: "stmt-1".to_owned(),
            entry_id: "entry-secret-origin".to_owned(),
        })
    );

    // 2. Direct citation to an ExistenceSensitive entry is rejected.
    let leak_citations = [StatementCitation {
        entry_id: "entry-secret-origin",
        anchor: CitationAnchor::Utf8ByteRange {
            start: 0,
            end: 10,
            is_unadjusted_original_offset: false,
        },
    }];
    let leak_block = [ReportBlock {
        citations: &leak_citations,
        ..base_block
    }];
    let leak_snapshot = ReportSnapshot {
        blocks: &leak_block,
        ..base_snapshot
    };
    assert_eq!(
        preview_report(&leak_snapshot, default_limits()?, || Ok(())),
        Err(ReportError::ExistenceSensitiveReferenceLeak {
            statement_id: "stmt-1".to_owned(),
            entry_id: "entry-secret-origin".to_owned(),
        })
    );

    // 3. Applying an unadjusted original byte offset to a redacted derivative is rejected.
    let unadjusted_citations = [StatementCitation {
        entry_id: "entry-redacted-a",
        anchor: CitationAnchor::Utf8ByteRange {
            start: 46,
            end: 109,
            is_unadjusted_original_offset: true,
        },
    }];
    let unadjusted_block = [ReportBlock {
        citations: &unadjusted_citations,
        ..base_block
    }];
    let unadjusted_snapshot = ReportSnapshot {
        blocks: &unadjusted_block,
        ..base_snapshot
    };
    assert_eq!(
        preview_report(&unadjusted_snapshot, default_limits()?, || Ok(())),
        Err(ReportError::UnadjustedRedactedAnchor {
            statement_id: "stmt-1".to_owned(),
            entry_id: "entry-redacted-a".to_owned(),
        })
    );

    // 4. Disclosing the excluded original entry as FullTarget (with bytes or artifact_id only) alongside its redacted derivative is rejected.
    let bundled_orig_entries = [
        entry_redacted,
        entry_ref_only,
        entry_generic,
        EvidenceInventoryEntry {
            availability: TargetAvailability::Available,
            omission_reason: None,
            disclosure: RecipientDisclosure::FullTarget,
            artifact_id: Some("secret-orig.pdf"),
            retained_utf8_bytes: None,
            ..entry_secret
        },
    ];
    let bundled_orig_snapshot = ReportSnapshot {
        evidence_entries: &bundled_orig_entries,
        ..base_snapshot
    };
    assert_eq!(
        preview_report(&bundled_orig_snapshot, default_limits()?, || Ok(())),
        Err(ReportError::DisclosurePolicyViolation {
            entry_id: "entry-redacted-a".to_owned(),
            reason: "excluded original entry must not be disclosed as FullTarget beside its redacted derivative",
        })
    );
    Ok(())
}

#[test]
fn neutralizes_hostile_markup_and_rejects_local_paths_and_unsafe_urls() -> Result<(), ReportError> {
    let base_entry = EvidenceInventoryEntry {
        entry_id: "entry-untrusted",
        citation_label: "CIT-ADV",
        source_id: "adversarial",
        source_record_id: "untrusted-source",
        source_record_version_id: "untrusted-source-version-1",
        artifact_id: Some("untrusted.txt"),
        artifact_version_id: Some("art-untrusted-v1"),
        availability: TargetAvailability::Available,
        omission_reason: None,
        disclosure: RecipientDisclosure::FullTarget,
        retained_utf8_bytes: Some(UNTRUSTED),
        external_reference_url: Some("https://adversarial.invalid/a03"),
        redacted_from_entry_id: None,
    };
    let entries = [base_entry];

    let citations = [StatementCitation {
        entry_id: "entry-untrusted",
        anchor: CitationAnchor::Utf8ByteRange {
            start: 108,
            end: 197,
            is_unadjusted_original_offset: false,
        },
    }];

    let hostile_statement = "Quoted hostile payload: <script>alert(\"xss\")</script> <img src=x onerror=alert(1)> ![beacon](https://evil.invalid/pixel.png) [click](javascript:alert(1)) See https://10.0.0.1/x?token=abc and www.evil.example \u{202E}bidi # Fake Heading";
    let blocks = [ReportBlock {
        block_id: "blk-adv",
        statement_id: "stmt-adv",
        section: ReportSection::ConflictingEvidence,
        actor_attribution: "analyst:cameron",
        origin_class: OriginClass::SourceAssertion,
        review_status: ReviewStatus::Challenged,
        chronology_time: None,
        uncertainty_note: None,
        source_limitation_note: Some("Contains untrusted prompt-injection and markup strings."),
        text: hostile_statement,
        citations: &citations,
        sensitive_dependency_entry_ids: &[],
    }];

    let snapshot = ReportSnapshot {
        report_id: "rep-inert",
        report_version_id: "rep-inert-v1",
        case_id: "case-inert",
        case_version_id: "case-inert-v1",
        title: "Hostile <script> & [Markdown] Neutralization Test می\u{200C}خواهم क्\u{200D}ष 👨\u{200D}👩 \u{200F} \u{E0001}",
        snapshot_created_at: TimeDisplay {
            text: "2026-11-02T19:15:00Z",
            precision: TimePrecision::ExactInstant,
            time_reference: "UTC",
        },
        reassessment_status: ReassessmentStatus::Current,
        coverage_summary: "Single adversarial fixture.",
        redaction_policy_id: "policy-inert-v1",
        generic_omission_note: None,
        evidence_entries: &entries,
        blocks: &blocks,
    };

    let projection = preview_report(&snapshot, default_limits()?, || Ok(()))?;
    let md = render_markdown(&projection, || Ok(()))?;
    let html = render_html(&projection, || Ok(()))?;

    assert!(!md.contains("<script>"));
    assert!(!md.contains("<img"));
    assert!(!md.contains("![beacon]("));
    assert!(!md.contains("[click](javascript:"));
    // B2: Bare URLs and www. domains in quoted text / statements are defanged against GFM autolinking.
    assert!(!md.contains("https://fixture.invalid/private"));
    assert!(!md.contains("https://10.0.0.1/x?token=abc"));
    assert!(!md.contains("www.evil.example"));
    assert!(md.contains("https&#58;//fixture&#46;invalid/private"));
    assert!(md.contains("https&#58;//10&#46;0&#46;0&#46;1/x?token=abc"));
    assert!(md.contains("www&#46;evil&#46;example"));
    assert!(md.contains("&lt;script&gt;"));
    assert!(md.contains("\\!\\[beacon\\]\\("));
    assert!(md.contains("Ignore previous instructions&#46;"));
    assert!(!md.contains('\u{202E}'));
    assert!(md.contains("\\u{202E}"));

    // A16: preserve multilingual shaping without hiding bidi/tag controls.
    for output in [&md, &html] {
        assert!(output.contains("می\u{200C}خواهم क्\u{200D}ष 👨\u{200D}👩"));
        assert!(!output.contains('\u{200F}'));
        assert!(!output.contains('\u{E0001}'));
        assert!(output.contains("\\u{200F}"));
        assert!(output.contains("\\u{E0001}"));
    }

    assert!(!html.contains("<script"));
    assert!(!html.contains("<img"));
    assert!(!html.contains('\u{202E}'));
    assert!(html.contains("&lt;script&gt;alert(&quot;xss&quot;)&lt;/script&gt;"));
    assert!(html.contains("http-equiv=\"Content-Security-Policy\""));
    assert!(html.contains("rel=\"noopener noreferrer external\""));

    for bad_path in [
        "C:\\Users\\Cameron\\secret.txt",
        "/Users/cameron/secret.txt",
        "/home/analyst/case.json",
        "\\\\fileserver\\share\\doc.pdf",
        "file:///etc/passwd",
    ] {
        let mut path_snapshot = snapshot;
        path_snapshot.coverage_summary = bad_path;
        assert_eq!(
            preview_report(&path_snapshot, default_limits()?, || Ok(())),
            Err(ReportError::LocalPathDisclosed {
                field: "coverage_summary",
            }),
            "expected local path rejection for {bad_path}"
        );
    }

    for bad_url in [
        "javascript:alert(1)",
        "data:text/html,<script>alert(1)</script>",
        "https://user:pass@publisher.invalid/a03",
        "https://publisher.invalid/a03?api_key=secret123",
        "http://127.0.0.1:8080/internal",
        "http://localhost/admin",
        "http://192.168.1.50/stream",
        "http://172.16.0.5/feed",
        "https://node.internal/api",
    ] {
        let bad_entries = [EvidenceInventoryEntry {
            external_reference_url: Some(bad_url),
            ..base_entry
        }];
        let bad_snapshot = ReportSnapshot {
            evidence_entries: &bad_entries,
            ..snapshot
        };
        assert!(
            matches!(
                preview_report(&bad_snapshot, default_limits()?, || Ok(())),
                Err(ReportError::UnsafeExternalUrl { .. })
            ),
            "expected unsafe URL rejection for {bad_url}"
        );
    }
    Ok(())
}

#[test]
fn enforces_resource_budgets_date_only_precision_and_cooperative_stops() -> Result<(), ReportError>
{
    assert_eq!(
        ReportLimits::new(0, 1, 1, 1024, 1024),
        Err(ReportError::ZeroLimit(Limit::Blocks))
    );
    assert_eq!(
        ReportLimits::new(1, 0, 1, 1024, 1024),
        Err(ReportError::ZeroLimit(Limit::Citations))
    );
    assert_eq!(
        ReportLimits::new(1, 1, 0, 1024, 1024),
        Err(ReportError::ZeroLimit(Limit::EvidenceEntries))
    );
    assert_eq!(
        ReportLimits::new(1, 1, 1, 0, 1024),
        Err(ReportError::ZeroLimit(Limit::TextBytes))
    );
    assert_eq!(
        ReportLimits::new(1, 1, 1, 1024, 0),
        Err(ReportError::ZeroLimit(Limit::OutputBytes))
    );

    let entries = [EvidenceInventoryEntry {
        entry_id: "entry-1",
        citation_label: "CIT-1",
        source_id: "publisher",
        source_record_id: "report-original",
        source_record_version_id: "report-original-version-1",
        artifact_id: Some("report-v1.txt"),
        artifact_version_id: Some("art-v1"),
        availability: TargetAvailability::Available,
        omission_reason: None,
        disclosure: RecipientDisclosure::FullTarget,
        retained_utf8_bytes: Some(REPORT_V1),
        external_reference_url: None,
        redacted_from_entry_id: None,
    }];

    // Byte 50 in REPORT_V1 is the trailing byte of 'é' in "Café" (bytes 49..51).
    let non_boundary_citations = [StatementCitation {
        entry_id: "entry-1",
        anchor: CitationAnchor::Utf8ByteRange {
            start: 50,
            end: 60,
            is_unadjusted_original_offset: false,
        },
    }];
    let base_block = ReportBlock {
        block_id: "blk-1",
        statement_id: "stmt-1",
        section: ReportSection::QuestionAndScope,
        actor_attribution: "analyst:cameron",
        origin_class: OriginClass::Observation,
        review_status: ReviewStatus::Reviewed,
        chronology_time: None,
        uncertainty_note: None,
        source_limitation_note: None,
        text: "Scope statement.",
        citations: &non_boundary_citations,
        sensitive_dependency_entry_ids: &[],
    };
    let blocks = [base_block];
    let snapshot = ReportSnapshot {
        report_id: "rep-limits",
        report_version_id: "rep-limits-v1",
        case_id: "case-limits",
        case_version_id: "case-limits-v1",
        title: "Limits & Precision Check",
        snapshot_created_at: TimeDisplay {
            text: "2026-11-02T19:20:00Z",
            precision: TimePrecision::ExactInstant,
            time_reference: "UTC",
        },
        reassessment_status: ReassessmentStatus::Current,
        coverage_summary: "Budget checks.",
        redaction_policy_id: "policy-v1",
        generic_omission_note: None,
        evidence_entries: &entries,
        blocks: &blocks,
    };

    assert_eq!(
        preview_report(&snapshot, default_limits()?, || Ok(())),
        Err(ReportError::InvalidCitationAnchor {
            statement_id: "stmt-1".to_owned(),
            entry_id: "entry-1".to_owned(),
            reason: "Utf8ByteRange bounds do not lie on UTF-8 character boundaries",
        })
    );

    // DateOnly with midnight expansion is rejected.
    let valid_citations = [StatementCitation {
        entry_id: "entry-1",
        anchor: CitationAnchor::Utf8ByteRange {
            start: 85,
            end: 128,
            is_unadjusted_original_offset: false,
        },
    }];
    let bad_date_blocks = [ReportBlock {
        section: ReportSection::Chronology,
        chronology_time: Some(TimeDisplay {
            text: "2026-11-01T00:00:00Z",
            precision: TimePrecision::DateOnly,
            time_reference: "UTC",
        }),
        citations: &valid_citations,
        ..base_block
    }];
    let bad_date_snapshot = ReportSnapshot {
        blocks: &bad_date_blocks,
        ..snapshot
    };
    assert_eq!(
        preview_report(&bad_date_snapshot, default_limits()?, || Ok(())),
        Err(ReportError::InvalidTimeDisplay {
            field: "blocks.chronology_time",
            reason: "DateOnly source must not be expanded into a clock/midnight timestamp",
        })
    );

    // Valid snapshot for output budget and cancellation checks.
    let valid_blocks = [ReportBlock {
        citations: &valid_citations,
        ..base_block
    }];
    let valid_snapshot = ReportSnapshot {
        blocks: &valid_blocks,
        ..snapshot
    };

    let tiny_output_limits = ReportLimits::new(8, 8, 8, 16_384, 64)?;
    let projection = preview_report(&valid_snapshot, tiny_output_limits, || Ok(()))?;
    assert!(matches!(
        render_markdown(&projection, || Ok(())),
        Err(ReportError::LimitExceeded {
            limit: Limit::OutputBytes,
            ..
        })
    ));
    assert!(matches!(
        render_html(&projection, || Ok(())),
        Err(ReportError::LimitExceeded {
            limit: Limit::OutputBytes,
            ..
        })
    ));

    // Cooperative stop checks at every callback step.
    let full_projection = preview_report(&valid_snapshot, default_limits()?, || Ok(()))?;
    for step in 0..4 {
        let mut remaining = step;
        let result = preview_report(&valid_snapshot, default_limits()?, || {
            if remaining == 0 {
                Err(StopReason::Cancelled)
            } else {
                remaining -= 1;
                Ok(())
            }
        });
        assert_eq!(result, Err(ReportError::Stopped(StopReason::Cancelled)));
    }
    assert_eq!(
        render_markdown(&full_projection, || Err(StopReason::DeadlineExceeded)),
        Err(ReportError::Stopped(StopReason::DeadlineExceeded))
    );
    assert_eq!(
        render_html(&full_projection, || Err(StopReason::Superseded)),
        Err(ReportError::Stopped(StopReason::Superseded))
    );
    Ok(())
}
