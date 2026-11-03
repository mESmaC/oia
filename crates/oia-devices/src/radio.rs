//! Receive-only radio capability and session contracts (`RADIO-01/02`, subset of A11).
//!
//! These are adapter-supplied declarations and observations, not tested hardware support.
//! This module performs no I/O, decodes no audio, and exposes no transmission command.

use std::fmt;

use oia_domain::{OpaqueId, VersionId};

/// The actual acquisition class; simulations remain explicit in every readout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    DirectoryListing,
    AudioStream,
    RemoteReceiver,
    LocalReceiver,
    SimulatedReceiver,
}

impl SourceKind {
    fn is_receiver(self) -> bool {
        matches!(
            self,
            Self::RemoteReceiver | Self::LocalReceiver | Self::SimulatedReceiver
        )
    }
}

/// Spectrum meaning is independent of display style (line, spectrogram, or waterfall).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpectrumKind {
    Audio,
    RadioFrequency,
}

/// Supported receive demodulation. No transmit mode exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modulation {
    Am,
    Fm,
    Usb,
    Lsb,
    Cw,
}

/// Validated adapter capability declaration. A listing never grants playback or RF controls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RadioSource {
    source_id: OpaqueId,
    version_id: VersionId,
    kind: SourceKind,
    tuning: Option<TuningCapabilities>,
    spectrum: Option<SpectrumKind>,
}

impl RadioSource {
    pub fn new(
        source_id: OpaqueId,
        version_id: VersionId,
        kind: SourceKind,
        tuning: Option<TuningCapabilities>,
        spectrum: Option<SpectrumKind>,
    ) -> Result<Self, RadioError> {
        if (tuning.is_some() || spectrum == Some(SpectrumKind::RadioFrequency))
            && !kind.is_receiver()
            || kind == SourceKind::DirectoryListing && spectrum.is_some()
        {
            return Err(RadioError::UnsupportedCapability);
        }
        Ok(Self {
            source_id,
            version_id,
            kind,
            tuning,
            spectrum,
        })
    }

    pub fn source_id(&self) -> &OpaqueId {
        &self.source_id
    }
    pub fn version_id(&self) -> &VersionId {
        &self.version_id
    }
    pub fn kind(&self) -> SourceKind {
        self.kind
    }
    pub fn tuning(&self) -> Option<&TuningCapabilities> {
        self.tuning.as_ref()
    }
    pub fn spectrum(&self) -> Option<SpectrumKind> {
        self.spectrum
    }
}

/// Inclusive receive bounds in integer Hz. At most the five declared modulation modes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuningCapabilities {
    min_hz: u64,
    max_hz: u64,
    max_bandwidth_hz: u32,
    modes: Vec<Modulation>,
}

impl TuningCapabilities {
    /// Inclusive center-frequency range, in Hz; not a claim of current reception.
    pub fn frequency_range_hz(&self) -> std::ops::RangeInclusive<u64> {
        self.min_hz..=self.max_hz
    }

    pub fn max_bandwidth_hz(&self) -> u32 {
        self.max_bandwidth_hz
    }

    pub fn modes(&self) -> &[Modulation] {
        &self.modes
    }

    pub fn new(
        min_hz: u64,
        max_hz: u64,
        max_bandwidth_hz: u32,
        modes: &[Modulation],
    ) -> Result<Self, RadioError> {
        if min_hz == 0
            || max_hz < min_hz
            || max_bandwidth_hz == 0
            || modes.is_empty()
            || modes.len() > 5
        {
            return Err(RadioError::InvalidTuning);
        }
        let mut unique = Vec::with_capacity(modes.len());
        for mode in modes {
            if !unique.contains(mode) {
                unique.push(*mode);
            }
        }
        Ok(Self {
            min_hz,
            max_hz,
            max_bandwidth_hz,
            modes: unique,
        })
    }

    fn validate(&self, tuning: ReceiveTuning) -> Result<(), RadioError> {
        if !(self.min_hz..=self.max_hz).contains(&tuning.frequency_hz)
            || tuning.bandwidth_hz == 0
            || tuning.bandwidth_hz > self.max_bandwidth_hz
            || !self.modes.contains(&tuning.modulation)
        {
            return Err(RadioError::InvalidTuning);
        }
        Ok(())
    }
}

/// Requested tuning does not establish the frequency of received audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReceiveTuning {
    pub frequency_hz: u64,
    pub bandwidth_hz: u32,
    pub modulation: Modulation,
}

/// Last adapter observation. Receipt time is session-monotonic milliseconds, never UTC/capture time.
/// Latency is unknown unless supplied by an adapter with a separately documented clock method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reception {
    pub received_at_ms: u64,
    pub latency_ms: Option<u64>,
    pub tuning: Option<ReceiveTuning>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Connecting,
    Receiving,
    Stale,
    Disconnected,
    Restricted,
    Cancelled,
    Expired,
}

/// Immutable projection retains source/version, simulation class, and the last observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RadioReadout {
    pub source: RadioSource,
    pub state: PlaybackState,
    pub last_reception: Option<Reception>,
    pub requested_tuning: Option<ReceiveTuning>,
}

/// Constant-memory state machine. A stopped session is terminal: reconnect with a new session
/// owned by the adapter so late completions cannot republish as current audio.
#[derive(Debug)]
pub struct RadioSession {
    source: RadioSource,
    stale_after_ms: u64,
    last_event_ms: u64,
    terminal: Option<PlaybackState>,
    last_reception: Option<Reception>,
    requested_tuning: Option<ReceiveTuning>,
    deadline_ms: Option<u64>,
}

impl RadioSession {
    /// Start an externally budgeted session. The adapter owns duration/connection deadlines.
    pub fn new(
        source: RadioSource,
        started_at_ms: u64,
        stale_after_ms: u64,
    ) -> Result<Self, RadioError> {
        if source.kind == SourceKind::DirectoryListing {
            return Err(RadioError::ListingOnly);
        }
        if stale_after_ms == 0 {
            return Err(RadioError::InvalidStaleThreshold);
        }
        Ok(Self {
            source,
            stale_after_ms,
            last_event_ms: started_at_ms,
            terminal: None,
            last_reception: None,
            requested_tuning: None,
            deadline_ms: None,
        })
    }

    /// Bound the entire session, including connection and reception, on the same monotonic
    /// clock as events. At `deadline_ms` no further data or commands can be accepted.
    /// The adapter must still cancel its transport and release resources independently.
    pub fn with_deadline(
        source: RadioSource,
        started_at_ms: u64,
        stale_after_ms: u64,
        deadline_ms: u64,
    ) -> Result<Self, RadioError> {
        if deadline_ms <= started_at_ms {
            return Err(RadioError::InvalidDeadline);
        }
        let mut session = Self::new(source, started_at_ms, stale_after_ms)?;
        session.deadline_ms = Some(deadline_ms);
        Ok(session)
    }

    pub fn deadline_ms(&self) -> Option<u64> {
        self.deadline_ms
    }

    fn check_event(&self, at_ms: u64) -> Result<(), RadioError> {
        if self.terminal.is_some() {
            return Err(RadioError::SessionEnded);
        }
        if at_ms < self.last_event_ms {
            return Err(RadioError::ClockRegression);
        }
        if self.deadline_ms.is_some_and(|deadline| at_ms >= deadline) {
            return Err(RadioError::DeadlineExpired);
        }
        Ok(())
    }

    /// Validate a receive request only; no device action is performed or acknowledged here.
    pub fn request_tuning(&mut self, at_ms: u64, tuning: ReceiveTuning) -> Result<(), RadioError> {
        self.check_event(at_ms)?;
        self.source
            .tuning
            .as_ref()
            .ok_or(RadioError::UnsupportedCapability)?
            .validate(tuning)?;
        self.requested_tuning = Some(tuning);
        self.last_event_ms = at_ms;
        Ok(())
    }

    /// Accept an adapter observation; validation errors leave all previous state intact.
    pub fn receive(&mut self, reception: Reception) -> Result<(), RadioError> {
        self.check_event(reception.received_at_ms)?;
        if let Some(tuning) = reception.tuning {
            self.source
                .tuning
                .as_ref()
                .ok_or(RadioError::UnsupportedCapability)?
                .validate(tuning)?;
        }
        self.last_event_ms = reception.received_at_ms;
        self.last_reception = Some(reception);
        Ok(())
    }

    /// Stop permanently after disconnection or access restriction. Retained data is historical.
    pub fn stop(&mut self, at_ms: u64, restricted: bool) -> Result<(), RadioError> {
        self.check_event(at_ms)?;
        self.terminal = Some(if restricted {
            PlaybackState::Restricted
        } else {
            PlaybackState::Disconnected
        });
        self.last_event_ms = at_ms;
        Ok(())
    }

    /// Permanently cancel without discarding historical observations or requested tuning.
    /// An already-expired session reports DeadlineExpired and remains Expired.
    pub fn cancel(&mut self, at_ms: u64) -> Result<(), RadioError> {
        self.check_event(at_ms)?;
        self.terminal = Some(PlaybackState::Cancelled);
        self.last_event_ms = at_ms;
        Ok(())
    }

    /// Pure query: no reconnect, tuning, network operation, or state mutation.
    /// The freshness threshold is inclusive: age >= threshold is stale.
    pub fn readout(&self, now_ms: u64) -> Result<RadioReadout, RadioError> {
        if now_ms < self.last_event_ms {
            return Err(RadioError::ClockRegression);
        }
        let state = self.terminal.unwrap_or_else(|| match self.last_reception {
            _ if self.deadline_ms.is_some_and(|deadline| now_ms >= deadline) => {
                PlaybackState::Expired
            }
            Some(reception) if now_ms - reception.received_at_ms < self.stale_after_ms => {
                PlaybackState::Receiving
            }
            Some(_) => PlaybackState::Stale,
            None => PlaybackState::Connecting,
        });
        Ok(RadioReadout {
            source: self.source.clone(),
            state,
            last_reception: self.last_reception,
            requested_tuning: self.requested_tuning,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadioError {
    UnsupportedCapability,
    InvalidTuning,
    ListingOnly,
    InvalidStaleThreshold,
    SessionEnded,
    ClockRegression,
    InvalidDeadline,
    DeadlineExpired,
}

impl fmt::Display for RadioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::UnsupportedCapability => "source does not declare this receive capability",
            Self::InvalidTuning => {
                "frequency, bandwidth or modulation is outside declared receive bounds"
            }
            Self::ListingOnly => "a directory listing cannot establish reception",
            Self::InvalidStaleThreshold => "staleness threshold must be positive milliseconds",
            Self::SessionEnded => "session ended; late data and tuning requests are rejected",
            Self::ClockRegression => "time precedes the last accepted session event",
            Self::InvalidDeadline => "session deadline must be later than its start time",
            Self::DeadlineExpired => {
                "session deadline reached; late data and commands are rejected"
            }
        })
    }
}

impl std::error::Error for RadioError {}
