//! Bounded binary Brier scoring with version references and explicit unresolved rows.
//!
//! This is numerical evaluation of caller-selected inputs, not evidence that
//! probabilities are calibrated or that a cohort is independent or leakage-free.

use std::{collections::BTreeSet, fmt};

use oia_domain::VersionId;

/// Hard ceiling on rows in one evaluation (including unresolved rows).
pub const MAX_EVALUATION_ROWS: usize = 100_000;

/// A finite binary-event probability in the inclusive range 0..=1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Probability(f64);

impl Probability {
    /// Validate a probability without clamping or replacing invalid values.
    pub fn new(value: f64) -> Result<Self, ScoringError> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(ScoringError::InvalidProbability);
        }
        Ok(Self(value))
    }

    /// The validated probability.
    pub fn value(self) -> f64 {
        self.0
    }
}

/// Why an outcome cannot currently be scored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnresolvedReason {
    /// Follow-up has not yet established an outcome.
    Pending,
    /// Follow-up ended without sufficient coverage to resolve the outcome.
    Censored,
    /// Available outcome evidence is disputed.
    Disputed,
}

/// A pinned resolution, or an explicitly unscored outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinaryOutcome {
    /// Whether the defined event occurred, with its immutable resolution version.
    Resolved { occurred: bool, version: VersionId },
    /// Never treated as a negative outcome or silently dropped.
    Unresolved(UnresolvedReason),
}

/// One forecast vintage and the baseline selected for the same target/horizon.
///
/// Callers must validate issuance, target alignment, information cutoff, source
/// coverage and authorization before passing rows. Version references identify
/// upstream records; this module does not fetch or validate those records.
#[derive(Debug, Clone, PartialEq)]
pub struct BinaryForecastRow {
    pub forecast_version: VersionId,
    pub probability: Probability,
    pub baseline_version: VersionId,
    pub baseline_probability: Probability,
    pub outcome: BinaryOutcome,
}

/// Scores for a resolved row. Lower is better; values are in 0..=1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrierScores {
    pub forecast: f64,
    pub baseline: f64,
}

/// Evaluation retains every input in its supplied order, including failures,
/// revisions and unresolved cases. It never selects only the latest vintage.
#[derive(Debug, PartialEq)]
pub struct ScoredRow<'a> {
    pub input: &'a BinaryForecastRow,
    pub scores: Option<BrierScores>,
}

/// Descriptive, equal-row-weight scores over resolved vintages only.
///
/// Revisions of the same event may be dependent. These means do not establish
/// statistical significance, calibration or independent-event performance.
#[derive(Debug, PartialEq)]
pub struct BinaryEvaluation<'a> {
    pub rows: Vec<ScoredRow<'a>>,
    pub resolved_count: usize,
    pub unresolved_count: usize,
    /// `None` for empty or wholly unresolved input, never a fabricated zero.
    pub mean_scores: Option<BrierScores>,
}

/// Invalid evaluation requests are refused as a whole, never truncated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScoringError {
    InvalidProbability,
    /// Limit must lie in 1..=MAX_EVALUATION_ROWS.
    InvalidRowLimit,
    RowLimitExceeded {
        supplied: usize,
        limit: usize,
    },
    DuplicateForecastVersion(VersionId),
}

impl fmt::Display for ScoringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProbability => write!(f, "probability must be finite and in 0..=1"),
            Self::InvalidRowLimit => write!(f, "row limit must be in 1..={MAX_EVALUATION_ROWS}"),
            Self::RowLimitExceeded { supplied, limit } => {
                write!(f, "{supplied} rows exceed the evaluation limit of {limit}")
            }
            Self::DuplicateForecastVersion(version) => {
                write!(f, "forecast version {version} occurs more than once")
            }
        }
    }
}

impl std::error::Error for ScoringError {}

/// Score all supplied rows using `(probability - observed_binary_value)^2`.
///
/// The caller chooses the cohort; even empty input is preserved as such. No
/// latest-version filtering, sampling, baseline fitting or outcome imputation
/// occurs. Corrected resolutions require a new evaluation with new references.
pub fn evaluate_binary(
    rows: &[BinaryForecastRow],
    row_limit: usize,
) -> Result<BinaryEvaluation<'_>, ScoringError> {
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
    let mut scored = Vec::with_capacity(rows.len());
    let mut resolved_count = 0;
    let mut forecast_sum = 0.0;
    let mut baseline_sum = 0.0;
    for row in rows {
        if !versions.insert(&row.forecast_version) {
            return Err(ScoringError::DuplicateForecastVersion(
                row.forecast_version.clone(),
            ));
        }
        let scores = match &row.outcome {
            BinaryOutcome::Resolved { occurred, .. } => {
                let observed = if *occurred { 1.0 } else { 0.0 };
                let forecast = (row.probability.value() - observed).powi(2);
                let baseline = (row.baseline_probability.value() - observed).powi(2);
                forecast_sum += forecast;
                baseline_sum += baseline;
                resolved_count += 1;
                Some(BrierScores { forecast, baseline })
            }
            BinaryOutcome::Unresolved(_) => None,
        };
        scored.push(ScoredRow { input: row, scores });
    }
    Ok(BinaryEvaluation {
        rows: scored,
        resolved_count,
        unresolved_count: rows.len() - resolved_count,
        mean_scores: (resolved_count > 0).then(|| BrierScores {
            forecast: forecast_sum / resolved_count as f64,
            baseline: baseline_sum / resolved_count as f64,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(
        version: &str,
        p: f64,
        outcome: BinaryOutcome,
    ) -> Result<BinaryForecastRow, Box<dyn std::error::Error>> {
        Ok(BinaryForecastRow {
            forecast_version: VersionId::new(version)?,
            probability: Probability::new(p)?,
            baseline_version: VersionId::new("baseline:v1")?,
            baseline_probability: Probability::new(0.5)?,
            outcome,
        })
    }

    // FORECAST-04: losing originals survive revision and unresolved coverage stays visible.
    #[test]
    fn scores_original_and_revision_without_hiding_unresolved()
    -> Result<(), Box<dyn std::error::Error>> {
        let outcome = BinaryOutcome::Resolved {
            occurred: true,
            version: VersionId::new("outcome:v1")?,
        };
        let rows = [
            row("forecast:v1", 0.0, outcome.clone())?,
            row("forecast:v2", 1.0, outcome)?,
            row(
                "other:v1",
                0.5,
                BinaryOutcome::Unresolved(UnresolvedReason::Censored),
            )?,
        ];
        let result = evaluate_binary(&rows, 3)?;
        assert_eq!((result.resolved_count, result.unresolved_count), (2, 1));
        assert_eq!(
            result.mean_scores,
            Some(BrierScores {
                forecast: 0.5,
                baseline: 0.25
            })
        );
        assert_eq!(result.rows[0].input, &rows[0]);
        assert_eq!(
            result.rows[0].scores,
            Some(BrierScores {
                forecast: 1.0,
                baseline: 0.25
            })
        );
        assert_eq!(result.rows[2].scores, None);
        assert_eq!(evaluate_binary(&rows[2..], 1)?.mean_scores, None);
        assert_eq!(evaluate_binary(&[], 1)?.mean_scores, None);
        let negative = [row(
            "negative:v1",
            0.25,
            BinaryOutcome::Resolved {
                occurred: false,
                version: VersionId::new("outcome:v2")?,
            },
        )?];
        assert_eq!(
            evaluate_binary(&negative, 1)?.mean_scores,
            Some(BrierScores {
                forecast: 0.0625,
                baseline: 0.25
            })
        );
        Ok(())
    }

    // Reject malformed numeric inputs and requests that could duplicate or silently omit rows.
    #[test]
    fn refuses_invalid_probabilities_duplicates_and_excess_rows()
    -> Result<(), Box<dyn std::error::Error>> {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.01, 1.01] {
            assert_eq!(
                Probability::new(value),
                Err(ScoringError::InvalidProbability)
            );
        }
        let rows = [row(
            "forecast:v1",
            0.5,
            BinaryOutcome::Unresolved(UnresolvedReason::Pending),
        )?];
        assert_eq!(
            evaluate_binary(&rows, 0),
            Err(ScoringError::InvalidRowLimit)
        );
        assert_eq!(
            evaluate_binary(&rows, MAX_EVALUATION_ROWS + 1),
            Err(ScoringError::InvalidRowLimit)
        );
        let duplicates = [rows[0].clone(), rows[0].clone()];
        assert_eq!(
            evaluate_binary(&duplicates, 1),
            Err(ScoringError::RowLimitExceeded {
                supplied: 2,
                limit: 1
            })
        );
        assert_eq!(
            evaluate_binary(&duplicates, 2),
            Err(ScoringError::DuplicateForecastVersion(
                rows[0].forecast_version.clone()
            ))
        );
        Ok(())
    }
}
