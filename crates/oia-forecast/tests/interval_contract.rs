use oia_domain::{OpaqueId, VersionId};
use oia_forecast::interval::{
    IntervalError, IntervalForecastRow, IntervalMean, IntervalMeans, IntervalSide,
    PredictionInterval, ScalarOutcome, evaluate_intervals,
};
use oia_forecast::scoring::{MAX_EVALUATION_ROWS, ScoringError, UnresolvedReason};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn row(version: &str, value: f64) -> Result<IntervalForecastRow, Box<dyn std::error::Error>> {
    Ok(IntervalForecastRow {
        forecast_version: VersionId::new(version)?,
        interval: PredictionInterval::new(2.0, 4.0, 0.5)?,
        baseline_version: VersionId::new("baseline:v1")?,
        baseline_interval: PredictionInterval::new(1.0, 5.0, 0.5)?,
        unit: OpaqueId::new("unit:kelvin")?,
        outcome: ScalarOutcome::Resolved {
            value,
            version: VersionId::new(format!("outcome:{version}"))?,
        },
    })
}

// FORECAST-04 / MARKET-05: interval math, losing originals, pinned corrections,
// and unresolved coverage must survive evaluation without invented observations.
#[test]
fn scores_intervals_without_hiding_vintages_or_unresolved_outcomes() -> TestResult {
    let mut rows = vec![row("forecast:v1", 1.0)?, row("forecast:v2", 1.0)?];
    rows[1].interval = PredictionInterval::new(1.0, 3.0, 0.5)?;
    rows.extend([row("above:v1", 5.0)?, row("boundary:v1", 4.0)?]);
    for (i, reason) in [
        UnresolvedReason::Pending,
        UnresolvedReason::Censored,
        UnresolvedReason::Disputed,
    ]
    .into_iter()
    .enumerate()
    {
        let mut pending = row(&format!("unresolved:{i}"), 0.0)?;
        pending.outcome = ScalarOutcome::Unresolved(reason);
        rows.push(pending);
    }
    let evaluation = evaluate_intervals(&rows, 7)?;
    assert_eq!(
        (evaluation.resolved_count, evaluation.unresolved_count),
        (4, 3)
    );
    assert_eq!(evaluation.nominal_coverage, Some(0.5));
    assert_eq!(evaluation.unit, Some(&rows[0].unit));
    assert_eq!(
        evaluation.means,
        Some(IntervalMeans {
            forecast: IntervalMean {
                score: 4.0,
                width: 2.0,
                empirical_coverage: 0.5
            },
            baseline: IntervalMean {
                score: 4.0,
                width: 4.0,
                empirical_coverage: 1.0
            },
        })
    );
    for (i, scored) in evaluation.rows.iter().enumerate() {
        assert_eq!(scored.input, &rows[i]);
        if i >= 4 {
            assert_eq!(scored.scores, None);
        }
    }
    assert_eq!(
        evaluation.rows[0].scores.map(|s| s.forecast.score),
        Some(6.0)
    );
    assert_eq!(
        evaluation.rows[1].scores.map(|s| s.forecast.score),
        Some(2.0)
    );
    assert_eq!(
        evaluation.rows[2].scores.map(|s| s.forecast.score),
        Some(6.0)
    );
    assert_eq!(evaluate_intervals(&rows[4..], 3)?.means, None);
    let empty = evaluate_intervals(&[], 1)?;
    assert_eq!(
        (empty.unit, empty.nominal_coverage, empty.means),
        (None, None, None)
    );

    let mut corrected = rows[0].clone();
    corrected.outcome = ScalarOutcome::Resolved {
        value: 3.0,
        version: VersionId::new("outcome:corrected:v2")?,
    };
    let corrected_rows = [corrected];
    assert_eq!(
        evaluate_intervals(&corrected_rows, 1)?.rows[0]
            .scores
            .map(|s| s.forecast.score),
        Some(2.0)
    );
    assert_eq!(evaluation.rows[0].input.outcome, rows[0].outcome);
    assert_eq!(
        evaluation.rows[0].scores.map(|s| s.forecast.score),
        Some(6.0)
    );

    let mut degenerate = row("point:v1", 2.0)?;
    degenerate.interval = PredictionInterval::new(2.0, 2.0, 0.5)?;
    assert_eq!(
        evaluate_intervals(&[degenerate], 1)?.rows[0]
            .scores
            .map(|s| (s.forecast.score, s.forecast.covered)),
        Some((0.0, true))
    );
    Ok(())
}

// Numerical and input boundaries must reject an entire request, not silently
// alter its cohort; large representable scores must not overflow their mean.
#[test]
fn refuses_incomparable_or_unrepresentable_requests() -> TestResult {
    for coverage in [f64::NAN, f64::INFINITY, -0.1, 0.0, 1.0, 1.1] {
        assert_eq!(
            PredictionInterval::new(0.0, 1.0, coverage),
            Err(IntervalError::InvalidCoverage)
        );
    }
    for (lower, upper) in [
        (2.0, 1.0),
        (f64::NAN, 1.0),
        (0.0, f64::INFINITY),
        (-f64::MAX, f64::MAX),
    ] {
        assert_eq!(
            PredictionInterval::new(lower, upper, 0.5),
            Err(IntervalError::InvalidBounds)
        );
    }
    let first = row("forecast:v1", 3.0)?;
    for limit in [0, MAX_EVALUATION_ROWS + 1] {
        assert_eq!(
            evaluate_intervals(&[], limit),
            Err(IntervalError::Request(ScoringError::InvalidRowLimit))
        );
    }
    let duplicates = [first.clone(), first.clone()];
    assert_eq!(
        evaluate_intervals(&duplicates, 1),
        Err(IntervalError::Request(ScoringError::RowLimitExceeded {
            supplied: 2,
            limit: 1
        }))
    );
    assert_eq!(
        evaluate_intervals(&duplicates, 2),
        Err(IntervalError::Request(
            ScoringError::DuplicateForecastVersion(first.forecast_version.clone())
        ))
    );

    let mut other = row("forecast:v2", 3.0)?;
    other.unit = OpaqueId::new("unit:celsius")?;
    assert_eq!(
        evaluate_intervals(&[first.clone(), other.clone()], 2),
        Err(IntervalError::MismatchedUnit(
            other.forecast_version.clone()
        ))
    );
    other.unit = first.unit.clone();
    other.interval = PredictionInterval::new(2.0, 4.0, 0.75)?;
    other.baseline_interval = other.interval;
    assert_eq!(
        evaluate_intervals(&[first.clone(), other.clone()], 2),
        Err(IntervalError::MismatchedCoverage(
            other.forecast_version.clone()
        ))
    );
    other.interval = first.interval;
    other.outcome = ScalarOutcome::Unresolved(UnresolvedReason::Pending);
    assert_eq!(
        evaluate_intervals(std::slice::from_ref(&other), 1),
        Err(IntervalError::MismatchedCoverage(
            other.forecast_version.clone()
        ))
    );

    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let invalid = row("invalid:v1", value)?;
        assert_eq!(
            evaluate_intervals(std::slice::from_ref(&invalid), 1),
            Err(IntervalError::NonFiniteOutcome(
                invalid.forecast_version.clone()
            ))
        );
    }
    let overflow = row("overflow:v1", f64::MAX)?;
    assert_eq!(
        evaluate_intervals(std::slice::from_ref(&overflow), 1),
        Err(IntervalError::ScoreOverflow {
            forecast_version: overflow.forecast_version.clone(),
            side: IntervalSide::Forecast
        })
    );
    let mut baseline_overflow = row("baseline-overflow:v1", f64::MAX)?;
    baseline_overflow.interval = PredictionInterval::new(f64::MAX, f64::MAX, 0.5)?;
    assert_eq!(
        evaluate_intervals(std::slice::from_ref(&baseline_overflow), 1),
        Err(IntervalError::ScoreOverflow {
            forecast_version: baseline_overflow.forecast_version.clone(),
            side: IntervalSide::Baseline
        })
    );

    let mut large = [row("large:v1", 0.0)?, row("large:v2", 0.0)?];
    for row in &mut large {
        row.interval = PredictionInterval::new(0.0, f64::MAX, 0.5)?;
        row.baseline_interval = row.interval;
    }
    assert_eq!(
        evaluate_intervals(&large, 2)?
            .means
            .map(|m| m.forecast.score),
        Some(f64::MAX)
    );
    Ok(())
}
