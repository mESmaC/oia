use oia_domain::VersionId;
use oia_forecast::{
    scoring::UnresolvedReason,
    timing::{TimingForecastRow, TimingOutcome, evaluate_timing},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut rows = Vec::new();
    for (version, predicted_unix_ms, outcome) in [
        (
            "forecast:v1",
            6_000,
            TimingOutcome::Occurred {
                unix_ms: 10_000,
                version: VersionId::new("outcome:v1")?,
            },
        ),
        (
            "forecast:v2",
            11_000,
            TimingOutcome::Occurred {
                unix_ms: 10_000,
                version: VersionId::new("outcome:v1")?,
            },
        ),
        (
            "other:v1",
            20_000,
            TimingOutcome::Unresolved(UnresolvedReason::Censored),
        ),
        (
            "absent:v1",
            30_000,
            TimingOutcome::DidNotOccur {
                version: VersionId::new("absence:v1")?,
            },
        ),
    ] {
        rows.push(TimingForecastRow {
            forecast_version: VersionId::new(version)?,
            predicted_unix_ms,
            baseline_version: VersionId::new("baseline:v1")?,
            baseline_unix_ms: 12_000,
            outcome,
        });
    }
    println!("SIMULATED fixture: event timing evaluation; no observed performance claim.");
    println!("{:#?}", evaluate_timing(&rows, 4)?);
    Ok(())
}
