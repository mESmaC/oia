//! Runnable synthetic verification of shared-case roles, sensitive-artifact isolation,
//! scoped reviews, distinct save/private-export/publish actions, and offline field
//! synchronization with explicit conflict preservation (`#38`, `A16`, `A13`).

use oia_domain::collaboration::{
    AccessSurface, CaseItemInput, CaseRole, ClearanceGrant, CollaborationError,
    CollaborationLimits, DeviceClockQuality, EvidenceOriginKind, OfflineFieldCapture,
    ReviewDisposition, ReviewScope, ScopedReviewInput, SensitivityFlags, SharedCaseWorkspace,
};
use oia_domain::{ActorId, OpaqueId, SchemaVersion, VersionId};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let owner = ActorId::new("actor:owner-lead")?;
    let editor = ActorId::new("actor:field-editor")?;
    let reviewer = ActorId::new("actor:analyst-reviewer")?;
    let viewer = ActorId::new("actor:external-viewer")?;

    let mut workspace = SharedCaseWorkspace::new(
        OpaqueId::new("case:shared-field-2026")?,
        SchemaVersion::V1,
        owner.clone(),
        ClearanceGrant::full(),
        CollaborationLimits::default(),
    );

    workspace.set_collaborator_role(
        &owner,
        editor.clone(),
        CaseRole::Editor,
        ClearanceGrant {
            allow_unpublished_drafts: true,
            allow_sensitive_artifacts: true,
            allow_sensitive_locations: false,
        },
    )?;
    workspace.set_collaborator_role(
        &owner,
        reviewer.clone(),
        CaseRole::Reviewer,
        ClearanceGrant {
            allow_unpublished_drafts: true,
            allow_sensitive_artifacts: false,
            allow_sensitive_locations: false,
        },
    )?;
    workspace.set_collaborator_role(
        &owner,
        viewer.clone(),
        CaseRole::Viewer,
        ClearanceGrant::standard_only(),
    )?;

    let public_obs = OpaqueId::new("item:ridge-observation")?;
    let sensitive_sensor = OpaqueId::new("item:private-property-sensor")?;
    let sim_fixture = OpaqueId::new("item:noise-blur-sim")?;

    workspace.add_case_item(
        &owner,
        CaseItemInput {
            item_id: public_obs.clone(),
            version: VersionId::new("ver:ridge-v1")?,
            sensitivity: SensitivityFlags {
                unpublished_draft: false,
                sensitive_artifact: false,
                sensitive_location: true,
            },
            summary_text: "Initial ridge optical observation at azimuth 042 deg.".to_owned(),
            sensitive_location_detail: Some(
                "Exact private lookout coordinates: 34.51234 N, 111.78901 W".to_owned(),
            ),
            content_sha256: "a".repeat(64),
            origin: EvidenceOriginKind::ObservedOrReported,
            clock_quality: DeviceClockQuality::SynchronizedUtc { uncertainty_ms: 15 },
        },
    )?;

    workspace.add_case_item(
        &owner,
        CaseItemInput {
            item_id: sensitive_sensor.clone(),
            version: VersionId::new("ver:sensor-v1")?,
            sensitivity: SensitivityFlags {
                unpublished_draft: false,
                sensitive_artifact: true,
                sensitive_location: true,
            },
            summary_text: "Unredacted acoustic sensor calibration log.".to_owned(),
            sensitive_location_detail: Some("Backyard sensor array pad #2".to_owned()),
            content_sha256: "b".repeat(64),
            origin: EvidenceOriginKind::ObservedOrReported,
            clock_quality: DeviceClockQuality::SynchronizedUtc { uncertainty_ms: 5 },
        },
    )?;

    workspace.add_case_item(
        &editor,
        CaseItemInput {
            item_id: sim_fixture.clone(),
            version: VersionId::new("ver:sim-v1")?,
            sensitivity: SensitivityFlags::standard(),
            summary_text: "Synthetic point-source track for angular error benchmark.".to_owned(),
            sensitive_location_detail: None,
            content_sha256: "c".repeat(64),
            origin: EvidenceOriginKind::LabeledSimulation {
                simulation_parameters: "gaussian_blur_px=2.5, clock_drift_ms=120".to_owned(),
            },
            clock_quality: DeviceClockQuality::SynchronizedUtc { uncertainty_ms: 0 },
        },
    )?;

    // Verify surface filtering for viewer across Search, ModelInput, and DirectArtifactRequest.
    let viewer_search = workspace.query_surface(&viewer, AccessSurface::Search)?;
    println!(
        "viewer search permitted={} redactions={}",
        viewer_search.permitted_items().len(),
        viewer_search.redaction_notices().len()
    );
    let denied_direct: Result<_, CollaborationError> = workspace.request_item_on_surface(
        &viewer,
        &sensitive_sensor,
        AccessSurface::DirectArtifactRequest,
    );
    println!("viewer direct request to sensitive artifact: {denied_direct:?}");

    // Prepare an offline field package and submit two concurrent offline captures with clock drift.
    let pkg = workspace.prepare_offline_package(
        &editor,
        OpaqueId::new("pkg:field-night-01")?,
        1_700_000_000_000,
        1_700_003_600_000,
        None,
    )?;
    println!(
        "offline package {} prepared: included={} omitted={}",
        pkg.package_id(),
        pkg.included_items().len(),
        pkg.omitted_notices().len()
    );

    let first_sync = workspace.synchronize_offline_capture(
        &editor,
        OfflineFieldCapture {
            capture_id: OpaqueId::new("cap:field-edit-a")?,
            target_item_id: public_obs.clone(),
            base_version: Some(VersionId::new("ver:ridge-v1")?),
            new_version: VersionId::new("ver:ridge-v2-editor")?,
            sensitivity: SensitivityFlags {
                unpublished_draft: false,
                sensitive_artifact: false,
                sensitive_location: true,
            },
            summary_text: "Updated ridge observation: duration 18 s, elevation 19 deg.".to_owned(),
            sensitive_location_detail: None,
            content_sha256: "d".repeat(64),
            origin: EvidenceOriginKind::ObservedOrReported,
            clock_quality: DeviceClockQuality::DriftingLocalClock {
                estimated_offset_ms: -420,
                uncertainty_ms: 85,
            },
            observed_device_unix_ms: Some(1_700_005_400_000),
            prepared_package_id: Some(pkg.package_id().clone()),
        },
    )?;
    println!("first offline sync outcome: {first_sync:?}");

    let conflicting_sync = workspace.synchronize_offline_capture(
        &owner,
        OfflineFieldCapture {
            capture_id: OpaqueId::new("conflict:ridge-concurrent")?,
            target_item_id: public_obs.clone(),
            base_version: Some(VersionId::new("ver:ridge-v1")?),
            new_version: VersionId::new("ver:ridge-v2-owner")?,
            sensitivity: SensitivityFlags {
                unpublished_draft: false,
                sensitive_artifact: false,
                sensitive_location: true,
            },
            summary_text: "Concurrent owner note: cloud deck obscured target after 14 s."
                .to_owned(),
            sensitive_location_detail: Some(
                "Exact private lookout coordinates: 34.51234 N, 111.78901 W".to_owned(),
            ),
            content_sha256: "e".repeat(64),
            origin: EvidenceOriginKind::ObservedOrReported,
            clock_quality: DeviceClockQuality::SynchronizedUtc { uncertainty_ms: 10 },
            observed_device_unix_ms: Some(1_700_002_000_000),
            prepared_package_id: None,
        },
    )?;
    println!("second concurrent offline sync outcome: {conflicting_sync:?}");

    workspace.resolve_sync_conflict(
        &owner,
        &OpaqueId::new("conflict:ridge-concurrent")?,
        VersionId::new("ver:ridge-v3-merged")?,
        (
            VersionId::new("ver:ridge-v2-editor")?,
            VersionId::new("ver:ridge-v2-owner")?,
        ),
        "Merged ridge account: 18 s total track, cloud deck obscured target after 14 s.".to_owned(),
        Some("Exact private lookout coordinates: 34.51234 N, 111.78901 W".to_owned()),
        "f".repeat(64),
        DeviceClockQuality::SynchronizedUtc { uncertainty_ms: 10 },
        "Preserved both timing observations in merged version ver:ridge-v3-merged.".to_owned(),
    )?;

    let review = workspace.submit_scoped_review(
        &reviewer,
        ScopedReviewInput {
            review_id: OpaqueId::new("rev:ridge-01")?,
            review_version: VersionId::new("ver:rev-ridge-01")?,
            target_item_id: public_obs.clone(),
            target_version: VersionId::new("ver:ridge-v3-merged")?,
            scope: ReviewScope::CalibrationAndGeometryOnly,
            disposition: ReviewDisposition::SupportedWithinScope,
            rationale: "Azimuth/elevation bounds consistent with tripod calibration; range remains unverified.".to_owned(),
            cited_evidence_versions: vec![VersionId::new("ver:ridge-v3-merged")?],
            supersedes_review_version: None,
        },
    )?;
    println!(
        "scoped review {} recorded (implies_universal_truth={})",
        review.review_id(),
        review.implies_universal_truth()
    );

    let selected = vec![
        public_obs.clone(),
        sensitive_sensor.clone(),
        sim_fixture.clone(),
    ];
    let private_export = workspace.export_report_privately(
        &editor,
        OpaqueId::new("rep:ridge-summary")?,
        VersionId::new("ver:rep-private-01")?,
        "Private Working Export".to_owned(),
        &selected,
    )?;
    let published = workspace.publish_report_snapshot(
        &owner,
        OpaqueId::new("rep:ridge-summary")?,
        VersionId::new("ver:rep-public-01")?,
        "Public Issued Report".to_owned(),
        &selected,
    )?;
    let flagged = workspace.flag_dependent_reports_on_correction(
        &owner,
        VersionId::new("ver:ridge-v3-merged")?,
        VersionId::new("ver:ridge-v4-corrected")?,
        "Updated tripod magnetic declination correction by +0.4 deg.".to_owned(),
    )?;
    println!(
        "private_export.is_publicly_published={} published.is_publicly_published={} flagged_reports={}",
        private_export.is_publicly_published(),
        published.is_publicly_published(),
        flagged
    );

    Ok(())
}
