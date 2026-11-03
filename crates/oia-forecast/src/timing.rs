//! Point-event timing errors with version-pinned inputs and explicit coverage.
//!
//! Times are normalized Unix milliseconds supplied by the caller. This kernel
//! performs no calendar/time-scale conversion, issuance, or outcome resolution.

use std::collections::BTreeSet;

use oia_domain::VersionId;

use crate::scoring::{MAX_EVALUATION_ROWS, ScoringError, UnresolvedReason};

/// An exact event time or an explicit reason timing cannot be scored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimingOutcome {
    Occurred {
        unix_ms: i64,
        version: VersionId,
    },
    /// The defined event did not occur in its outcome window. This is a known
    /// outcome, but has no event time; it must not receive a zero timing error.
    DidNotOccur {
        version: VersionId,
    },
    Unresolved(UnresolvedReason),
    /// The outcome/forecast was invalidated; retain its decision version.
    Invalidated {
        version: VersionId,
    },
}

/// A supplied forecast vintage and matched baseline for the same event/window.
///
/// Callers establish target alignment, source precision, cutoff, authorization,
/// and version existence upstream. Approximate/date-only times must not be
/// promoted to exact milliseconds merely to use this evaluator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimingForecastRow {
    pub forecast_version: VersionId,
    pub predicted_unix_ms: i64,
    pub baseline_version: VersionId,
    pub baseline_unix_ms: i64,
    pub outcome: TimingOutcome,
}

/// Exact millisecond errors: predicted minus observed (negative means early).
/// Widening before subtraction preserves the full i64 timestamp domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimingError {
    pub signed_ms: i128,
    pub absolute_ms: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimingErrors {
    pub forecast: TimingError,
    pub baseline: TimingError,
}

/// Descriptive equal-vintage means over occurred events only. Revisions may be
/// dependent; these statistics do not establish predictive skill.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeanTimingError {
    pub signed_ms: f64,
    pub absolute_ms: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeanTimingErrors {
    pub forecast: MeanTimingError,
    pub baseline: MeanTimingError,
}

#[derive(Debug, PartialEq)]
pub struct ScoredTimingRow<'a> {
    pub input: &'a TimingForecastRow,
    pub errors: Option<TimingErrors>,
}

/// Every input remains in supplied order, including losing originals and rows
/// without a score. Non-occurrence is counted separately from unknown outcomes.
#[derive(Debug, PartialEq)]
pub struct TimingEvaluation<'a> {
    pub rows: Vec<ScoredTimingRow<'a>>,
    pub occurred_count: usize,
    pub did_not_occur_count: usize,
    pub unresolved_count: usize,
    pub invalidated_count: usize,
    /// Absent for empty input or when no event has an exact resolved time.
    pub means: Option<MeanTimingErrors>,
}

/// Score the caller's cohort without selecting vintages or imputing event times.
///
/// Limits include all rows and are checked before allocation. Invalid budgets,
/// excess rows, and duplicate forecast versions reject the entire request.
/// Exact integer sums are safe under MAX_EVALUATION_ROWS and the i64 input
/// domain; means convert once to f64 and may round. Runtime is O(n log n), with
/// O(n) auxiliary memory. Corrected outcomes require a new evaluation.
pub fn evaluate_timing(
    rows: &[TimingForecastRow],
    row_limit: usize,
) -> Result<TimingEvaluation<'_>, ScoringError> {
    if !(1..=MAX_EVALUATION_ROWS).contains(&row_limit) {
        return Err(ScoringError::InvalidRowLimit);
    }
    if rows.len() > row_limit {
        return Err(ScoringError::RowLimitExceeded {
            supplied: rows.len(),
            limit: row_limit,
        });
    }
    let mut versions = BTreeSet::new();
    let mut result = TimingEvaluation {
        rows: Vec::with_capacity(rows.len()),
        occurred_count: 0,
        did_not_occur_count: 0,
        unresolved_count: 0,
        invalidated_count: 0,
        means: None,
    };
    let mut forecast_signed = 0_i128;
    let mut forecast_absolute = 0_u128;
    let mut baseline_signed = 0_i128;
    let mut baseline_absolute = 0_u128;
    for row in rows {
        if !versions.insert(&row.forecast_version) {
            return Err(ScoringError::DuplicateForecastVersion(
                row.forecast_version.clone(),
            ));
        }
        let errors = match row.outcome {
            TimingOutcome::Occurred { unix_ms, .. } => {
                let errors = TimingErrors {
                    forecast: timing_error(row.predicted_unix_ms, unix_ms),
                    baseline: timing_error(row.baseline_unix_ms, unix_ms),
                };
                forecast_signed += errors.forecast.signed_ms;
                forecast_absolute += errors.forecast.absolute_ms;
                baseline_signed += errors.baseline.signed_ms;
                baseline_absolute += errors.baseline.absolute_ms;
                result.occurred_count += 1;
                Some(errors)
            }
            TimingOutcome::DidNotOccur { .. } => {
                result.did_not_occur_count += 1;
                None
            }
            TimingOutcome::Unresolved(_) => {
                result.unresolved_count += 1;
                None
            }
            TimingOutcome::Invalidated { .. } => {
                result.invalidated_count += 1;
                None
            }
        };
        result.rows.push(ScoredTimingRow { input: row, errors });
    }
    if result.occurred_count > 0 {
        let count = result.occurred_count as f64;
        result.means = Some(MeanTimingErrors {
            forecast: MeanTimingError {
                signed_ms: forecast_signed as f64 / count,
                absolute_ms: forecast_absolute as f64 / count,
            },
            baseline: MeanTimingError {
                signed_ms: baseline_signed as f64 / count,
                absolute_ms: baseline_absolute as f64 / count,
            },
        });
    }
    Ok(result)
}

fn timing_error(predicted: i64, observed: i64) -> TimingError {
    let signed_ms = i128::from(predicted) - i128::from(observed);
    TimingError {
        signed_ms,
        absolute_ms: signed_ms.unsigned_abs(),
    }
}
