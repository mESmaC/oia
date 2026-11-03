use oia_domain::VersionId;
use oia_forecast::{
    scoring::{MAX_EVALUATION_ROWS, ScoringError, UnresolvedReason},
    timing::{TimingForecastRow, TimingOutcome, evaluate_timing},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn row(
    version: &str,
    predicted_unix_ms: i64,
) -> Result<TimingForecastRow, Box<dyn std::error::Error>> {
    Ok(TimingForecastRow {
        forecast_version: VersionId::new(version)?,
        predicted_unix_ms,
        baseline_version: VersionId::new("baseline:v1")?,
        baseline_unix_ms: 12_000,
        outcome: TimingOutcome::Occurred {
            unix_ms: 10_000,
            version: VersionId::new("resolution:v1")?,
        },
    })
}

// FORECAST-04 / A15: retain losing original vintages and explicit missing
// coverage; a correction must not change a previously evaluated resolution.
#[test]
fn timing_preserves_vintages_coverage_and_corrections() -> TestResult {
    let mut rows = vec![row("forecast:v1", 6_000)?, row("forecast:v2", 11_000)?];
    let unscored = [
        TimingOutcome::Unresolved(UnresolvedReason::Pending),
        TimingOutcome::Unresolved(UnresolvedReason::Censored),
        TimingOutcome::Unresolved(UnresolvedReason::Disputed),
        TimingOutcome::DidNotOccur {
            version: VersionId::new("absence:v1")?,
        },
        TimingOutcome::Invalidated {
            version: VersionId::new("invalidation:v1")?,
        },
    ];
    for (index, outcome) in unscored.into_iter().enumerate() {
        let mut input = row(&format!("unscored:{index}"), 0)?;
        input.outcome = outcome;
        rows.push(input);
    }
    let evaluated = evaluate_timing(&rows, 7)?;
    assert_eq!(
        (
            evaluated.occurred_count,
            evaluated.did_not_occur_count,
            evaluated.unresolved_count,
            evaluated.invalidated_count
        ),
        (2, 1, 3, 1)
    );
    assert_eq!(
        evaluated.means.map(|m| (
            m.forecast.signed_ms,
            m.forecast.absolute_ms,
            m.baseline.signed_ms,
            m.baseline.absolute_ms
        )),
        Some((-1_500.0, 2_500.0, 2_000.0, 2_000.0))
    );
    for (scored, input) in evaluated.rows.iter().zip(&rows) {
        assert_eq!(scored.input, input);
    }
    assert_eq!(
        evaluated.rows[0].errors.map(|e| e.forecast.signed_ms),
        Some(-4_000)
    );
    assert_eq!(
        evaluated.rows[1].errors.map(|e| e.forecast.signed_ms),
        Some(1_000)
    );
    assert!(evaluated.rows[2..].iter().all(|r| r.errors.is_none()));
    assert_eq!(evaluate_timing(&rows[2..], 5)?.means, None);
    assert_eq!(evaluate_timing(&[], 1)?.means, None);
    let mut corrected = rows[0].clone();
    corrected.outcome = TimingOutcome::Occurred {
        unix_ms: 6_000,
        version: VersionId::new("resolution:v2")?,
    };
    assert_eq!(
        evaluate_timing(&[corrected], 1)?
            .means
            .map(|m| m.forecast.absolute_ms),
        Some(0.0)
    );
    assert_eq!(
        evaluated.rows[0].errors.map(|e| e.forecast.absolute_ms),
        Some(4_000)
    );
    Ok(())
}

// Full-domain subtraction must not saturate or overflow; budgets and duplicate
// rejection apply to unresolved rows too, so failed requests cannot hide rows.
#[test]
fn timing_handles_extreme_arithmetic_and_rejects_invalid_requests() -> TestResult {
    let mut early = row("early:v1", i64::MIN)?;
    early.outcome = TimingOutcome::Occurred {
        unix_ms: i64::MAX,
        version: VersionId::new("extreme:v1")?,
    };
    let mut late = row("late:v1", i64::MAX)?;
    late.outcome = TimingOutcome::Occurred {
        unix_ms: i64::MIN,
        version: VersionId::new("extreme:v2")?,
    };
    let rows = [early, late];
    let evaluation = evaluate_timing(&rows, 2)?;
    let span = i128::from(u64::MAX);
    assert_eq!(
        evaluation.rows[0].errors.map(|e| e.forecast.signed_ms),
        Some(-span)
    );
    assert_eq!(
        evaluation.rows[1].errors.map(|e| e.forecast.signed_ms),
        Some(span)
    );
    assert_eq!(evaluation.means.map(|m| m.forecast.signed_ms), Some(0.0));
    assert_eq!(
        evaluation.means.map(|m| m.forecast.absolute_ms),
        Some(u64::MAX as f64)
    );
    let reversed = [rows[1].clone(), rows[0].clone()];
    assert_eq!(evaluate_timing(&reversed, 2)?.means, evaluation.means);
    for budget in [0, MAX_EVALUATION_ROWS + 1] {
        assert_eq!(
            evaluate_timing(&[], budget),
            Err(ScoringError::InvalidRowLimit)
        );
    }
    assert_eq!(
        evaluate_timing(&rows, 1),
        Err(ScoringError::RowLimitExceeded {
            supplied: 2,
            limit: 1
        })
    );
    let mut pending = row("duplicate:v1", 0)?;
    pending.outcome = TimingOutcome::Unresolved(UnresolvedReason::Pending);
    assert_eq!(
        evaluate_timing(&[pending.clone(), pending.clone()], 2),
        Err(ScoringError::DuplicateForecastVersion(
            pending.forecast_version
        ))
    );
    Ok(())
}
