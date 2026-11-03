//! Deterministic simulated observations, with no network, audio decoding or hardware access.
use oia_devices::radio::{RadioSession, RadioSource, Reception, SourceKind, SpectrumKind};
use oia_domain::{OpaqueId, VersionId};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = RadioSource::new(
        OpaqueId::new("radio:simulated")?,
        VersionId::new("radio:simulated-v1")?,
        SourceKind::SimulatedReceiver,
        None,
        Some(SpectrumKind::Audio),
    )?;
    let mut session = RadioSession::new(source, 0, 1000)?;
    println!("SIMULATED: {:?}", session.readout(0)?);
    session.receive(Reception {
        received_at_ms: 100,
        latency_ms: None,
        tuning: None,
    })?;
    println!("SIMULATED: {:?}", session.readout(100)?);
    println!("SIMULATED: {:?}", session.readout(1100)?);
    session.stop(1200, false)?;
    println!("SIMULATED: {:?}", session.readout(1200)?);
    Ok(())
}
