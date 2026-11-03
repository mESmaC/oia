//! Targeted contract tests for the proposed role × surface × clearance decision function
//! and the offline field conflict-synchronization kernel (`#38`, `COLLAB-01`, `COLLAB-03`, `FIELD-04`, `A13`, `A16`).

use oia_domain::collaboration::{
    AccessDecision, AccessDenialReason, AccessSurface, CaseRole, ClearanceGrant,
    ConflictResolutionState, DeviceClockQuality, InitialSyncItemInput, MAX_CLOCK_UNCERTAINTY_MS,
    OfflineFieldCapture, OfflineReferenceFreshness, OfflineSyncItem, SensitivityFlags, SyncError,
    SyncLimitKind, SyncLimits, SyncOutcome, TimestampWithClock, VISIBLE_SIMULATION_LABEL,
    evaluate_proposed_access_decision,
};
use oia_domain::witness::AccountOrigin;
use oia_domain::{ActorId, OpaqueId, SchemaVersion, VersionId};

fn utc_clock(recorded_unix_ms: i64, uncertainty_ms: u64) -> TimestampWithClock {
    TimestampWithClock {
        recorded_unix_ms,
        clock_quality: DeviceClockQuality::SynchronizedUtc { uncertainty_ms },
    }
}

/// Protects `COLLAB-01`, `COLLAB-03`, `A16`, and review findings B1–B8, N2, N3:
/// - `evaluate_proposed_access_decision` consults both `(CaseRole, AccessSurface)` from
///   `PROPOSED_ROLE_SURFACE_TABLE` and `ClearanceGrant` independently.
/// - `Viewer` and `Reviewer` are denied on surfaces outside their role table even with `full()` clearance.
/// - `Editor` is denied on `CollaboratorShare` and `PublicPublication` (`docs/architecture.md` §9).
/// - `PublicPublication` enforces `ClearanceGrant::standard_only()` even for `Owner` with `full()`.
/// - `inspect_history_on_surface`, `read_head_on_surface`, and `fmt::Debug` strip/redact
///   `sensitive_location_detail` across all versions (B4, N3).
/// - Un-cleared editors cannot submit sensitive location strings or use deduplication as a
///   location-guessing oracle (N2).
/// - `prepare_offline_window` enforces both `OfflinePreparation` role/clearance and `max_prepared_packages` (B8).
#[test]
fn proposed_access_decision_enforces_role_surface_table_and_independent_sensitivity()
-> Result<(), Box<dyn std::error::Error>> {
    for surface in [
        AccessSurface::ReviewSubmission,
        AccessSurface::PrivateExport,
        AccessSurface::ModelInput,
        AccessSurface::ReportDraftSave,
        AccessSurface::OfflinePreparation,
        AccessSurface::EvidenceMutation,
        AccessSurface::CollaboratorShare,
        AccessSurface::PublicPublication,
    ] {
        assert_eq!(
            evaluate_proposed_access_decision(
                CaseRole::Viewer,
                ClearanceGrant::full(),
                surface,
                SensitivityFlags::standard()
            ),
            AccessDecision::Deny(AccessDenialReason::RoleNotPermittedForSurface {
                role: CaseRole::Viewer,
                surface,
            })
        );
    }

    assert_eq!(
        evaluate_proposed_access_decision(
            CaseRole::Reviewer,
            ClearanceGrant::full(),
            AccessSurface::ReviewSubmission,
            SensitivityFlags::standard()
        ),
        AccessDecision::AllowFull
    );
    assert_eq!(
        evaluate_proposed_access_decision(
            CaseRole::Reviewer,
            ClearanceGrant::full(),
            AccessSurface::ModelInput,
            SensitivityFlags::standard()
        ),
        AccessDecision::Deny(AccessDenialReason::RoleNotPermittedForSurface {
            role: CaseRole::Reviewer,
            surface: AccessSurface::ModelInput,
        })
    );

    for surface in [
        AccessSurface::CollaboratorShare,
        AccessSurface::PublicPublication,
    ] {
        assert_eq!(
            evaluate_proposed_access_decision(
                CaseRole::Editor,
                ClearanceGrant::full(),
                surface,
                SensitivityFlags::standard()
            ),
            AccessDecision::Deny(AccessDenialReason::RoleNotPermittedForSurface {
                role: CaseRole::Editor,
                surface,
            })
        );
    }

    assert_eq!(
        evaluate_proposed_access_decision(
            CaseRole::Owner,
            ClearanceGrant::full(),
            AccessSurface::PublicPublication,
            SensitivityFlags {
                unpublished_draft: false,
                sensitive_artifact: true,
                sensitive_location: false,
            }
        ),
        AccessDecision::Deny(AccessDenialReason::SensitiveArtifactRestricted)
    );
    assert_eq!(
        evaluate_proposed_access_decision(
            CaseRole::Owner,
            ClearanceGrant::full(),
            AccessSurface::PublicPublication,
            SensitivityFlags {
                unpublished_draft: false,
                sensitive_artifact: false,
                sensitive_location: true,
            }
        ),
        AccessDecision::AllowWithStrippedLocation
    );

    let owner = ActorId::new("actor:owner")?;
    let mut item = OfflineSyncItem::new(
        CaseRole::Owner,
        ClearanceGrant::full(),
        InitialSyncItemInput {
            item_id: OpaqueId::new("item:loc-test")?,
            schema_version: SchemaVersion::V1,
            initial_version: VersionId::new("ver:v1")?,
            author: owner.clone(),
            origin: AccountOrigin::Reported,
            simulation_parameters: None,
            sensitivity: SensitivityFlags {
                unpublished_draft: false,
                sensitive_artifact: false,
                sensitive_location: true,
            },
            summary_text: "Observation near ridge.".to_owned(),
            sensitive_location_detail: Some("51.5,-0.1 SECRET".to_owned()),
            declared_content_sha256: "1".repeat(64),
            capture_time: utc_clock(1_000, 5),
            receipt_time: utc_clock(1_100, 5),
            sync_time: utc_clock(1_200, 5),
        },
        SyncLimits::new(512, 8, 2, 1)?,
    )?;

    // N3: Debug formatting redacts sensitive_location_detail.
    let debug_str = format!("{item:?}");
    assert!(!debug_str.contains("51.5,-0.1 SECRET"));
    assert!(debug_str.contains("<redacted>"));

    let viewer_history = item.inspect_history_on_surface(
        CaseRole::Viewer,
        ClearanceGrant::standard_only(),
        AccessSurface::DirectArtifactRequest,
    )?;
    assert_eq!(viewer_history.len(), 1);
    assert_eq!(viewer_history[0].sensitive_location_detail(), None);
    assert!(viewer_history[0].location_stripped());

    // N2: An Editor without allow_sensitive_locations cannot submit a location guess.
    assert_eq!(
        item.synchronize_offline_capture(
            CaseRole::Editor,
            ClearanceGrant::standard_only(),
            OfflineFieldCapture {
                capture_id: OpaqueId::new("cap:loc-guess")?,
                expected_base_version: VersionId::new("ver:v1")?,
                new_version: VersionId::new("ver:v2-guess")?,
                author: ActorId::new("actor:stripped-editor")?,
                origin: AccountOrigin::Reported,
                simulation_parameters: None,
                sensitivity: SensitivityFlags::standard(),
                summary_text: "Observation near ridge.".to_owned(),
                sensitive_location_detail: Some("51.5,-0.1 SECRET".to_owned()),
                declared_content_sha256: "1".repeat(64),
                capture_time: utc_clock(1_500, 5),
                receipt_time: utc_clock(1_500, 5),
                sync_time: utc_clock(1_500, 5),
                prepared_package_id: None,
            }
        ),
        Err(SyncError::AccessDenied(
            AccessDenialReason::SensitiveLocationRestricted
        ))
    );

    assert_eq!(
        item.prepare_offline_window(
            CaseRole::Viewer,
            ClearanceGrant::standard_only(),
            ActorId::new("actor:viewer")?,
            OpaqueId::new("pkg:viewer-denied")?,
            1_000,
            2_000,
        ),
        Err(SyncError::AccessDenied(
            AccessDenialReason::RoleNotPermittedForSurface {
                role: CaseRole::Viewer,
                surface: AccessSurface::OfflinePreparation,
            }
        ))
    );
    item.prepare_offline_window(
        CaseRole::Editor,
        ClearanceGrant::standard_only(),
        owner.clone(),
        OpaqueId::new("pkg:1")?,
        1_000,
        2_000,
    )?;
    item.prepare_offline_window(
        CaseRole::Editor,
        ClearanceGrant::standard_only(),
        owner.clone(),
        OpaqueId::new("pkg:2")?,
        1_000,
        2_000,
    )?;
    assert_eq!(
        item.prepare_offline_window(
            CaseRole::Editor,
            ClearanceGrant::standard_only(),
            owner,
            OpaqueId::new("pkg:3-over-limit")?,
            1_000,
            2_000,
        ),
        Err(SyncError::LimitExceeded {
            kind: SyncLimitKind::PreparedPackages,
            limit: 2,
        })
    );

    Ok(())
}

/// Protects `FIELD-04`, `COLLAB-01`, `A13`, `A16`, and review findings B9, B10, B11, N1, N7:
/// - Separate `capture_time`, `receipt_time`, and `sync_time` are preserved on every version.
/// - `evaluate_freshness` reports `BeforePreparationWindow`, `FreshWithinWindow`, `CachedExpired`,
///   and `ExpiryUncertain` across both B11 boundary probes, and `validate_clock` bounds `uncertainty_ms` (N7).
/// - Deduplicating an exact declared-digest capture unions `SensitivityFlags` (B10).
/// - Two concurrent observers (conflict A: `v1->vA` and conflict B: `v1->vB` against head `v2`):
///   after resolving A into `m1`, resolving B with stale `(v2, vB)` is refused (`StaleConflictHeadMoved`),
///   and resolving B against the descended current head `(m1, vB)` succeeds (`m2`), preserving
///   both `m1` and `vB` in `m2`'s lineage; only unresolved conflicts count toward `max_conflicts` (N1).
#[test]
fn offline_sync_preserves_three_clocks_uncertainty_sensitivity_and_resolves_multi_observer_conflicts()
-> Result<(), Box<dyn std::error::Error>> {
    let owner = ActorId::new("actor:owner")?;
    let observer_a = ActorId::new("actor:observer-a")?;
    let observer_b = ActorId::new("actor:observer-b")?;

    // Use max_conflicts = 2 so both observer A and observer B can be concurrently open,
    // then verify a 3rd conflict succeeds after resolving one (unresolved-only cap, N1).
    let mut item = OfflineSyncItem::new(
        CaseRole::Owner,
        ClearanceGrant::full(),
        InitialSyncItemInput {
            item_id: OpaqueId::new("item:field-log")?,
            schema_version: SchemaVersion::V1,
            initial_version: VersionId::new("ver:v1")?,
            author: owner.clone(),
            origin: AccountOrigin::Reported,
            simulation_parameters: None,
            sensitivity: SensitivityFlags::standard(),
            summary_text: "Initial field log v1.".to_owned(),
            sensitive_location_detail: None,
            declared_content_sha256: "a".repeat(64),
            capture_time: utc_clock(1_000, 5),
            receipt_time: utc_clock(1_050, 5),
            sync_time: utc_clock(1_100, 5),
        },
        SyncLimits::new(512, 16, 4, 2)?,
    )?;

    let pkg = item.prepare_offline_window(
        CaseRole::Editor,
        ClearanceGrant::full(),
        observer_a.clone(),
        OpaqueId::new("pkg:window")?,
        1_000_000,
        2_000_000,
    )?;

    // N7: capture strictly before preparation window -> BeforePreparationWindow.
    assert_eq!(
        pkg.evaluate_freshness(&utc_clock(900_000, 1_000)),
        OfflineReferenceFreshness::BeforePreparationWindow {
            min_premature_by_ms: 99_000
        }
    );

    // B11 Probe 1: DriftingLocalClock { offset: +600_000, unc: 300_000 }, read 1 s before expiry -> ExpiryUncertain.
    let probe1 = pkg.evaluate_freshness(&TimestampWithClock {
        recorded_unix_ms: 1_999_000,
        clock_quality: DeviceClockQuality::DriftingLocalClock {
            estimated_offset_ms: 600_000,
            uncertainty_ms: 300_000,
        },
    });
    assert!(matches!(
        probe1,
        OfflineReferenceFreshness::ExpiryUncertain { .. }
    ));

    // B11 Probe 2: SynchronizedUtc { unc: 60_000 }, read 1 ms before expiry -> ExpiryUncertain.
    let probe2 = pkg.evaluate_freshness(&TimestampWithClock {
        recorded_unix_ms: 1_999_999,
        clock_quality: DeviceClockQuality::SynchronizedUtc {
            uncertainty_ms: 60_000,
        },
    });
    assert!(matches!(
        probe2,
        OfflineReferenceFreshness::ExpiryUncertain { .. }
    ));

    assert_eq!(
        pkg.evaluate_freshness(&utc_clock(1_500_000, 1_000)),
        OfflineReferenceFreshness::FreshWithinWindow
    );
    assert_eq!(
        pkg.evaluate_freshness(&utc_clock(2_010_000, 2_000)),
        OfflineReferenceFreshness::CachedExpired {
            min_expired_by_ms: 8_000
        }
    );

    // N7: oversized clock uncertainty is rejected.
    assert_eq!(
        item.synchronize_offline_capture(
            CaseRole::Editor,
            ClearanceGrant::full(),
            OfflineFieldCapture {
                capture_id: OpaqueId::new("cap:bad-clock")?,
                expected_base_version: VersionId::new("ver:v1")?,
                new_version: VersionId::new("ver:v2-bad-clock")?,
                author: observer_a.clone(),
                origin: AccountOrigin::Reported,
                simulation_parameters: None,
                sensitivity: SensitivityFlags::standard(),
                summary_text: "Bad clock uncertainty.".to_owned(),
                sensitive_location_detail: None,
                declared_content_sha256: "b".repeat(64),
                capture_time: utc_clock(1_500_000, MAX_CLOCK_UNCERTAINTY_MS + 1),
                receipt_time: utc_clock(1_500_000, 5),
                sync_time: utc_clock(1_500_000, 5),
                prepared_package_id: None,
            }
        ),
        Err(SyncError::ClockUncertaintyOutOfRange {
            max_allowed_ms: MAX_CLOCK_UNCERTAINTY_MS,
        })
    );

    // Step 1 (N1 / A13 two-observer sequence): v1 -> v2 applies cleanly.
    let v2_outcome = item.synchronize_offline_capture(
        CaseRole::Editor,
        ClearanceGrant::full(),
        OfflineFieldCapture {
            capture_id: OpaqueId::new("cap:v2")?,
            expected_base_version: VersionId::new("ver:v1")?,
            new_version: VersionId::new("ver:v2")?,
            author: observer_a.clone(),
            origin: AccountOrigin::Reported,
            simulation_parameters: None,
            sensitivity: SensitivityFlags::standard(),
            summary_text: "Field log v2.".to_owned(),
            sensitive_location_detail: None,
            declared_content_sha256: "b".repeat(64),
            capture_time: utc_clock(1_999_999, 60_000),
            receipt_time: utc_clock(2_050_000, 10),
            sync_time: utc_clock(2_060_000, 10),
            prepared_package_id: Some(pkg.package_id().clone()),
        },
    )?;
    assert!(matches!(
        v2_outcome,
        SyncOutcome::AppliedCleanly {
            reference_freshness: Some(OfflineReferenceFreshness::ExpiryUncertain { .. }),
            ..
        }
    ));
    let head_v2 = item.read_head_on_surface(
        CaseRole::Viewer,
        ClearanceGrant::standard_only(),
        AccessSurface::Search,
    )?;
    assert_eq!(head_v2.capture_time().recorded_unix_ms, 1_999_999);
    assert_eq!(head_v2.receipt_time().recorded_unix_ms, 2_050_000);
    assert_eq!(head_v2.sync_time().recorded_unix_ms, 2_060_000);

    // B10: exact duplicate of v2 arriving with sensitive_artifact: true unions sensitivity flags.
    let dedup_outcome = item.synchronize_offline_capture(
        CaseRole::Owner,
        ClearanceGrant::full(),
        OfflineFieldCapture {
            capture_id: OpaqueId::new("cap:v2-dup-sensitive")?,
            expected_base_version: VersionId::new("ver:v1")?,
            new_version: VersionId::new("ver:v2-dup")?,
            author: owner.clone(),
            origin: AccountOrigin::Reported,
            simulation_parameters: None,
            sensitivity: SensitivityFlags {
                unpublished_draft: false,
                sensitive_artifact: true,
                sensitive_location: false,
            },
            summary_text: "Field log v2.".to_owned(),
            sensitive_location_detail: None,
            declared_content_sha256: "b".repeat(64),
            capture_time: utc_clock(2_000_000, 5),
            receipt_time: utc_clock(2_050_100, 5),
            sync_time: utc_clock(2_060_100, 5),
            prepared_package_id: None,
        },
    )?;
    assert_eq!(
        dedup_outcome,
        SyncOutcome::DeduplicatedByDeclaredDigest {
            retained_head_version: VersionId::new("ver:v2")?,
            recorded_duplicate_version: VersionId::new("ver:v2-dup")?,
        }
    );
    assert_eq!(
        item.read_head_on_surface(
            CaseRole::Viewer,
            ClearanceGrant::standard_only(),
            AccessSurface::Search
        ),
        Err(SyncError::AccessDenied(
            AccessDenialReason::SensitiveArtifactRestricted
        ))
    );

    // Step 2 (N1): Observer A syncs v1 -> vA, giving conflict A.
    let conflict_a = OpaqueId::new("conflict:a")?;
    assert_eq!(
        item.synchronize_offline_capture(
            CaseRole::Editor,
            ClearanceGrant::full(),
            OfflineFieldCapture {
                capture_id: conflict_a.clone(),
                expected_base_version: VersionId::new("ver:v1")?,
                new_version: VersionId::new("ver:vA")?,
                author: observer_a,
                origin: AccountOrigin::Reported,
                simulation_parameters: None,
                sensitivity: SensitivityFlags::standard(),
                summary_text: "Observer A bearing log.".to_owned(),
                sensitive_location_detail: None,
                declared_content_sha256: "c".repeat(64),
                capture_time: utc_clock(2_000_200, 5),
                receipt_time: utc_clock(2_050_200, 5),
                sync_time: utc_clock(2_060_200, 5),
                prepared_package_id: None,
            },
        )?,
        SyncOutcome::ConflictPreserved {
            conflict_id: conflict_a.clone(),
            preserved_head_version: VersionId::new("ver:v2")?,
            preserved_offline_version: VersionId::new("ver:vA")?,
        }
    );

    // Step 3 (N1): Observer B syncs v1 -> vB, giving conflict B.
    let conflict_b = OpaqueId::new("conflict:b")?;
    assert_eq!(
        item.synchronize_offline_capture(
            CaseRole::Editor,
            ClearanceGrant::full(),
            OfflineFieldCapture {
                capture_id: conflict_b.clone(),
                expected_base_version: VersionId::new("ver:v1")?,
                new_version: VersionId::new("ver:vB")?,
                author: observer_b.clone(),
                origin: AccountOrigin::Reported,
                simulation_parameters: None,
                sensitivity: SensitivityFlags::standard(),
                summary_text: "Observer B bearing log.".to_owned(),
                sensitive_location_detail: None,
                declared_content_sha256: "d".repeat(64),
                capture_time: utc_clock(2_000_300, 5),
                receipt_time: utc_clock(2_050_300, 5),
                sync_time: utc_clock(2_060_300, 5),
                prepared_package_id: None,
            },
        )?,
        SyncOutcome::ConflictPreserved {
            conflict_id: conflict_b.clone(),
            preserved_head_version: VersionId::new("ver:v2")?,
            preserved_offline_version: VersionId::new("ver:vB")?,
        }
    );

    // Step 4 (N1): Resolving conflict A with (v2, vA) succeeds; head is now m1.
    item.resolve_sync_conflict(
        CaseRole::Owner,
        ClearanceGrant::full(),
        owner.clone(),
        &conflict_a,
        VersionId::new("ver:m1")?,
        (VersionId::new("ver:v2")?, VersionId::new("ver:vA")?),
        "Merged v2 and vA into m1.".to_owned(),
        None,
        "e".repeat(64),
        None,
        utc_clock(2_100_000, 5),
        utc_clock(2_100_000, 5),
        utc_clock(2_100_000, 5),
        "Resolved conflict A".to_owned(),
    )?;

    // Step 5a (B9): Resolving conflict B with stale (v2, vB) is refused so m1 is not dropped!
    assert_eq!(
        item.resolve_sync_conflict(
            CaseRole::Owner,
            ClearanceGrant::full(),
            owner.clone(),
            &conflict_b,
            VersionId::new("ver:m2-stale")?,
            (VersionId::new("ver:v2")?, VersionId::new("ver:vB")?),
            "Stale merge that would drop m1.".to_owned(),
            None,
            "f".repeat(64),
            None,
            utc_clock(2_200_000, 5),
            utc_clock(2_200_000, 5),
            utc_clock(2_200_000, 5),
            "Attempt stale resolution".to_owned(),
        ),
        Err(SyncError::StaleConflictHeadMoved {
            conflict_id: conflict_b.clone(),
            conflict_head: VersionId::new("ver:v2")?,
            current_head: VersionId::new("ver:m1")?,
        })
    );

    // Step 5b (N1): Resolving conflict B against the current head (m1, vB) succeeds!
    item.resolve_sync_conflict(
        CaseRole::Owner,
        ClearanceGrant::full(),
        owner.clone(),
        &conflict_b,
        VersionId::new("ver:m2")?,
        (VersionId::new("ver:m1")?, VersionId::new("ver:vB")?),
        "Merged m1 and vB into m2 preserving v2, vA, and vB.".to_owned(),
        None,
        "0".repeat(64),
        None,
        utc_clock(2_200_000, 5),
        utc_clock(2_200_000, 5),
        utc_clock(2_200_000, 5),
        "Resolved conflict B against descended head m1".to_owned(),
    )?;
    assert_eq!(
        item.inspect_conflicts_on_surface(
            CaseRole::Owner,
            ClearanceGrant::full(),
            AccessSurface::DirectArtifactRequest
        )?[&conflict_b]
            .resolution(),
        &ConflictResolutionState::Resolved {
            resolution_version: VersionId::new("ver:m2")?,
            resolved_by: owner.clone(),
            rationale: "Resolved conflict B against descended head m1".to_owned(),
        }
    );

    // N1: Because resolved conflicts do not count against max_conflicts (2), a new divergent
    // offline capture is accepted rather than rejected with LimitExceeded.
    let conflict_c = OpaqueId::new("conflict:c")?;
    assert_eq!(
        item.synchronize_offline_capture(
            CaseRole::Editor,
            ClearanceGrant::full(),
            OfflineFieldCapture {
                capture_id: conflict_c.clone(),
                expected_base_version: VersionId::new("ver:v1")?,
                new_version: VersionId::new("ver:vC")?,
                author: observer_b,
                origin: AccountOrigin::Reported,
                simulation_parameters: None,
                sensitivity: SensitivityFlags::standard(),
                summary_text: "Late offline packet C after A and B were resolved.".to_owned(),
                sensitive_location_detail: None,
                declared_content_sha256: "8".repeat(64),
                capture_time: utc_clock(2_000_400, 5),
                receipt_time: utc_clock(2_300_000, 5),
                sync_time: utc_clock(2_300_100, 5),
                prepared_package_id: None,
            },
        )?,
        SyncOutcome::ConflictPreserved {
            conflict_id: conflict_c,
            preserved_head_version: VersionId::new("ver:m2")?,
            preserved_offline_version: VersionId::new("ver:vC")?,
        }
    );

    // Simulation item preserves VISIBLE_SIMULATION_LABEL and refuses origin change to Reported.
    let mut sim_item = OfflineSyncItem::new(
        CaseRole::Owner,
        ClearanceGrant::full(),
        InitialSyncItemInput {
            item_id: OpaqueId::new("item:sim-01")?,
            schema_version: SchemaVersion::V1,
            initial_version: VersionId::new("ver:sim-1")?,
            author: owner.clone(),
            origin: AccountOrigin::Simulated,
            simulation_parameters: Some("blur_px=2.0, clock_drift_ms=250".to_owned()),
            sensitivity: SensitivityFlags::standard(),
            summary_text: "Synthetic trajectory fixture.".to_owned(),
            sensitive_location_detail: None,
            declared_content_sha256: "f".repeat(64),
            capture_time: utc_clock(1_000, 0),
            receipt_time: utc_clock(1_000, 0),
            sync_time: utc_clock(1_000, 0),
        },
        SyncLimits::default(),
    )?;
    let sim_head = sim_item.read_head_on_surface(
        CaseRole::Viewer,
        ClearanceGrant::standard_only(),
        AccessSurface::Search,
    )?;
    assert_eq!(
        sim_head.visible_simulation_label(),
        Some(VISIBLE_SIMULATION_LABEL)
    );
    assert_eq!(
        sim_item.synchronize_offline_capture(
            CaseRole::Owner,
            ClearanceGrant::full(),
            OfflineFieldCapture {
                capture_id: OpaqueId::new("cap:sim-promote")?,
                expected_base_version: VersionId::new("ver:sim-1")?,
                new_version: VersionId::new("ver:sim-2")?,
                author: owner,
                origin: AccountOrigin::Reported,
                simulation_parameters: None,
                sensitivity: SensitivityFlags::standard(),
                summary_text: "Attempt to promote simulation to reported.".to_owned(),
                sensitive_location_detail: None,
                declared_content_sha256: "9".repeat(64),
                capture_time: utc_clock(2_000, 0),
                receipt_time: utc_clock(2_000, 0),
                sync_time: utc_clock(2_000, 0),
                prepared_package_id: None,
            },
        ),
        Err(SyncError::OriginMismatch {
            expected: AccountOrigin::Simulated,
            actual: AccountOrigin::Reported,
        })
    );

    Ok(())
}
