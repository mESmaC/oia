//! `oia-forecast` — Immutable forecast vintages, strict point-in-time cutoff admissibility
//! (corresponding to `proofs/OiaProofs/CutoffAdmissibility.lean`), scenario assumptions,
//! and independent outcome resolution (`MARKET-04`, `MARKET-05`, `FORECAST-01`–`FORECAST-04`, `A14`, `A15`).

use std::fmt;

use oia_domain::{DomainError, ObjectEnvelope, OpaqueId, VersionId};

/// Evaluation cutoff mode (`docs/architecture.md` Section 3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutoffMode {
    /// Strict prospective evaluation: requires known public availability `<= cutoff_ms`
    /// AND OIA ingestion `<= cutoff_ms`. Unknown availability is rejected.
    StrictProspective,
    /// Retrospective reconstruction: allows records with documented external availability
    /// `<= cutoff_ms` even if ingested into OIA later; still rejects unknown availability.
    RetrospectiveReconstruction,
}

/// Checks whether a candidate evidence record is admissible at `cutoff_ms` under `mode`.
///
/// Matches the formal specification in `proofs/OiaProofs/CutoffAdmissibility.lean`:
/// - If `public_availability_ms` is `None` (unknown when the information became publicly available),
///   returns `false` in both modes (`docs/architecture.md` Section 3.1: "Unknown availability
///   cannot silently qualify a record for a strict cutoff test").
/// - In `CutoffMode::StrictProspective`, requires `avail_ms <= cutoff_ms && ingested_at_ms <= cutoff_ms`.
/// - In `CutoffMode::RetrospectiveReconstruction`, requires `avail_ms <= cutoff_ms`.
#[must_use]
pub const fn is_admissible_at_cutoff(
    mode: CutoffMode,
    cutoff_ms: i64,
    public_availability_ms: Option<i64>,
    ingested_at_ms: i64,
) -> bool {
    let Some(avail_ms) = public_availability_ms else {
        return false;
    };
    match mode {
        CutoffMode::StrictProspective => avail_ms <= cutoff_ms && ingested_at_ms <= cutoff_ms,
        CutoffMode::RetrospectiveReconstruction => avail_ms <= cutoff_ms,
    }
}

/// Resolution state of a forecast outcome (`docs/architecture.md` Section 6.4).
#[derive(Debug, Clone, PartialEq)]
pub enum OutcomeResolutionState {
    /// Horizon has not yet elapsed or outcome data is not yet available.
    Unresolved,
    /// Observation was censored before the horizon completed.
    Censored {
        /// Explanation of censoring.
        reason: String,
    },
    /// Forecast preconditions were invalidated (e.g., conditional scenario trigger did not occur).
    InvalidatedPrecondition {
        /// Explanation.
        reason: String,
    },
    /// Outcome resolved against the frozen definition.
    Resolved {
        /// Resolution record version ID.
        resolution_version: VersionId,
        /// Observed outcome summary.
        observed_outcome: String,
        /// Absolute Brier or interval error vs baseline.
        absolute_error: f64,
        /// Baseline error on the same horizon.
        baseline_error: f64,
    },
}

/// Immutable forecast vintage (`Forecast`, `FORECAST-04`, `MARKET-04`, `MARKET-05`).
///
/// Revisions create a new `ForecastVintage` referencing `revised_from_vintage` so original
/// predictions and failed forecasts remain permanently auditable and scored under their original
/// horizon and outcome definition (`AGENTS.md` Section 2).
#[derive(Debug, Clone, PartialEq)]
pub struct ForecastVintage {
    /// Common object envelope (`DomainObjectType::Forecast`).
    pub envelope: ObjectEnvelope,
    /// Target identifier (instrument or event series).
    pub target_id: OpaqueId,
    /// Frozen outcome definition and resolution rule.
    pub outcome_definition: String,
    /// Forecast issuance timestamp (UTC ms).
    pub origin_utc_ms: i64,
    /// Information cutoff timestamp (UTC ms, must satisfy `cutoff_utc_ms <= origin_utc_ms`).
    pub cutoff_utc_ms: i64,
    /// Target horizon end timestamp (UTC ms, must satisfy `horizon_end_utc_ms > origin_utc_ms`).
    pub horizon_end_utc_ms: i64,
    /// Explicit baseline model name (e.g., `"persistence_random_walk"`, `"historical_base_rate"`).
    pub baseline_name: String,
    /// Explicit scenario assumptions.
    pub assumptions: Vec<String>,
    /// Pinned input evidence versions verified against `cutoff_utc_ms`.
    pub admissible_input_versions: Vec<VersionId>,
    /// Whether the probability/interval output has been empirically calibrated on held-out data.
    pub calibrated_on_held_out_data: bool,
    /// If this is a revision, pins the original forecast vintage it revises (never overwrites it).
    pub revised_from_vintage: Option<VersionId>,
    /// Independent outcome resolution state.
    pub resolution: OutcomeResolutionState,
}

impl ForecastVintage {
    /// Validates temporal ordering (`cutoff_utc_ms <= origin_utc_ms < horizon_end_utc_ms`) and envelope invariants.
    pub fn validate(&self) -> Result<(), ForecastError> {
        self.envelope.validate().map_err(ForecastError::Domain)?;
        if self.cutoff_utc_ms > self.origin_utc_ms {
            return Err(ForecastError::CutoffAfterOrigin {
                cutoff_ms: self.cutoff_utc_ms,
                origin_ms: self.origin_utc_ms,
            });
        }
        if self.horizon_end_utc_ms <= self.origin_utc_ms {
            return Err(ForecastError::HorizonNotInFuture {
                origin_ms: self.origin_utc_ms,
                horizon_ms: self.horizon_end_utc_ms,
            });
        }
        if self.outcome_definition.trim().is_empty() || self.baseline_name.trim().is_empty() {
            return Err(ForecastError::MissingOutcomeOrBaseline);
        }
        Ok(())
    }
}

/// Typed errors for `oia-forecast`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForecastError {
    /// Domain error.
    Domain(DomainError),
    /// Information cutoff exceeded forecast origin timestamp (future leakage).
    CutoffAfterOrigin {
        /// Cutoff ms.
        cutoff_ms: i64,
        /// Origin ms.
        origin_ms: i64,
    },
    /// Target horizon was not strictly after origin timestamp.
    HorizonNotInFuture {
        /// Origin ms.
        origin_ms: i64,
        /// Horizon ms.
        horizon_ms: i64,
    },
    /// Outcome definition or baseline name was empty.
    MissingOutcomeOrBaseline,
}

impl fmt::Display for ForecastError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(e) => write!(f, "forecast domain error: {e}"),
            Self::CutoffAfterOrigin {
                cutoff_ms,
                origin_ms,
            } => write!(
                f,
                "information cutoff {cutoff_ms} cannot exceed forecast origin {origin_ms}"
            ),
            Self::HorizonNotInFuture {
                origin_ms,
                horizon_ms,
            } => write!(
                f,
                "forecast horizon {horizon_ms} must be strictly after origin {origin_ms}"
            ),
            Self::MissingOutcomeOrBaseline => {
                write!(
                    f,
                    "forecast requires non-empty outcome_definition and baseline_name"
                )
            }
        }
    }
}

impl std::error::Error for ForecastError {}
