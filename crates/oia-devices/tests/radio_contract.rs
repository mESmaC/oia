//! Focused A11 assertions: capability integrity and stale/late observation handling.
use oia_devices::radio::*;
use oia_domain::{OpaqueId, VersionId};

fn source(
    kind: SourceKind,
    tuning: Option<TuningCapabilities>,
    spectrum: Option<SpectrumKind>,
) -> Result<RadioSource, Box<dyn std::error::Error>> {
    Ok(RadioSource::new(
        OpaqueId::new("radio:fixture")?,
        VersionId::new("radio:v1")?,
        kind,
        tuning,
        spectrum,
    )?)
}

#[test]
fn listings_and_audio_cannot_claim_receiver_capabilities() -> Result<(), Box<dyn std::error::Error>>
{
    assert!(
        source(
            SourceKind::AudioStream,
            None,
            Some(SpectrumKind::RadioFrequency)
        )
        .is_err()
    );
    let listing = source(SourceKind::DirectoryListing, None, None)?;
    assert!(matches!(
        RadioSession::new(listing, 0, 100),
        Err(RadioError::ListingOnly)
    ));
    let audio = source(SourceKind::AudioStream, None, Some(SpectrumKind::Audio))?;
    let mut session = RadioSession::new(audio, 0, 100)?;
    assert_eq!(
        session.request_tuning(
            0,
            ReceiveTuning {
                frequency_hz: 1000,
                bandwidth_hz: 10,
                modulation: Modulation::Am
            }
        ),
        Err(RadioError::UnsupportedCapability)
    );
    assert_eq!(session.readout(0)?.state, PlaybackState::Connecting);
    assert_eq!(session.readout(0)?.last_reception, None);
    Ok(())
}

#[test]
fn tuning_is_not_an_observation_and_invalid_updates_are_atomic()
-> Result<(), Box<dyn std::error::Error>> {
    let capabilities = TuningCapabilities::new(1000, 2000, 100, &[Modulation::Am])?;
    let receiver = source(
        SourceKind::SimulatedReceiver,
        Some(capabilities),
        Some(SpectrumKind::RadioFrequency),
    )?;
    let mut session = RadioSession::new(receiver, 0, 100)?;
    let tuning = ReceiveTuning {
        frequency_hz: 1500,
        bandwidth_hz: 50,
        modulation: Modulation::Am,
    };
    session.request_tuning(10, tuning)?;
    let before = session.readout(10)?;
    assert_eq!(before.last_reception, None);
    assert_eq!(before.source.kind(), SourceKind::SimulatedReceiver);
    assert_eq!(
        session.receive(Reception {
            received_at_ms: 20,
            latency_ms: None,
            tuning: Some(ReceiveTuning {
                frequency_hz: 3000,
                ..tuning
            })
        }),
        Err(RadioError::InvalidTuning)
    );
    assert_eq!(session.readout(10)?, before);
    session.receive(Reception {
        received_at_ms: 20,
        latency_ms: None,
        tuning: Some(tuning),
    })?;
    assert_eq!(
        session
            .readout(20)?
            .last_reception
            .and_then(|r| r.latency_ms),
        None
    );
    assert_eq!(
        session.receive(Reception {
            received_at_ms: 19,
            latency_ms: None,
            tuning: None
        }),
        Err(RadioError::ClockRegression)
    );
    Ok(())
}

#[test]
fn freshness_and_terminal_sessions_do_not_promote_old_data()
-> Result<(), Box<dyn std::error::Error>> {
    let mut session = RadioSession::new(source(SourceKind::AudioStream, None, None)?, 0, 100)?;
    let reception = Reception {
        received_at_ms: 10,
        latency_ms: Some(500),
        tuning: None,
    };
    session.receive(reception)?;
    assert_eq!(session.readout(109)?.state, PlaybackState::Receiving);
    assert_eq!(session.readout(110)?.state, PlaybackState::Stale);
    session.stop(120, true)?;
    assert_eq!(session.readout(120)?.state, PlaybackState::Restricted);
    assert_eq!(
        session.receive(Reception {
            received_at_ms: 130,
            ..reception
        }),
        Err(RadioError::SessionEnded)
    );
    assert_eq!(session.readout(130)?.last_reception, Some(reception));
    assert_eq!(session.readout(119), Err(RadioError::ClockRegression));
    let mut disconnected = RadioSession::new(source(SourceKind::AudioStream, None, None)?, 0, 100)?;
    disconnected.stop(0, false)?;
    assert_eq!(disconnected.readout(0)?.state, PlaybackState::Disconnected);
    Ok(())
}
