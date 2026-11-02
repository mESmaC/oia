//! Synthetic offline demonstration; no file, network or persistence operations.
use oia_domain::witness::{AccountOrigin, WitnessCapture, WitnessForm};
use oia_domain::{ActorId, OpaqueId, VersionId};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let actor = ActorId::new("fixture-author")?;
    let version = VersionId::new("account-v1")?;
    let mut capture = WitnessCapture::new(
        OpaqueId::new("synthetic-account")?,
        AccountOrigin::Simulated,
        version.clone(),
        actor.clone(),
        WitnessForm {
            narrative: "Two lights near the horizon".into(),
            reported_time: Some("Around dusk; date uncertain".into()),
            location: None,
            duration: None,
            direction: None,
            apparent_size: None,
            motion: None,
            sound: None,
            conditions: None,
        },
    )?;
    println!("SIMULATED fixture, in-memory only");
    println!("Initial: {}", capture.initial().form().narrative);
    capture.expose_explanations(&version)?;
    let mut revision = capture.initial().form().clone();
    revision.narrative = "The lights might have been aircraft".into();
    capture.revise(
        &version,
        VersionId::new("account-v2")?,
        actor,
        revision,
        "Recollection after viewing an explanation".into(),
    )?;
    println!("Revised: {}", capture.latest().form().narrative);
    println!("Original retained: {}", capture.initial().form().narrative);
    println!(
        "After in-workflow explanations: {}",
        capture.latest().after_explanations()
    );
    println!("Reported location: {:?}", capture.latest().form().location);
    Ok(())
}
