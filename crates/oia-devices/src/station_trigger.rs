//! Constant-memory sensor threshold decisions for FIELD-01. No device or storage effects.
use crate::station::{Origin, Window};
use oia_domain::{OpaqueId, VersionId};
use std::{error::Error, fmt};

/// One rule instance belongs to one device, signal/unit definition and origin class.
/// Changing any setting requires a new rule version and a fresh evaluator.
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub version: VersionId,
    pub device: OpaqueId,
    /// Versioned signal definition, including physical unit and normalization method.
    pub signal_version: VersionId,
    pub origin: Origin,
    pub threshold: f64,
    pub release_threshold: f64,
    pub consecutive_samples: u32,
    pub max_gap_ms: u32,
    pub pre_ms: u32,
    pub post_ms: u32,
}

/// Time is caller-aligned UTC epoch milliseconds, never substituted receipt time.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub device: OpaqueId,
    pub signal_version: VersionId,
    pub origin: Origin,
    pub version: VersionId,
    pub at_utc_ms: i64,
    pub uncertainty_ms: u32,
    /// None is missing, not zero. Invalid numeric values are rejected without changing state.
    pub value: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Trigger {
    pub rule: Rule,
    /// First qualifying and final confirming sample versions; intervening samples are not retained.
    pub first_sample: VersionId,
    pub confirming_sample: Sample,
    pub first_at_utc_ms: i64,
    /// Requested context only; feed this to station::plan_capture for actual inventory coverage.
    pub requested: Window,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Missing,
    BelowThreshold,
    Pending {
        samples: u32,
    },
    /// Already fired; a sample at or below release_threshold must rearm the rule.
    Latched,
    Fired(Box<Trigger>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Evaluation {
    /// A gap greater than max_gap_ms reset pending persistence before this decision.
    pub gap_before: bool,
    pub decision: Decision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerError {
    InvalidRule,
    NonFiniteValue,
    OutOfOrder,
    WrongStream,
    ClockRange,
}
impl fmt::Display for TriggerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidRule => "rule requires finite release < threshold, 1..=65536 samples, positive gap and post-window",
            Self::NonFiniteValue => "sample value must be finite or explicitly missing",
            Self::OutOfOrder => "sample times must strictly increase; replay requires a fresh evaluator",
            Self::WrongStream => "sample device, signal version and origin must match the rule",
            Self::ClockRange => "sample uncertainty or requested context exceeds i64 UTC range",
        })
    }
}
impl Error for TriggerError {}

#[derive(Debug, Clone)]
struct Pending {
    first: VersionId,
    at: i64,
    earliest: i64,
    latest: i64,
    count: u32,
}

#[derive(Debug, Clone)]
pub struct Evaluator {
    rule: Rule,
    last_at: Option<i64>,
    pending: Option<Pending>,
    latched: bool,
}
impl Evaluator {
    pub fn new(rule: Rule) -> Result<Self, TriggerError> {
        if !rule.threshold.is_finite()
            || !rule.release_threshold.is_finite()
            || rule.release_threshold >= rule.threshold
            || !(1..=65_536).contains(&rule.consecutive_samples)
            || rule.max_gap_ms == 0
            || rule.post_ms == 0
        {
            return Err(TriggerError::InvalidRule);
        }
        Ok(Self {
            rule,
            last_at: None,
            pending: None,
            latched: false,
        })
    }

    /// The caller routes only this rule's device/signal/origin stream here. Samples carry
    /// immutable version references; source storage owns their uniqueness and retained bytes.
    /// Errors leave the evaluator unchanged. Missing samples and gaps break pending runs,
    /// but never rearm an already-fired rule or imply observed absence.
    pub fn push(&mut self, sample: Sample) -> Result<Evaluation, TriggerError> {
        if sample.device != self.rule.device
            || sample.signal_version != self.rule.signal_version
            || sample.origin != self.rule.origin
        {
            return Err(TriggerError::WrongStream);
        }
        if sample.value.is_some_and(|v| !v.is_finite()) {
            return Err(TriggerError::NonFiniteValue);
        }
        if self.last_at.is_some_and(|last| sample.at_utc_ms <= last) {
            return Err(TriggerError::OutOfOrder);
        }
        let earliest = sample
            .at_utc_ms
            .checked_sub(i64::from(sample.uncertainty_ms))
            .ok_or(TriggerError::ClockRange)?;
        let latest = sample
            .at_utc_ms
            .checked_add(i64::from(sample.uncertainty_ms))
            .ok_or(TriggerError::ClockRange)?;
        let gap_before = self.last_at.is_some_and(|last| {
            i128::from(sample.at_utc_ms) - i128::from(last) > i128::from(self.rule.max_gap_ms)
        });
        // Work on a bounded snapshot so even a late context-overflow error is transactional.
        let mut next = self.clone();
        next.last_at = Some(sample.at_utc_ms);
        if gap_before {
            next.pending = None;
        }
        let decision = match sample.value {
            None => {
                next.pending = None;
                Decision::Missing
            }
            Some(value) if value <= next.rule.release_threshold => {
                next.pending = None;
                next.latched = false;
                Decision::BelowThreshold
            }
            Some(_) if next.latched => Decision::Latched,
            Some(value) if value < next.rule.threshold => {
                next.pending = None;
                Decision::BelowThreshold
            }
            Some(_) => {
                let pending = next.pending.get_or_insert_with(|| Pending {
                    first: sample.version.clone(),
                    at: sample.at_utc_ms,
                    earliest,
                    latest,
                    count: 0,
                });
                pending.count += 1;
                pending.earliest = pending.earliest.min(earliest);
                pending.latest = pending.latest.max(latest);
                if pending.count < next.rule.consecutive_samples {
                    Decision::Pending {
                        samples: pending.count,
                    }
                } else {
                    let start = pending
                        .earliest
                        .checked_sub(i64::from(next.rule.pre_ms))
                        .ok_or(TriggerError::ClockRange)?;
                    let end = pending
                        .latest
                        .checked_add(i64::from(next.rule.post_ms))
                        .ok_or(TriggerError::ClockRange)?;
                    let requested =
                        Window::new(start, end).map_err(|_| TriggerError::ClockRange)?;
                    let trigger = Trigger {
                        rule: next.rule.clone(),
                        first_sample: pending.first.clone(),
                        first_at_utc_ms: pending.at,
                        confirming_sample: sample,
                        requested,
                    };
                    next.pending = None;
                    next.latched = true;
                    Decision::Fired(Box::new(trigger))
                }
            }
        };
        *self = next;
        Ok(Evaluation {
            gap_before,
            decision,
        })
    }
}
