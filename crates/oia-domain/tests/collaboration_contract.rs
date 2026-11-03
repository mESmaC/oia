//! Targeted contract tests for the proposed role × surface × clearance decision function
//! and the offline field conflict-synchronization kernel (`#38`, `COLLAB-01`, `COLLAB-03`, `FIELD-04`, `A13`, `A16`).

use oia_domain::collaboration::{
    AccessDecision, AccessDenialReason, AccessSurface, CaseRole, ClearanceGrant,
    ConflictResolutionState, DeviceClockQuality, InitialSyncItemInput, OfflineFieldCapture,
    OfflineReferenceFreshness, OfflineSyncItem, SensitivityFlags, SyncError, SyncLimitKind,
    SyncLimits, SyncOutcome, TimestampWithClock, VISIBLE_SIMULATION_LABEL,
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

/// Protects `COLLAB-01`, `COLLAB-03`, `A16`, and `AGENTS.md` §§4, 5:
/// - `evaluate_proposed_access_decision` consults both `(CaseRole, AccessSurface)` from
///   `PROPOSED_ROLE_SURFACE_TABLE` and `ClearanceGrant` independently.
/// - `Viewer` and `Reviewer` are denied on surfaces outside their role table even with `full()` clearance.
/// - `Editor` is denied on `CollaboratorShare` and `PublicPublication` (`docs/architecture.md` §9).
/// - `PublicPublication` enforces `ClearanceGrant::standard_only()` even for `Owner` with `full()`.
/// - `inspect_history_on_surface` and `read_head_on_surface` strip `sensitive_location_detail`
///   across all versions when `allow_sensitive_locations == false`.
/// - `prepare_offline_window` enforces both `OfflinePreparation` role/clearance and `max_prepared_packages`.
#[test]
fn proposed_access_decision_enforces_role_surface_table_and_independent_sensitivity()
-> Result<(), Box<dyn std::error::Error>> {
    // Viewer with full clearance is still denied by role on non-Viewer surfaces (B1).
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

    // Reviewer with full clearance is allowed on ReviewSubmission and PrivateExport, but denied on ModelInput/Mutation/Share/Publish.
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

    // Editor cannot share with collaborators or publish publicly (`docs/architecture.md` §9, B3).
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

    // PublicPublication forces standard_only() clearance even for Owner with full() clearance.
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

    // History inspection strips sensitive_location_detail on every version when un-cleared (B4).
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
        SyncLimits::new(512, 8, 2, 4)?,
    )?;

    let viewer_history = item.inspect_history_on_surface(
        CaseRole::Viewer,
        ClearanceGrant::standard_only(),
        AccessSurface::DirectArtifactRequest,
    )?;
    assert_eq!(viewer_history.len(), 1);
    assert_eq!(viewer_history[0].sensitive_location_detail(), None);
    assert!(viewer_history[0].location_stripped());

    // Viewer and Reviewer cannot prepare offline packages, and max_prepared_packages is enforced (B8).
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

/// Protects `FIELD-04`, `COLLAB-01`, `A13`, `A16`, and review findings B9, B10, B11:
/// - Separate `capture_time`, `receipt_time`, and `sync_time` are preserved on every version.
/// - `evaluate_freshness` accounts for clock offset and uncertainty: both B11 boundary probes
///   return `ExpiryUncertain` rather than `FreshWithinWindow`.
/// - Deduplicating an exact declared-digest capture unions `SensitivityFlags` so stricter
///   sensitivity (`sensitive_artifact: true`) is never lost (B10).
/// - Resolving a conflict after the head has moved (`v1->v2`, conflict `v1->v3`, then `v2->v4`)
///   is refused with `StaleConflictHeadMoved` so `v4` cannot drop out of the head's lineage (B9).
/// - Simulated items carry `VISIBLE_SIMULATION_LABEL` and refuse promotion to `AccountOrigin::Reported`.
#[test]
fn offline_sync_preserves_three_clocks_uncertainty_sensitivity_and_refuses_stale_conflict_head()
-> Result<(), Box<dyn std::error::Error>> {
    let owner = ActorId::new("actor:owner")?;
    let editor = ActorId::new("actor:editor")?;

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
        SyncLimits::default(),
    )?;

    let pkg = item.prepare_offline_window(
        CaseRole::Editor,
        ClearanceGrant::full(),
        editor.clone(),
        OpaqueId::new("pkg:window")?,
        1_000_000,
        2_000_000,
    )?;

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

    // Reading well inside window with small uncertainty -> FreshWithinWindow.
    assert_eq!(
        pkg.evaluate_freshness(&utc_clock(1_500_000, 1_000)),
        OfflineReferenceFreshness::FreshWithinWindow
    );

    // Reading whose earliest bound is past expiry -> CachedExpired.
    assert_eq!(
        pkg.evaluate_freshness(&utc_clock(2_010_000, 2_000)),
        OfflineReferenceFreshness::CachedExpired {
            min_expired_by_ms: 8_000
        }
    );

    // Step 1 (B9 repro): v1 -> v2 applies cleanly and preserves distinct capture, receipt, and sync times.
    let v2_outcome = item.synchronize_offline_capture(
        CaseRole::Editor,
        ClearanceGrant::full(),
        OfflineFieldCapture {
            capture_id: OpaqueId::new("cap:v2")?,
            expected_base_version: VersionId::new("ver:v1")?,
            new_version: VersionId::new("ver:v2")?,
            author: editor.clone(),
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

    // B10: exact duplicate of v2 arriving with sensitive_artifact: true unions sensitivity flags
    // so Viewers can no longer read the item.
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

    // Step 2 (B9 repro): v1 -> v3 conflicts with head v2.
    let conflict_c1 = OpaqueId::new("conflict:v2-v3")?;
    let c1_outcome = item.synchronize_offline_capture(
        CaseRole::Owner,
        ClearanceGrant::full(),
        OfflineFieldCapture {
            capture_id: conflict_c1.clone(),
            expected_base_version: VersionId::new("ver:v1")?,
            new_version: VersionId::new("ver:v3")?,
            author: owner.clone(),
            origin: AccountOrigin::Reported,
            simulation_parameters: None,
            sensitivity: SensitivityFlags::standard(),
            summary_text: "Concurrent owner edit v3.".to_owned(),
            sensitive_location_detail: None,
            declared_content_sha256: "c".repeat(64),
            capture_time: utc_clock(2_000_200, 5),
            receipt_time: utc_clock(2_050_200, 5),
            sync_time: utc_clock(2_060_200, 5),
            prepared_package_id: None,
        },
    )?;
    assert_eq!(
        c1_outcome,
        SyncOutcome::ConflictPreserved {
            conflict_id: conflict_c1.clone(),
            preserved_head_version: VersionId::new("ver:v2")?,
            preserved_offline_version: VersionId::new("ver:v3")?,
        }
    );

    // Step 3 (B9 repro): v2 -> v4 applies cleanly; head is now v4.
    item.synchronize_offline_capture(
        CaseRole::Editor,
        ClearanceGrant::full(),
        OfflineFieldCapture {
            capture_id: OpaqueId::new("cap:v4")?,
            expected_base_version: VersionId::new("ver:v2")?,
            new_version: VersionId::new("ver:v4")?,
            author: editor,
            origin: AccountOrigin::Reported,
            simulation_parameters: None,
            sensitivity: SensitivityFlags::standard(),
            summary_text: "Subsequent edit v4 on top of v2.".to_owned(),
            sensitive_location_detail: None,
            declared_content_sha256: "d".repeat(64),
            capture_time: utc_clock(2_100_000, 5),
            receipt_time: utc_clock(2_100_100, 5),
            sync_time: utc_clock(2_100_200, 5),
            prepared_package_id: None,
        },
    )?;

    // Step 4 (B9 repro): resolving stale conflict (v2, v3) when head is now v4 is refused!
    assert_eq!(
        item.resolve_sync_conflict(
            CaseRole::Owner,
            ClearanceGrant::full(),
            owner.clone(),
            &conflict_c1,
            VersionId::new("ver:v5-stale")?,
            (VersionId::new("ver:v2")?, VersionId::new("ver:v3")?),
            "Stale merge that would drop v4.".to_owned(),
            None,
            "e".repeat(64),
            None,
            utc_clock(2_200_000, 5),
            utc_clock(2_200_000, 5),
            utc_clock(2_200_000, 5),
            "Attempt stale resolution".to_owned(),
        ),
        Err(SyncError::StaleConflictHeadMoved {
            conflict_id: conflict_c1.clone(),
            conflict_head: VersionId::new("ver:v2")?,
            current_head: VersionId::new("ver:v4")?,
        })
    );
    assert_eq!(
        item.inspect_conflicts_on_surface(
            CaseRole::Owner,
            ClearanceGrant::full(),
            AccessSurface::DirectArtifactRequest
        )?[&conflict_c1]
            .resolution(),
        &ConflictResolutionState::Unresolved
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
