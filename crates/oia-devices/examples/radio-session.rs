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
    let mut session = RadioSession::with_deadline(source.clone(), 0, 1000, 1500)?;
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
    let mut bounded = RadioSession::with_deadline(source, 0, 1000, 1500)?;
    println!("SIMULATED expired connection: {:?}", bounded.readout(1500)?);
    println!(
        "SIMULATED late observation: {:?}",
        bounded.receive(Reception {
            received_at_ms: 1500,
            latency_ms: None,
            tuning: None,
        })
    );
    // The original terminal reason survives the deadline.
    println!(
        "SIMULATED disconnected history: {:?}",
        session.readout(1500)?
    );
    Ok(())
}
