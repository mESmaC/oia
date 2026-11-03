//! Session-only recreation-prompt export from the held witness account
//! (`MEDIA-04`, A10 subset of #31 over the FIELD-02 slice of #27).
//!
//! The desktop builds a [`RecreationPromptInput`] from one retained witness version: each
//! nonempty reported field and each nonempty narrative line becomes a reported detail pinned
//! to that version; fields the witness did not report stay explicit unknowns unless the user
//! ties them to a declared assumption. The merged `oia-media` kernel validates and renders the
//! portable text; this adapter never edits its label, caveat or wording. Nothing here is a
//! case record, a durable save, an observation or a generated image.

use oia_domain::{OpaqueId, VersionId};
use oia_media::recreation::{
    ApparentScale, AssetExportPolicy, ContextField, ExplicitAssumption, ExplicitUnknown,
    ExportedRecreationPrompt, FIDELITY_AND_EXTERNAL_LABEL_CAVEAT, RecreationError,
    RecreationLimits, RecreationPromptInput, ReportedDetail, SceneContext,
    VISIBLE_RECREATION_LABEL, export_recreation_prompt,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::local_tools::{Result, ToolError};
use crate::witness::{AccountVersionView, WitnessFormData, WitnessView};

/// Reported details (fields plus narrative lines) accepted in one prompt.
pub const MAX_REPORTED_DETAILS: usize = 64;
/// Declared assumptions accepted in one prompt.
pub const MAX_ASSUMPTIONS: usize = 16;
/// Declared unknowns accepted in one prompt.
pub const MAX_UNKNOWNS: usize = 16;
/// Total UTF-8 bytes of identifiers and text the kernel may charge for one prompt.
pub const MAX_TEXT_BYTES: usize = 128 * 1024;
/// Rendered portable-text budget.
pub const MAX_OUTPUT_BYTES: usize = 256 * 1024;

const PROMPT_ID: &str = "prompt:witness-session-1";
const CASE_ID: &str = "case:local-session";
const RETENTION: &str = "Memory only; discarded on exit. The prompt is a portable illustration brief, not evidence, a case record or a generated image.";
const SCALE_UNKNOWN_REASON: &str = "The witness form records apparent size as reported wording only; no range, calibration or multi-station baseline is captured.";

/// One declared assumption from the page.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssumptionInput {
    pub assumption: String,
    pub rationale: String,
}

/// One declared unknown from the page.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UnknownInput {
    pub aspect: String,
    pub impact: String,
}

/// Which declared assumption (by index) stands in for an unreported context field.
/// `None` leaves the field an explicit unknown. A reported field cannot be assumed.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ContextChoices {
    #[serde(default)]
    pub scene: Option<usize>,
    #[serde(default)]
    pub time: Option<usize>,
    #[serde(default)]
    pub viewpoint: Option<usize>,
    #[serde(default)]
    pub lighting: Option<usize>,
}

/// Page intent to build a prompt from one retained witness version.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RecreationRequest {
    pub version: String,
    pub title: String,
    #[serde(default)]
    pub assumptions: Vec<AssumptionInput>,
    #[serde(default)]
    pub unknowns: Vec<UnknownInput>,
    #[serde(default)]
    pub context: ContextChoices,
}

/// How one context field was filled, so the page can show reported, assumed and unknown apart.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct ContextRow {
    pub field: &'static str,
    pub status: &'static str,
    pub text: String,
}

/// The last validated export, with the kernel text verbatim.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct PromptView {
    pub prompt_id: String,
    pub prompt_version: String,
    pub case_id: String,
    pub title: String,
    pub source_version: String,
    pub reported_details: usize,
    pub assumptions: usize,
    pub unknowns: usize,
    pub context: Vec<ContextRow>,
    pub text: String,
    pub bytes: usize,
    pub sha256: String,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub struct LimitsView {
    pub max_reported_details: usize,
    pub max_assumptions: usize,
    pub max_unknowns: usize,
    pub max_text_bytes: usize,
    pub max_output_bytes: usize,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct RecreationView {
    pub prompt: Option<PromptView>,
    pub limits: LimitsView,
    pub visible_label: &'static str,
    pub caveat: &'static str,
    pub retention: &'static str,
}

/// Session-only holder of the last validated prompt. Independent of the file worker;
/// saving the text goes through the desktop's single-operation gate.
#[derive(Debug, Default)]
pub struct RecreationState {
    exported: Option<Exported>,
    exports: usize,
}

#[derive(Debug)]
struct Exported {
    prompt: ExportedRecreationPrompt,
    source_version: String,
    context: Vec<ContextRow>,
}

fn limits() -> Result<RecreationLimits> {
    RecreationLimits::new(
        MAX_REPORTED_DETAILS,
        MAX_ASSUMPTIONS,
        MAX_UNKNOWNS,
        8,
        1,
        MAX_TEXT_BYTES,
        MAX_OUTPUT_BYTES,
    )
    .map_err(|e| ToolError::new("internal", e))
}

fn opaque(id: String) -> Result<OpaqueId> {
    OpaqueId::new(id).map_err(|e| ToolError::new("internal", e))
}

fn map_kernel_error(error: RecreationError) -> ToolError {
    let code = match &error {
        RecreationError::EmptyReportedContent => "recreation_empty",
        RecreationError::EmptyField { .. } => "empty_field",
        RecreationError::ControlCharacterInField { .. } => "control_character",
        RecreationError::LimitExceeded { .. } | RecreationError::ZeroLimit(_) => "limit",
        RecreationError::InvalidId(_) => "invalid_version",
        RecreationError::AllocationFailed => "allocation_failed",
        _ => "internal",
    };
    let message = match &error {
        RecreationError::ControlCharacterInField { field } => format!(
            "{field} contains a control or invisible formatting character (a tab counts). Revise the account wording or the declaration; nothing was changed."
        ),
        RecreationError::EmptyField { field } => {
            format!("{field} is empty. Declarations need wording; nothing was changed.")
        }
        other => other.to_string(),
    };
    ToolError::new(code, message)
}

fn find_version<'a>(view: &'a WitnessView, version: &str) -> Result<&'a AccountVersionView> {
    let capture = view.capture.as_ref().ok_or_else(|| {
        ToolError::new(
            "capture_missing",
            "Capture a witness account before building a recreation prompt.",
        )
    })?;
    std::iter::once(&capture.initial)
        .chain(capture.revisions.iter())
        .find(|account| account.version == version)
        .ok_or_else(|| {
            ToolError::new(
                "invalid_version",
                "The chosen witness version is not held in this session. Inspect the account and retry.",
            )
        })
}

fn reported(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|text| !text.trim().is_empty())
}

/// Builds one context field: reported wording pinned to the version, the chosen declared
/// assumption, or an explicit unknown. Choosing an assumption for a reported field is refused
/// so that an assumption can never replace reported wording.
fn context_field(
    field: &'static str,
    value: Option<&str>,
    choice: Option<usize>,
    assumptions: &[AssumptionInput],
    version: &VersionId,
) -> Result<(ContextField, ContextRow)> {
    match (value, choice) {
        (Some(text), None) => Ok((
            ContextField::Reported {
                description: text.to_owned(),
                source_versions: vec![version.clone()],
            },
            ContextRow {
                field,
                status: "reported",
                text: text.to_owned(),
            },
        )),
        (Some(_), Some(_)) => Err(ToolError::new(
            "reported_field_assumed",
            format!(
                "{field} was reported by the witness; an assumption cannot replace reported wording."
            ),
        )),
        (None, Some(index)) => {
            let assumption = assumptions.get(index).ok_or_else(|| {
                ToolError::new(
                    "unknown_assumption",
                    format!("{field} refers to an assumption that is not declared."),
                )
            })?;
            Ok((
                ContextField::Assumed {
                    description: assumption.assumption.clone(),
                    assumption_id: opaque(format!("assumption:a{}", index + 1))?,
                },
                ContextRow {
                    field,
                    status: "assumed",
                    text: assumption.assumption.clone(),
                },
            ))
        }
        (None, None) => {
            let reason = format!("{field} was not reported in {}.", version.as_str());
            Ok((
                ContextField::Unknown {
                    reason: reason.clone(),
                },
                ContextRow {
                    field,
                    status: "unknown",
                    text: reason,
                },
            ))
        }
    }
}

fn field_detail(
    id: &str,
    label: &str,
    value: &Option<String>,
    version: &VersionId,
) -> Result<Option<ReportedDetail>> {
    let Some(text) = reported(value) else {
        return Ok(None);
    };
    Ok(Some(ReportedDetail {
        detail_id: opaque(format!("detail:{id}"))?,
        statement: text.to_owned(),
        source_versions: vec![version.clone()],
        source_note: Some(format!("{} · {label}", version.as_str())),
    }))
}

/// Reported details from one version: the narrative line by line (blank lines skipped,
/// numbering kept), then the free-text fields that are not context. Wording is exact.
fn reported_details(form: &WitnessFormData, version: &VersionId) -> Result<Vec<ReportedDetail>> {
    let mut details = Vec::new();
    for (index, line) in form.narrative.split('\n').enumerate() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.trim().is_empty() {
            continue;
        }
        let number = index + 1;
        details.push(ReportedDetail {
            detail_id: opaque(format!("detail:narrative-l{number}"))?,
            statement: line.to_owned(),
            source_versions: vec![version.clone()],
            source_note: Some(format!("{} · narrative line {number}", version.as_str())),
        });
    }
    for (id, label, value) in [
        ("duration", "duration", &form.duration),
        ("direction", "direction", &form.direction),
        ("motion", "motion", &form.motion),
        ("sound", "sound", &form.sound),
    ] {
        details.extend(field_detail(id, label, value, version)?);
    }
    Ok(details)
}

impl RecreationState {
    #[must_use]
    pub fn view(&self) -> RecreationView {
        RecreationView {
            prompt: self.exported.as_ref().map(|e| {
                let text = e.prompt.portable_prompt_text();
                PromptView {
                    prompt_id: e.prompt.prompt_id().as_str().to_owned(),
                    prompt_version: e.prompt.prompt_version().as_str().to_owned(),
                    case_id: e.prompt.case_id().as_str().to_owned(),
                    title: e.prompt.title().to_owned(),
                    source_version: e.source_version.clone(),
                    reported_details: e.prompt.shared_reported_details().len(),
                    assumptions: e.prompt.assumptions().len(),
                    unknowns: e.prompt.unknowns().len(),
                    context: e.context.clone(),
                    text: text.to_owned(),
                    bytes: text.len(),
                    sha256: format!("{:x}", Sha256::digest(text.as_bytes())),
                }
            }),
            limits: LimitsView {
                max_reported_details: MAX_REPORTED_DETAILS,
                max_assumptions: MAX_ASSUMPTIONS,
                max_unknowns: MAX_UNKNOWNS,
                max_text_bytes: MAX_TEXT_BYTES,
                max_output_bytes: MAX_OUTPUT_BYTES,
            },
            visible_label: VISIBLE_RECREATION_LABEL,
            caveat: FIDELITY_AND_EXTERNAL_LABEL_CAVEAT,
            retention: RETENTION,
        }
    }

    /// Validates and renders a prompt from the chosen witness version. A refused build keeps
    /// the previous prompt and consumes no prompt version.
    pub fn export(
        &mut self,
        witness: &WitnessView,
        request: &RecreationRequest,
    ) -> Result<RecreationView> {
        let account = find_version(witness, &request.version)?;
        let version = VersionId::new(request.version.as_str())
            .map_err(|_| ToolError::new("invalid_version", "The witness version is invalid."))?;
        if request.assumptions.len() > MAX_ASSUMPTIONS || request.unknowns.len() > MAX_UNKNOWNS {
            return Err(ToolError::new(
                "limit",
                format!(
                    "At most {MAX_ASSUMPTIONS} assumptions and {MAX_UNKNOWNS} unknowns can be declared."
                ),
            ));
        }
        let assumptions = request
            .assumptions
            .iter()
            .enumerate()
            .map(|(i, a)| {
                Ok(ExplicitAssumption {
                    assumption_id: opaque(format!("assumption:a{}", i + 1))?,
                    assumption: a.assumption.clone(),
                    rationale: a.rationale.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let unknowns = request
            .unknowns
            .iter()
            .enumerate()
            .map(|(i, u)| {
                Ok(ExplicitUnknown {
                    unknown_id: opaque(format!("unknown:u{}", i + 1))?,
                    aspect: u.aspect.clone(),
                    impact_on_recreation: u.impact.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let form = &account.form;
        let choices = request.context;
        let (scene, scene_row) = context_field(
            "scene",
            reported(&form.location),
            choices.scene,
            &request.assumptions,
            &version,
        )?;
        let (time_context, time_row) = context_field(
            "time",
            reported(&form.reported_time),
            choices.time,
            &request.assumptions,
            &version,
        )?;
        let (viewpoint, viewpoint_row) = context_field(
            "viewpoint",
            None,
            choices.viewpoint,
            &request.assumptions,
            &version,
        )?;
        let (lighting, lighting_row) = context_field(
            "lighting",
            reported(&form.conditions),
            choices.lighting,
            &request.assumptions,
            &version,
        )?;
        let (apparent_scale, scale_row) = match reported(&form.apparent_size) {
            Some(text) => (
                ApparentScale::ReportedAngularOrRelative {
                    description: text.to_owned(),
                    source_versions: vec![version.clone()],
                    physical_size_unknown_reason: SCALE_UNKNOWN_REASON.to_owned(),
                },
                ContextRow {
                    field: "apparent scale",
                    status: "reported",
                    text: format!("{text} · physical size unknown"),
                },
            ),
            None => {
                let reason = format!("apparent size was not reported in {}.", version.as_str());
                (
                    ApparentScale::Unknown {
                        reason: reason.clone(),
                    },
                    ContextRow {
                        field: "apparent scale",
                        status: "unknown",
                        text: reason,
                    },
                )
            }
        };
        let shared_reported_details = reported_details(form, &version)?;
        let next = self.exports + 1;
        let input = RecreationPromptInput {
            prompt_id: opaque(PROMPT_ID.to_owned())?,
            prompt_version: VersionId::new(format!("ver:recreation-{next}"))
                .map_err(|e| ToolError::new("internal", e))?,
            case_id: opaque(CASE_ID.to_owned())?,
            title: request.title.clone(),
            context: SceneContext {
                scene,
                time_context,
                viewpoint,
                lighting,
                apparent_scale,
            },
            shared_reported_details,
            conflicting_variants: Vec::new(),
            assumptions,
            unknowns,
            reference_assets: Vec::new(),
        };
        let prompt =
            export_recreation_prompt(&input, limits()?, AssetExportPolicy::RefuseUnpermitted)
                .map_err(map_kernel_error)?;
        self.exports = next;
        self.exported = Some(Exported {
            prompt,
            source_version: request.version.clone(),
            context: vec![scene_row, time_row, viewpoint_row, lighting_row, scale_row],
        });
        Ok(self.view())
    }

    /// The held prompt's text, only if it is still the version the page is looking at.
    pub fn text(&self, prompt_version: &str) -> Result<String> {
        self.exported
            .as_ref()
            .filter(|e| e.prompt.prompt_version().as_str() == prompt_version)
            .map(|e| e.prompt.portable_prompt_text().to_owned())
            .ok_or_else(|| {
                ToolError::new(
                    "stale_prompt",
                    "The prompt changed since it was shown. Build it again and retry.",
                )
            })
    }

    #[must_use]
    pub fn file_stem(&self) -> String {
        self.exported.as_ref().map_or_else(
            || "recreation-prompt".to_owned(),
            |e| format!("recreation-prompt-{}", e.source_version),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::witness::{WitnessOrigin, WitnessState};

    fn witness(narrative: &str, apparent_size: Option<&str>) -> Result<WitnessView> {
        let mut state = WitnessState::default();
        state.capture_initial(
            WitnessOrigin::Simulated,
            WitnessFormData {
                narrative: narrative.to_owned(),
                reported_time: Some("Around dusk; date uncertain".to_owned()),
                location: None,
                duration: Some("about 40 s".to_owned()),
                direction: None,
                apparent_size: apparent_size.map(str::to_owned),
                motion: None,
                sound: None,
                conditions: None,
            },
        )
    }

    /// MEDIA-04 adapter slice: reported wording is pinned verbatim to the witness version,
    /// unreported fields stay unknown and cannot be overwritten by an assumption, a refused
    /// build keeps the previous prompt, and saving a superseded prompt is refused.
    #[test]
    fn prompt_keeps_reported_assumed_and_unknown_apart() -> Result<()> {
        let view = witness(
            "Two lights\r\n\n  moved west",
            Some("thumb-width at arm's length"),
        )?;
        let mut state = RecreationState::default();
        let request = RecreationRequest {
            version: "witness-v1".to_owned(),
            title: "Illustrative recreation".to_owned(),
            assumptions: vec![AssumptionInput {
                assumption: "Clear sky".to_owned(),
                rationale: "Needed for framing; not reported".to_owned(),
            }],
            unknowns: vec![UnknownInput {
                aspect: "Range".to_owned(),
                impact: "Do not read depth as distance".to_owned(),
            }],
            context: ContextChoices {
                lighting: Some(0),
                ..ContextChoices::default()
            },
        };
        let prompt = state
            .export(&view, &request)?
            .prompt
            .ok_or_else(|| ToolError::new("internal", "prompt missing after export"))?;
        assert_eq!(prompt.source_version, "witness-v1");
        assert_eq!(prompt.reported_details, 3); // two narrative lines + duration; the blank line is skipped
        assert_eq!(prompt.prompt_version, "ver:recreation-1");
        let statuses: Vec<_> = prompt.context.iter().map(|c| (c.field, c.status)).collect();
        assert_eq!(
            statuses,
            vec![
                ("scene", "unknown"),
                ("time", "reported"),
                ("viewpoint", "unknown"),
                ("lighting", "assumed"),
                ("apparent scale", "reported"),
            ]
        );
        assert!(
            prompt
                .text
                .starts_with(&format!("# {VISIBLE_RECREATION_LABEL}"))
        );
        assert!(prompt.text.contains("\"  moved west\""));
        assert!(prompt.text.contains("witness-v1 · narrative line 3"));
        assert!(prompt.text.contains(SCALE_UNKNOWN_REASON));
        assert_eq!(prompt.bytes, prompt.text.len());

        // An assumption may not replace reported wording; the held prompt is unchanged.
        let refused = state.export(
            &view,
            &RecreationRequest {
                context: ContextChoices {
                    time: Some(0),
                    ..ContextChoices::default()
                },
                ..request.clone()
            },
        );
        assert_eq!(
            refused.err().map(|e| e.code),
            Some("reported_field_assumed")
        );
        let tab = state.export(
            &view,
            &RecreationRequest {
                title: "tab\there".to_owned(),
                ..request.clone()
            },
        );
        assert_eq!(tab.err().map(|e| e.code), Some("control_character"));
        let stale = state.export(
            &view,
            &RecreationRequest {
                version: "witness-v9".to_owned(),
                ..request.clone()
            },
        );
        assert_eq!(stale.err().map(|e| e.code), Some("invalid_version"));
        assert_eq!(
            state.view().prompt.map(|p| p.prompt_version),
            Some("ver:recreation-1".to_owned())
        );
        assert_eq!(state.text("ver:recreation-1")?, prompt.text);
        assert_eq!(
            state.text("ver:recreation-0").err().map(|e| e.code),
            Some("stale_prompt")
        );
        Ok(())
    }
}
