//! Critical FIELD-01 risks: overstated coverage and hidden budget loss.
use oia_devices::station::*;
use oia_domain::{OpaqueId, VersionId};

fn record(
    id: &str,
    start: i64,
    end: i64,
) -> Result<RetainedObservation, Box<dyn std::error::Error>> {
    Ok(RetainedObservation {
        artifact_version: VersionId::new(id)?,
        timing: Timing::Aligned {
            interval: Window::new(start, end)?,
            alignment_version: VersionId::new("clock:v1")?,
            uncertainty_ms: 0,
        },
        expires_at_utc_ms: 100,
        origin: Origin::Observation,
    })
}

#[test]
fn coverage_retains_gaps_uncertainty_expiry_and_simulation()
-> Result<(), Box<dyn std::error::Error>> {
    // Manufactured contract inputs, not hardware measurements.
    let mut expired = record("artifact:expired", 0, 30)?;
    expired.expires_at_utc_ms = 40;
    let mut unknown = record("artifact:unknown", 0, 30)?;
    unknown.timing = Timing::Unknown;
    let mut simulation = record("artifact:simulation", 0, 30)?;
    simulation.origin = Origin::Simulation;
    let mut uncertain = record("artifact:uncertain", 0, 30)?;
    uncertain.timing = Timing::Aligned {
        interval: Window::new(0, 30)?,
        alignment_version: VersionId::new("clock:v2")?,
        uncertainty_ms: 1,
    };
    let inventory = vec![DeviceInventory {
        device_id: OpaqueId::new("device:a")?,
        source_version: VersionId::new("source:v1")?,
        health: Health::Disconnected,
        observations: vec![
            record("artifact:b", 10, 20)?,
            record("artifact:a", 5, 12)?,
            expired,
            unknown,
            simulation,
            uncertain,
            record("artifact:outside", 30, 35)?,
        ],
    }];
    let original = inventory.clone();
    let manifest = plan_capture(
        VersionId::new("event:v1")?,
        Window::new(0, 30)?,
        40,
        &inventory,
        CaptureLimits::default(),
    )?;
    assert_eq!(inventory, original);
    let device = manifest.devices().first().ok_or("missing device")?;
    assert_eq!(device.health(), Health::Disconnected);
    assert_eq!(device.recorded_coverage(), &[Window::new(5, 20)?]);
    assert_eq!(device.gaps(), &[Window::new(0, 5)?, Window::new(20, 30)?]);
    for (id, expected) in [
        ("artifact:expired", Disposition::Expired),
        ("artifact:unknown", Disposition::UncertainTiming),
        ("artifact:uncertain", Disposition::UncertainTiming),
        ("artifact:outside", Disposition::OutsideWindow),
    ] {
        let item = device
            .observations()
            .iter()
            .find(|r| r.observation().artifact_version.as_str() == id)
            .ok_or("missing record")?;
        assert_eq!(item.disposition(), expected);
    }
    let mut reversed = inventory.clone();
    for device in &mut reversed {
        device.observations.reverse();
    }
    assert_eq!(
        manifest,
        plan_capture(
            VersionId::new("event:v1")?,
            Window::new(0, 30)?,
            40,
            &reversed,
            CaptureLimits::default()
        )?
    );
    Ok(())
}

#[test]
fn bounds_and_conflicting_identity_fail_without_partial_output()
-> Result<(), Box<dyn std::error::Error>> {
    let mut inventory = vec![DeviceInventory {
        device_id: OpaqueId::new("device:a")?,
        source_version: VersionId::new("source:v1")?,
        health: Health::Unknown,
        observations: vec![record("artifact:a", 0, 10)?, record("artifact:b", 10, 20)?],
    }];
    let event = VersionId::new("event:v1")?;
    let window = Window::new(0, 30)?;
    let plan =
        |input: &[DeviceInventory], limits| plan_capture(event.clone(), window, 40, input, limits);
    assert_eq!(
        plan(&inventory, CaptureLimits::new(1, 1)?),
        Err(CaptureError::ObservationLimit)
    );
    inventory.push(inventory.first().ok_or("missing inventory")?.clone());
    assert_eq!(
        plan(&inventory, CaptureLimits::default()),
        Err(CaptureError::DuplicateDevice)
    );
    inventory.pop();
    let first = inventory.first_mut().ok_or("missing inventory")?;
    first.observations = vec![record("artifact:a", 0, 10)?, record("artifact:a", 10, 20)?];
    assert_eq!(
        plan(&inventory, CaptureLimits::default()),
        Err(CaptureError::DuplicateArtifact)
    );
    let first = inventory.first_mut().ok_or("missing inventory")?;
    first.observations = vec![record("artifact:overflow", i64::MIN, 0)?];
    let item = first
        .observations
        .first_mut()
        .ok_or("missing observation")?;
    item.timing = Timing::Aligned {
        interval: Window::new(i64::MIN, 0)?,
        alignment_version: VersionId::new("clock:v1")?,
        uncertainty_ms: 1,
    };
    assert_eq!(
        plan(&inventory, CaptureLimits::default()),
        Err(CaptureError::ClockRange)
    );
    assert_eq!(Window::new(1, 1), Err(CaptureError::InvalidWindow));
    assert_eq!(CaptureLimits::new(257, 1), Err(CaptureError::InvalidLimits));
    Ok(())
}
