//! FIELD-01 risks: fabricated continuity, duplicate captures, origin promotion and late mutation.
use oia_devices::{station::*, station_trigger::*};
use oia_domain::{OpaqueId, VersionId};

fn rule() -> Result<Rule, Box<dyn std::error::Error>> {
    Ok(Rule {
        version: VersionId::new("threshold:v1")?,
        device: OpaqueId::new("sensor:a")?,
        signal_version: VersionId::new("temperature-celsius:v1")?,
        origin: Origin::Simulation,
        threshold: 30.0,
        release_threshold: 28.0,
        consecutive_samples: 2,
        max_gap_ms: 10,
        pre_ms: 5,
        post_ms: 5,
    })
}
fn sample(at: i64, value: Option<f64>) -> Result<Sample, Box<dyn std::error::Error>> {
    let r = rule()?;
    Ok(Sample {
        device: r.device,
        signal_version: r.signal_version,
        origin: r.origin,
        version: VersionId::new(format!("sample:{at}"))?,
        at_utc_ms: at,
        uncertainty_ms: 1,
        value,
    })
}

#[test]
fn missing_gaps_latching_and_planner_keep_coverage_honest() -> Result<(), Box<dyn std::error::Error>>
{
    let mut evaluator = Evaluator::new(rule()?)?;
    assert_eq!(
        evaluator.push(sample(0, Some(30.0))?)?.decision,
        Decision::Pending { samples: 1 }
    );
    assert_eq!(
        evaluator.push(sample(1, None)?)?.decision,
        Decision::Missing
    );
    assert_eq!(
        evaluator.push(sample(2, Some(31.0))?)?.decision,
        Decision::Pending { samples: 1 }
    );
    let gap = evaluator.push(sample(13, Some(31.0))?)?;
    assert!(gap.gap_before);
    assert_eq!(gap.decision, Decision::Pending { samples: 1 });
    let Decision::Fired(trigger) = evaluator.push(sample(23, Some(32.0))?)?.decision else {
        return Err("expected confirming trigger".into());
    };
    assert_eq!(trigger.first_at_utc_ms, 13);
    assert_eq!(trigger.first_sample.as_str(), "sample:13");
    assert_eq!(trigger.confirming_sample.version.as_str(), "sample:23");
    assert_eq!(trigger.requested, Window::new(7, 29)?);
    assert_eq!(trigger.rule.origin, Origin::Simulation);
    // A trigger requests context; it cannot manufacture bytes or fill evidence gaps.
    let manifest = plan_capture(
        VersionId::new("event:v1")?,
        trigger.requested,
        30,
        &[DeviceInventory {
            device_id: rule()?.device,
            source_version: VersionId::new("source:v1")?,
            health: Health::Unknown,
            observations: vec![],
        }],
        CaptureLimits::default(),
    )?;
    let device = manifest.devices().first().ok_or("missing device")?;
    assert!(device.recorded_coverage().is_empty());
    assert_eq!(device.gaps(), &[trigger.requested]);
    assert_eq!(
        evaluator.push(sample(24, None)?)?.decision,
        Decision::Missing
    );
    assert_eq!(
        evaluator.push(sample(50, Some(31.0))?)?.decision,
        Decision::Latched
    );
    assert_eq!(
        evaluator.push(sample(51, Some(29.0))?)?.decision,
        Decision::Latched
    );
    assert_eq!(
        evaluator.push(sample(52, Some(28.0))?)?.decision,
        Decision::BelowThreshold
    );
    assert_eq!(
        evaluator.push(sample(53, Some(30.0))?)?.decision,
        Decision::Pending { samples: 1 }
    );
    Ok(())
}

#[test]
fn refusals_preserve_state_and_origin() -> Result<(), Box<dyn std::error::Error>> {
    let mut evaluator = Evaluator::new(rule()?)?;
    evaluator.push(sample(0, Some(31.0))?)?;
    let mut wrong = sample(1, Some(31.0))?;
    wrong.origin = Origin::Observation;
    assert_eq!(evaluator.push(wrong), Err(TriggerError::WrongStream));
    assert_eq!(
        evaluator.push(sample(1, Some(f64::NAN))?),
        Err(TriggerError::NonFiniteValue)
    );
    assert_eq!(
        evaluator.push(sample(0, Some(31.0))?),
        Err(TriggerError::OutOfOrder)
    );
    assert!(matches!(
        evaluator.push(sample(1, Some(31.0))?)?.decision,
        Decision::Fired(_)
    ));
    let mut r = rule()?;
    r.consecutive_samples = 1;
    let mut edge = Evaluator::new(r)?;
    assert_eq!(
        edge.push(sample(i64::MIN + 2, Some(31.0))?),
        Err(TriggerError::ClockRange)
    );
    assert!(matches!(
        edge.push(sample(0, Some(31.0))?)?.decision,
        Decision::Fired(_)
    ));
    let mut invalid = rule()?;
    invalid.release_threshold = invalid.threshold;
    assert!(matches!(
        Evaluator::new(invalid),
        Err(TriggerError::InvalidRule)
    ));
    Ok(())
}
