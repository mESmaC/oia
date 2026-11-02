//! Synthetic arithmetic illustration, not observed forecast performance.

use oia_domain::{OpaqueId, VersionId};
use oia_forecast::interval::{
    IntervalForecastRow, PredictionInterval, ScalarOutcome, evaluate_intervals,
};
use oia_forecast::scoring::UnresolvedReason;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rows = [
        make_row(
            "forecast:v1",
            2.0,
            4.0,
            ScalarOutcome::Resolved {
                value: 1.0,
                version: VersionId::new("outcome:v1")?,
            },
        )?,
        make_row(
            "forecast:v2",
            1.0,
            3.0,
            ScalarOutcome::Resolved {
                value: 1.0,
                version: VersionId::new("outcome:v1")?,
            },
        )?,
        make_row(
            "other:v1",
            2.0,
            4.0,
            ScalarOutcome::Unresolved(UnresolvedReason::Censored),
        )?,
    ];
    let result = evaluate_intervals(&rows, 3)?;
    println!("SIMULATED interval arithmetic; nominal coverage=0.5, unit=kelvin");
    println!("Original and revised vintages share an outcome; these are not independent events.");
    println!(
        "resolved={}, unresolved={}",
        result.resolved_count, result.unresolved_count
    );
    for row in &result.rows {
        println!(
            "{} baseline={} outcome={:?} scores={:?}",
            row.input.forecast_version, row.input.baseline_version, row.input.outcome, row.scores
        );
    }
    println!("Resolved-vintage means: {:?}", result.means);
    Ok(())
}

fn make_row(
    version: &str,
    lower: f64,
    upper: f64,
    outcome: ScalarOutcome,
) -> Result<IntervalForecastRow, Box<dyn std::error::Error>> {
    Ok(IntervalForecastRow {
        forecast_version: VersionId::new(version)?,
        interval: PredictionInterval::new(lower, upper, 0.5)?,
        baseline_version: VersionId::new("baseline:v1")?,
        baseline_interval: PredictionInterval::new(1.0, 5.0, 0.5)?,
        unit: OpaqueId::new("unit:kelvin")?,
        outcome,
    })
}
