//! Targeted contract tests for shared-case roles, sensitive-material isolation across
//! surfaces, scoped reviews, distinct report delivery modes, and offline field
//! synchronization with explicit conflict preservation (`#38`, `COLLAB-01`, `COLLAB-03`, `FIELD-04`, `A13`, `A16`).

use oia_domain::collaboration::{
    AccessSurface, AssignmentStatus, CaseItemInput, CaseRole, ClearanceGrant, CollaborationError,
    CollaborationLimitKind, CollaborationLimits, ConflictResolutionState, DeviceClockQuality,
    EvidenceOriginKind, MANDATORY_REVIEW_SCOPE_CAVEAT, OfflineFieldCapture,
    OfflineReferenceFreshness, RedactionReason, ReportDeliveryMode, ReviewDisposition, ReviewScope,
    ScopedReviewInput, SensitivityFlags, SharedCaseWorkspace, SyncOutcome,
    VISIBLE_SIMULATION_LABEL,
};
use oia_domain::{ActorId, OpaqueId, SchemaVersion, VersionId};

/// Protects `COLLAB-01`, `COLLAB-03`, and `A16`:
/// - Sensitive artifacts, unpublished drafts, and sensitive locations are isolated across
///   `Search`, `BlobRead`, `Subscription`, `ModelInput`, `ReportGeneration`,
///   `DirectArtifactRequest`, `OfflinePreparation`, and `Export`.
/// - Physical redaction strips `sensitive_location_detail` when `allow_sensitive_locations == false`.
/// - Reviews record author and scope (`implies_universal_truth() == false`) and complete matching assignments.
/// - Saving a draft, exporting privately, sharing with collaborators, and publishing are distinct;
///   only `Owner` can publish; downstream corrections flag issued reports without mutating their
///   original content or version IDs.
#[test]
fn role_and_sensitivity_isolation_across_surfaces_and_distinct_report_delivery()
-> Result<(), Box<dyn std::error::Error>> {
    let owner = ActorId::new("actor:owner")?;
    let editor = ActorId::new("actor:editor")?;
    let reviewer = ActorId::new("actor:reviewer")?;
    let viewer = ActorId::new("actor:viewer")?;

    let mut ws = SharedCaseWorkspace::new(
        OpaqueId::new("case:a16-shared")?,
        SchemaVersion::V1,
        owner.clone(),
        ClearanceGrant::full(),
        CollaborationLimits::default(),
    );

    ws.set_collaborator_role(
        &owner,
        editor.clone(),
        CaseRole::Editor,
        ClearanceGrant {
            allow_unpublished_drafts: true,
            allow_sensitive_artifacts: true,
            allow_sensitive_locations: false,
        },
    )?;
    ws.set_collaborator_role(
        &owner,
        reviewer.clone(),
        CaseRole::Reviewer,
        ClearanceGrant {
            allow_unpublished_drafts: true,
            allow_sensitive_artifacts: false,
            allow_sensitive_locations: false,
        },
    )?;
    ws.set_collaborator_role(
        &owner,
        viewer.clone(),
        CaseRole::Viewer,
        ClearanceGrant::standard_only(),
    )?;

    // Non-owner cannot alter collaborator roles or remove the last owner.
    assert_eq!(
        ws.set_collaborator_role(
            &editor,
            viewer.clone(),
            CaseRole::Editor,
            ClearanceGrant::full()
        ),
        Err(CollaborationError::UnauthorizedRole {
            actor: editor.clone(),
            role: CaseRole::Editor,
            action: "set_collaborator_role",
        })
    );
    assert_eq!(
        ws.set_collaborator_role(
            &owner,
            owner.clone(),
            CaseRole::Editor,
            ClearanceGrant::full()
        ),
        Err(CollaborationError::CannotRemoveLastOwner)
    );

    let public_with_sensitive_loc = OpaqueId::new("item:public-obs")?;
    let sensitive_artifact = OpaqueId::new("item:sensitive-raw")?;
    let unpublished_note = OpaqueId::new("item:draft-note")?;

    ws.add_case_item(
        &owner,
        CaseItemInput {
            item_id: public_with_sensitive_loc.clone(),
            version: VersionId::new("ver:obs-v1")?,
            sensitivity: SensitivityFlags {
                unpublished_draft: false,
                sensitive_artifact: false,
                sensitive_location: true,
            },
            summary_text: "Witness reported bright stationary object above ridge.".to_owned(),
            sensitive_location_detail: Some("Private residence: 1420 Willow Creek Rd".to_owned()),
            content_sha256: "1".repeat(64),
            origin: EvidenceOriginKind::ObservedOrReported,
            clock_quality: DeviceClockQuality::SynchronizedUtc { uncertainty_ms: 10 },
        },
    )?;
    ws.add_case_item(
        &editor,
        CaseItemInput {
            item_id: sensitive_artifact.clone(),
            version: VersionId::new("ver:raw-v1")?,
            sensitivity: SensitivityFlags {
                unpublished_draft: false,
                sensitive_artifact: true,
                sensitive_location: true,
            },
            summary_text: "Unredacted home driveway camera clip.".to_owned(),
            sensitive_location_detail: Some("1420 Willow Creek Rd driveway".to_owned()),
            content_sha256: "2".repeat(64),
            origin: EvidenceOriginKind::ObservedOrReported,
            clock_quality: DeviceClockQuality::SynchronizedUtc { uncertainty_ms: 25 },
        },
    )?;
    ws.add_case_item(
        &editor,
        CaseItemInput {
            item_id: unpublished_note.clone(),
            version: VersionId::new("ver:draft-v1")?,
            sensitivity: SensitivityFlags {
                unpublished_draft: true,
                sensitive_artifact: false,
                sensitive_location: false,
            },
            summary_text: "Unpublished working note on optical reflection hypothesis.".to_owned(),
            sensitive_location_detail: None,
            content_sha256: "3".repeat(64),
            origin: EvidenceOriginKind::ObservedOrReported,
            clock_quality: DeviceClockQuality::SynchronizedUtc { uncertainty_ms: 10 },
        },
    )?;

    // Check every access surface for Viewer: only `public_with_sensitive_loc` is permitted,
    // its sensitive_location_detail is physically stripped (`None`), and 3 redaction notices
    // (1 location strip + 1 sensitive artifact + 1 unpublished draft) are emitted.
    for surface in [
        AccessSurface::Search,
        AccessSurface::BlobRead,
        AccessSurface::Subscription,
        AccessSurface::ModelInput,
        AccessSurface::ReportGeneration,
        AccessSurface::OfflinePreparation,
        AccessSurface::Export,
    ] {
        let env = ws.query_surface(&viewer, surface)?;
        assert_eq!(env.permitted_items().len(), 1);
        assert_eq!(
            env.permitted_items()[0].item_id(),
            &public_with_sensitive_loc
        );
        assert_eq!(env.permitted_items()[0].sensitive_location_detail(), None);
        assert!(env.permitted_items()[0].redacted_sensitive_location());
        assert_eq!(env.redaction_notices().len(), 3);
    }

    assert_eq!(
        ws.request_item_on_surface(
            &viewer,
            &sensitive_artifact,
            AccessSurface::DirectArtifactRequest
        ),
        Err(CollaborationError::AccessDenied {
            actor: viewer.clone(),
            item_id: sensitive_artifact.clone(),
            surface: AccessSurface::DirectArtifactRequest,
            reason: RedactionReason::SensitiveArtifactRestricted,
        })
    );
    assert_eq!(
        ws.inspect_item_history(&reviewer, &sensitive_artifact),
        Err(CollaborationError::AccessDenied {
            actor: reviewer.clone(),
            item_id: sensitive_artifact.clone(),
            surface: AccessSurface::DirectArtifactRequest,
            reason: RedactionReason::SensitiveArtifactRestricted,
        })
    );

    // Assign and submit scoped review + competing interpretations (`COLLAB-01`).
    let assignment = ws.assign_review(
        &editor,
        OpaqueId::new("assign:obs-01")?,
        reviewer.clone(),
        public_with_sensitive_loc.clone(),
        VersionId::new("ver:obs-v1")?,
        "Check passage wording and geometry scope.".to_owned(),
    )?;
    assert_eq!(assignment.status(), AssignmentStatus::Open);

    let review_v1 = ws.submit_scoped_review(
        &reviewer,
        ScopedReviewInput {
            review_id: OpaqueId::new("rev:obs-01")?,
            review_version: VersionId::new("ver:rev-01")?,
            target_item_id: public_with_sensitive_loc.clone(),
            target_version: VersionId::new("ver:obs-v1")?,
            scope: ReviewScope::PassageByteRange {
                start_byte: 0,
                end_byte: 16,
            },
            disposition: ReviewDisposition::SupportedWithinScope,
            rationale: "Initial narrative wording matches witness intake transcript.".to_owned(),
            cited_evidence_versions: vec![VersionId::new("ver:obs-v1")?],
            supersedes_review_version: None,
        },
    )?;
    assert!(!review_v1.implies_universal_truth());
    assert_eq!(review_v1.scope_caveat(), MANDATORY_REVIEW_SCOPE_CAVEAT);
    assert_eq!(
        ws.assignments()[0].status(),
        AssignmentStatus::CompletedWithReview
    );

    // Superseding review appends to history rather than overwriting `review_v1`.
    let _review_v2 = ws.submit_scoped_review(
        &reviewer,
        ScopedReviewInput {
            review_id: OpaqueId::new("rev:obs-01")?,
            review_version: VersionId::new("ver:rev-02")?,
            target_item_id: public_with_sensitive_loc.clone(),
            target_version: VersionId::new("ver:obs-v1")?,
            scope: ReviewScope::CalibrationAndGeometryOnly,
            disposition: ReviewDisposition::UnresolvedNeedsCollection,
            rationale: "Elevation angle requires second observer baseline.".to_owned(),
            cited_evidence_versions: vec![VersionId::new("ver:obs-v1")?],
            supersedes_review_version: Some(VersionId::new("ver:rev-01")?),
        },
    )?;
    assert_eq!(ws.reviews().len(), 2);

    ws.add_competing_interpretation(
        &reviewer,
        OpaqueId::new("interp:landing-light")?,
        VersionId::new("ver:interp-01")?,
        public_with_sensitive_loc.clone(),
        "Distant aircraft approach light on 040 deg corridor.".to_owned(),
        vec![VersionId::new("ver:obs-v1")?],
        vec![],
        "Assumes clear line of sight to approach corridor.".to_owned(),
    )?;
    ws.add_competing_interpretation(
        &editor,
        OpaqueId::new("interp:tethered-aerostat")?,
        VersionId::new("ver:interp-02")?,
        public_with_sensitive_loc.clone(),
        "Stationary tethered telemetry balloon over ridge.".to_owned(),
        vec![VersionId::new("ver:obs-v1")?],
        vec![],
        "Assumes low wind shear at 500 m AGL.".to_owned(),
    )?;
    assert_eq!(ws.competing_interpretations().len(), 2);

    // Distinct report transitions (`COLLAB-03`): Save != Private Export != Share != Publish.
    let all_items = vec![
        public_with_sensitive_loc.clone(),
        sensitive_artifact.clone(),
        unpublished_note.clone(),
    ];
    let saved_draft = ws.save_report_draft(
        &editor,
        OpaqueId::new("rep:case-summary")?,
        VersionId::new("ver:rep-draft-1")?,
        "Working Draft".to_owned(),
        &all_items,
    )?;
    assert_eq!(saved_draft.mode(), ReportDeliveryMode::SavedDraftOnly);
    assert!(!saved_draft.is_publicly_published());

    let private_export = ws.export_report_privately(
        &editor,
        OpaqueId::new("rep:case-summary")?,
        VersionId::new("ver:rep-priv-1")?,
        "Editor Private Export".to_owned(),
        &all_items,
    )?;
    assert_eq!(private_export.mode(), ReportDeliveryMode::PrivateExportOnly);
    assert!(!private_export.is_publicly_published());
    // Editor has sensitive_artifact + unpublished_draft clearance, but not sensitive_location.
    assert_eq!(private_export.included_items().len(), 3);
    assert_eq!(private_export.redaction_preview().len(), 2);

    let shared_collab = ws.share_report_with_collaborators(
        &editor,
        OpaqueId::new("rep:case-summary")?,
        VersionId::new("ver:rep-shared-1")?,
        "Reviewer Share".to_owned(),
        &all_items,
        ClearanceGrant {
            allow_unpublished_drafts: true,
            allow_sensitive_artifacts: false,
            allow_sensitive_locations: false,
        },
    )?;
    assert_eq!(
        shared_collab.mode(),
        ReportDeliveryMode::SharedWithCollaborators
    );
    assert!(!shared_collab.is_publicly_published());
    assert_eq!(shared_collab.included_items().len(), 2);

    // Editor cannot publish publicly; only Owner can publish.
    assert_eq!(
        ws.publish_report_snapshot(
            &editor,
            OpaqueId::new("rep:case-summary")?,
            VersionId::new("ver:rep-pub-denied")?,
            "Unauthorized Publish".to_owned(),
            &all_items,
        ),
        Err(CollaborationError::UnauthorizedRole {
            actor: editor.clone(),
            role: CaseRole::Editor,
            action: "publish_report_snapshot",
        })
    );

    let published = ws.publish_report_snapshot(
        &owner,
        OpaqueId::new("rep:case-summary")?,
        VersionId::new("ver:rep-pub-1")?,
        "Public Issued Report".to_owned(),
        &all_items,
    )?;
    assert!(published.is_publicly_published());
    // Public snapshot strips unpublished_note, sensitive_artifact, and sensitive_location_detail!
    assert_eq!(published.included_items().len(), 1);
    assert_eq!(
        published.included_items()[0].sensitive_location_detail(),
        None
    );
    assert_eq!(published.redaction_preview().len(), 3);

    // Source correction flags dependent report snapshots without overwriting their issued versions.
    let flagged = ws.flag_dependent_reports_on_correction(
        &editor,
        VersionId::new("ver:obs-v1")?,
        VersionId::new("ver:obs-v2-corrected")?,
        "Witness corrected azimuth reference landmark.".to_owned(),
    )?;
    assert_eq!(flagged, 4);
    let pub_after = &ws.report_snapshots()[3];
    assert_eq!(
        pub_after.report_version(),
        &VersionId::new("ver:rep-pub-1")?
    );
    assert_eq!(pub_after.title(), "Public Issued Report");
    assert!(pub_after.needs_reassessment());
    assert_eq!(pub_after.dependency_flags().len(), 1);

    Ok(())
}

/// Protects `FIELD-04`, `COLLAB-01`, `A13`, and `AGENTS.md` §§2, 5:
/// - Offline packages mark cached/expired data and clock-uncertain captures.
/// - Concurrent offline edits against the same base version retain both versions and create
///   an explicit `SyncConflictRecord` unless exact-byte deduplication applies.
/// - Conflict resolution requires linking both conflicting parent versions; both parent versions
///   remain in the item's history.
/// - Simulation fixtures retain `VISIBLE_SIMULATION_LABEL` across surfaces/exports and cannot
///   be promoted to observed evidence.
#[test]
fn offline_sync_preserves_cached_expiry_clock_uncertainty_simulations_and_conflicts()
-> Result<(), Box<dyn std::error::Error>> {
    let owner = ActorId::new("actor:owner")?;
    let editor = ActorId::new("actor:editor")?;
    let viewer = ActorId::new("actor:viewer")?;

    let mut ws = SharedCaseWorkspace::new(
        OpaqueId::new("case:a13-fieldwork")?,
        SchemaVersion::V1,
        owner.clone(),
        ClearanceGrant::full(),
        CollaborationLimits::new(512, 8, 16, 8, 16, 8)?,
    );
    ws.set_collaborator_role(
        &owner,
        editor.clone(),
        CaseRole::Editor,
        ClearanceGrant::full(),
    )?;
    ws.set_collaborator_role(
        &owner,
        viewer.clone(),
        CaseRole::Viewer,
        ClearanceGrant::standard_only(),
    )?;

    let field_log = OpaqueId::new("item:field-log")?;
    let sim_track = OpaqueId::new("item:sim-track")?;

    ws.add_case_item(
        &owner,
        CaseItemInput {
            item_id: field_log.clone(),
            version: VersionId::new("ver:log-v1")?,
            sensitivity: SensitivityFlags::standard(),
            summary_text: "Baseline tripod setup at Site Alpha.".to_owned(),
            sensitive_location_detail: None,
            content_sha256: "a".repeat(64),
            origin: EvidenceOriginKind::ObservedOrReported,
            clock_quality: DeviceClockQuality::SynchronizedUtc { uncertainty_ms: 5 },
        },
    )?;
    ws.add_case_item(
        &owner,
        CaseItemInput {
            item_id: sim_track.clone(),
            version: VersionId::new("ver:sim-v1")?,
            sensitivity: SensitivityFlags::standard(),
            summary_text: "Simulated low-elevation trajectory with 1.5 deg Gaussian noise."
                .to_owned(),
            sensitive_location_detail: None,
            content_sha256: "b".repeat(64),
            origin: EvidenceOriginKind::LabeledSimulation {
                simulation_parameters: "sigma_deg=1.5, clock_drift_ms=250".to_owned(),
            },
            clock_quality: DeviceClockQuality::SynchronizedUtc { uncertainty_ms: 0 },
        },
    )?;

    // Attempt to promote a simulated item to ObservedOrReported during offline sync is rejected.
    assert_eq!(
        ws.synchronize_offline_capture(
            &editor,
            OfflineFieldCapture {
                capture_id: OpaqueId::new("cap:illegal-promotion")?,
                target_item_id: sim_track.clone(),
                base_version: Some(VersionId::new("ver:sim-v1")?),
                new_version: VersionId::new("ver:sim-v2-promoted")?,
                sensitivity: SensitivityFlags::standard(),
                summary_text: "Re-labeled as real observation.".to_owned(),
                sensitive_location_detail: None,
                content_sha256: "c".repeat(64),
                origin: EvidenceOriginKind::ObservedOrReported,
                clock_quality: DeviceClockQuality::SynchronizedUtc { uncertainty_ms: 5 },
                observed_device_unix_ms: Some(1_000),
                prepared_package_id: None,
            }
        ),
        Err(CollaborationError::OriginPromotionForbidden {
            item_id: sim_track.clone(),
        })
    );

    // Simulation label is preserved across search and report export (`A13`).
    let search_env = ws.query_surface(&viewer, AccessSurface::Search)?;
    let sim_view = search_env
        .permitted_items()
        .iter()
        .find(|i| i.item_id() == &sim_track)
        .ok_or("missing simulation item in search")?;
    assert_eq!(
        sim_view.visible_simulation_label(),
        Some(VISIBLE_SIMULATION_LABEL)
    );

    // Prepare offline package with window [1_000, 2_000] and capture at 2_500 (expired by 500 ms).
    let pkg = ws.prepare_offline_package(
        &editor,
        OpaqueId::new("pkg:offline-01")?,
        1_000,
        2_000,
        None,
    )?;
    let sync_expired = ws.synchronize_offline_capture(
        &editor,
        OfflineFieldCapture {
            capture_id: OpaqueId::new("cap:edit-1")?,
            target_item_id: field_log.clone(),
            base_version: Some(VersionId::new("ver:log-v1")?),
            new_version: VersionId::new("ver:log-v2-editor")?,
            sensitivity: SensitivityFlags::standard(),
            summary_text: "Offline observation using cached sky chart (+500 ms past expiry)."
                .to_owned(),
            sensitive_location_detail: None,
            content_sha256: "d".repeat(64),
            origin: EvidenceOriginKind::ObservedOrReported,
            clock_quality: DeviceClockQuality::DriftingLocalClock {
                estimated_offset_ms: 140,
                uncertainty_ms: 60,
            },
            observed_device_unix_ms: Some(2_500),
            prepared_package_id: Some(pkg.package_id().clone()),
        },
    )?;
    assert_eq!(
        sync_expired,
        SyncOutcome::AppliedCleanly {
            item_id: field_log.clone(),
            new_head_version: VersionId::new("ver:log-v2-editor")?,
            reference_freshness: Some(OfflineReferenceFreshness::CachedExpired {
                expired_by_ms: 500
            }),
        }
    );

    // Concurrent edit against `ver:log-v1` with identical bytes deduplicates (`docs/architecture.md` §9).
    let sync_dedup = ws.synchronize_offline_capture(
        &owner,
        OfflineFieldCapture {
            capture_id: OpaqueId::new("cap:edit-dedup")?,
            target_item_id: field_log.clone(),
            base_version: Some(VersionId::new("ver:log-v1")?),
            new_version: VersionId::new("ver:log-v2-dup")?,
            sensitivity: SensitivityFlags::standard(),
            summary_text: "Offline observation using cached sky chart (+500 ms past expiry)."
                .to_owned(),
            sensitive_location_detail: None,
            content_sha256: "d".repeat(64),
            origin: EvidenceOriginKind::ObservedOrReported,
            clock_quality: DeviceClockQuality::SynchronizedUtc { uncertainty_ms: 5 },
            observed_device_unix_ms: Some(1_500),
            prepared_package_id: None,
        },
    )?;
    assert_eq!(
        sync_dedup,
        SyncOutcome::DeduplicatedExactBytes {
            item_id: field_log.clone(),
            retained_head_version: VersionId::new("ver:log-v2-editor")?,
            recorded_duplicate_version: VersionId::new("ver:log-v2-dup")?,
        }
    );

    // Concurrent edit against `ver:log-v1` with different content records an explicit conflict
    // and preserves both versions without erasing either (`A13`, `A16`).
    let conflict_id = OpaqueId::new("conflict:log-v2")?;
    let sync_conflict = ws.synchronize_offline_capture(
        &owner,
        OfflineFieldCapture {
            capture_id: conflict_id.clone(),
            target_item_id: field_log.clone(),
            base_version: Some(VersionId::new("ver:log-v1")?),
            new_version: VersionId::new("ver:log-v2-owner")?,
            sensitivity: SensitivityFlags::standard(),
            summary_text: "Concurrent owner field note with unreferenced battery-reset clock."
                .to_owned(),
            sensitive_location_detail: None,
            content_sha256: "e".repeat(64),
            origin: EvidenceOriginKind::ObservedOrReported,
            clock_quality: DeviceClockQuality::UnreferencedLocalClock {
                uncertainty_reason: "RTC battery lost sync during cold soak".to_owned(),
            },
            observed_device_unix_ms: None,
            prepared_package_id: None,
        },
    )?;
    assert_eq!(
        sync_conflict,
        SyncOutcome::ConflictPreserved {
            conflict_id: conflict_id.clone(),
            item_id: field_log.clone(),
            preserved_head_version: VersionId::new("ver:log-v2-editor")?,
            preserved_offline_version: VersionId::new("ver:log-v2-owner")?,
        }
    );

    // Resolving with wrong parent versions fails without losing the conflict.
    assert_eq!(
        ws.resolve_sync_conflict(
            &owner,
            &conflict_id,
            VersionId::new("ver:log-v3-bad")?,
            (
                VersionId::new("ver:log-v1")?,
                VersionId::new("ver:log-v2-owner")?
            ),
            "Bad merge".to_owned(),
            None,
            "f".repeat(64),
            DeviceClockQuality::SynchronizedUtc { uncertainty_ms: 5 },
            "Wrong parent".to_owned(),
        ),
        Err(CollaborationError::ConflictParentMismatch {
            conflict_id: conflict_id.clone()
        })
    );

    ws.resolve_sync_conflict(
        &owner,
        &conflict_id,
        VersionId::new("ver:log-v3-merged")?,
        (
            VersionId::new("ver:log-v2-editor")?,
            VersionId::new("ver:log-v2-owner")?,
        ),
        "Merged Site Alpha log retaining editor expired-chart note and owner RTC-reset note."
            .to_owned(),
        None,
        "f".repeat(64),
        DeviceClockQuality::SynchronizedUtc { uncertainty_ms: 5 },
        "Explicitly combined both field accounts.".to_owned(),
    )?;

    let history = ws.inspect_item_history(&owner, &field_log)?;
    // Initial + v2-editor + v2-dup + v2-owner + v3-merged = 5 preserved versions.
    assert_eq!(history.len(), 5);
    assert_eq!(
        ws.conflicts()[&conflict_id].resolution(),
        &ConflictResolutionState::Resolved {
            resolution_version: VersionId::new("ver:log-v3-merged")?,
            resolved_by: owner.clone(),
            rationale: "Explicitly combined both field accounts.".to_owned(),
        }
    );

    // Reject oversized text and invalid SHA-256 digests before mutating history.
    assert_eq!(
        ws.add_case_item(
            &owner,
            CaseItemInput {
                item_id: OpaqueId::new("item:oversized")?,
                version: VersionId::new("ver:oversized-1")?,
                sensitivity: SensitivityFlags::standard(),
                summary_text: "x".repeat(513),
                sensitive_location_detail: None,
                content_sha256: "a".repeat(64),
                origin: EvidenceOriginKind::ObservedOrReported,
                clock_quality: DeviceClockQuality::SynchronizedUtc { uncertainty_ms: 1 },
            }
        ),
        Err(CollaborationError::LimitExceeded {
            kind: CollaborationLimitKind::TextBytes,
            limit: 512,
        })
    );
    assert_eq!(
        ws.add_case_item(
            &owner,
            CaseItemInput {
                item_id: OpaqueId::new("item:bad-hash")?,
                version: VersionId::new("ver:bad-hash-1")?,
                sensitivity: SensitivityFlags::standard(),
                summary_text: "Valid text".to_owned(),
                sensitive_location_detail: None,
                content_sha256: "not-a-sha256".to_owned(),
                origin: EvidenceOriginKind::ObservedOrReported,
                clock_quality: DeviceClockQuality::SynchronizedUtc { uncertainty_ms: 1 },
            }
        ),
        Err(CollaborationError::InvalidSha256Hex)
    );

    Ok(())
}
