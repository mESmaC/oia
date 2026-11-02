#![forbid(unsafe_code)]
#![deny(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use std::env;
use std::io::{self, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use oia_export::{
    CitationAnchor, EvidenceInventoryEntry, OmissionReason, OriginClass, ReassessmentStatus,
    RecipientDisclosure, ReportBlock, ReportLimits, ReportSection, ReportSnapshot, ReviewStatus,
    StatementCitation, StopReason, TargetAvailability, TimeDisplay, TimePrecision, preview_report,
    render_html, render_markdown,
};

const REPORT_V1: &[u8] = include_bytes!("../../../fixtures/documents/a03/originals/report-v1.txt");
const REPORT_V2: &[u8] = include_bytes!("../../../fixtures/documents/a03/originals/report-v2.txt");
const REDACTED_A: &[u8] =
    include_bytes!("../../../fixtures/documents/a03/originals/redacted-a.txt");
const CONFLICTING: &[u8] =
    include_bytes!("../../../fixtures/documents/a03/originals/conflicting.txt");
const UNTRUSTED: &[u8] = include_bytes!("../../../fixtures/documents/a03/originals/untrusted.txt");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OutputMode {
    Summary,
    Markdown,
    Html,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            let _ = writeln!(io::stderr(), "error: {message}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let mode = match args.next().as_deref() {
        None | Some("--summary") => OutputMode::Summary,
        Some("--markdown") => OutputMode::Markdown,
        Some("--html") => OutputMode::Html,
        Some(other) => {
            return Err(format!(
                "unsupported argument `{other}`; expected [--summary | --markdown | --html] [MAX_OUTPUT_BYTES]"
            ));
        }
    };

    let max_output_bytes = match args.next() {
        Some(raw) => raw
            .parse::<usize>()
            .map_err(|_| format!("invalid MAX_OUTPUT_BYTES `{raw}`"))?,
        None => 262_144,
    };
    if args.next().is_some() {
        return Err(
            "too many arguments; expected [--summary | --markdown | --html] [MAX_OUTPUT_BYTES]"
                .to_owned(),
        );
    }

    let limits =
        ReportLimits::new(64, 128, 64, 262_144, max_output_bytes).map_err(|e| e.to_string())?;
    let started = Instant::now();
    let deadline = Duration::from_secs(5);
    let check_control = || {
        if started.elapsed() > deadline {
            Err(StopReason::DeadlineExceeded)
        } else {
            Ok(())
        }
    };

    let entries = [
        EvidenceInventoryEntry {
            entry_id: "entry-report-original",
            citation_label: "CIT-1",
            source_id: "publisher",
            source_record_id: "report-original",
            source_record_version_id: "report-original-version-1",
            artifact_id: Some("report-v1.txt"),
            artifact_version_id: Some("artifact-v1"),
            availability: TargetAvailability::Available,
            omission_reason: None,
            disclosure: RecipientDisclosure::FullTarget,
            retained_utf8_bytes: Some(REPORT_V1),
            external_reference_url: Some("https://publisher.invalid/a03"),
            redacted_from_entry_id: None,
        },
        EvidenceInventoryEntry {
            entry_id: "entry-report-republication",
            citation_label: "CIT-2",
            source_id: "mirror",
            source_record_id: "report-republication",
            source_record_version_id: "report-republication-version-1",
            artifact_id: Some("report-v1.txt"),
            artifact_version_id: Some("artifact-v1"),
            availability: TargetAvailability::Available,
            omission_reason: None,
            disclosure: RecipientDisclosure::FullTarget,
            retained_utf8_bytes: Some(REPORT_V1),
            external_reference_url: Some("https://mirror.invalid/a03"),
            redacted_from_entry_id: None,
        },
        EvidenceInventoryEntry {
            entry_id: "entry-report-correction",
            citation_label: "CIT-3",
            source_id: "publisher",
            source_record_id: "report-correction",
            source_record_version_id: "report-correction-version-1",
            artifact_id: Some("report-v2.txt"),
            artifact_version_id: Some("artifact-v2"),
            availability: TargetAvailability::Available,
            omission_reason: None,
            disclosure: RecipientDisclosure::FullTarget,
            retained_utf8_bytes: Some(REPORT_V2),
            external_reference_url: Some("https://publisher.invalid/a03"),
            redacted_from_entry_id: None,
        },
        EvidenceInventoryEntry {
            entry_id: "entry-conflicting",
            citation_label: "CIT-4",
            source_id: "independent",
            source_record_id: "conflicting-account",
            source_record_version_id: "conflicting-account-version-1",
            artifact_id: Some("conflicting.txt"),
            artifact_version_id: Some("artifact-conflicting-v1"),
            availability: TargetAvailability::Available,
            omission_reason: None,
            disclosure: RecipientDisclosure::FullTarget,
            retained_utf8_bytes: Some(CONFLICTING),
            external_reference_url: Some("https://independent.invalid/a03"),
            redacted_from_entry_id: None,
        },
        EvidenceInventoryEntry {
            entry_id: "entry-release-a",
            citation_label: "CIT-5",
            source_id: "archive",
            source_record_id: "release-a",
            source_record_version_id: "release-a-version-1",
            artifact_id: Some("redacted-a.txt"),
            artifact_version_id: Some("artifact-redacted-a-v1"),
            availability: TargetAvailability::Available,
            omission_reason: None,
            disclosure: RecipientDisclosure::FullTarget,
            retained_utf8_bytes: Some(REDACTED_A),
            external_reference_url: Some("https://archive.invalid/a03"),
            redacted_from_entry_id: None,
        },
        EvidenceInventoryEntry {
            entry_id: "entry-untrusted",
            citation_label: "CIT-6",
            source_id: "adversarial",
            source_record_id: "untrusted-source",
            source_record_version_id: "untrusted-source-version-1",
            artifact_id: Some("untrusted.txt"),
            artifact_version_id: Some("artifact-untrusted-v1"),
            availability: TargetAvailability::Available,
            omission_reason: None,
            disclosure: RecipientDisclosure::FullTarget,
            retained_utf8_bytes: Some(UNTRUSTED),
            external_reference_url: None,
            redacted_from_entry_id: None,
        },
        EvidenceInventoryEntry {
            entry_id: "entry-reference-only",
            citation_label: "CIT-7",
            source_id: "restricted",
            source_record_id: "reference-only",
            source_record_version_id: "reference-only-version-1",
            artifact_id: None,
            artifact_version_id: None,
            availability: TargetAvailability::ReferenceOnly,
            omission_reason: Some(OmissionReason::RetentionProhibited),
            disclosure: RecipientDisclosure::MetadataOnly,
            retained_utf8_bytes: None,
            external_reference_url: Some("https://restricted.invalid/a03"),
            redacted_from_entry_id: None,
        },
        EvidenceInventoryEntry {
            entry_id: "entry-unredacted-sensitive",
            citation_label: "CIT-HIDDEN",
            source_id: "internal-restricted",
            source_record_id: "unredacted-record",
            source_record_version_id: "unredacted-record-version-1",
            artifact_id: None,
            artifact_version_id: None,
            availability: TargetAvailability::Restricted,
            omission_reason: Some(OmissionReason::Restricted),
            disclosure: RecipientDisclosure::ExistenceSensitive,
            retained_utf8_bytes: None,
            external_reference_url: None,
            redacted_from_entry_id: None,
        },
    ];

    let citations_q = [StatementCitation {
        entry_id: "entry-report-original",
        anchor: CitationAnchor::Utf8ByteRange {
            start: 85,
            end: 128,
            is_unadjusted_original_offset: false,
        },
    }];
    let citations_lineage = [
        StatementCitation {
            entry_id: "entry-report-original",
            anchor: CitationAnchor::Utf8ByteRange {
                start: 85,
                end: 128,
                is_unadjusted_original_offset: false,
            },
        },
        StatementCitation {
            entry_id: "entry-report-republication",
            anchor: CitationAnchor::Utf8ByteRange {
                start: 85,
                end: 128,
                is_unadjusted_original_offset: false,
            },
        },
        StatementCitation {
            entry_id: "entry-report-correction",
            anchor: CitationAnchor::Utf8ByteRange {
                start: 85,
                end: 142,
                is_unadjusted_original_offset: false,
            },
        },
    ];
    let citations_support = [
        StatementCitation {
            entry_id: "entry-release-a",
            anchor: CitationAnchor::Utf8ByteRange {
                start: 46,
                end: 109,
                is_unadjusted_original_offset: false,
            },
        },
        StatementCitation {
            entry_id: "entry-reference-only",
            anchor: CitationAnchor::Utf8ByteRange {
                start: 0,
                end: 32,
                is_unadjusted_original_offset: false,
            },
        },
    ];
    let citations_conflict = [
        StatementCitation {
            entry_id: "entry-conflicting",
            anchor: CitationAnchor::Utf8ByteRange {
                start: 46,
                end: 118,
                is_unadjusted_original_offset: false,
            },
        },
        StatementCitation {
            entry_id: "entry-untrusted",
            anchor: CitationAnchor::Utf8ByteRange {
                start: 108,
                end: 197,
                is_unadjusted_original_offset: false,
            },
        },
    ];

    let blocks = [
        ReportBlock {
            block_id: "blk-1",
            statement_id: "stmt-scope-1",
            section: ReportSection::QuestionAndScope,
            actor_attribution: "analyst:cameron",
            origin_class: OriginClass::Assumption,
            review_status: ReviewStatus::Reviewed,
            chronology_time: None,
            uncertainty_note: Some("Event time and location remain unknown in synthetic A03."),
            source_limitation_note: None,
            text: "Evaluate the synthetic A03 light-count accounts across original, mirror, corrected, and conflicting releases.",
            citations: &citations_q,
            sensitive_dependency_entry_ids: &[],
        },
        ReportBlock {
            block_id: "blk-2",
            statement_id: "stmt-chronology-1",
            section: ReportSection::Chronology,
            actor_attribution: "analyst:cameron",
            origin_class: OriginClass::SourceAssertion,
            review_status: ReviewStatus::Reviewed,
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
            source_limitation_note: None,
            text: "Initial synthetic edition 1 and mirror republication share identical UTF-8 bytes while retaining separate citation identities; edition 2 corrects the reported count.",
            citations: &citations_lineage,
            sensitive_dependency_entry_ids: &[],
        },
        ReportBlock {
            block_id: "blk-3",
            statement_id: "stmt-support-1",
            section: ReportSection::SupportingEvidence,
            actor_attribution: "model:grounded-drafter-v1",
            origin_class: OriginClass::Generated,
            review_status: ReviewStatus::Unreviewed,
            chronology_time: None,
            uncertainty_note: None,
            source_limitation_note: Some(
                "CIT-7 prohibits local byte retention; its anchor remains unvalidated.",
            ),
            text: "Archive Release A (a provider-published redacted release with an alternate_redaction relation to Release B) preserves the two-light count in its own byte coordinate space while the restricted source remains reference-only.",
            citations: &citations_support,
            sensitive_dependency_entry_ids: &[],
        },
        ReportBlock {
            block_id: "blk-4",
            statement_id: "stmt-conflict-1",
            section: ReportSection::ConflictingEvidence,
            actor_attribution: "analyst:cameron",
            origin_class: OriginClass::SourceAssertion,
            review_status: ReviewStatus::Reviewed,
            chronology_time: None,
            uncertainty_note: Some(
                "Conflicting account reports one light rather than two or three.",
            ),
            source_limitation_note: Some(
                "Adversarial source text is quoted inertly and executes no instructions.",
            ),
            text: "An independent synthetic account contradicts edition 1, and an adversarial fixture passage is retained strictly as inert quoted data.",
            citations: &citations_conflict,
            sensitive_dependency_entry_ids: &[],
        },
    ];

    let snapshot = ReportSnapshot {
        report_id: "rep-a03-demo",
        report_version_id: "rep-a03-demo-v1",
        case_id: "case-a03-synthetic",
        case_version_id: "case-a03-synthetic-v1",
        title: "SYNTHETIC A03 REPORT PROJECTION DEMO (NOT OBSERVED EVIDENCE)",
        snapshot_created_at: TimeDisplay {
            text: "2026-11-02T19:00:00Z",
            precision: TimePrecision::ExactInstant,
            time_reference: "UTC",
        },
        reassessment_status: ReassessmentStatus::NeedsReassessment {
            flagging_record_version_id: "report-correction-version-1",
            reason: "report-correction supersedes report-original (two lights -> three lights)",
        },
        coverage_summary: "Partial synthetic A03 coverage: 6 available UTF-8 artifacts and 1 reference-only source selected for this projection.",
        redaction_policy_id: "recipient-collaborator-redacted-v1",
        generic_omission_note: None,
        evidence_entries: &entries,
        blocks: &blocks,
    };

    let projection = preview_report(&snapshot, limits, check_control).map_err(|e| e.to_string())?;
    let mut stdout = io::stdout().lock();

    match mode {
        OutputMode::Summary => {
            let counts = projection.counts();
            writeln!(
                stdout,
                "status=ok report={}@{} visible_entries={} blocks={} full_targets={} reference_only={} validated_citations={} unvalidated_citations={}",
                projection.report_id(),
                projection.report_version_id(),
                projection.visible_inventory().len(),
                projection.blocks().len(),
                counts.included_full_targets,
                counts.reference_only_targets,
                counts.validated_citations,
                counts.unvalidated_citations,
            )
            .map_err(|e| e.to_string())?;
        }
        OutputMode::Markdown => {
            let md = render_markdown(&projection, check_control).map_err(|e| e.to_string())?;
            stdout.write_all(md.as_bytes()).map_err(|e| e.to_string())?;
        }
        OutputMode::Html => {
            let html = render_html(&projection, check_control).map_err(|e| e.to_string())?;
            stdout
                .write_all(html.as_bytes())
                .map_err(|e| e.to_string())?;
        }
    }
    stdout.flush().map_err(|e| e.to_string())
}
