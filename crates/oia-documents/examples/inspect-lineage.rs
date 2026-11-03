use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use oia_documents::StopReason;
use oia_documents::lineage::{
    ArtifactDescriptor, AvailabilityState, ClaimCorroborationInputs, ComparisonSegmentKind,
    DerivationNode, DerivationOriginClass, LineageLimits, PageAttachment, RecordRelation,
    RelationKind, ReleaseComparisonSegment, ReleaseRedactionComparison, RetentionMode,
    SourceLifecycleState, SourceRecordDescriptor, UnresolvedExternalCitation, Utf8ByteCitation,
    compute_exact_duplicate_groups, compute_invalidation_propagation, evaluate_claim_corroboration,
    validate_records, verify_release_comparison, verify_utf8_byte_citation,
};

fn invalid_argument(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn parse_limit(argument: Option<OsString>, default: usize) -> Result<usize, Box<dyn Error>> {
    match argument {
        Some(argument) => Ok(argument
            .into_string()
            .map_err(|_| invalid_argument("limits must be decimal integers"))?
            .parse()?),
        None => Ok(default),
    }
}

fn read_bounded_file(path: &Path, max_bytes: usize) -> Result<Vec<u8>, Box<dyn Error>> {
    let read_limit = max_bytes
        .checked_add(1)
        .ok_or_else(|| invalid_argument("byte limit is too large for bounded file read"))?;
    let file = File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(invalid_argument("lineage inspection requires a regular local file").into());
    }
    let mut input = Vec::new();
    input.try_reserve_exact(read_limit)?;
    file.take(u64::try_from(read_limit)?)
        .read_to_end(&mut input)?;
    if input.len() > max_bytes {
        return Err(invalid_argument("fixture file exceeds bounded read limit").into());
    }
    Ok(input)
}

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

fn build_a03_a08_lineage_fixture(root: &Path) -> Result<LineageFixture, Box<dyn Error>> {
    let artifact_specs = [
        (
            "dispatch-root-v1.txt",
            "8a4f33b3889b6650f1d36d1843d05fdb86edc297c013b5cbc9006358b4391539",
            218usize,
        ),
        (
            "dispatch-root-v2-corrected.txt",
            "cbccc2264187b7fb2384426cce2ad2e5f74c29d647129a23dea86ab2e2bd9cbd",
            270,
        ),
        (
            "syndicated-quote-v1.txt",
            "3dc3e0a1f84aab4a8fb9d9d4ead28dedb5ade3ad9df0e4c7582caab07d525f4c",
            279,
        ),
        (
            "independent-log-v1.txt",
            "cbe2783010d93ddc20adf09e3d6f7762263d4d3cc820ea9d8d2a1f3dc7e1794b",
            225,
        ),
        (
            "cycle-node-a.txt",
            "358b5de399a70e004a4951f8710b2d5bc9526398bf22cbecd98d421ea42f7d41",
            171,
        ),
        (
            "cycle-node-b.txt",
            "698800d787c490dd00c6bbe630e23c1c9343df4c210d4ca95f5ac531d6625507",
            179,
        ),
        (
            "cycle-node-c.txt",
            "18989a22b1cd518c0fe77dcd45a49e509aa6e0abb0c298f654c9e204ab5c7307",
            186,
        ),
        (
            "foia-release-2024-redacted.txt",
            "0e6383f5d713148f5976b2398514bcb7f45fbeb3672822d479184f6485c5016d",
            202,
        ),
        (
            "foia-release-2026-declassified.txt",
            "4a0b88fcfb48e9e827f74358cf4eccdf256c281d9ca6e85e73fc0e781ba14059",
            192,
        ),
        (
            "foia-release-partner-redacted.txt",
            "1ccac973452a622cbdd098bb3c555a67e759c28e61ea685e7cf59ee0666776c4",
            219,
        ),
        (
            "retracted-advisory-v1.txt",
            "191c073e8afe5f157574d31d3881562f3bf3218d7ed9b9f288f06b427afe25fc",
            170,
        ),
        (
            "retraction-notice-v2.txt",
            "177fe70ecb5f61ec64b6cb7266b8a3cfcb78e665a212ebed693246d79652e4a0",
            206,
        ),
        (
            "removed-endpoint-v1.txt",
            "34ec59fc4c4cc74cd3d702b57f971b0823822c2458b839dee2b8e6663a257013",
            168,
        ),
    ];

    let mut artifacts = BTreeMap::new();
    let mut payloads = BTreeMap::new();
    for (id, sha256, byte_length) in artifact_specs {
        let path = root.join("originals").join(id);
        let bytes = read_bounded_file(&path, 65_536)?;
        if bytes.len() != byte_length {
            return Err(
                invalid_argument("fixture original byte length does not match manifest").into(),
            );
        }
        artifacts.insert(
            id.to_owned(),
            ArtifactDescriptor {
                id: id.to_owned(),
                sha256: sha256.to_owned(),
                byte_length,
            },
        );
        payloads.insert(id.to_owned(), bytes);
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

    Ok(LineageFixture {
        artifacts,
        payloads,
        records,
        relations,
        derivations,
        citations,
        comparisons,
        citing_inputs,
    })
}

fn run() -> Result<ExitCode, Box<dyn Error>> {
    let mut arguments = env::args_os().skip(1);
    let usage = "usage: inspect-lineage [FIXTURE_ROOT] [MAX_NODES] [MAX_EDGES] [MAX_SEGMENTS]";
    let root = match arguments.next() {
        Some(arg) if arg == "--help" || arg == "-h" => {
            return Err(invalid_argument(usage).into());
        }
        Some(arg) => PathBuf::from(arg),
        None => PathBuf::from("fixtures/documents/lineage"),
    };
    let limits = LineageLimits::new(
        parse_limit(arguments.next(), 200)?,
        parse_limit(arguments.next(), 500)?,
        parse_limit(arguments.next(), 100)?,
    )?;
    if arguments.next().is_some() {
        return Err(invalid_argument("too many arguments").into());
    }

    let LineageFixture {
        artifacts,
        payloads,
        records: records_list,
        relations,
        derivations: derivations_list,
        citations,
        comparisons,
        citing_inputs,
    } = build_a03_a08_lineage_fixture(&root)?;

    let started = Instant::now();
    let mut check_control = || {
        if started.elapsed() >= Duration::from_secs(5) {
            Err(StopReason::DeadlineExceeded)
        } else {
            Ok(())
        }
    };

    let records = validate_records(&records_list, &artifacts, limits, &mut check_control)?;
    let dup_groups =
        compute_exact_duplicate_groups(&records_list, &artifacts, limits, &mut check_control)?;
    for citation in &citations {
        verify_utf8_byte_citation(citation, &records, &payloads, &mut check_control)?;
    }
    for comparison in &comparisons {
        verify_release_comparison(comparison, &records, &payloads, limits, &mut check_control)?;
    }
    let derivations_map: BTreeMap<String, DerivationNode> = derivations_list
        .iter()
        .map(|d| (d.version_id.clone(), d.clone()))
        .collect();
    let corroboration = evaluate_claim_corroboration(
        ClaimCorroborationInputs {
            supporting: &citing_inputs,
            ..ClaimCorroborationInputs::default()
        },
        &records,
        &derivations_map,
        &relations,
        &artifacts,
        limits,
        &mut check_control,
    )?;
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
            reference: "UNRELATED citation excluded from calibration claim coverage".to_owned(),
        },
    ];
    let stance_corroboration = evaluate_claim_corroboration(
        ClaimCorroborationInputs {
            supporting: &["retracted-advisory-v1".to_owned()],
            contradicting: &[
                "retraction-notice-v2".to_owned(),
                "summary-coastal-track-v1".to_owned(),
            ],
            external_citations: &external_citations,
        },
        &records,
        &derivations_map,
        &relations,
        &artifacts,
        limits,
        &mut check_control,
    )?;
    let invalidation = compute_invalidation_propagation(
        &records,
        &derivations_list,
        &relations,
        limits,
        &mut check_control,
    )?;

    let mut output = io::stdout().lock();
    writeln!(
        output,
        "Local document lineage, release comparison, and correction propagation verification (simulation fixture)"
    )?;
    writeln!(
        output,
        "root={root:?}; records={}; artifacts={}; derivations={}; citations={}; comparisons={}",
        records.len(),
        artifacts.len(),
        derivations_list.len(),
        citations.len(),
        comparisons.len()
    )?;
    writeln!(
        output,
        "limits: max_nodes={}; max_edges={}; max_segments={}; processing_deadline_seconds=5",
        limits.max_nodes(),
        limits.max_edges(),
        limits.max_segments()
    )?;
    for group in &dup_groups {
        writeln!(
            output,
            "exact_duplicate_group: artifact={} sha256={} versions={:?} sources={:?}",
            group.artifact_id,
            group.sha256,
            group.member_record_versions,
            group.distinct_source_ids
        )?;
    }
    writeln!(
        output,
        "corroboration: independent_roots={:?} (count={}); exact_duplicate_group_count={}; contradicting_roots={:?}; circular_only={:?}; excluded_derivatives={:?}; earliest_resolution={:?}; earliest_dated={:?}; undated_roots={:?}; omitted_reference_only={:?}",
        corroboration.independent_supporting_roots,
        corroboration.independent_root_count,
        corroboration.exact_duplicate_group_count,
        corroboration.contradicting_independent_roots,
        corroboration.circular_only_record_versions,
        corroboration.excluded_derivative_runs,
        corroboration.earliest_known_source_view.resolution,
        corroboration.earliest_known_source_view.earliest_dated_root,
        corroboration
            .earliest_known_source_view
            .undated_candidate_roots,
        corroboration
            .earliest_known_source_view
            .coverage_qualification
            .omitted_reference_only_records
    )?;
    writeln!(
        output,
        "calibration_claim: supporting_roots={:?}; contradicting_roots={:?}; excluded_derivatives={:?}; omitted_reference_only={:?}; unresolved_external_citations={:?}; detected_cycle_count={}",
        stance_corroboration.independent_supporting_roots,
        stance_corroboration.contradicting_independent_roots,
        stance_corroboration.excluded_derivative_runs,
        stance_corroboration
            .earliest_known_source_view
            .coverage_qualification
            .omitted_reference_only_records,
        stance_corroboration
            .earliest_known_source_view
            .coverage_qualification
            .unresolved_external_citations,
        stance_corroboration
            .earliest_known_source_view
            .coverage_qualification
            .detected_cycle_count
    )?;
    for notice in &invalidation.needs_reassessment {
        writeln!(
            output,
            "needs_reassessment: target={} trigger={} replacement={} reason={} preserve_old_citations={}",
            notice.target_version_id,
            notice.trigger_record_version_id,
            notice.replacement_record_version_id,
            notice.reason.as_str(),
            notice.preserve_old_citations
        )?;
    }
    writeln!(
        output,
        "unaffected_fresh_versions={:?}; removed_not_auto_retracted={:?}",
        invalidation.unaffected_fresh_versions,
        invalidation.removed_not_auto_retracted_record_versions
    )?;
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    match run() {
        Ok(exit_code) => exit_code,
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "inspect-lineage: {error}");
            ExitCode::FAILURE
        }
    }
}
