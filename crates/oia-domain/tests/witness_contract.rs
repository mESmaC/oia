use oia_domain::witness::{
    AccountOrigin, CaptureError, MAX_FIELD_BYTES, MAX_VERSIONS, WitnessCapture, WitnessForm,
};
use oia_domain::{ActorId, OpaqueId, VersionId};

fn form(narrative: &str) -> WitnessForm {
    WitnessForm {
        narrative: narrative.into(),
        reported_time: Some("Around dusk; date uncertain".into()),
        location: None,
        duration: None,
        direction: None,
        apparent_size: None,
        motion: None,
        sound: None,
        conditions: None,
    }
}

/// FIELD-02/A13 slice: exact original bytes and revision/exposure ordering survive
/// edits, duplicate identifiers and stale writes. This is simulated testimony.
#[test]
fn original_and_exposure_history_are_preserved() -> Result<(), Box<dyn std::error::Error>> {
    let actor = ActorId::new("fixture-author")?;
    let v1 = VersionId::new("v1")?;
    let v2 = VersionId::new("v2")?;
    let v3 = VersionId::new("v3")?;
    let original = form("  Two lights.\r\nDirection unknown. 🛸");
    let mut capture = WitnessCapture::new(
        OpaqueId::new("account-1")?,
        AccountOrigin::Simulated,
        v1.clone(),
        actor.clone(),
        original.clone(),
    )?;
    assert_eq!(capture.first_exposure_version(), None);
    capture.revise(
        &v1,
        v2.clone(),
        actor.clone(),
        form("Perhaps three lights"),
        "Recalled another light".into(),
    )?;
    assert!(!capture.latest().after_explanations());
    assert_eq!(
        capture.expose_explanations(&v1),
        Err(CaptureError::StaleVersion)
    );
    assert_eq!(capture.first_exposure_version(), None);
    capture.expose_explanations(&v2)?;
    capture.revise(
        &v2,
        v3.clone(),
        actor.clone(),
        form("Possibly aircraft"),
        "After seeing candidate explanations".into(),
    )?;
    capture.expose_explanations(&v3)?;
    assert_eq!(capture.first_exposure_version(), Some(&v2));
    assert!(capture.latest().after_explanations());
    assert_eq!(capture.latest().actor(), &actor);
    assert_eq!(
        capture.latest().rationale(),
        Some("After seeing candidate explanations")
    );
    assert_eq!(capture.initial().form(), &original);
    assert_eq!(capture.initial().form().location, None);
    assert_eq!(capture.origin(), AccountOrigin::Simulated);
    assert_eq!(capture.revisions().len(), 2);
    let before = format!("{capture:?}");
    assert_eq!(
        capture.revise(
            &v2,
            VersionId::new("v4")?,
            actor.clone(),
            form("stale"),
            "stale".into()
        ),
        Err(CaptureError::StaleVersion)
    );
    assert_eq!(
        capture.revise(&v3, v1, actor, form("replacement"), "replacement".into()),
        Err(CaptureError::DuplicateVersion)
    );
    assert_eq!(format!("{capture:?}"), before);
    Ok(())
}

/// Bounds and malformed fields cannot partially mutate or silently trim history.
#[test]
fn invalid_and_over_budget_edits_leave_capture_intact() -> Result<(), Box<dyn std::error::Error>> {
    let actor = ActorId::new("fixture-author")?;
    let v1 = VersionId::new("v1")?;
    let mut capture = WitnessCapture::new(
        OpaqueId::new("account-1")?,
        AccountOrigin::Reported,
        v1.clone(),
        actor.clone(),
        form("Initial"),
    )?;
    let mut invalid = form("Revised");
    invalid.sound = Some(" ".into());
    assert_eq!(
        capture.revise(
            &v1,
            VersionId::new("v2")?,
            actor.clone(),
            invalid,
            "Correction".into()
        ),
        Err(CaptureError::EmptyField("sound"))
    );
    assert_eq!(
        capture.revise(
            &v1,
            VersionId::new("v2")?,
            actor.clone(),
            form("\u{1b}[31m"),
            "Correction".into()
        ),
        Err(CaptureError::ControlCharacter("narrative"))
    );
    assert_eq!(
        capture.revise(
            &v1,
            VersionId::new("v2")?,
            actor.clone(),
            form(&"é".repeat(MAX_FIELD_BYTES / 2 + 1)),
            "Correction".into()
        ),
        Err(CaptureError::FieldTooLong("narrative"))
    );
    assert_eq!(capture.latest().version(), &v1);
    assert!(capture.revisions().is_empty());
    for index in 1..MAX_VERSIONS {
        let expected = capture.latest().version().clone();
        capture.revise(
            &expected,
            VersionId::new(format!("revision-{index}"))?,
            actor.clone(),
            form("Revised"),
            "Correction".into(),
        )?;
    }
    let expected = capture.latest().version().clone();
    assert_eq!(
        capture.revise(
            &expected,
            VersionId::new("overflow")?,
            actor,
            form("Overflow"),
            "Correction".into()
        ),
        Err(CaptureError::VersionLimit)
    );
    assert_eq!(capture.revisions().len(), MAX_VERSIONS - 1);
    assert_eq!(capture.initial().form().narrative, "Initial");
    Ok(())
}
