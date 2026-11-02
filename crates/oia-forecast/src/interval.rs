//! Descriptive central prediction-interval scores with pinned input versions.
//!
//! This numerical kernel does not issue scalar forecasts, fit intervals, select
//! cohorts, or establish calibration, independence, or point-in-time validity.

use std::{collections::BTreeSet, fmt};

use oia_domain::{OpaqueId, VersionId};

use crate::scoring::{MAX_EVALUATION_ROWS, ScoringError, UnresolvedReason};

/// Central interval with lower/upper quantiles at alpha/2 and 1-alpha/2,
/// where alpha = 1 - nominal coverage. Endpoints count as covered.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PredictionInterval {
    lower: f64,
    upper: f64,
    nominal_coverage: f64,
}

impl PredictionInterval {
    /// Refuse non-finite, reversed, or unrepresentably wide bounds and coverage
    /// outside the open interval (0, 1). Equal bounds are permitted.
    pub fn new(lower: f64, upper: f64, nominal_coverage: f64) -> Result<Self, IntervalError> {
        if !lower.is_finite() || !upper.is_finite() || lower > upper || !(upper - lower).is_finite()
        {
            return Err(IntervalError::InvalidBounds);
        }
        if !nominal_coverage.is_finite() || nominal_coverage <= 0.0 || nominal_coverage >= 1.0 {
            return Err(IntervalError::InvalidCoverage);
        }
        Ok(Self {
            lower,
            upper,
            nominal_coverage,
        })
    }

    pub fn lower(self) -> f64 {
        self.lower
    }

    pub fn upper(self) -> f64 {
        self.upper
    }

    pub fn nominal_coverage(self) -> f64 {
        self.nominal_coverage
    }
}

/// A versioned scalar resolution or an explicitly unscored outcome.
#[derive(Debug, Clone, PartialEq)]
pub enum ScalarOutcome {
    Resolved { value: f64, version: VersionId },
    Unresolved(UnresolvedReason),
}

/// A forecast vintage and baseline for the same scalar target and horizon.
///
/// `unit` declares the unit of both intervals and the outcome (e.g. `unit:kelvin`).
/// Callers must validate this declaration, target alignment, version existence,
/// cutoff, and authorization upstream. There are no implicit unit conversions.
#[derive(Debug, Clone, PartialEq)]
pub struct IntervalForecastRow {
    pub forecast_version: VersionId,
    pub interval: PredictionInterval,
    pub baseline_version: VersionId,
    pub baseline_interval: PredictionInterval,
    pub unit: OpaqueId,
    pub outcome: ScalarOutcome,
}

/// The central interval score (lower is better), width, and endpoint-inclusive
/// coverage for one resolved vintage. Score and width have the input unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IntervalScore {
    pub score: f64,
    pub width: f64,
    pub covered: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IntervalScores {
    pub forecast: IntervalScore,
    pub baseline: IntervalScore,
}

/// Equal-weight resolved-vintage means; dependent revisions remain separate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IntervalMean {
    pub score: f64,
    pub width: f64,
    pub empirical_coverage: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IntervalMeans {
    pub forecast: IntervalMean,
    pub baseline: IntervalMean,
}

#[derive(Debug, PartialEq)]
pub struct ScoredIntervalRow<'a> {
    pub input: &'a IntervalForecastRow,
    /// Unresolved rows never receive a zero score or a fabricated observation.
    pub scores: Option<IntervalScores>,
}

/// All supplied rows in original order, including unresolved and losing vintages.
#[derive(Debug, PartialEq)]
pub struct IntervalEvaluation<'a> {
    pub rows: Vec<ScoredIntervalRow<'a>>,
    pub resolved_count: usize,
    pub unresolved_count: usize,
    /// Absent only for empty input. All rows must have exactly this unit/level.
    pub unit: Option<&'a OpaqueId>,
    pub nominal_coverage: Option<f64>,
    /// Absent for empty or wholly unresolved input.
    pub means: Option<IntervalMeans>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntervalSide {
    Forecast,
    Baseline,
}

/// Invalid requests fail as a whole, without truncating or dropping bad rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntervalError {
    InvalidBounds,
    InvalidCoverage,
    Request(ScoringError),
    MismatchedUnit(VersionId),
    MismatchedCoverage(VersionId),
    NonFiniteOutcome(VersionId),
    ScoreOverflow {
        forecast_version: VersionId,
        side: IntervalSide,
    },
}

impl fmt::Display for IntervalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBounds => write!(
                f,
                "interval bounds must be finite, ordered, and have finite width"
            ),
            Self::InvalidCoverage => write!(
                f,
                "nominal coverage must be finite and strictly between 0 and 1"
            ),
            Self::Request(error) => error.fmt(f),
            Self::MismatchedUnit(version) => {
                write!(f, "forecast {version} has a different unit from the cohort")
            }
            Self::MismatchedCoverage(version) => write!(
                f,
                "forecast {version} or its baseline has a different nominal coverage"
            ),
            Self::NonFiniteOutcome(version) => {
                write!(f, "forecast {version} has a non-finite scalar outcome")
            }
            Self::ScoreOverflow {
                forecast_version,
                side,
            } => write!(
                f,
                "forecast {forecast_version} has an unrepresentable {side:?} interval score"
            ),
        }
    }
}

impl std::error::Error for IntervalError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Request(error) => Some(error),
            _ => None,
        }
    }
}

/// Score central intervals as width + 2/alpha times the distance outside them.
///
/// Uses one declared unit and nominal coverage across the entire cohort and
/// matched baselines, including unresolved rows. No fitting, imputation, latest
/// vintage selection, or sampling occurs. Limits include every supplied row.
pub fn evaluate_intervals(
    rows: &[IntervalForecastRow],
    row_limit: usize,
) -> Result<IntervalEvaluation<'_>, IntervalError> {
    if !(1..=MAX_EVALUATION_ROWS).contains(&row_limit) {
        return Err(IntervalError::Request(ScoringError::InvalidRowLimit));
    }
    if rows.len() > row_limit {
        return Err(IntervalError::Request(ScoringError::RowLimitExceeded {
            supplied: rows.len(),
            limit: row_limit,
        }));
    }
    let mut versions = BTreeSet::new();
    let mut scored = Vec::with_capacity(rows.len());
    let mut resolved_count = 0;
    let mut means: Option<IntervalMeans> = None;
    for row in rows {
        if !versions.insert(&row.forecast_version) {
            return Err(IntervalError::Request(
                ScoringError::DuplicateForecastVersion(row.forecast_version.clone()),
            ));
        }
        if let Some(first) = rows.first() {
            if row.unit != first.unit {
                return Err(IntervalError::MismatchedUnit(row.forecast_version.clone()));
            }
            if row.interval.nominal_coverage != first.interval.nominal_coverage
                || row.baseline_interval.nominal_coverage != first.interval.nominal_coverage
            {
                return Err(IntervalError::MismatchedCoverage(
                    row.forecast_version.clone(),
                ));
            }
        }
        let scores = match row.outcome {
            ScalarOutcome::Resolved { value, .. } => {
                if !value.is_finite() {
                    return Err(IntervalError::NonFiniteOutcome(
                        row.forecast_version.clone(),
                    ));
                }
                let scores = IntervalScores {
                    forecast: score_interval(row, row.interval, value, IntervalSide::Forecast)?,
                    baseline: score_interval(
                        row,
                        row.baseline_interval,
                        value,
                        IntervalSide::Baseline,
                    )?,
                };
                resolved_count += 1;
                means = Some(IntervalMeans {
                    forecast: update_mean(
                        means.map(|m| m.forecast),
                        scores.forecast,
                        resolved_count,
                    ),
                    baseline: update_mean(
                        means.map(|m| m.baseline),
                        scores.baseline,
                        resolved_count,
                    ),
                });
                Some(scores)
            }
            ScalarOutcome::Unresolved(_) => None,
        };
        scored.push(ScoredIntervalRow { input: row, scores });
    }
    Ok(IntervalEvaluation {
        rows: scored,
        resolved_count,
        unresolved_count: rows.len() - resolved_count,
        unit: rows.first().map(|row| &row.unit),
        nominal_coverage: rows.first().map(|row| row.interval.nominal_coverage),
        means,
    })
}

fn score_interval(
    row: &IntervalForecastRow,
    interval: PredictionInterval,
    value: f64,
    side: IntervalSide,
) -> Result<IntervalScore, IntervalError> {
    let width = interval.upper - interval.lower;
    let distance = if value < interval.lower {
        interval.lower - value
    } else if value > interval.upper {
        value - interval.upper
    } else {
        0.0
    };
    let score = width + (2.0 / (1.0 - interval.nominal_coverage)) * distance;
    if !score.is_finite() {
        return Err(IntervalError::ScoreOverflow {
            forecast_version: row.forecast_version.clone(),
            side,
        });
    }
    Ok(IntervalScore {
        score,
        width,
        covered: value >= interval.lower && value <= interval.upper,
    })
}

fn update_mean(previous: Option<IntervalMean>, score: IntervalScore, count: usize) -> IntervalMean {
    let coverage = if score.covered { 1.0 } else { 0.0 };
    match previous {
        Some(previous) => IntervalMean {
            // Nonnegative finite values allow this running mean without a sum overflow.
            score: previous.score + (score.score - previous.score) / count as f64,
            width: previous.width + (score.width - previous.width) / count as f64,
            empirical_coverage: previous.empirical_coverage
                + (coverage - previous.empirical_coverage) / count as f64,
        },
        None => IntervalMean {
            score: score.score,
            width: score.width,
            empirical_coverage: coverage,
        },
    }
}
