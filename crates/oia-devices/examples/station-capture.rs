//! Synthetic station inventory; no hardware or acquired observation data.
use oia_devices::station::*;
use oia_domain::{OpaqueId, VersionId};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let inventory = vec![DeviceInventory {
        device_id: OpaqueId::new("device:synthetic-camera")?,
        source_version: VersionId::new("source:synthetic-v1")?,
        health: Health::Disconnected,
        observations: vec![RetainedObservation {
            artifact_version: VersionId::new("artifact:synthetic-clip-v1")?,
            timing: Timing::Aligned {
                interval: Window::new(1000, 2000)?,
                alignment_version: VersionId::new("clock:synthetic-v1")?,
                uncertainty_ms: 0,
            },
            expires_at_utc_ms: 10000,
            origin: Origin::Simulation,
        }],
    }];
    let manifest = plan_capture(
        VersionId::new("event:synthetic-v1")?,
        Window::new(0, 3000)?,
        4000,
        &inventory,
        CaptureLimits::default(),
    )?;
    println!("SYNTHETIC FIXTURE — plan only; no bytes retained or published");
    for device in manifest.devices() {
        println!(
            "{}: {:?}; recorded coverage {:?}; gaps {:?}",
            device.device_id().as_str(),
            device.health(),
            device.recorded_coverage(),
            device.gaps()
        );
        for observation in device.observations() {
            println!(
                "  {} {:?} {:?}",
                observation.observation().artifact_version.as_str(),
                observation.observation().origin,
                observation.disposition()
            );
        }
    }
    Ok(())
}
