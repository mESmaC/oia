//! Runnable synthetic demonstration of the proposed role × surface × clearance access
//! decision function and the offline field conflict-synchronization kernel (`#38`, `A16`, `A13`).
//!
//! Note: [`evaluate_proposed_access_decision`] is a proposed default policy pending `D10`;
//! application services (`#14`), `oia-export` (`#22`), and workspace UI (`#16`) do not
//! call it yet.

use oia_domain::collaboration::{
    AccessDecision, AccessSurface, CaseRole, ClearanceGrant, DeviceClockQuality,
    InitialSyncItemInput, OfflineFieldCapture, OfflineSyncItem, SensitivityFlags, SyncLimits,
    TimestampWithClock, evaluate_proposed_access_decision,
};
use oia_domain::witness::AccountOrigin;
use oia_domain::{ActorId, OpaqueId, SchemaVersion, VersionId};

fn utc_ts(recorded_unix_ms: i64, uncertainty_ms: u64) -> TimestampWithClock {
    TimestampWithClock {
        recorded_unix_ms,
        clock_quality: DeviceClockQuality::SynchronizedUtc { uncertainty_ms },
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let owner = ActorId::new("actor:owner-lead")?;
    let editor = ActorId::new("actor:field-editor")?;

    let sensitivity = SensitivityFlags {
        unpublished_draft: false,
        sensitive_artifact: false,
        sensitive_location: true,
    };

    for (role, clearance, surface) in [
        (
            CaseRole::Owner,
            ClearanceGrant::full(),
            AccessSurface::CollaboratorShare,
        ),
        (
            CaseRole::Editor,
            ClearanceGrant::full(),
            AccessSurface::CollaboratorShare,
        ),
        (
            CaseRole::Viewer,
            ClearanceGrant::standard_only(),
            AccessSurface::Search,
        ),
        (
            CaseRole::Viewer,
            ClearanceGrant::full(),
            AccessSurface::ModelInput,
        ),
        (
            CaseRole::Owner,
            ClearanceGrant::full(),
            AccessSurface::PublicPublication,
        ),
    ] {
        let decision = evaluate_proposed_access_decision(role, clearance, surface, sensitivity);
        println!("proposed decision ({role}, {surface}) -> {decision:?}");
    }

    let mut sync_item = OfflineSyncItem::new(
        CaseRole::Owner,
        ClearanceGrant::full(),
        InitialSyncItemInput {
            item_id: OpaqueId::new("item:ridge-observation")?,
            schema_version: SchemaVersion::V1,
            initial_version: VersionId::new("ver:ridge-v1")?,
            author: owner.clone(),
            origin: AccountOrigin::Reported,
            simulation_parameters: None,
            sensitivity,
            summary_text: "Initial ridge optical observation at azimuth 042 deg.".to_owned(),
            sensitive_location_detail: Some(
                "Private lookout coordinates: 34.51234 N, 111.78901 W".to_owned(),
            ),
            declared_content_sha256: "a".repeat(64),
            capture_time: utc_ts(1_700_000_000_000, 15),
            receipt_time: utc_ts(1_700_000_000_100, 5),
            sync_time: utc_ts(1_700_000_000_200, 5),
        },
        SyncLimits::default(),
    )?;

    let pkg = sync_item.prepare_offline_window(
        CaseRole::Editor,
        ClearanceGrant::full(),
        editor.clone(),
        OpaqueId::new("pkg:field-night-01")?,
        1_700_000_000_000,
        1_700_003_600_000,
    )?;

    let first_sync = sync_item.synchronize_offline_capture(
        CaseRole::Editor,
        ClearanceGrant::full(),
        OfflineFieldCapture {
            capture_id: OpaqueId::new("cap:field-edit-a")?,
            expected_base_version: VersionId::new("ver:ridge-v1")?,
            new_version: VersionId::new("ver:ridge-v2-editor")?,
            author: editor,
            origin: AccountOrigin::Reported,
            simulation_parameters: None,
            sensitivity,
            summary_text: "Updated ridge observation: duration 18 s, elevation 19 deg.".to_owned(),
            sensitive_location_detail: Some(
                "Private lookout coordinates: 34.51234 N, 111.78901 W".to_owned(),
            ),
            declared_content_sha256: "b".repeat(64),
            capture_time: TimestampWithClock {
                recorded_unix_ms: 1_700_003_599_000,
                clock_quality: DeviceClockQuality::DriftingLocalClock {
                    estimated_offset_ms: 600_000,
                    uncertainty_ms: 300_000,
                },
            },
            receipt_time: utc_ts(1_700_006_000_000, 10),
            sync_time: utc_ts(1_700_006_000_100, 10),
            prepared_package_id: Some(pkg.package_id().clone()),
        },
    )?;
    println!("first offline sync outcome: {first_sync:?}");

    let conflict_sync = sync_item.synchronize_offline_capture(
        CaseRole::Owner,
        ClearanceGrant::full(),
        OfflineFieldCapture {
            capture_id: OpaqueId::new("conflict:ridge-concurrent")?,
            expected_base_version: VersionId::new("ver:ridge-v1")?,
            new_version: VersionId::new("ver:ridge-v2-owner")?,
            author: owner.clone(),
            origin: AccountOrigin::Reported,
            simulation_parameters: None,
            sensitivity,
            summary_text: "Concurrent owner note: cloud deck obscured target after 14 s."
                .to_owned(),
            sensitive_location_detail: Some(
                "Private lookout coordinates: 34.51234 N, 111.78901 W".to_owned(),
            ),
            declared_content_sha256: "c".repeat(64),
            capture_time: utc_ts(1_700_002_000_000, 10),
            receipt_time: utc_ts(1_700_006_000_200, 10),
            sync_time: utc_ts(1_700_006_000_300, 10),
            prepared_package_id: None,
        },
    )?;
    println!("second concurrent offline sync outcome: {conflict_sync:?}");

    sync_item.resolve_sync_conflict(
        CaseRole::Owner,
        ClearanceGrant::full(),
        owner,
        &OpaqueId::new("conflict:ridge-concurrent")?,
        VersionId::new("ver:ridge-v3-merged")?,
        (
            VersionId::new("ver:ridge-v2-editor")?,
            VersionId::new("ver:ridge-v2-owner")?,
        ),
        "Merged ridge account: 18 s total track, cloud deck obscured target after 14 s.".to_owned(),
        Some("Private lookout coordinates: 34.51234 N, 111.78901 W".to_owned()),
        "d".repeat(64),
        None,
        utc_ts(1_700_006_000_500, 10),
        utc_ts(1_700_006_000_500, 10),
        utc_ts(1_700_006_000_500, 10),
        "Preserved both timing observations in merged version ver:ridge-v3-merged.".to_owned(),
    )?;

    let viewer_history = sync_item.inspect_history_on_surface(
        CaseRole::Viewer,
        ClearanceGrant::standard_only(),
        AccessSurface::Search,
    )?;
    let all_stripped = viewer_history
        .iter()
        .all(|v| v.sensitive_location_detail().is_none() && v.location_stripped());
    println!(
        "viewer history versions={} all_locations_stripped={all_stripped} latest_decision={:?}",
        viewer_history.len(),
        AccessDecision::AllowWithStrippedLocation
    );

    Ok(())
}
