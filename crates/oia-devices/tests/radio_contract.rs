//! Focused A11 assertions: capability integrity and stale/late observation handling.
use oia_devices::radio::*;
use oia_domain::{OpaqueId, VersionId};

#[test]
fn deadline_bounds_connection_and_reception_without_erasing_history()
-> Result<(), Box<dyn std::error::Error>> {
    let audio = source(SourceKind::AudioStream, None, None)?;
    for deadline in [0, 10] {
        assert!(matches!(
            RadioSession::with_deadline(audio.clone(), 10, 100, deadline),
            Err(RadioError::InvalidDeadline)
        ));
    }
    let mut session = RadioSession::with_deadline(audio.clone(), 10, 100, 50)?;
    assert_eq!(session.deadline_ms(), Some(50));
    assert_eq!(session.readout(49)?.state, PlaybackState::Connecting);
    assert_eq!(session.readout(50)?.state, PlaybackState::Expired);
    let late = Reception {
        received_at_ms: 50,
        latency_ms: None,
        tuning: None,
    };
    assert_eq!(session.receive(late), Err(RadioError::DeadlineExpired));
    assert_eq!(session.readout(50)?.last_reception, None);

    let mut receiving = RadioSession::with_deadline(audio, 10, 100, 50)?;
    let reception = Reception {
        received_at_ms: 49,
        ..late
    };
    receiving.receive(reception)?;
    assert_eq!(receiving.readout(49)?.state, PlaybackState::Receiving);
    let before = receiving.readout(50)?;
    assert_eq!(before.state, PlaybackState::Expired);
    assert_eq!(receiving.receive(late), Err(RadioError::DeadlineExpired));
    assert_eq!(receiving.cancel(50), Err(RadioError::DeadlineExpired));
    assert_eq!(receiving.stop(50, false), Err(RadioError::DeadlineExpired));
    assert_eq!(receiving.readout(50)?, before);
    assert_eq!(before.last_reception, Some(reception));
    assert_eq!(before.source.version_id().as_str(), "radio:v1");
    assert_eq!(receiving.readout(u64::MAX)?.state, PlaybackState::Expired);
    Ok(())
}

#[test]
fn cancellation_rejects_late_observations_and_tuning() -> Result<(), Box<dyn std::error::Error>> {
    let receiver = source(
        SourceKind::SimulatedReceiver,
        Some(TuningCapabilities::new(1000, 2000, 100, &[Modulation::Am])?),
        None,
    )?;
    let mut session = RadioSession::with_deadline(receiver, 0, 100, 1000)?;
    let tuning = ReceiveTuning {
        frequency_hz: 1500,
        bandwidth_hz: 50,
        modulation: Modulation::Am,
    };
    session.request_tuning(1, tuning)?;
    let reception = Reception {
        received_at_ms: 2,
        latency_ms: None,
        tuning: Some(tuning),
    };
    session.receive(reception)?;
    let before = session.readout(2)?;
    assert_eq!(session.cancel(1), Err(RadioError::ClockRegression));
    assert_eq!(session.readout(2)?, before);
    session.cancel(3)?;
    assert_eq!(
        session.request_tuning(4, tuning),
        Err(RadioError::SessionEnded)
    );
    assert_eq!(
        session.receive(Reception {
            received_at_ms: 4,
            ..reception
        }),
        Err(RadioError::SessionEnded)
    );
    assert_eq!(session.cancel(4), Err(RadioError::SessionEnded));
    let cancelled = session.readout(1000)?;
    assert_eq!(cancelled.state, PlaybackState::Cancelled);
    assert_eq!(cancelled.last_reception, Some(reception));
    assert_eq!(cancelled.requested_tuning, Some(tuning));
    assert_eq!(cancelled.source.kind(), SourceKind::SimulatedReceiver);

    let mut connecting = RadioSession::new(source(SourceKind::AudioStream, None, None)?, 0, 100)?;
    assert_eq!(connecting.deadline_ms(), None);
    connecting.cancel(0)?;
    assert_eq!(connecting.readout(0)?.last_reception, None);
    assert_eq!(connecting.readout(0)?.state, PlaybackState::Cancelled);
    Ok(())
}

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
