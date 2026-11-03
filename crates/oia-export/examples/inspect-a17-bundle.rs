#![forbid(unsafe_code)]
#![deny(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use std::env;
use std::io::{self, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use oia_export::bundle::{
    AnalysisLicenseStatus, AnalysisReplayOutcome, AnalysisReplayRequest, AssetUseConditions,
    BundleEntryRole, BundleImportInput, BundleLimits, BundleManifestEntry, BundleOmissionEntry,
    BundleRecordEntry, BundleRecordKind, BundleReference, BundleReferenceTarget,
    CalibrationValidity, CandidateReplayOutput, CapabilityDeclaration, CapabilityKind,
    CapabilityRequirement, PackageIdentity, PinnedAnalysisSpec, PinnedCalibrationSpec,
    PinnedForecastSpec, PinnedGroupingDecision, PinnedQuerySpec, RedistributionCondition,
    ReplayContract, RetentionCondition, StagedFilesystemEntryKind, StagedPayloadFile,
    SupportedCapabilities, replay_analysis, validate_and_stage_bundle,
};
use oia_export::{
    OmissionReason, OriginClass, ReviewStatus, StopReason, TimeDisplay, TimePrecision,
};

const REPORT_V1: &[u8] = include_bytes!("../../../fixtures/documents/a03/originals/report-v1.txt");
const REPORT_V1_SHA256: &str = "fdc16e37d2fa6715d0b23141026e8d70f77de1f4e6630bcd2e1f47096c5b7ee5";
const REPORT_V2: &[u8] = include_bytes!("../../../fixtures/documents/a03/originals/report-v2.txt");
const REPORT_V2_SHA256: &str = "d9b9bb89b383c31fbb95d56691d2c2544fef72157a6e19107fb61b2b5dd6d55c";
const REDACTED_A: &[u8] =
    include_bytes!("../../../fixtures/documents/a03/originals/redacted-a.txt");
const REDACTED_A_SHA256: &str = "b0fa14e6423a0e11a3fd227ab6640b6f6490e9289a2813a9d818d4736825ba76";
const RENDERED_SUMMARY: &[u8] =
    b"# Synthetic Bundle Report\n\n[ORIGIN: OBSERVATION] [REVIEW: REVIEWED] Cited summary.\n";
const RENDERED_SUMMARY_SHA256: &str =
    "d8044944049b6203f4e8689b9cba31ee6e362ef139f037e31bca490199f11219";
const RECORDS_GRAPH_JSON: &[u8] =
    b"{\"schema\":\"oia-records-v1\",\"case\":\"case:a17-synthetic-harbor\"}";
const RECORDS_GRAPH_JSON_SHA256: &str =
    "794666151dde55f813edc7657bd305285e6e92d1c09976a36e11f6c249ad1457";
const REPRO_MANIFEST_JSON: &[u8] =
    b"{\"schema\":\"oia-repro-v1\",\"analyses\":[\"analysis:passage-count\",\"analysis:sky-track\",\"analysis:llm-draft\"]}";
const REPRO_MANIFEST_JSON_SHA256: &str =
    "e7a53d50a05a4d03fba6135610cfebb9537a464699bc28f01587a64f3a9a98eb";
const DETERMINISTIC_ANALYSIS_OUTPUT: &[u8] =
    b"passage_count=2;matched_query=\"two lights\";source_version=ver:report-v1";
const DETERMINISTIC_ANALYSIS_OUTPUT_SHA256: &str =
    "9c64084a44c1707cdb503063b0494a1c1cc0d4cd913dd3e50d4d21d8f60bf7ea";

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
    let max_total_bytes = match args.next() {
        None => BundleLimits::DEFAULT_MAX_TOTAL_BYTES,
        Some(flag) if flag == "--summary" => match args.next() {
            Some(raw) => raw
                .parse::<usize>()
                .map_err(|_| format!("invalid MAX_TOTAL_BYTES `{raw}`"))?,
            None => BundleLimits::DEFAULT_MAX_TOTAL_BYTES,
        },
        Some(raw) => raw.parse::<usize>().map_err(|_| {
            format!("invalid argument `{raw}`; expected [--summary] [MAX_TOTAL_BYTES]")
        })?,
    };
    if args.next().is_some() {
        return Err("too many arguments; expected [--summary] [MAX_TOTAL_BYTES]".to_owned());
    }

    let limits = BundleLimits::new(
        32,
        64,
        128,
        256,
        64,
        64,
        1024 * 1024,
        max_total_bytes,
        262_144,
        4096,
    )
    .map_err(|e| e.to_string())?;

    let started = Instant::now();
    let deadline = Duration::from_secs(5);
    let mut check_stop = || {
        if started.elapsed() > deadline {
            Err(StopReason::DeadlineExceeded)
        } else {
            Ok(())
        }
    };

    let v1_asset_path = format!("assets/{REPORT_V1_SHA256}.txt");
    let v2_asset_path = format!("assets/{REPORT_V2_SHA256}.txt");
    let red_asset_path = format!("assets/{REDACTED_A_SHA256}.txt");

    let capabilities = [
        CapabilityDeclaration {
            capability_id: "oia.record.evidence",
            kind: CapabilityKind::RecordSchema,
            version: 1,
            requirement: CapabilityRequirement::Required,
        },
        CapabilityDeclaration {
            capability_id: "oia.analysis.exact-passage-count",
            kind: CapabilityKind::AnalysisMethod,
            version: 1,
            requirement: CapabilityRequirement::Required,
        },
        CapabilityDeclaration {
            capability_id: "oia.analysis.sky-elevation-series",
            kind: CapabilityKind::AnalysisMethod,
            version: 1,
            requirement: CapabilityRequirement::Required,
        },
        CapabilityDeclaration {
            capability_id: "oia.optional.experimental-telemetry-overlay",
            kind: CapabilityKind::RecordSchema,
            version: 1,
            requirement: CapabilityRequirement::Optional,
        },
    ];

    let manifest_entries = [
        BundleManifestEntry {
            relative_path: "records/domain-records.json",
            role: BundleEntryRole::Records,
            media_type: "application/json",
            declared_byte_len: RECORDS_GRAPH_JSON.len(),
            declared_sha256_hex: RECORDS_GRAPH_JSON_SHA256,
            schema_version: Some(1),
        },
        BundleManifestEntry {
            relative_path: &v1_asset_path,
            role: BundleEntryRole::Assets,
            media_type: "text/plain; charset=utf-8",
            declared_byte_len: REPORT_V1.len(),
            declared_sha256_hex: REPORT_V1_SHA256,
            schema_version: None,
        },
        BundleManifestEntry {
            relative_path: &v2_asset_path,
            role: BundleEntryRole::Assets,
            media_type: "text/plain; charset=utf-8",
            declared_byte_len: REPORT_V2.len(),
            declared_sha256_hex: REPORT_V2_SHA256,
            schema_version: None,
        },
        BundleManifestEntry {
            relative_path: &red_asset_path,
            role: BundleEntryRole::Assets,
            media_type: "text/plain; charset=utf-8",
            declared_byte_len: REDACTED_A.len(),
            declared_sha256_hex: REDACTED_A_SHA256,
            schema_version: None,
        },
        BundleManifestEntry {
            relative_path: "reports/case-summary.md",
            role: BundleEntryRole::Reports,
            media_type: "text/markdown; charset=utf-8",
            declared_byte_len: RENDERED_SUMMARY.len(),
            declared_sha256_hex: RENDERED_SUMMARY_SHA256,
            schema_version: Some(1),
        },
        BundleManifestEntry {
            relative_path: "reproduction/runs.json",
            role: BundleEntryRole::Reproduction,
            media_type: "application/json",
            declared_byte_len: REPRO_MANIFEST_JSON.len(),
            declared_sha256_hex: REPRO_MANIFEST_JSON_SHA256,
            schema_version: Some(1),
        },
    ];

    let staged_files = [
        StagedPayloadFile {
            relative_path: "records/domain-records.json",
            entry_kind: StagedFilesystemEntryKind::RegularFile,
            staged_bytes: RECORDS_GRAPH_JSON,
            rechecked_sha256_hex: Some(RECORDS_GRAPH_JSON_SHA256),
        },
        StagedPayloadFile {
            relative_path: &v1_asset_path,
            entry_kind: StagedFilesystemEntryKind::RegularFile,
            staged_bytes: REPORT_V1,
            rechecked_sha256_hex: Some(REPORT_V1_SHA256),
        },
        StagedPayloadFile {
            relative_path: &v2_asset_path,
            entry_kind: StagedFilesystemEntryKind::RegularFile,
            staged_bytes: REPORT_V2,
            rechecked_sha256_hex: Some(REPORT_V2_SHA256),
        },
        StagedPayloadFile {
            relative_path: &red_asset_path,
            entry_kind: StagedFilesystemEntryKind::RegularFile,
            staged_bytes: REDACTED_A,
            rechecked_sha256_hex: Some(REDACTED_A_SHA256),
        },
        StagedPayloadFile {
            relative_path: "reports/case-summary.md",
            entry_kind: StagedFilesystemEntryKind::RegularFile,
            staged_bytes: RENDERED_SUMMARY,
            rechecked_sha256_hex: Some(RENDERED_SUMMARY_SHA256),
        },
        StagedPayloadFile {
            relative_path: "reproduction/runs.json",
            entry_kind: StagedFilesystemEntryKind::RegularFile,
            staged_bytes: REPRO_MANIFEST_JSON,
            rechecked_sha256_hex: Some(REPRO_MANIFEST_JSON_SHA256),
        },
    ];

    let use_conditions = [
        AssetUseConditions {
            asset_path: &v1_asset_path,
            source_id: "src:harbor-log",
            ordinary_access_method: "Public municipal archive HTTP download (synthetic fixture)",
            attribution_notice: "Synthetic A03 harbor observation log v1",
            automated_processing_permitted: true,
            retention: RetentionCondition::Permitted,
            redistribution: RedistributionCondition::PermittedWithAttribution,
        },
        AssetUseConditions {
            asset_path: &v2_asset_path,
            source_id: "src:harbor-log",
            ordinary_access_method: "Public municipal archive HTTP download (synthetic fixture)",
            attribution_notice: "Synthetic A03 harbor observation log v2 (corrected)",
            automated_processing_permitted: true,
            retention: RetentionCondition::Permitted,
            redistribution: RedistributionCondition::PermittedWithAttribution,
        },
        AssetUseConditions {
            asset_path: &red_asset_path,
            source_id: "src:archive-foia",
            ordinary_access_method: "Public records release package (synthetic fixture)",
            attribution_notice: "Synthetic A03 FOIA release A (redacted derivative)",
            automated_processing_permitted: true,
            retention: RetentionCondition::Permitted,
            redistribution: RedistributionCondition::PrivateCaseTransferOnly,
        },
    ];

    let records = [
        BundleRecordEntry {
            origin_namespace: "ws:field-team-alpha",
            record_id: "doc:report-original",
            version_id: "ver:report-v1",
            kind: BundleRecordKind::SourceDocument,
            origin_class: OriginClass::Observation,
            review_status: ReviewStatus::Reviewed,
            origin_actor_id: "actor:analyst-01",
            recorded_time: TimeDisplay {
                text: "2026-03-14T22:15:00Z",
                precision: TimePrecision::Minute,
                time_reference: "UTC",
            },
            uncertainty_note: Some("Clock synchronized within +/- 30s"),
            asset_relative_path: Some(&v1_asset_path),
            required_capability: Some(("oia.record.evidence", 1)),
        },
        BundleRecordEntry {
            origin_namespace: "ws:field-team-alpha",
            record_id: "doc:report-mirror-republication",
            version_id: "ver:mirror-v1",
            kind: BundleRecordKind::SourceDocument,
            origin_class: OriginClass::SourceAssertion,
            review_status: ReviewStatus::Reviewed,
            origin_actor_id: "actor:analyst-01",
            recorded_time: TimeDisplay {
                text: "2026-03-15",
                precision: TimePrecision::DateOnly,
                time_reference: "UTC date-only",
            },
            uncertainty_note: Some(
                "Mirror copy repeats v1 bytes verbatim; not independent corroboration",
            ),
            asset_relative_path: Some(&v1_asset_path),
            required_capability: Some(("oia.record.evidence", 1)),
        },
        BundleRecordEntry {
            origin_namespace: "ws:field-team-alpha",
            record_id: "doc:report-original",
            version_id: "ver:report-v2-corrected",
            kind: BundleRecordKind::SourceDocument,
            origin_class: OriginClass::Observation,
            review_status: ReviewStatus::Reviewed,
            origin_actor_id: "actor:analyst-02",
            recorded_time: TimeDisplay {
                text: "2026-03-16T09:00:00Z",
                precision: TimePrecision::ExactInstant,
                time_reference: "UTC",
            },
            uncertainty_note: None,
            asset_relative_path: Some(&v2_asset_path),
            required_capability: Some(("oia.record.evidence", 1)),
        },
        BundleRecordEntry {
            origin_namespace: "ws:field-team-alpha",
            record_id: "doc:archive-release-a",
            version_id: "ver:redacted-a-v1",
            kind: BundleRecordKind::EvidenceItem,
            origin_class: OriginClass::SourceAssertion,
            review_status: ReviewStatus::Reviewed,
            origin_actor_id: "actor:reviewer-01",
            recorded_time: TimeDisplay {
                text: "2026-03-18T12:00:00Z",
                precision: TimePrecision::ExactInstant,
                time_reference: "UTC",
            },
            uncertainty_note: Some("Redacted derivative release"),
            asset_relative_path: Some(&red_asset_path),
            required_capability: Some(("oia.record.evidence", 1)),
        },
    ];

    let references = [
        BundleReference {
            reference_id: "ref:v2-corrects-v1",
            from_record_id: "doc:report-original",
            from_version_id: "ver:report-v2-corrected",
            relation: "corrects",
            target: BundleReferenceTarget::IncludedRecord {
                target_record_id: "doc:report-original",
                target_version_id: "ver:report-v1",
            },
        },
        BundleReference {
            reference_id: "ref:restricted-radar-feed",
            from_record_id: "doc:report-original",
            from_version_id: "ver:report-v2-corrected",
            relation: "corroborated-by-external",
            target: BundleReferenceTarget::ExternalReferenceOnly {
                target_record_id: "sensor:coastal-radar-04",
                target_version_id: "ver:radar-20260314",
                source_id: "src:regional-aviation-feed",
                external_url: Some("https://example.org/public-archive/radar-20260314"),
                reason: OmissionReason::RedistributionProhibited,
            },
        },
    ];

    let omissions = [BundleOmissionEntry {
        omitted_id: "sensor:coastal-radar-04",
        source_id: "src:regional-aviation-feed",
        version_id: "ver:radar-20260314",
        reason: OmissionReason::RedistributionProhibited,
        coverage_effect: "Raw radar track samples omitted under provider redistribution terms",
        unreproducible_step_note: Some(
            "Radar track correlation cannot be re-executed offline without licensed feed",
        ),
    }];

    let query_inputs = [("doc:report-original", "ver:report-v1")];
    let queries = [PinnedQuerySpec {
        query_id: "query:two-lights",
        version_id: "ver:query-01",
        query_expression: "exact_phrase:\"two lights\"",
        pinned_input_versions: &query_inputs,
        coverage_note: "Evaluated over pinned UTF-8 extraction of doc:report-original@ver:report-v1",
    }];

    let group_members = [
        ("doc:report-original", "ver:report-v1"),
        ("doc:report-mirror-republication", "ver:mirror-v1"),
    ];
    let groupings = [PinnedGroupingDecision {
        grouping_id: "group:harbor-sighting-cluster-01",
        version_id: "ver:group-01",
        rationale: "Grouped original dispatch with verbatim mirror republication while preserving distinct source counts",
        member_record_versions: &group_members,
        is_reversible: true,
    }];

    let calibrations = [
        PinnedCalibrationSpec {
            calibration_id: "cal:pier-cam-north",
            version_id: "ver:cal-01",
            sensor_or_view_id: "view:pier-cam-north",
            reference_description: "Star-field and lighthouse survey azimuth/elevation fit",
            residual_rms: 0.18,
            residual_unit: "arcsec",
            validity: CalibrationValidity::Valid,
        },
        PinnedCalibrationSpec {
            calibration_id: "cal:pier-cam-moved",
            version_id: "ver:cal-02-invalidated",
            sensor_or_view_id: "view:pier-cam-south",
            reference_description: "Pre-storm tripod orientation",
            residual_rms: 0.42,
            residual_unit: "arcsec",
            validity: CalibrationValidity::Invalidated {
                reason: "Camera tripod shifted during wind gust at 22:18Z",
            },
        },
    ];

    let forecasts = [PinnedForecastSpec {
        forecast_id: "fcst:follow-up-release",
        version_id: "ver:fcst-01",
        cutoff_time: TimeDisplay {
            text: "2026-03-16T12:00:00Z",
            precision: TimePrecision::ExactInstant,
            time_reference: "UTC",
        },
        prediction_summary: "Probability 0.70 of declassified annex release within 30 days",
        pinned_input_versions: &query_inputs,
    }];

    let expected_elevations = [14.250_000, 14.875_000, 15.500_000];
    let analysis_v1_assets = [v1_asset_path.as_str()];
    let analyses = [
        PinnedAnalysisSpec {
            analysis_id: "analysis:passage-count",
            version_id: "ver:ana-01",
            method_id: "oia.documents.exact_search_v1",
            required_capability_id: "oia.analysis.exact-passage-count",
            required_capability_version: 1,
            model_id: None,
            model_version: None,
            configuration_summary: "case_sensitive=true;max_matches=100",
            license_status: AnalysisLicenseStatus::Compatible {
                license_id: "MIT OR Apache-2.0",
            },
            pinned_input_versions: &query_inputs,
            pinned_asset_paths: &analysis_v1_assets,
            calibration_ref: None,
            replay_contract: ReplayContract::DeterministicBitwise {
                expected_output_sha256_hex: DETERMINISTIC_ANALYSIS_OUTPUT_SHA256,
                expected_byte_len: DETERMINISTIC_ANALYSIS_OUTPUT.len(),
            },
        },
        PinnedAnalysisSpec {
            analysis_id: "analysis:sky-track",
            version_id: "ver:ana-02",
            method_id: "oia.sky.topocentric_elevation_v1",
            required_capability_id: "oia.analysis.sky-elevation-series",
            required_capability_version: 1,
            model_id: None,
            model_version: None,
            configuration_summary: "ellipsoid=WGS84;refraction=Saemundsson",
            license_status: AnalysisLicenseStatus::Compatible {
                license_id: "MIT OR Apache-2.0",
            },
            pinned_input_versions: &query_inputs,
            pinned_asset_paths: &analysis_v1_assets,
            calibration_ref: Some(("cal:pier-cam-north", "ver:cal-01")),
            replay_contract: ReplayContract::NumericalTolerance {
                expected_series: &expected_elevations,
                max_abs_diff: 1e-4,
                max_rel_diff: 1e-5,
                tolerance_justification: "IEEE-754 f64 trigonometric reduction across platforms within 1e-4 deg",
            },
        },
        PinnedAnalysisSpec {
            analysis_id: "analysis:llm-summary-draft",
            version_id: "ver:ana-03",
            method_id: "oia.models.grounded_summary_v1",
            required_capability_id: "oia.analysis.exact-passage-count",
            required_capability_version: 1,
            model_id: Some("model:local-GroundedSummarizer-7B"),
            model_version: Some("sha256:9f86d081884c7d659a2feaa0c55ad015"),
            configuration_summary: "temperature=0.7;top_p=0.9",
            license_status: AnalysisLicenseStatus::Compatible {
                license_id: "Apache-2.0",
            },
            pinned_input_versions: &query_inputs,
            pinned_asset_paths: &analysis_v1_assets,
            calibration_ref: None,
            replay_contract: ReplayContract::NonReplayableStochastic {
                reason: "Non-zero sampling temperature produces stochastic token sequences",
            },
        },
    ];

    let existing_cases = ["case:existing-local-01"];
    let import_input = BundleImportInput {
        identity: PackageIdentity {
            format: oia_export::bundle::SUPPORTED_BUNDLE_FORMAT,
            format_version: oia_export::bundle::SUPPORTED_BUNDLE_FORMAT_VERSION,
            package_id: "pkg:a17-harbor-bundle-20261102",
            created_time: TimeDisplay {
                text: "2026-11-02T18:00:00Z",
                precision: TimePrecision::ExactInstant,
                time_reference: "UTC",
            },
            producer_id: "oia-export-cli",
            producer_version: "0.1.0",
        },
        destination_case_id: "case:imported-harbor-01",
        local_namespace_prefix: "ws:local-import-01",
        existing_case_ids: &existing_cases,
        capabilities: &capabilities,
        manifest_entries: &manifest_entries,
        staged_files: &staged_files,
        use_conditions: &use_conditions,
        records: &records,
        references: &references,
        omissions: &omissions,
        queries: &queries,
        groupings: &groupings,
        calibrations: &calibrations,
        forecasts: &forecasts,
        analyses: &analyses,
    };

    let supported = [
        ("oia.record.evidence", 1),
        ("oia.analysis.exact-passage-count", 1),
        ("oia.analysis.sky-elevation-series", 1),
    ];
    let supported_caps = SupportedCapabilities {
        supported: &supported,
        allow_inert_optional_records: true,
    };

    let bundle = validate_and_stage_bundle(&import_input, &supported_caps, limits, &mut check_stop)
        .map_err(|e| e.to_string())?;

    let bitwise_outcome = replay_analysis(
        &bundle,
        &AnalysisReplayRequest {
            analysis_id: "analysis:passage-count",
            version_id: "ver:ana-01",
            available_models: &[],
            candidate_output: CandidateReplayOutput::Bytes(DETERMINISTIC_ANALYSIS_OUTPUT),
        },
        limits,
        &mut check_stop,
    )
    .map_err(|e| e.to_string())?;

    let candidate_elevations = [14.250_020, 14.874_985, 15.500_010];
    let tolerance_outcome = replay_analysis(
        &bundle,
        &AnalysisReplayRequest {
            analysis_id: "analysis:sky-track",
            version_id: "ver:ana-02",
            available_models: &[],
            candidate_output: CandidateReplayOutput::NumericalSeries(&candidate_elevations),
        },
        limits,
        &mut check_stop,
    )
    .map_err(|e| e.to_string())?;

    let missing_model_outcome = replay_analysis(
        &bundle,
        &AnalysisReplayRequest {
            analysis_id: "analysis:llm-summary-draft",
            version_id: "ver:ana-03",
            available_models: &[],
            candidate_output: CandidateReplayOutput::NotExecuted,
        },
        limits,
        &mut check_stop,
    )
    .map_err(|e| e.to_string())?;

    let mut out = io::stdout().lock();
    writeln!(
        out,
        "[SYNTHETIC FIXTURE] Validated bundle `{}` -> destination case `{}` (namespace `{}`)",
        bundle.package_id(),
        bundle.destination_case_id(),
        bundle.local_namespace_prefix()
    )
    .map_err(|e| e.to_string())?;
    writeln!(
        out,
        "[SYNTHETIC FIXTURE] Verified staged entries: {} | Mapped records: {} | Explicit omissions: {} | Omitted optional capabilities: {}",
        bundle.verified_entries().len(),
        bundle.mapped_records().len(),
        bundle.omission_count(),
        bundle.omitted_optional_capabilities().len()
    )
    .map_err(|e| e.to_string())?;
    for rec in bundle.mapped_records() {
        writeln!(
            out,
            "  [SYNTHETIC FIXTURE] record {}@{} -> {}@{} [origin={}] [authority={:?}]",
            rec.origin_record_id,
            rec.origin_version_id,
            rec.local_record_id,
            rec.local_version_id,
            rec.origin_class.label(),
            rec.attributed_actor.local_authority
        )
        .map_err(|e| e.to_string())?;
    }
    writeln!(out, "[SYNTHETIC FIXTURE] Replay outcomes:").map_err(|e| e.to_string())?;
    for outcome in [&bitwise_outcome, &tolerance_outcome, &missing_model_outcome] {
        match outcome {
            AnalysisReplayOutcome::ReproducedBitwise {
                analysis_id,
                output_sha256_hex,
                byte_len,
                ..
            } => writeln!(
                out,
                "  [SYNTHETIC FIXTURE] {analysis_id}: REPRODUCED BITWISE ({byte_len} bytes, sha256={output_sha256_hex})"
            )
            .map_err(|e| e.to_string())?,
            AnalysisReplayOutcome::ReproducedWithinTolerance {
                analysis_id,
                sample_count,
                max_observed_abs_diff,
                max_allowed_abs_diff,
                ..
            } => writeln!(
                out,
                "  [SYNTHETIC FIXTURE] {analysis_id}: REPRODUCED WITHIN TOLERANCE ({sample_count} samples, max_abs_diff={max_observed_abs_diff:.6} <= {max_allowed_abs_diff:.6})"
            )
            .map_err(|e| e.to_string())?,
            AnalysisReplayOutcome::NonReplayable {
                analysis_id,
                reason,
                ..
            } => writeln!(
                out,
                "  [SYNTHETIC FIXTURE] {analysis_id}: NON-REPLAYABLE ({reason:?})"
            )
            .map_err(|e| e.to_string())?,
            other => writeln!(out, "  [SYNTHETIC FIXTURE] unexpected outcome: {other:?}")
                .map_err(|e| e.to_string())?,
        }
    }

    Ok(())
}
