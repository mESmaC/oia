//! Manufactured temperature samples; no device access or real measurements.
use oia_devices::{station::Origin, station_trigger::*};
use oia_domain::{OpaqueId, VersionId};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rule = Rule {
        version: VersionId::new("threshold:v1")?,
        device: OpaqueId::new("simulated:sensor")?,
        signal_version: VersionId::new("simulated:temperature-celsius:v1")?,
        origin: Origin::Simulation,
        threshold: 30.0,
        release_threshold: 28.0,
        consecutive_samples: 2,
        max_gap_ms: 1000,
        pre_ms: 500,
        post_ms: 500,
    };
    let mut evaluator = Evaluator::new(rule.clone())?;
    for (at, value) in [
        (1000, Some(31.0)),
        (1500, None),
        (2000, Some(31.0)),
        (2500, Some(32.0)),
    ] {
        let result = evaluator.push(Sample {
            device: rule.device.clone(),
            signal_version: rule.signal_version.clone(),
            origin: rule.origin,
            version: VersionId::new(format!("simulated:sample:{at}"))?,
            at_utc_ms: at,
            uncertainty_ms: 10,
            value,
        })?;
        println!("SIMULATION {at}: {result:?}");
    }
    Ok(())
}
