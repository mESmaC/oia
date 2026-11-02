//! Descriptive reliability tables over pinned ledger evaluations, without fitting
//! probabilities or claiming calibration, independence, or prospective accuracy.

use std::fmt;

use oia_domain::VersionId;

use crate::issuance::{
    EvaluatedVintageSummary, EvaluationRunRecord, ForecastLedger, ResolutionState,
};
use crate::scoring::{MAX_EVALUATION_ROWS, UnresolvedReason};

/// Maximum number of equal-width probability bins in a report.
pub const MAX_RELIABILITY_BINS: usize = 100;

/// Every selected vintage is counted, including outcomes excluded from frequencies.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct OutcomeCounts {
    pub resolved: usize,
    pub pending: usize,
    pub censored: usize,
    pub disputed: usize,
    pub invalidated: usize,
}

impl OutcomeCounts {
    /// Number of vintages, not independent events.
    pub fn total(self) -> usize {
        self.resolved + self.pending + self.censored + self.disputed + self.invalidated
    }

    fn record(&mut self, state: &ResolutionState) {
        match state {
            ResolutionState::Resolved { .. } => self.resolved += 1,
            ResolutionState::Unresolved(UnresolvedReason::Pending) => self.pending += 1,
            ResolutionState::Unresolved(UnresolvedReason::Censored) => self.censored += 1,
            ResolutionState::Unresolved(UnresolvedReason::Disputed) => self.disputed += 1,
            ResolutionState::Invalidated { .. } => self.invalidated += 1,
        }
    }
}

/// Both means use exactly the same resolved vintages with equal row weights.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReliabilityStatistics {
    pub mean_probability: f64,
    pub observed_frequency: f64,
}

/// Interval `[lower, upper)`, except the final bin includes probability 1.
#[derive(Debug, PartialEq)]
pub struct ReliabilityBin {
    pub lower: f64,
    pub upper: f64,
    pub upper_inclusive: bool,
    pub counts: OutcomeCounts,
    /// `None` when there are no resolved vintages, including an empty bin.
    pub statistics: Option<ReliabilityStatistics>,
}

/// An inspectable link to the original vintage and both independently assigned bins.
#[derive(Debug, PartialEq)]
pub struct ReliabilityRow<'a> {
    pub vintage: &'a EvaluatedVintageSummary,
    pub forecast_bin: usize,
    pub baseline_bin: usize,
}

/// Descriptive snapshot of one caller-selected evaluation. The borrowed source
/// exposes exact versions, prior scores, leakage disclosures and reassessment
/// reasons. No original, revision, failure, or unresolved row is filtered out.
#[derive(Debug, PartialEq)]
pub struct ReliabilityReport<'a> {
    pub evaluation: &'a EvaluationRunRecord,
    pub rows: Vec<ReliabilityRow<'a>>,
    pub forecast_bins: Vec<ReliabilityBin>,
    pub baseline_bins: Vec<ReliabilityBin>,
}

/// Refused requests never yield a truncated report.
#[derive(Debug, PartialEq, Eq)]
pub enum ReliabilityError {
    InvalidBinCount,
    InvalidRowLimit,
    UnknownEvaluation(VersionId),
    RowLimitExceeded {
        supplied: usize,
        limit: usize,
    },
    AllocationFailed,
    /// Defensive error if a validated probability cannot be assigned a bin.
    BinAssignmentFailed,
}

impl fmt::Display for ReliabilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBinCount => write!(f, "bin count must be in 1..={MAX_RELIABILITY_BINS}"),
            Self::InvalidRowLimit => write!(f, "row limit must be in 1..={MAX_EVALUATION_ROWS}"),
            Self::UnknownEvaluation(version) => write!(f, "unknown evaluation {version}"),
            Self::RowLimitExceeded { supplied, limit } => {
                write!(f, "{supplied} vintages exceed report limit {limit}")
            }
            Self::AllocationFailed => write!(f, "unable to allocate reliability report"),
            Self::BinAssignmentFailed => write!(f, "probability does not match report bins"),
        }
    }
}

impl std::error::Error for ReliabilityError {}

impl ForecastLedger {
    /// Summarize an existing evaluation without changing its probabilities or
    /// choosing newer vintages/resolutions. All bin edges are computed as i/n;
    /// assignment compares to those exact f64 edges (no rounded p*n indexing).
    ///
    /// The bin count is explicit and retained in the output. This descriptive
    /// report does not fit a calibrator, assess statistical significance, or
    /// correct selection bias or dependence between revisions of one event.
    pub fn reliability_report(
        &self,
        evaluation_version: &VersionId,
        bin_count: usize,
        row_limit: usize,
    ) -> Result<ReliabilityReport<'_>, ReliabilityError> {
        if !(1..=MAX_RELIABILITY_BINS).contains(&bin_count) {
            return Err(ReliabilityError::InvalidBinCount);
        }
        if !(1..=MAX_EVALUATION_ROWS).contains(&row_limit) {
            return Err(ReliabilityError::InvalidRowLimit);
        }
        let evaluation = self
            .evaluation(evaluation_version)
            .ok_or_else(|| ReliabilityError::UnknownEvaluation(evaluation_version.clone()))?;
        if evaluation.vintages.len() > row_limit {
            return Err(ReliabilityError::RowLimitExceeded {
                supplied: evaluation.vintages.len(),
                limit: row_limit,
            });
        }

        let mut forecast = accumulators(bin_count)?;
        let mut baseline = accumulators(bin_count)?;
        let mut rows = Vec::new();
        rows.try_reserve_exact(evaluation.vintages.len())
            .map_err(|_| ReliabilityError::AllocationFailed)?;
        for vintage in &evaluation.vintages {
            rows.push(ReliabilityRow {
                vintage,
                forecast_bin: record(
                    &mut forecast,
                    vintage.forecast_probability.value(),
                    &vintage.resolution_state,
                )?,
                baseline_bin: record(
                    &mut baseline,
                    vintage.baseline_probability.value(),
                    &vintage.resolution_state,
                )?,
            });
        }
        Ok(ReliabilityReport {
            evaluation,
            rows,
            forecast_bins: finish(forecast)?,
            baseline_bins: finish(baseline)?,
        })
    }
}

struct Accumulator {
    bin: ReliabilityBin,
    probability_sum: f64,
    occurred_count: usize,
}

fn accumulators(count: usize) -> Result<Vec<Accumulator>, ReliabilityError> {
    let mut bins = Vec::new();
    bins.try_reserve_exact(count)
        .map_err(|_| ReliabilityError::AllocationFailed)?;
    for index in 0..count {
        bins.push(Accumulator {
            bin: ReliabilityBin {
                lower: index as f64 / count as f64,
                upper: (index + 1) as f64 / count as f64,
                upper_inclusive: index + 1 == count,
                counts: OutcomeCounts::default(),
                statistics: None,
            },
            probability_sum: 0.0,
            occurred_count: 0,
        });
    }
    Ok(bins)
}

fn record(
    bins: &mut [Accumulator],
    probability: f64,
    state: &ResolutionState,
) -> Result<usize, ReliabilityError> {
    for (index, accumulator) in bins.iter_mut().enumerate() {
        let bin = &mut accumulator.bin;
        if probability >= bin.lower
            && (probability < bin.upper || bin.upper_inclusive && probability <= bin.upper)
        {
            bin.counts.record(state);
            if let ResolutionState::Resolved { occurred, .. } = state {
                accumulator.probability_sum += probability;
                accumulator.occurred_count += usize::from(*occurred);
            }
            return Ok(index);
        }
    }
    Err(ReliabilityError::BinAssignmentFailed)
}

fn finish(accumulators: Vec<Accumulator>) -> Result<Vec<ReliabilityBin>, ReliabilityError> {
    let mut bins = Vec::new();
    bins.try_reserve_exact(accumulators.len())
        .map_err(|_| ReliabilityError::AllocationFailed)?;
    for mut accumulator in accumulators {
        let resolved = accumulator.bin.counts.resolved;
        if resolved > 0 {
            accumulator.bin.statistics = Some(ReliabilityStatistics {
                mean_probability: accumulator.probability_sum / resolved as f64,
                observed_frequency: accumulator.occurred_count as f64 / resolved as f64,
            });
        }
        bins.push(accumulator.bin);
    }
    Ok(bins)
}
