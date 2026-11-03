//! Session-only witness capture adapter over `oia-domain` (FIELD-02, #27).
//!
//! The desktop owns identifiers and transitions. This is not a case record or a
//! durable save; persistence, authorization and capture timestamps remain open.

use oia_domain::witness::{
    AccountOrigin, AccountVersion, CaptureError, MAX_FIELD_BYTES, MAX_VERSIONS, WitnessCapture,
    WitnessForm,
};
use oia_domain::{ActorId, OpaqueId, VersionId};
use serde::{Deserialize, Serialize};

use crate::local_tools::{Result, ToolError};

const ACTOR_ID: &str = "local-session";
const CAPTURE_ID: &str = "witness-session-1";
const RETENTION: &str = "Memory only; discarded on exit. Capture is not a durable save.";
const EXPOSURE_QUALIFICATION: &str =
    "After-explanations markers cover only this in-memory workflow, not exposure elsewhere.";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WitnessOrigin {
    Reported,
    Simulated,
}

impl From<WitnessOrigin> for AccountOrigin {
    fn from(origin: WitnessOrigin) -> Self {
        match origin {
            WitnessOrigin::Reported => Self::Reported,
            WitnessOrigin::Simulated => Self::Simulated,
        }
    }
}

impl From<AccountOrigin> for WitnessOrigin {
    fn from(origin: AccountOrigin) -> Self {
        match origin {
            AccountOrigin::Reported => Self::Reported,
            AccountOrigin::Simulated => Self::Simulated,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WitnessFormData {
    pub narrative: String,
    pub reported_time: Option<String>,
    pub location: Option<String>,
    pub duration: Option<String>,
    pub direction: Option<String>,
    pub apparent_size: Option<String>,
    pub motion: Option<String>,
    pub sound: Option<String>,
    pub conditions: Option<String>,
}

impl From<WitnessFormData> for WitnessForm {
    fn from(form: WitnessFormData) -> Self {
        Self {
            narrative: form.narrative,
            reported_time: form.reported_time,
            location: form.location,
            duration: form.duration,
            direction: form.direction,
            apparent_size: form.apparent_size,
            motion: form.motion,
            sound: form.sound,
            conditions: form.conditions,
        }
    }
}

impl From<&WitnessForm> for WitnessFormData {
    fn from(form: &WitnessForm) -> Self {
        Self {
            narrative: form.narrative.clone(),
            reported_time: form.reported_time.clone(),
            location: form.location.clone(),
            duration: form.duration.clone(),
            direction: form.direction.clone(),
            apparent_size: form.apparent_size.clone(),
            motion: form.motion.clone(),
            sound: form.sound.clone(),
            conditions: form.conditions.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct AccountVersionView {
    pub version: String,
    pub actor: String,
    pub form: WitnessFormData,
    pub rationale: Option<String>,
    pub after_explanations: bool,
}

impl From<&AccountVersion> for AccountVersionView {
    fn from(version: &AccountVersion) -> Self {
        Self {
            version: version.version().as_str().to_owned(),
            actor: version.actor().as_str().to_owned(),
            form: version.form().into(),
            rationale: version.rationale().map(str::to_owned),
            after_explanations: version.after_explanations(),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct WitnessCaptureView {
    pub id: String,
    pub origin: WitnessOrigin,
    pub initial: AccountVersionView,
    pub latest: AccountVersionView,
    pub revisions: Vec<AccountVersionView>,
    pub first_exposure_version: Option<String>,
}

impl From<&WitnessCapture> for WitnessCaptureView {
    fn from(capture: &WitnessCapture) -> Self {
        Self {
            id: capture.id().as_str().to_owned(),
            origin: capture.origin().into(),
            initial: capture.initial().into(),
            latest: capture.latest().into(),
            revisions: capture.revisions().iter().map(Into::into).collect(),
            first_exposure_version: capture
                .first_exposure_version()
                .map(|version| version.as_str().to_owned()),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct WitnessView {
    pub capture: Option<WitnessCaptureView>,
    pub max_field_bytes: usize,
    pub max_versions: usize,
    pub retention: &'static str,
    pub exposure_qualification: &'static str,
}

#[derive(Debug, Default)]
pub struct WitnessState {
    capture: Option<WitnessCapture>,
}

fn map_capture_error(error: CaptureError) -> ToolError {
    let code = match error {
        CaptureError::EmptyField(_) => "empty_field",
        CaptureError::FieldTooLong(_) => "field_too_long",
        CaptureError::ControlCharacter(_) => "control_character",
        CaptureError::StaleVersion => "stale_version",
        CaptureError::DuplicateVersion => "duplicate_version",
        CaptureError::VersionLimit => "version_limit",
        CaptureError::AllocationFailed => "allocation_failed",
    };
    ToolError::new(code, error)
}

fn actor() -> Result<ActorId> {
    ActorId::new(ACTOR_ID).map_err(|_| {
        ToolError::new(
            "internal",
            "The local witness actor identifier is invalid. Restart OIA.",
        )
    })
}

fn version(number: usize) -> Result<VersionId> {
    VersionId::new(format!("witness-v{number}")).map_err(|_| {
        ToolError::new(
            "internal",
            "The local witness version identifier is invalid. Restart OIA.",
        )
    })
}

fn expected_version(expected: &str) -> Result<VersionId> {
    VersionId::new(expected).map_err(|_| {
        ToolError::new(
            "invalid_version",
            "The expected witness version is invalid. Inspect the latest account and retry.",
        )
    })
}

impl WitnessState {
    #[must_use]
    pub fn view(&self) -> WitnessView {
        WitnessView {
            capture: self.capture.as_ref().map(Into::into),
            max_field_bytes: MAX_FIELD_BYTES,
            max_versions: MAX_VERSIONS,
            retention: RETENTION,
            exposure_qualification: EXPOSURE_QUALIFICATION,
        }
    }

    pub fn capture_initial(
        &mut self,
        origin: WitnessOrigin,
        form: WitnessFormData,
    ) -> Result<WitnessView> {
        if self.capture.is_some() {
            return Err(ToolError::new(
                "capture_exists",
                "An initial witness account already exists. Revise it instead of replacing it.",
            ));
        }
        let id = OpaqueId::new(CAPTURE_ID).map_err(|_| {
            ToolError::new(
                "internal",
                "The local witness capture identifier is invalid. Restart OIA.",
            )
        })?;
        let capture = WitnessCapture::new(id, origin.into(), version(1)?, actor()?, form.into())
            .map_err(map_capture_error)?;
        self.capture = Some(capture);
        Ok(self.view())
    }

    pub fn revise(
        &mut self,
        expected: &str,
        form: WitnessFormData,
        rationale: String,
    ) -> Result<WitnessView> {
        let expected = expected_version(expected)?;
        let capture = self.capture.as_mut().ok_or_else(|| {
            ToolError::new(
                "capture_missing",
                "Capture an initial witness account before recording a revision.",
            )
        })?;
        // Derived from retained history, so a rejected revision consumes no identifier.
        let next = version(capture.revisions().len() + 2)?;
        capture
            .revise(&expected, next, actor()?, form.into(), rationale)
            .map_err(map_capture_error)?;
        Ok(self.view())
    }

    pub fn expose_explanations(&mut self, expected: &str) -> Result<WitnessView> {
        let expected = expected_version(expected)?;
        let capture = self.capture.as_mut().ok_or_else(|| {
            ToolError::new(
                "capture_missing",
                "Capture an initial witness account before exposing explanations.",
            )
        })?;
        capture
            .expose_explanations(&expected)
            .map_err(map_capture_error)?;
        Ok(self.view())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(narrative: &str) -> WitnessFormData {
        WitnessFormData {
            narrative: narrative.to_owned(),
            reported_time: Some("Around dusk; date uncertain".to_owned()),
            location: None,
            duration: None,
            direction: None,
            apparent_size: None,
            motion: None,
            sound: None,
            conditions: None,
        }
    }

    /// FIELD-02/A13 adapter slice: generated versions do not advance on rejected
    /// edits, and the initial account and exposure ordering survive the DTO path.
    #[test]
    fn adapter_preserves_initial_and_rejected_revision_versions() -> Result<()> {
        let mut state = WitnessState::default();
        let view = state.capture_initial(WitnessOrigin::Simulated, form("  Initial\r\ntext"))?;
        let capture = view.capture.as_ref().ok_or_else(|| {
            ToolError::new("internal", "capture view missing after initial capture")
        })?;
        assert_eq!(capture.latest.version, "witness-v1");
        assert_eq!(capture.initial.form.narrative, "  Initial\r\ntext");
        assert_eq!(capture.initial.form.location, None);
        assert!(
            state
                .capture_initial(WitnessOrigin::Reported, form("replacement"))
                .is_err()
        );

        let stale = state.expose_explanations("witness-v99");
        assert_eq!(stale.err().map(|error| error.code), Some("stale_version"));
        assert!(
            state
                .revise("witness-v1", form(" "), "blank".to_owned())
                .is_err()
        );
        let view = state.expose_explanations("witness-v1")?;
        assert_eq!(
            view.capture
                .as_ref()
                .and_then(|capture| capture.first_exposure_version.as_deref()),
            Some("witness-v1")
        );

        let view = state.revise(
            "witness-v1",
            form("Revised"),
            "Clarified recollection".to_owned(),
        )?;
        let capture = view
            .capture
            .as_ref()
            .ok_or_else(|| ToolError::new("internal", "capture view missing after revision"))?;
        assert_eq!(capture.latest.version, "witness-v2");
        assert!(capture.latest.after_explanations);
        assert_eq!(capture.initial.form.narrative, "  Initial\r\ntext");
        assert_eq!(capture.revisions.len(), 1);
        assert_eq!(capture.origin, WitnessOrigin::Simulated);

        let stale = state.revise("witness-v1", form("Stale"), "stale".to_owned());
        assert_eq!(stale.err().map(|error| error.code), Some("stale_version"));
        let capture = state.view().capture.ok_or_else(|| {
            ToolError::new("internal", "capture view missing after stale revision")
        })?;
        assert_eq!(capture.latest.version, "witness-v2");
        assert_eq!(capture.revisions.len(), 1);
        Ok(())
    }
}
