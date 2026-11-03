use std::collections::BTreeMap;
use std::error::Error;

use oia_documents::StopReason;
use oia_documents::lineage::{
    ArtifactDescriptor, AvailabilityState, ClaimCorroborationInputs, ComparisonSegmentKind,
    CycleClassification, DerivationNode, DerivationOriginClass, EarliestSourceResolution,
    ExactDuplicateGroup, InvalidationNotice, InvalidationReason, LineageError, LineageLimit,
    LineageLimits, PageAttachment, RecordRelation, RelationKind, ReleaseComparisonSegment,
    ReleaseRedactionComparison, RetentionMode, SourceLifecycleState, SourceRecordDescriptor,
    UnresolvedExternalCitation, Utf8ByteCitation, compute_exact_duplicate_groups,
    compute_invalidation_propagation, compute_lineage_sccs, evaluate_claim_corroboration,
    evaluate_corroboration, validate_records, verify_release_comparison, verify_utf8_byte_citation,
};

struct LineageFixture {
    artifacts: BTreeMap<String, ArtifactDescriptor>,
    payloads: BTreeMap<String, Vec<u8>>,
    records: Vec<SourceRecordDescriptor>,
    relations: Vec<RecordRelation>,
    derivations: Vec<DerivationNode>,
    citations: Vec<Utf8ByteCitation>,
    comparisons: Vec<ReleaseRedactionComparison>,
    citing_inputs: Vec<String>,
}

fn build_fixture() -> LineageFixture {
    let raw_files: [(&str, &str, &[u8]); 13] = [
        (
            "dispatch-root-v1.txt",
            "8a4f33b3889b6650f1d36d1843d05fdb86edc297c013b5cbc9006358b4391539",
            include_bytes!("../../../fixtures/documents/lineage/originals/dispatch-root-v1.txt"),
        ),
        (
            "dispatch-root-v2-corrected.txt",
            "cbccc2264187b7fb2384426cce2ad2e5f74c29d647129a23dea86ab2e2bd9cbd",
            include_bytes!(
                "../../../fixtures/documents/lineage/originals/dispatch-root-v2-corrected.txt"
            ),
        ),
        (
            "syndicated-quote-v1.txt",
            "3dc3e0a1f84aab4a8fb9d9d4ead28dedb5ade3ad9df0e4c7582caab07d525f4c",
            include_bytes!("../../../fixtures/documents/lineage/originals/syndicated-quote-v1.txt"),
        ),
        (
            "independent-log-v1.txt",
            "cbe2783010d93ddc20adf09e3d6f7762263d4d3cc820ea9d8d2a1f3dc7e1794b",
            include_bytes!("../../../fixtures/documents/lineage/originals/independent-log-v1.txt"),
        ),
        (
            "cycle-node-a.txt",
            "358b5de399a70e004a4951f8710b2d5bc9526398bf22cbecd98d421ea42f7d41",
            include_bytes!("../../../fixtures/documents/lineage/originals/cycle-node-a.txt"),
        ),
        (
            "cycle-node-b.txt",
            "698800d787c490dd00c6bbe630e23c1c9343df4c210d4ca95f5ac531d6625507",
            include_bytes!("../../../fixtures/documents/lineage/originals/cycle-node-b.txt"),
        ),
        (
            "cycle-node-c.txt",
            "18989a22b1cd518c0fe77dcd45a49e509aa6e0abb0c298f654c9e204ab5c7307",
            include_bytes!("../../../fixtures/documents/lineage/originals/cycle-node-c.txt"),
        ),
        (
            "foia-release-2024-redacted.txt",
            "0e6383f5d713148f5976b2398514bcb7f45fbeb3672822d479184f6485c5016d",
            include_bytes!(
                "../../../fixtures/documents/lineage/originals/foia-release-2024-redacted.txt"
            ),
        ),
        (
            "foia-release-2026-declassified.txt",
            "4a0b88fcfb48e9e827f74358cf4eccdf256c281d9ca6e85e73fc0e781ba14059",
            include_bytes!(
                "../../../fixtures/documents/lineage/originals/foia-release-2026-declassified.txt"
            ),
        ),
        (
            "foia-release-partner-redacted.txt",
            "1ccac973452a622cbdd098bb3c555a67e759c28e61ea685e7cf59ee0666776c4",
            include_bytes!(
                "../../../fixtures/documents/lineage/originals/foia-release-partner-redacted.txt"
            ),
        ),
        (
            "retracted-advisory-v1.txt",
            "191c073e8afe5f157574d31d3881562f3bf3218d7ed9b9f288f06b427afe25fc",
            include_bytes!(
                "../../../fixtures/documents/lineage/originals/retracted-advisory-v1.txt"
            ),
        ),
        (
            "retraction-notice-v2.txt",
            "177fe70ecb5f61ec64b6cb7266b8a3cfcb78e665a212ebed693246d79652e4a0",
            include_bytes!(
                "../../../fixtures/documents/lineage/originals/retraction-notice-v2.txt"
            ),
        ),
        (
            "removed-endpoint-v1.txt",
            "34ec59fc4c4cc74cd3d702b57f971b0823822c2458b839dee2b8e6663a257013",
            include_bytes!("../../../fixtures/documents/lineage/originals/removed-endpoint-v1.txt"),
        ),
    ];

    let mut artifacts = BTreeMap::new();
    let mut payloads = BTreeMap::new();
    for (id, sha256, bytes) in raw_files {
        artifacts.insert(
            id.to_owned(),
            ArtifactDescriptor {
                id: id.to_owned(),
                sha256: sha256.to_owned(),
                byte_length: bytes.len(),
            },
        );
        payloads.insert(id.to_owned(), bytes.to_vec());
    }

    let retained_record = |id: &str,
                           version_id: &str,
                           source_id: &str,
                           availability: AvailabilityState,
                           lifecycle_state: SourceLifecycleState,
                           predecessor: Option<&str>,
                           event_time: Option<&str>,
                           publication_time: Option<&str>,
                           retrieval_time: &str,
                           unknown_reason: Option<&str>,
                           artifact_id: &str| {
        SourceRecordDescriptor {
            id: id.to_owned(),
            version_id: version_id.to_owned(),
            source_id: source_id.to_owned(),
            availability,
            lifecycle_state,
            predecessor_version_id: predecessor.map(str::to_owned),
            event_time: event_time.map(str::to_owned),
            publication_time: publication_time.map(str::to_owned),
            retrieval_time: retrieval_time.to_owned(),
            unknown_time_reason: unknown_reason.map(str::to_owned),
            retention: RetentionMode::Retained,
            omission_reason: None,
            pages: vec![PageAttachment {
                number: 1,
                artifact_id: Some(artifact_id.to_owned()),
            }],
        }
    };

    let records = vec![
        retained_record(
            "dispatch-primary",
            "dispatch-primary-v1",
            "archive-alpha",
            AvailabilityState::Available,
            SourceLifecycleState::SupersededByCorrection,
            None,
            Some("2026-09-30T22:10:00Z"),
            Some("2026-10-01T04:15:00Z"),
            "2026-10-01T05:00:00Z",
            None,
            "dispatch-root-v1.txt",
        ),
        retained_record(
            "dispatch-mirror",
            "dispatch-mirror-v1",
            "mirror-beta",
            AvailabilityState::Available,
            SourceLifecycleState::Active,
            None,
            Some("2026-09-30T22:10:00Z"),
            Some("2026-10-02T09:00:00Z"),
            "2026-10-02T10:00:00Z",
            None,
            "dispatch-root-v1.txt",
        ),
        retained_record(
            "dispatch-primary",
            "dispatch-primary-v2",
            "archive-alpha",
            AvailabilityState::Available,
            SourceLifecycleState::Active,
            Some("dispatch-primary-v1"),
            Some("2026-09-30T22:10:00Z"),
            Some("2026-10-05T12:00:00Z"),
            "2026-10-05T12:30:00Z",
            None,
            "dispatch-root-v2-corrected.txt",
        ),
        retained_record(
            "regional-digest",
            "regional-digest-v1",
            "digest-gamma",
            AvailabilityState::Available,
            SourceLifecycleState::Active,
            None,
            None,
            Some("2026-10-03T18:30:00Z"),
            "2026-10-03T19:00:00Z",
            Some("digest omits underlying observation timestamp"),
            "syndicated-quote-v1.txt",
        ),
        retained_record(
            "harbor-radar-log",
            "harbor-radar-log-v1",
            "independent-observatory",
            AvailabilityState::Available,
            SourceLifecycleState::Active,
            None,
            None,
            None,
            "2026-10-04T08:00:00Z",
            Some("undated archival log sheet; no timestamp invented"),
            "independent-log-v1.txt",
        ),
        SourceRecordDescriptor {
            id: "restricted-deck-log".to_owned(),
            version_id: "restricted-deck-log-v1".to_owned(),
            source_id: "restricted-custodian".to_owned(),
            availability: AvailabilityState::ReferenceOnly,
            lifecycle_state: SourceLifecycleState::Active,
            predecessor_version_id: None,
            event_time: Some("2026-09-30T22:11:00Z".to_owned()),
            publication_time: Some("2026-10-01T01:00:00Z".to_owned()),
            retrieval_time: "2026-10-01T06:00:00Z".to_owned(),
            unknown_time_reason: None,
            retention: RetentionMode::ReferenceOnly,
            omission_reason: Some(
                "source license prohibits payload retention; catalog metadata only".to_owned(),
            ),
            pages: Vec::new(),
        },
        retained_record(
            "cycle-memo-a",
            "cycle-memo-a-v1",
            "commentary-network",
            AvailabilityState::Available,
            SourceLifecycleState::Active,
            None,
            None,
            Some("2026-10-06T10:00:00Z"),
            "2026-10-06T12:00:00Z",
            Some("commentary omits primary event time"),
            "cycle-node-a.txt",
        ),
        retained_record(
            "cycle-bulletin-b",
            "cycle-bulletin-b-v1",
            "commentary-network",
            AvailabilityState::Available,
            SourceLifecycleState::Active,
            None,
            None,
            Some("2026-10-06T11:00:00Z"),
            "2026-10-06T12:05:00Z",
            Some("bulletin omits primary event time"),
            "cycle-node-b.txt",
        ),
        retained_record(
            "cycle-review-c",
            "cycle-review-c-v1",
            "commentary-network",
            AvailabilityState::Available,
            SourceLifecycleState::Active,
            None,
            None,
            Some("2026-10-06T11:30:00Z"),
            "2026-10-06T12:10:00Z",
            Some("review omits primary event time"),
            "cycle-node-c.txt",
        ),
        retained_record(
            "foia-case-2024-09",
            "foia-release-2024-v1",
            "foia-reading-room",
            AvailabilityState::Available,
            SourceLifecycleState::Active,
            None,
            None,
            Some("2024-11-15T00:00:00Z"),
            "2026-10-01T00:00:00Z",
            Some("underlying incident date redacted in 2024 release"),
            "foia-release-2024-redacted.txt",
        ),
        retained_record(
            "foia-case-2024-09",
            "foia-release-2026-v1",
            "foia-reading-room",
            AvailabilityState::Available,
            SourceLifecycleState::Active,
            Some("foia-release-2024-v1"),
            None,
            Some("2026-08-20T00:00:00Z"),
            "2026-10-01T00:05:00Z",
            Some("underlying incident date not stated in release header"),
            "foia-release-2026-declassified.txt",
        ),
        retained_record(
            "foia-case-2024-09-partner",
            "foia-release-partner-v1",
            "partner-reading-room",
            AvailabilityState::Available,
            SourceLifecycleState::Active,
            None,
            None,
            Some("2025-03-10T00:00:00Z"),
            "2026-10-01T00:10:00Z",
            Some("underlying incident date not stated in partner release"),
            "foia-release-partner-redacted.txt",
        ),
        retained_record(
            "calibration-advisory",
            "retracted-advisory-v1",
            "calibration-board",
            AvailabilityState::Available,
            SourceLifecycleState::Retracted,
            None,
            None,
            Some("2026-10-02T14:00:00Z"),
            "2026-10-02T15:00:00Z",
            Some("advisory covers bench calibration rather than field event"),
            "retracted-advisory-v1.txt",
        ),
        retained_record(
            "calibration-advisory-retraction",
            "retraction-notice-v2",
            "calibration-board",
            AvailabilityState::Available,
            SourceLifecycleState::Active,
            Some("retracted-advisory-v1"),
            None,
            Some("2026-10-07T09:30:00Z"),
            "2026-10-07T10:00:00Z",
            Some("retraction notice timestamped by publication only"),
            "retraction-notice-v2.txt",
        ),
        retained_record(
            "municipal-weather-notice",
            "removed-endpoint-v1",
            "municipal-portal",
            AvailabilityState::Removed,
            SourceLifecycleState::Active,
            None,
            None,
            Some("2026-10-01T08:00:00Z"),
            "2026-10-01T08:30:00Z",
            Some("scheduled window omitted in notice snapshot"),
            "removed-endpoint-v1.txt",
        ),
    ];

    let relations = vec![
        RecordRelation {
            from: "dispatch-mirror-v1".to_owned(),
            to: "dispatch-primary-v1".to_owned(),
            kind: RelationKind::Republishes,
        },
        RecordRelation {
            from: "dispatch-primary-v2".to_owned(),
            to: "dispatch-primary-v1".to_owned(),
            kind: RelationKind::Supersedes,
        },
        RecordRelation {
            from: "regional-digest-v1".to_owned(),
            to: "dispatch-mirror-v1".to_owned(),
            kind: RelationKind::Quotes,
        },
        RecordRelation {
            from: "cycle-memo-a-v1".to_owned(),
            to: "cycle-bulletin-b-v1".to_owned(),
            kind: RelationKind::Cites,
        },
        RecordRelation {
            from: "cycle-bulletin-b-v1".to_owned(),
            to: "cycle-review-c-v1".to_owned(),
            kind: RelationKind::Quotes,
        },
        RecordRelation {
            from: "cycle-review-c-v1".to_owned(),
            to: "cycle-memo-a-v1".to_owned(),
            kind: RelationKind::Cites,
        },
        RecordRelation {
            from: "foia-release-2026-v1".to_owned(),
            to: "foia-release-2024-v1".to_owned(),
            kind: RelationKind::AlternateRedaction,
        },
        RecordRelation {
            from: "foia-release-partner-v1".to_owned(),
            to: "foia-release-2024-v1".to_owned(),
            kind: RelationKind::AlternateRedaction,
        },
        RecordRelation {
            from: "retraction-notice-v2".to_owned(),
            to: "retracted-advisory-v1".to_owned(),
            kind: RelationKind::RetractedBy,
        },
        RecordRelation {
            from: "retraction-notice-v2".to_owned(),
            to: "retracted-advisory-v1".to_owned(),
            kind: RelationKind::Quotes,
        },
        RecordRelation {
            from: "dispatch-primary-v2".to_owned(),
            to: "cycle-memo-a-v1".to_owned(),
            kind: RelationKind::Cites,
        },
    ];

    let derivations = vec![
        DerivationNode {
            version_id: "claim-bearing-045-v1".to_owned(),
            origin_class: DerivationOriginClass::ExtractedClaim,
            derived_from: vec!["dispatch-primary-v1".to_owned()],
        },
        DerivationNode {
            version_id: "summary-coastal-track-v1".to_owned(),
            origin_class: DerivationOriginClass::AnalyticalInference,
            derived_from: vec![
                "claim-bearing-045-v1".to_owned(),
                "harbor-radar-log-v1".to_owned(),
            ],
        },
        DerivationNode {
            version_id: "report-coastal-dossier-v1".to_owned(),
            origin_class: DerivationOriginClass::AnalyticalInference,
            derived_from: vec!["summary-coastal-track-v1".to_owned()],
        },
        DerivationNode {
            version_id: "claim-sensor-fault-v1".to_owned(),
            origin_class: DerivationOriginClass::ExtractedClaim,
            derived_from: vec!["retracted-advisory-v1".to_owned()],
        },
        DerivationNode {
            version_id: "summary-foia-overview-v1".to_owned(),
            origin_class: DerivationOriginClass::AnalyticalInference,
            derived_from: vec![
                "foia-release-2024-v1".to_owned(),
                "removed-endpoint-v1".to_owned(),
            ],
        },
    ];

    let citations = vec![
        Utf8ByteCitation {
            record_version_id: "dispatch-primary-v1".to_owned(),
            artifact_id: "dispatch-root-v1.txt".to_owned(),
            page: 1,
            byte_range: 108..171,
            quote: "Optical track bearing 045 deg true over synthetic sector Alpha.".to_owned(),
        },
        Utf8ByteCitation {
            record_version_id: "dispatch-primary-v2".to_owned(),
            artifact_id: "dispatch-root-v2-corrected.txt".to_owned(),
            page: 1,
            byte_range: 120..198,
            quote: "Correction: Optical track bearing 054 deg true after magnetic declination fix."
                .to_owned(),
        },
        Utf8ByteCitation {
            record_version_id: "regional-digest-v1".to_owned(),
            artifact_id: "syndicated-quote-v1.txt".to_owned(),
            page: 1,
            byte_range: 138..203,
            quote: "\"Optical track bearing 045 deg true over synthetic sector Alpha.\"".to_owned(),
        },
        Utf8ByteCitation {
            record_version_id: "harbor-radar-log-v1".to_owned(),
            artifact_id: "independent-log-v1.txt".to_owned(),
            page: 1,
            byte_range: 108..190,
            quote:
                "Visual log recorded luminous transit along bearing 053 deg true from pier station."
                    .to_owned(),
        },
        Utf8ByteCitation {
            record_version_id: "retracted-advisory-v1".to_owned(),
            artifact_id: "retracted-advisory-v1.txt".to_owned(),
            page: 1,
            byte_range: 100..169,
            quote: "Anomaly attributed to thermal sensor latch fault in synthetic unit 4."
                .to_owned(),
        },
        Utf8ByteCitation {
            record_version_id: "retraction-notice-v2".to_owned(),
            artifact_id: "retraction-notice-v2.txt".to_owned(),
            page: 1,
            byte_range: 117..205,
            quote:
                "Retracts Advisory 2026-07; bench test confirmed thermal sensor latch operated nominally."
                    .to_owned(),
        },
    ];

    let comparisons = vec![
        ReleaseRedactionComparison {
            comparison_id: "foia-2024-vs-2026".to_owned(),
            left_record_version_id: "foia-release-2024-v1".to_owned(),
            right_record_version_id: "foia-release-2026-v1".to_owned(),
            forbidden_cleartext_in_left: vec!["Sector-7 Telemetry".to_owned()],
            segments: vec![
                ReleaseComparisonSegment {
                    page: 1,
                    segment_kind: ComparisonSegmentKind::UnchangedUnredacted,
                    left_byte_range: Some(52..99),
                    right_byte_range: Some(52..99),
                    left_text: Some("Document: Declassification Review Case 2024-09\n".to_owned()),
                    right_text: Some("Document: Declassification Review Case 2024-09\n".to_owned()),
                },
                ReleaseComparisonSegment {
                    page: 1,
                    segment_kind: ComparisonSegmentKind::NewlyDisclosedInRight,
                    left_byte_range: Some(105..142),
                    right_byte_range: Some(105..123),
                    left_text: Some("[REDACTED PER EXEMPTION B1: 18 CHARS]".to_owned()),
                    right_text: Some("Sector-7 Telemetry".to_owned()),
                },
                ReleaseComparisonSegment {
                    page: 1,
                    segment_kind: ComparisonSegmentKind::UnchangedUnredacted,
                    left_byte_range: Some(152..179),
                    right_byte_range: Some(133..160),
                    left_text: Some("Operator Callsign Kestrel-4".to_owned()),
                    right_text: Some("Operator Callsign Kestrel-4".to_owned()),
                },
                ReleaseComparisonSegment {
                    page: 1,
                    segment_kind: ComparisonSegmentKind::DifferentlyRedacted,
                    left_byte_range: Some(188..201),
                    right_byte_range: Some(169..191),
                    left_text: Some("[REDACTED B1]".to_owned()),
                    right_text: Some("[WITHHELD SECTION 3.2]".to_owned()),
                },
            ],
        },
        ReleaseRedactionComparison {
            comparison_id: "foia-2024-vs-partner".to_owned(),
            left_record_version_id: "foia-release-2024-v1".to_owned(),
            right_record_version_id: "foia-release-partner-v1".to_owned(),
            forbidden_cleartext_in_left: vec!["Sector-7 Telemetry".to_owned()],
            segments: vec![
                ReleaseComparisonSegment {
                    page: 1,
                    segment_kind: ComparisonSegmentKind::UnchangedUnredacted,
                    left_byte_range: Some(52..99),
                    right_byte_range: Some(52..99),
                    left_text: Some("Document: Declassification Review Case 2024-09\n".to_owned()),
                    right_text: Some("Document: Declassification Review Case 2024-09\n".to_owned()),
                },
                ReleaseComparisonSegment {
                    page: 1,
                    segment_kind: ComparisonSegmentKind::UnchangedUnredacted,
                    left_byte_range: Some(105..142),
                    right_byte_range: Some(105..142),
                    left_text: Some("[REDACTED PER EXEMPTION B1: 18 CHARS]".to_owned()),
                    right_text: Some("[REDACTED PER EXEMPTION B1: 18 CHARS]".to_owned()),
                },
                ReleaseComparisonSegment {
                    page: 1,
                    segment_kind: ComparisonSegmentKind::NewlyRedactedInRight,
                    left_byte_range: Some(152..179),
                    right_byte_range: Some(152..187),
                    left_text: Some("Operator Callsign Kestrel-4".to_owned()),
                    right_text: Some("[REDACTED PER PARTNER PRIVACY RULE]".to_owned()),
                },
                ReleaseComparisonSegment {
                    page: 1,
                    segment_kind: ComparisonSegmentKind::DifferentlyRedacted,
                    left_byte_range: Some(188..201),
                    right_byte_range: Some(196..218),
                    left_text: Some("[REDACTED B1]".to_owned()),
                    right_text: Some("[WITHHELD SECTION 3.2]".to_owned()),
                },
            ],
        },
    ];

    let citing_inputs = vec![
        "dispatch-primary-v1".to_owned(),
        "dispatch-mirror-v1".to_owned(),
        "regional-digest-v1".to_owned(),
        "harbor-radar-log-v1".to_owned(),
        "cycle-memo-a-v1".to_owned(),
        "cycle-bulletin-b-v1".to_owned(),
        "cycle-review-c-v1".to_owned(),
        "summary-coastal-track-v1".to_owned(),
    ];

    LineageFixture {
        artifacts,
        payloads,
        records,
        relations,
        derivations,
        citations,
        comparisons,
        citing_inputs,
    }
}

// DOC-02/DOC-04/DOC-05/INTEL-01 (A03/A08): verify exact-byte duplicate grouping,
// UTF-8 byte citations, release comparisons, Tarjan SCC cycle detection,
// corroboration independence, and transitive needs_reassessment propagation.
#[test]
fn lineage_fixture_satisfies_a03_and_a08_contracts() -> Result<(), Box<dyn Error>> {
    let LineageFixture {
        artifacts,
        payloads,
        records: records_list,
        relations,
        derivations: derivations_list,
        citations,
        comparisons,
        citing_inputs,
    } = build_fixture();
    let limits = LineageLimits::new(200, 500, 100)?;
    let records = validate_records(&records_list, &artifacts, limits, || Ok(()))?;

    let dup_groups = compute_exact_duplicate_groups(&records_list, &artifacts, limits, || Ok(()))?;
    assert_eq!(
        dup_groups,
        vec![ExactDuplicateGroup {
            artifact_id: "dispatch-root-v1.txt".to_owned(),
            sha256: "8a4f33b3889b6650f1d36d1843d05fdb86edc297c013b5cbc9006358b4391539".to_owned(),
            member_record_versions: vec![
                "dispatch-primary-v1".to_owned(),
                "dispatch-mirror-v1".to_owned(),
            ],
            distinct_source_ids: vec!["archive-alpha".to_owned(), "mirror-beta".to_owned()],
        }]
    );

    for citation in &citations {
        verify_utf8_byte_citation(citation, &records, &payloads, || Ok(()))?;
    }
    for comparison in &comparisons {
        verify_release_comparison(comparison, &records, &payloads, limits, || Ok(()))?;
    }

    let (_, scc_report) = compute_lineage_sccs(&records, &relations, limits, || Ok(()))?;
    assert_eq!(scc_report.detected_cycles.len(), 1);
    assert_eq!(
        scc_report.detected_cycles.first().map(|c| c.classification),
        Some(CycleClassification::UnanchoredCycle)
    );

    let derivations_map: BTreeMap<String, DerivationNode> = derivations_list
        .iter()
        .map(|d| (d.version_id.clone(), d.clone()))
        .collect();
    let corr = evaluate_corroboration(
        &citing_inputs,
        &records,
        &derivations_map,
        &relations,
        &artifacts,
        limits,
        || Ok(()),
    )?;
    assert_eq!(
        corr.supporting_record_versions,
        vec![
            "cycle-bulletin-b-v1",
            "cycle-memo-a-v1",
            "cycle-review-c-v1",
            "dispatch-mirror-v1",
            "dispatch-primary-v1",
            "harbor-radar-log-v1",
            "regional-digest-v1",
        ]
    );
    assert_eq!(
        corr.excluded_derivative_runs,
        vec!["summary-coastal-track-v1"]
    );
    assert_eq!(
        corr.circular_only_record_versions,
        vec![
            "cycle-bulletin-b-v1",
            "cycle-memo-a-v1",
            "cycle-review-c-v1",
        ]
    );
    assert_eq!(
        corr.primary_cycle_witness,
        Some(vec![
            "cycle-bulletin-b-v1".to_owned(),
            "cycle-review-c-v1".to_owned(),
            "cycle-memo-a-v1".to_owned(),
            "cycle-bulletin-b-v1".to_owned(),
        ])
    );
    assert_eq!(
        corr.independent_supporting_roots,
        vec!["dispatch-primary-v1", "harbor-radar-log-v1"]
    );
    assert_eq!(corr.independent_root_count, 2);
    assert_eq!(corr.exact_duplicate_group_count, 6);
    assert!(corr.contradicting_independent_roots.is_empty());
    assert!(
        corr.earliest_known_source_view
            .coverage_qualification
            .unresolved_external_citations
            .is_empty()
    );
    assert_eq!(
        corr.earliest_known_source_view.resolution,
        EarliestSourceResolution::AmbiguousCandidates
    );
    assert_eq!(
        corr.earliest_known_source_view
            .earliest_dated_root
            .as_deref(),
        Some("dispatch-primary-v1")
    );
    assert_eq!(
        corr.earliest_known_source_view.undated_candidate_roots,
        vec!["harbor-radar-log-v1"]
    );
    assert!(
        corr.earliest_known_source_view
            .coverage_qualification
            .omitted_reference_only_records
            .is_empty()
    );
    assert_eq!(
        corr.earliest_known_source_view
            .coverage_qualification
            .detected_cycle_count,
        1
    );

    let inv =
        compute_invalidation_propagation(&records, &derivations_list, &relations, limits, || {
            Ok(())
        })?;
    assert_eq!(
        inv.needs_reassessment,
        vec![
            InvalidationNotice {
                target_version_id: "claim-bearing-045-v1".to_owned(),
                trigger_record_version_id: "dispatch-primary-v1".to_owned(),
                replacement_record_version_id: "dispatch-primary-v2".to_owned(),
                reason: InvalidationReason::SourceCorrected,
                preserve_old_citations: true,
            },
            InvalidationNotice {
                target_version_id: "summary-coastal-track-v1".to_owned(),
                trigger_record_version_id: "dispatch-primary-v1".to_owned(),
                replacement_record_version_id: "dispatch-primary-v2".to_owned(),
                reason: InvalidationReason::SourceCorrected,
                preserve_old_citations: true,
            },
            InvalidationNotice {
                target_version_id: "report-coastal-dossier-v1".to_owned(),
                trigger_record_version_id: "dispatch-primary-v1".to_owned(),
                replacement_record_version_id: "dispatch-primary-v2".to_owned(),
                reason: InvalidationReason::SourceCorrected,
                preserve_old_citations: true,
            },
            InvalidationNotice {
                target_version_id: "claim-sensor-fault-v1".to_owned(),
                trigger_record_version_id: "retracted-advisory-v1".to_owned(),
                replacement_record_version_id: "retraction-notice-v2".to_owned(),
                reason: InvalidationReason::SourceRetracted,
                preserve_old_citations: true,
            },
        ]
    );
    assert_eq!(
        inv.unaffected_fresh_versions,
        vec!["summary-foia-overview-v1"]
    );
    assert_eq!(
        inv.removed_not_auto_retracted_record_versions,
        vec!["removed-endpoint-v1"]
    );
    Ok(())
}

// INTEL-01/DOC-02/DOC-04 (A08): stance, duplicate-hash and coverage reporting
// must remain claim-scoped and must not promote derivative runs or unrelated records.
#[test]
fn claim_stance_and_coverage_are_scoped_to_reached_ancestry() -> Result<(), Box<dyn Error>> {
    let LineageFixture {
        artifacts,
        records: records_list,
        mut relations,
        derivations: derivations_list,
        ..
    } = build_fixture();
    relations.push(RecordRelation {
        from: "retraction-notice-v2".to_owned(),
        to: "retracted-advisory-v1".to_owned(),
        kind: RelationKind::Quotes,
    });
    let limits = LineageLimits::new(200, 500, 100)?;
    let records = validate_records(&records_list, &artifacts, limits, || Ok(()))?;
    let derivations: BTreeMap<String, DerivationNode> = derivations_list
        .into_iter()
        .map(|node| (node.version_id.clone(), node))
        .collect();
    let external_citations = vec![
        UnresolvedExternalCitation {
            citing_record_version_id: "retracted-advisory-v1".to_owned(),
            reference: "Synthetic Calibration Board advisory bibliography entry CB-EXT-1"
                .to_owned(),
        },
        UnresolvedExternalCitation {
            citing_record_version_id: "retraction-notice-v2".to_owned(),
            reference: "Synthetic Calibration Board retraction bibliography entry RN-EXT-2"
                .to_owned(),
        },
        UnresolvedExternalCitation {
            citing_record_version_id: "removed-endpoint-v1".to_owned(),
            reference: "UNRELATED citation must not enter this claim".to_owned(),
        },
    ];

    let summary = evaluate_claim_corroboration(
        ClaimCorroborationInputs {
            supporting: &["retracted-advisory-v1".to_owned()],
            contradicting: &[
                "retraction-notice-v2".to_owned(),
                "summary-coastal-track-v1".to_owned(),
            ],
            external_citations: &external_citations,
        },
        &records,
        &derivations,
        &relations,
        &artifacts,
        limits,
        || Ok(()),
    )?;
    assert_eq!(summary.exact_duplicate_group_count, 1);
    assert_eq!(
        summary.independent_supporting_roots,
        vec!["retracted-advisory-v1"]
    );
    assert_eq!(
        summary.contradicting_independent_roots,
        vec!["retracted-advisory-v1"]
    );
    assert_eq!(
        summary.excluded_derivative_runs,
        vec!["summary-coastal-track-v1"]
    );
    assert_eq!(
        summary
            .earliest_known_source_view
            .coverage_qualification
            .detected_cycle_count,
        0
    );
    assert!(
        summary
            .earliest_known_source_view
            .coverage_qualification
            .omitted_reference_only_records
            .is_empty()
    );
    assert_eq!(
        summary
            .earliest_known_source_view
            .coverage_qualification
            .unresolved_external_citations,
        external_citations
            .iter()
            .take(2)
            .cloned()
            .collect::<Vec<_>>()
    );

    let oversized_citations = vec![UnresolvedExternalCitation {
        citing_record_version_id: "retracted-advisory-v1".to_owned(),
        reference: "x".repeat(oia_documents::lineage::MAX_EXTERNAL_CITATION_REFERENCE_BYTES + 1),
    }];
    assert_eq!(
        evaluate_claim_corroboration(
            ClaimCorroborationInputs {
                supporting: &["retracted-advisory-v1".to_owned()],
                contradicting: &[],
                external_citations: &oversized_citations,
            },
            &records,
            &derivations,
            &relations,
            &artifacts,
            limits,
            || Ok(()),
        ),
        Err(LineageError::InvalidExternalCitation {
            citing_record_version_id: "retracted-advisory-v1".to_owned(),
            reason: "bibliographic reference exceeds 4096 UTF-8 bytes",
        })
    );

    let coverage = evaluate_claim_corroboration(
        ClaimCorroborationInputs {
            supporting: &["restricted-deck-log-v1".to_owned()],
            contradicting: &[],
            external_citations: &external_citations,
        },
        &records,
        &derivations,
        &relations,
        &artifacts,
        limits,
        || Ok(()),
    )?;
    assert_eq!(coverage.exact_duplicate_group_count, 0);
    assert_eq!(
        coverage
            .earliest_known_source_view
            .coverage_qualification
            .omitted_reference_only_records,
        vec!["restricted-deck-log-v1"]
    );
    assert!(
        coverage
            .earliest_known_source_view
            .coverage_qualification
            .unresolved_external_citations
            .is_empty()
    );
    assert_eq!(
        coverage
            .earliest_known_source_view
            .coverage_qualification
            .detected_cycle_count,
        0
    );
    Ok(())
}

// DOC-02/DOC-04/DOC-05/INTEL-01: verify that shifted byte anchors, hidden
// cleartext inside redacted artifacts, derivation cycles, auto-retracted removed
// endpoints, resource budget overflows, and cooperative stops are rejected.
#[test]
fn adversarial_lineage_violations_and_control_stops_are_rejected() -> Result<(), Box<dyn Error>> {
    let LineageFixture {
        artifacts,
        payloads,
        records: records_list,
        relations,
        derivations: derivations_list,
        citations,
        comparisons,
        citing_inputs,
    } = build_fixture();
    let limits = LineageLimits::new(200, 500, 100)?;
    let records = validate_records(&records_list, &artifacts, limits, || Ok(()))?;

    // 1. Shifted UTF-8 byte anchor or mid-codepoint split must be rejected.
    let mut shifted_citation = citations
        .first()
        .cloned()
        .ok_or("missing fixture citation")?;
    shifted_citation.byte_range =
        (shifted_citation.byte_range.start + 1)..shifted_citation.byte_range.end;
    assert_eq!(
        verify_utf8_byte_citation(&shifted_citation, &records, &payloads, || Ok(())),
        Err(LineageError::CitationQuoteMismatch {
            record_version_id: "dispatch-primary-v1".to_owned(),
            artifact_id: "dispatch-root-v1.txt".to_owned(),
        })
    );
    // Split the 3-byte UTF-8 em-dash ('—' at byte 26..29 in header).
    let mut mid_codepoint_citation = shifted_citation.clone();
    mid_codepoint_citation.byte_range = 27..35;
    assert_eq!(
        verify_utf8_byte_citation(&mid_codepoint_citation, &records, &payloads, || Ok(())),
        Err(LineageError::InvalidByteAnchorRange {
            artifact_id: "dispatch-root-v1.txt".to_owned(),
            start: 27,
            end: 35,
            byte_length: 218,
        })
    );

    // 2. Hidden cleartext inside a redacted artifact must be rejected.
    let mut bad_payloads = payloads.clone();
    if let Some(redacted) = bad_payloads.get_mut("foia-release-2024-redacted.txt") {
        redacted.extend_from_slice(b"\nHidden: Sector-7 Telemetry\n");
    }
    let first_comparison = comparisons
        .first()
        .ok_or("missing fixture release comparison")?;
    assert_eq!(
        verify_release_comparison(first_comparison, &records, &bad_payloads, limits, || Ok(())),
        Err(LineageError::HiddenRedactionPayloadRejected {
            comparison_id: "foia-2024-vs-2026".to_owned(),
            artifact_id: "foia-release-2024-redacted.txt".to_owned(),
            forbidden_cleartext: "Sector-7 Telemetry".to_owned(),
        })
    );

    // 3. Cycle in derived_from DAG must be rejected.
    let mut cyclic_derivations = derivations_list.clone();
    if let Some(first_deriv) = cyclic_derivations.first_mut() {
        first_deriv
            .derived_from
            .push("report-coastal-dossier-v1".to_owned());
    }
    assert_eq!(
        compute_invalidation_propagation(&records, &cyclic_derivations, &relations, limits, || {
            Ok(())
        }),
        Err(LineageError::DerivationCycleDetected {
            version_id: "claim-bearing-045-v1".to_owned(),
        })
    );

    // 4. Removed endpoint auto-marked retracted must be rejected.
    let mut bad_removed_records = records.clone();
    if let Some(removed) = bad_removed_records.get_mut("removed-endpoint-v1") {
        removed.lifecycle_state = SourceLifecycleState::Retracted;
    }
    assert_eq!(
        compute_invalidation_propagation(
            &bad_removed_records,
            &derivations_list,
            &relations,
            limits,
            || Ok(())
        ),
        Err(LineageError::RemovedEndpointAutoRetracted {
            record_version_id: "removed-endpoint-v1".to_owned(),
        })
    );

    // 5. Anchored cycle vs unanchored cycle and budget checks.
    let mut anchored_relations = relations.clone();
    anchored_relations.push(RecordRelation {
        from: "cycle-review-c-v1".to_owned(),
        to: "harbor-radar-log-v1".to_owned(),
        kind: RelationKind::Cites,
    });
    let (_, anchored_report) =
        compute_lineage_sccs(&records, &anchored_relations, limits, || Ok(()))?;
    assert_eq!(
        anchored_report
            .detected_cycles
            .first()
            .map(|c| c.classification),
        Some(CycleClassification::AnchoredCycle)
    );

    // A downstream input and a chain into a second rootless cycle retain their
    // circular-only status and witness; neither cycle is falsely anchored.
    let mut rootless_cycle_relations = relations.clone();
    for (from, to) in [
        ("dispatch-primary-v2", "cycle-memo-a-v1"),
        ("cycle-review-c-v1", "foia-release-2024-v1"),
        ("foia-release-2024-v1", "foia-release-2026-v1"),
        ("foia-release-2026-v1", "foia-release-2024-v1"),
    ] {
        rootless_cycle_relations.push(RecordRelation {
            from: from.to_owned(),
            to: to.to_owned(),
            kind: RelationKind::Cites,
        });
    }
    let (_, rootless_report) =
        compute_lineage_sccs(&records, &rootless_cycle_relations, limits, || Ok(()))?;
    assert_eq!(rootless_report.detected_cycles.len(), 2);
    assert!(
        rootless_report
            .detected_cycles
            .iter()
            .all(|cycle| cycle.classification == CycleClassification::UnanchoredCycle)
    );
    let downstream_cycle = evaluate_corroboration(
        &["dispatch-primary-v2".to_owned()],
        &records,
        &BTreeMap::new(),
        &rootless_cycle_relations,
        &artifacts,
        limits,
        || Ok(()),
    )?;
    assert_eq!(
        downstream_cycle.circular_only_record_versions,
        vec!["dispatch-primary-v2"]
    );
    assert!(downstream_cycle.independent_supporting_roots.is_empty());
    assert!(rootless_report.detected_cycles.iter().any(|cycle| {
        Some(&cycle.witness_path) == downstream_cycle.primary_cycle_witness.as_ref()
    }));
    assert_eq!(
        downstream_cycle
            .earliest_known_source_view
            .coverage_qualification
            .detected_cycle_count,
        2
    );

    assert_eq!(
        LineageLimits::new(0, 10, 10),
        Err(LineageError::ZeroLimit(LineageLimit::Nodes))
    );
    assert_eq!(
        validate_records(
            &records_list,
            &artifacts,
            LineageLimits::new(5, 10, 10)?,
            || Ok(())
        ),
        Err(LineageError::LineageGraphBudgetExceeded {
            limit: LineageLimit::Nodes,
            actual: records_list.len(),
            max: 5,
        })
    );

    // 6. B1: Version chains (`Supersedes`, `AlternateRedaction`, `PreprintVersion`) and
    // shared `source_id` / document `id` collapse to 1 independent root (selecting earliest).
    let derivations_map: BTreeMap<String, DerivationNode> = derivations_list
        .iter()
        .map(|d| (d.version_id.clone(), d.clone()))
        .collect();
    let foia_corroboration = evaluate_corroboration(
        &[
            "foia-release-2024-v1".to_owned(),
            "foia-release-2026-v1".to_owned(),
            "foia-release-partner-v1".to_owned(),
        ],
        &records,
        &derivations_map,
        &relations,
        &artifacts,
        limits,
        || Ok(()),
    )?;
    assert_eq!(
        foia_corroboration.independent_supporting_roots,
        vec!["foia-release-2024-v1"]
    );
    assert_eq!(foia_corroboration.independent_root_count, 1);

    let dispatch_corroboration = evaluate_corroboration(
        &[
            "dispatch-primary-v1".to_owned(),
            "dispatch-primary-v2".to_owned(),
        ],
        &records,
        &derivations_map,
        &relations,
        &artifacts,
        limits,
        || Ok(()),
    )?;
    assert_eq!(
        dispatch_corroboration.independent_supporting_roots,
        vec!["dispatch-primary-v1"]
    );
    assert_eq!(dispatch_corroboration.independent_root_count, 1);

    // N1: Transitive equivalence-class collapse across shared source_id / version chain and shared hash
    // so a byte-exact copy of a skipped root never counts as independent corroboration regardless of order.
    if let Some(partner_rec) = records.get("foia-release-partner-v1").cloned() {
        let mut transitive_records = records.clone();
        let mut partner_copy = partner_rec;
        partner_copy.id = "doc-foia-partner-mirror".to_owned();
        partner_copy.version_id = "foia-partner-byte-copy-v1".to_owned();
        partner_copy.source_id = "mirror-beta".to_owned();
        partner_copy.publication_time = Some("2026-10-12T09:00:00Z".to_owned());
        partner_copy.retrieval_time = "2026-10-12T10:00:00Z".to_owned();
        transitive_records.insert(partner_copy.version_id.clone(), partner_copy);

        let transitive_corr = evaluate_corroboration(
            &[
                "foia-release-2024-v1".to_owned(),
                "foia-release-partner-v1".to_owned(),
                "foia-partner-byte-copy-v1".to_owned(),
            ],
            &transitive_records,
            &derivations_map,
            &relations,
            &artifacts,
            limits,
            || Ok(()),
        )?;
        assert_eq!(
            transitive_corr.independent_supporting_roots,
            vec!["foia-release-2024-v1"]
        );
        assert_eq!(transitive_corr.independent_root_count, 1);
    }

    // 7. B3: Segment-derived hidden cleartext detection even when `forbidden_cleartext_in_left`
    // is empty (`NewlyDisclosedInRight`) or when newly redacted text leaks into the right artifact
    // (`NewlyRedactedInRight`).
    let mut empty_forbidden_comp = first_comparison.clone();
    empty_forbidden_comp.forbidden_cleartext_in_left.clear();
    assert_eq!(
        verify_release_comparison(
            &empty_forbidden_comp,
            &records,
            &bad_payloads,
            limits,
            || Ok(())
        ),
        Err(LineageError::HiddenRedactionPayloadRejected {
            comparison_id: "foia-2024-vs-2026".to_owned(),
            artifact_id: "foia-release-2024-redacted.txt".to_owned(),
            forbidden_cleartext: "Sector-7 Telemetry".to_owned(),
        })
    );
    let partner_comparison = comparisons
        .get(1)
        .ok_or("missing partner release comparison")?;
    let mut leaked_right_payloads = payloads.clone();
    if let Some(partner_raw) = leaked_right_payloads.get_mut("foia-release-partner-redacted.txt") {
        partner_raw.extend_from_slice(b"\n<!-- Operator Callsign Kestrel-4 -->\n");
    }
    assert_eq!(
        verify_release_comparison(
            partner_comparison,
            &records,
            &leaked_right_payloads,
            limits,
            || Ok(())
        ),
        Err(LineageError::HiddenRedactionPayloadRejected {
            comparison_id: "foia-2024-vs-partner".to_owned(),
            artifact_id: "foia-release-partner-redacted.txt".to_owned(),
            forbidden_cleartext: "Operator Callsign Kestrel-4".to_owned(),
        })
    );

    // 8. B4: A record in `SourceLifecycleState::Retracted` with no `RetractedBy` relation
    // still triggers `needs_reassessment` on its downstream dependents.
    let relations_without_retracted_by: Vec<RecordRelation> = relations
        .iter()
        .filter(|rel| rel.kind != RelationKind::RetractedBy)
        .cloned()
        .collect();
    let inv_without_rel = compute_invalidation_propagation(
        &records,
        &derivations_list,
        &relations_without_retracted_by,
        limits,
        || Ok(()),
    )?;
    assert!(inv_without_rel.needs_reassessment.iter().any(|n| {
        n.target_version_id == "claim-sensor-fault-v1"
            && n.trigger_record_version_id == "retracted-advisory-v1"
            && n.reason == InvalidationReason::SourceRetracted
    }));

    // 9. B5: Shared-hash collapse selects the earliest instant (not lexical order),
    // RFC 3339 timezone offsets are normalized to UTC instants, ties report
    // `AmbiguousCandidates`, and malformed timestamps are rejected.
    let mut custom_records = records.clone();
    if let Some(base_rec) = records.get("dispatch-primary-v1").cloned() {
        let mut late_a = base_rec.clone();
        late_a.id = "doc-late-a".to_owned();
        late_a.version_id = "a-late-copy".to_owned();
        late_a.source_id = "source-a".to_owned();
        late_a.lifecycle_state = SourceLifecycleState::Active;
        late_a.publication_time = Some("2026-10-05T12:00:00Z".to_owned());
        late_a.retrieval_time = "2026-10-05T12:05:00Z".to_owned();

        let mut early_z = base_rec.clone();
        early_z.id = "doc-early-z".to_owned();
        early_z.version_id = "z-early-original".to_owned();
        early_z.source_id = "source-z".to_owned();
        early_z.lifecycle_state = SourceLifecycleState::Active;
        // 2026-10-01T05:00:00+09:00 == 2026-09-30T20:00:00Z (earlier than 2026-10-01T04:15:00Z)
        early_z.publication_time = Some("2026-10-01T05:00:00+09:00".to_owned());
        early_z.retrieval_time = "2026-10-01T05:05:00+09:00".to_owned();

        custom_records.insert(late_a.version_id.clone(), late_a);
        custom_records.insert(early_z.version_id.clone(), early_z);

        let hash_winner = evaluate_corroboration(
            &["a-late-copy".to_owned(), "z-early-original".to_owned()],
            &custom_records,
            &BTreeMap::new(),
            &[],
            &artifacts,
            limits,
            || Ok(()),
        )?;
        assert_eq!(
            hash_winner.independent_supporting_roots,
            vec!["z-early-original"]
        );
        assert_eq!(
            hash_winner
                .earliest_known_source_view
                .earliest_dated_root
                .as_deref(),
            Some("z-early-original")
        );
        assert_eq!(
            hash_winner.earliest_known_source_view.resolution,
            EarliestSourceResolution::SingleEarliestDated
        );

        // Exact publication-instant tie between two independent roots reports AmbiguousCandidates.
        let mut tie_other = base_rec.clone();
        tie_other.id = "doc-tie-other".to_owned();
        tie_other.version_id = "tie-other-v1".to_owned();
        tie_other.source_id = "source-tie".to_owned();
        tie_other.lifecycle_state = SourceLifecycleState::Active;
        tie_other.publication_time = Some("2026-09-30T20:00:00Z".to_owned());
        tie_other.pages = vec![PageAttachment {
            number: 1,
            artifact_id: Some("independent-log-v1.txt".to_owned()),
        }];
        custom_records.insert(tie_other.version_id.clone(), tie_other);

        let tie_corr = evaluate_corroboration(
            &["z-early-original".to_owned(), "tie-other-v1".to_owned()],
            &custom_records,
            &BTreeMap::new(),
            &[],
            &artifacts,
            limits,
            || Ok(()),
        )?;
        assert_eq!(
            tie_corr.earliest_known_source_view.resolution,
            EarliestSourceResolution::AmbiguousCandidates
        );

        let mut bad_time_rec = base_rec;
        bad_time_rec.version_id = "bad-time-v1".to_owned();
        bad_time_rec.publication_time = Some("garbage-not-a-time".to_owned());
        assert_eq!(
            validate_records(&[bad_time_rec], &artifacts, limits, || Ok(())),
            Err(LineageError::InvalidTimestamp {
                record_version_id: "bad-time-v1".to_owned(),
                field: "publication_time",
                value: "garbage-not-a-time".to_owned(),
            })
        );
    }

    // 10. B2: Deep lineage and `derived_from` chains (2,500 nodes) execute iteratively
    // without overflowing the thread stack.
    let chain_len = 2_500usize;
    let deep_limits = LineageLimits::new(chain_len + 10, chain_len + 10, 10)?;
    let mut deep_records = BTreeMap::new();
    let mut deep_relations = Vec::new();
    let mut deep_derivations = Vec::new();
    for i in 0..chain_len {
        let ver_id = format!("chain-rec-{i:04}");
        deep_records.insert(
            ver_id.clone(),
            SourceRecordDescriptor {
                id: ver_id.clone(),
                version_id: ver_id.clone(),
                source_id: format!("src-{i:04}"),
                availability: AvailabilityState::Available,
                lifecycle_state: SourceLifecycleState::Active,
                predecessor_version_id: None,
                event_time: Some("2026-10-01T00:00:00Z".to_owned()),
                publication_time: Some("2026-10-01T00:00:00Z".to_owned()),
                retrieval_time: "2026-10-01T00:05:00Z".to_owned(),
                unknown_time_reason: None,
                retention: RetentionMode::Retained,
                omission_reason: None,
                pages: vec![PageAttachment {
                    number: 1,
                    artifact_id: Some("dispatch-root-v1.txt".to_owned()),
                }],
            },
        );
        if i + 1 < chain_len {
            deep_relations.push(RecordRelation {
                from: ver_id,
                to: format!("chain-rec-{:04}", i + 1),
                kind: RelationKind::Cites,
            });
        }
        let deriv_id = format!("chain-deriv-{i:04}");
        let parent_id = if i == 0 {
            "chain-rec-0000".to_owned()
        } else {
            format!("chain-deriv-{:04}", i - 1)
        };
        deep_derivations.push(DerivationNode {
            version_id: deriv_id,
            origin_class: DerivationOriginClass::AnalyticalInference,
            derived_from: vec![parent_id],
        });
    }
    let (_, deep_scc) =
        compute_lineage_sccs(&deep_records, &deep_relations, deep_limits, || Ok(()))?;
    assert!(deep_scc.detected_cycles.is_empty());
    let single_record_map: BTreeMap<String, SourceRecordDescriptor> = deep_records
        .get("chain-rec-0000")
        .cloned()
        .into_iter()
        .map(|r| (r.version_id.clone(), r))
        .collect();
    let deep_inv = compute_invalidation_propagation(
        &single_record_map,
        &deep_derivations,
        &[],
        deep_limits,
        || Ok(()),
    )?;
    assert_eq!(deep_inv.unaffected_fresh_versions.len(), chain_len);

    // 11. Cooperative cancellation at every check boundary.
    let mut checks = 0;
    evaluate_corroboration(
        &citing_inputs,
        &records,
        &derivations_map,
        &relations,
        &artifacts,
        limits,
        || {
            checks += 1;
            Ok(())
        },
    )?;
    for stop_at in 1..=checks {
        let mut calls = 0;
        assert_eq!(
            evaluate_corroboration(
                &citing_inputs,
                &records,
                &derivations_map,
                &relations,
                &artifacts,
                limits,
                || {
                    calls += 1;
                    if calls == stop_at {
                        Err(StopReason::Cancelled)
                    } else {
                        Ok(())
                    }
                },
            ),
            Err(LineageError::Stopped(StopReason::Cancelled))
        );
    }
    Ok(())
}
