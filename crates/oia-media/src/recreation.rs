//! Portable illustrative recreation-prompt export and imported-recreation labeling (`MEDIA-04`, `A10`).
//!
//! Exports editable, portable image-generation prompts that keep reported details,
//! explicit assumptions, unknowns, and conflicting witness/source variants separate,
//! and validates imported recreations so generated imagery retains its prompt/settings
//! lineage and visible `ILLUSTRATIVE RECREATION` labeling without ever being promoted
//! to observation evidence (`docs/spec.md` `MEDIA-04`, `docs/architecture.md` §§2.2–2.3, §8).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write as _};

use oia_domain::{IdError, OpaqueId, SchemaVersion, VersionId};

/// Mandatory visible label applied to exported recreation prompts and imported recreations.
pub const VISIBLE_RECREATION_LABEL: &str =
    "ILLUSTRATIVE RECREATION — GENERATED FROM PROMPT (NOT OBSERVATION EVIDENCE)";

/// Mandatory caveat explaining that prompt wording alone neither proves physical fidelity
/// nor guarantees that an external image generator embeds provenance labels (`MEDIA-04`).
pub const FIDELITY_AND_EXTERNAL_LABEL_CAVEAT: &str = "Prompt wording organizes reported details, assumptions, unknowns, and conflicting variants for an illustrative recreation; it does not establish physical fidelity or guarantee that an external image generator preserves labeling.";

/// Which budget was zero or exceeded during recreation-prompt validation or rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecreationLimitKind {
    ReportedDetails,
    Assumptions,
    Unknowns,
    Variants,
    Assets,
    TextBytes,
    OutputBytes,
}

impl fmt::Display for RecreationLimitKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReportedDetails => f.write_str("max_reported_details"),
            Self::Assumptions => f.write_str("max_assumptions"),
            Self::Unknowns => f.write_str("max_unknowns"),
            Self::Variants => f.write_str("max_variants"),
            Self::Assets => f.write_str("max_assets"),
            Self::TextBytes => f.write_str("max_text_bytes"),
            Self::OutputBytes => f.write_str("max_output_bytes"),
        }
    }
}

/// Explicit resource bounds for validating and rendering a recreation prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecreationLimits {
    max_reported_details: usize,
    max_assumptions: usize,
    max_unknowns: usize,
    max_variants: usize,
    max_assets: usize,
    max_text_bytes: usize,
    max_output_bytes: usize,
}

impl RecreationLimits {
    /// Validates that all recreation-prompt budgets are non-zero.
    pub fn new(
        max_reported_details: usize,
        max_assumptions: usize,
        max_unknowns: usize,
        max_variants: usize,
        max_assets: usize,
        max_text_bytes: usize,
        max_output_bytes: usize,
    ) -> Result<Self, RecreationError> {
        if max_reported_details == 0 {
            return Err(RecreationError::ZeroLimit(
                RecreationLimitKind::ReportedDetails,
            ));
        }
        if max_assumptions == 0 {
            return Err(RecreationError::ZeroLimit(RecreationLimitKind::Assumptions));
        }
        if max_unknowns == 0 {
            return Err(RecreationError::ZeroLimit(RecreationLimitKind::Unknowns));
        }
        if max_variants == 0 {
            return Err(RecreationError::ZeroLimit(RecreationLimitKind::Variants));
        }
        if max_assets == 0 {
            return Err(RecreationError::ZeroLimit(RecreationLimitKind::Assets));
        }
        if max_text_bytes == 0 {
            return Err(RecreationError::ZeroLimit(RecreationLimitKind::TextBytes));
        }
        if max_output_bytes == 0 {
            return Err(RecreationError::ZeroLimit(RecreationLimitKind::OutputBytes));
        }
        Ok(Self {
            max_reported_details,
            max_assumptions,
            max_unknowns,
            max_variants,
            max_assets,
            max_text_bytes,
            max_output_bytes,
        })
    }

    #[must_use]
    pub const fn max_reported_details(self) -> usize {
        self.max_reported_details
    }

    #[must_use]
    pub const fn max_assumptions(self) -> usize {
        self.max_assumptions
    }

    #[must_use]
    pub const fn max_unknowns(self) -> usize {
        self.max_unknowns
    }

    #[must_use]
    pub const fn max_variants(self) -> usize {
        self.max_variants
    }

    #[must_use]
    pub const fn max_assets(self) -> usize {
        self.max_assets
    }

    #[must_use]
    pub const fn max_text_bytes(self) -> usize {
        self.max_text_bytes
    }

    #[must_use]
    pub const fn max_output_bytes(self) -> usize {
        self.max_output_bytes
    }
}

/// How unpermitted reference assets are handled when exporting a recreation prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetExportPolicy {
    /// Refuse the entire export if any selected reference asset lacks redistribution permission.
    RefuseUnpermitted,
    /// Omit unpermitted assets from the exported bundle and record an explicit omission notice.
    OmitWithNotice,
}

/// Origin/evidentiary class attempted for a recreation artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptedArtifactClass {
    /// Required class for any generated image imported from a recreation prompt.
    IllustrativeRecreation,
    /// Forbidden promotion: generated recreation presented as an observation.
    ObservationEvidence,
    /// Forbidden promotion: generated recreation presented as a source assertion.
    SourceAssertion,
    /// Forbidden promotion: generated recreation presented as a physical measurement.
    CalibratedMeasurement,
}

impl fmt::Display for AttemptedArtifactClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IllustrativeRecreation => f.write_str("illustrative_recreation"),
            Self::ObservationEvidence => f.write_str("observation_evidence"),
            Self::SourceAssertion => f.write_str("source_assertion"),
            Self::CalibratedMeasurement => f.write_str("calibrated_measurement"),
        }
    }
}

/// Errors returned when constructing, exporting, or importing a recreation prompt or artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecreationError {
    InvalidId(IdError),
    ZeroLimit(RecreationLimitKind),
    LimitExceeded {
        kind: RecreationLimitKind,
        actual: usize,
        max: usize,
    },
    EmptyField {
        field: &'static str,
    },
    ControlCharacterInField {
        field: &'static str,
    },
    DuplicateId {
        kind: &'static str,
        id: OpaqueId,
    },
    DuplicateSourceVersion {
        owner_id: OpaqueId,
        version: VersionId,
    },
    EmptyReportedContent,
    MissingSourceVersions {
        item_id: OpaqueId,
    },
    UnknownAssumptionReference {
        field: &'static str,
        assumption_id: OpaqueId,
    },
    InvalidVariantConflict {
        variant_id: OpaqueId,
        target_id: OpaqueId,
        reason: &'static str,
    },
    InvalidSha256Hex {
        item_id: OpaqueId,
    },
    UnpermittedAsset {
        asset_id: OpaqueId,
    },
    PromptVersionMismatch {
        expected_prompt_id: OpaqueId,
        expected_version: VersionId,
        actual_prompt_id: OpaqueId,
        actual_version: VersionId,
    },
    UnknownVariantReference {
        variant_id: OpaqueId,
    },
    ForbiddenEvidencePromotion {
        artifact_id: OpaqueId,
        attempted: AttemptedArtifactClass,
    },
    MissingVisibleRecreationLabel {
        artifact_id: OpaqueId,
    },
}

impl fmt::Display for RecreationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId(err) => write!(f, "invalid identifier: {err}"),
            Self::ZeroLimit(kind) => write!(f, "recreation limit {kind} must be >= 1"),
            Self::LimitExceeded { kind, actual, max } => {
                write!(f, "{kind} count/size {actual} exceeds limit {max}")
            }
            Self::EmptyField { field } => write!(f, "field `{field}` must not be empty"),
            Self::ControlCharacterInField { field } => {
                write!(f, "field `{field}` contains disallowed control characters")
            }
            Self::DuplicateId { kind, id } => write!(f, "duplicate {kind} identifier `{id}`"),
            Self::DuplicateSourceVersion { owner_id, version } => {
                write!(
                    f,
                    "duplicate source version `{version}` on item `{owner_id}`"
                )
            }
            Self::EmptyReportedContent => {
                write!(
                    f,
                    "recreation prompt requires at least one reported detail or account variant"
                )
            }
            Self::MissingSourceVersions { item_id } => {
                write!(
                    f,
                    "reported item `{item_id}` must cite at least one source version"
                )
            }
            Self::UnknownAssumptionReference {
                field,
                assumption_id,
            } => {
                write!(
                    f,
                    "scene field `{field}` references undeclared assumption `{assumption_id}`"
                )
            }
            Self::InvalidVariantConflict {
                variant_id,
                target_id,
                reason,
            } => {
                write!(
                    f,
                    "variant `{variant_id}` has invalid conflict reference `{target_id}`: {reason}"
                )
            }
            Self::InvalidSha256Hex { item_id } => {
                write!(
                    f,
                    "item `{item_id}` must carry a 64-character lowercase hex SHA-256 digest"
                )
            }
            Self::UnpermittedAsset { asset_id } => {
                write!(
                    f,
                    "reference asset `{asset_id}` is not permitted for redistribution"
                )
            }
            Self::PromptVersionMismatch {
                expected_prompt_id,
                expected_version,
                actual_prompt_id,
                actual_version,
            } => {
                write!(
                    f,
                    "imported recreation references prompt `{actual_prompt_id}`@{actual_version}, expected `{expected_prompt_id}`@{expected_version}"
                )
            }
            Self::UnknownVariantReference { variant_id } => {
                write!(
                    f,
                    "imported recreation references unknown variant `{variant_id}`"
                )
            }
            Self::ForbiddenEvidencePromotion {
                artifact_id,
                attempted,
            } => {
                write!(
                    f,
                    "recreation artifact `{artifact_id}` cannot be promoted to `{attempted}`; recreations remain illustrative only"
                )
            }
            Self::MissingVisibleRecreationLabel { artifact_id } => {
                write!(
                    f,
                    "recreation artifact `{artifact_id}` must preserve the visible recreation label"
                )
            }
        }
    }
}

impl std::error::Error for RecreationError {}

impl From<IdError> for RecreationError {
    fn from(value: IdError) -> Self {
        Self::InvalidId(value)
    }
}

/// A scene, time, viewpoint, or lighting attribute that keeps reported facts,
/// declared assumptions, and explicit unknowns distinct (`MEDIA-04`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextField {
    /// Grounded in one or more pinned source versions.
    Reported {
        description: String,
        source_versions: Vec<VersionId>,
    },
    /// Chosen for illustrative framing; tied to a declared [`ExplicitAssumption`].
    Assumed {
        description: String,
        assumption_id: OpaqueId,
    },
    /// Not established by sources; preserved as an explicit unknown rather than defaulted.
    Unknown { reason: String },
}

/// Apparent or calibrated scale status for a recreation (`MEDIA-04`, `DEVICE-02`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApparentScale {
    /// Only angular/relative appearance is reported; true range and physical dimensions remain unknown.
    ReportedAngularOrRelative {
        description: String,
        source_versions: Vec<VersionId>,
        physical_size_unknown_reason: String,
    },
    /// Derived from a pinned calibration/reconstruction version with explicit uncertainty bounds.
    CalibratedEstimate {
        description: String,
        calibration_version: VersionId,
        uncertainty_bounds: String,
    },
    /// Scale and distance are unknown; no default size or range is invented.
    Unknown { reason: String },
}

/// Scene, time, viewpoint, lighting, and apparent-scale context for a recreation prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneContext {
    pub scene: ContextField,
    pub time_context: ContextField,
    pub viewpoint: ContextField,
    pub lighting: ContextField,
    pub apparent_scale: ApparentScale,
}

/// A specific detail reported by one or more pinned source versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportedDetail {
    pub detail_id: OpaqueId,
    pub statement: String,
    pub source_versions: Vec<VersionId>,
    pub source_note: Option<String>,
}

/// An explicit assumption introduced for illustrative composition when sources are silent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplicitAssumption {
    pub assumption_id: OpaqueId,
    pub assumption: String,
    pub rationale: String,
}

/// An aspect that remains unknown and must not be mistaken for an observed property.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplicitUnknown {
    pub unknown_id: OpaqueId,
    pub aspect: String,
    pub impact_on_recreation: String,
}

/// One competing witness/source variant when accounts disagree (`MEDIA-04`, `A10`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountVariant {
    pub variant_id: OpaqueId,
    pub summary: String,
    pub reported_details: Vec<ReportedDetail>,
    pub conflicts_with: Vec<OpaqueId>,
}

/// Reference asset selected to accompany a recreation prompt (`MEDIA-04`, `A10`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceAssetInput {
    pub asset_id: OpaqueId,
    pub source_version: VersionId,
    pub sha256_hex: String,
    pub media_type: String,
    pub role: String,
    pub redistribution_permitted: bool,
    pub usage_conditions: String,
}

/// Permitted reference asset included in an exported recreation-prompt manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermittedReferenceAsset {
    pub asset_id: OpaqueId,
    pub source_version: VersionId,
    pub sha256_hex: String,
    pub media_type: String,
    pub role: String,
    pub usage_conditions: String,
}

/// Explicit omission notice recorded when a reference asset is excluded by policy (`AGENTS.md` §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OmittedReferenceAsset {
    pub asset_id: OpaqueId,
    pub source_version: VersionId,
    pub reason: String,
    pub usage_conditions: String,
}

/// Input specification for building and exporting an illustrative recreation prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecreationPromptInput {
    pub prompt_id: OpaqueId,
    pub prompt_version: VersionId,
    pub case_id: OpaqueId,
    pub title: String,
    pub context: SceneContext,
    pub shared_reported_details: Vec<ReportedDetail>,
    pub conflicting_variants: Vec<AccountVariant>,
    pub assumptions: Vec<ExplicitAssumption>,
    pub unknowns: Vec<ExplicitUnknown>,
    pub reference_assets: Vec<ReferenceAssetInput>,
}

/// Validated, portable recreation-prompt export with its editable text representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportedRecreationPrompt {
    pub schema_version: SchemaVersion,
    pub prompt_id: OpaqueId,
    pub prompt_version: VersionId,
    pub case_id: OpaqueId,
    pub visible_label: &'static str,
    pub fidelity_caveat: &'static str,
    pub title: String,
    pub context: SceneContext,
    pub shared_reported_details: Vec<ReportedDetail>,
    pub conflicting_variants: Vec<AccountVariant>,
    pub assumptions: Vec<ExplicitAssumption>,
    pub unknowns: Vec<ExplicitUnknown>,
    pub permitted_assets: Vec<PermittedReferenceAsset>,
    pub omitted_assets: Vec<OmittedReferenceAsset>,
    pub portable_prompt_text: String,
}

/// Input for registering an external image generated from a recreation prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedRecreationInput {
    pub artifact_id: OpaqueId,
    pub artifact_version: VersionId,
    pub sha256_hex: String,
    pub media_type: String,
    pub prompt_id: OpaqueId,
    pub prompt_version: VersionId,
    pub variant_id: Option<OpaqueId>,
    pub generator_model: String,
    pub generator_settings: String,
    pub attempted_class: AttemptedArtifactClass,
    pub visible_label: String,
}

/// Validated imported recreation artifact preserving prompt lineage and non-evidentiary status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedImportedRecreation {
    pub schema_version: SchemaVersion,
    pub artifact_id: OpaqueId,
    pub artifact_version: VersionId,
    pub sha256_hex: String,
    pub media_type: String,
    pub prompt_id: OpaqueId,
    pub prompt_version: VersionId,
    pub variant_id: Option<OpaqueId>,
    pub generator_model: String,
    pub generator_settings: String,
    pub visible_label: String,
    pub fidelity_caveat: &'static str,
    pub can_serve_as_observation_evidence: bool,
    pub can_establish_physical_measurement: bool,
}

fn validate_single_line(
    value: &str,
    field: &'static str,
    text_budget: &mut usize,
    max_text_bytes: usize,
) -> Result<(), RecreationError> {
    if value.trim().is_empty() {
        return Err(RecreationError::EmptyField { field });
    }
    if value.chars().any(char::is_control) {
        return Err(RecreationError::ControlCharacterInField { field });
    }
    *text_budget = text_budget.saturating_add(value.len());
    if *text_budget > max_text_bytes {
        return Err(RecreationError::LimitExceeded {
            kind: RecreationLimitKind::TextBytes,
            actual: *text_budget,
            max: max_text_bytes,
        });
    }
    Ok(())
}

fn validate_sha256_hex(hex: &str, item_id: &OpaqueId) -> Result<(), RecreationError> {
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(RecreationError::InvalidSha256Hex {
            item_id: item_id.clone(),
        });
    }
    Ok(())
}

fn validate_source_versions(
    owner_id: &OpaqueId,
    versions: &[VersionId],
) -> Result<(), RecreationError> {
    if versions.is_empty() {
        return Err(RecreationError::MissingSourceVersions {
            item_id: owner_id.clone(),
        });
    }
    let mut seen = BTreeSet::new();
    for version in versions {
        if !seen.insert(version) {
            return Err(RecreationError::DuplicateSourceVersion {
                owner_id: owner_id.clone(),
                version: version.clone(),
            });
        }
    }
    Ok(())
}

fn validate_reported_detail(
    detail: &ReportedDetail,
    seen_ids: &mut BTreeSet<OpaqueId>,
    text_budget: &mut usize,
    max_text_bytes: usize,
) -> Result<(), RecreationError> {
    if !seen_ids.insert(detail.detail_id.clone()) {
        return Err(RecreationError::DuplicateId {
            kind: "reported_detail",
            id: detail.detail_id.clone(),
        });
    }
    validate_single_line(
        &detail.statement,
        "reported_detail.statement",
        text_budget,
        max_text_bytes,
    )?;
    validate_source_versions(&detail.detail_id, &detail.source_versions)?;
    if let Some(note) = &detail.source_note {
        validate_single_line(
            note,
            "reported_detail.source_note",
            text_budget,
            max_text_bytes,
        )?;
    }
    Ok(())
}

fn validate_context_field(
    field_value: &ContextField,
    field_name: &'static str,
    owner_id: &OpaqueId,
    declared_assumptions: &BTreeSet<OpaqueId>,
    text_budget: &mut usize,
    max_text_bytes: usize,
) -> Result<(), RecreationError> {
    match field_value {
        ContextField::Reported {
            description,
            source_versions,
        } => {
            validate_single_line(description, field_name, text_budget, max_text_bytes)?;
            validate_source_versions(owner_id, source_versions)?;
        }
        ContextField::Assumed {
            description,
            assumption_id,
        } => {
            validate_single_line(description, field_name, text_budget, max_text_bytes)?;
            if !declared_assumptions.contains(assumption_id) {
                return Err(RecreationError::UnknownAssumptionReference {
                    field: field_name,
                    assumption_id: assumption_id.clone(),
                });
            }
        }
        ContextField::Unknown { reason } => {
            validate_single_line(reason, field_name, text_budget, max_text_bytes)?;
        }
    }
    Ok(())
}

fn format_version_list(versions: &[VersionId]) -> String {
    let mut out = String::new();
    for (idx, v) in versions.iter().enumerate() {
        if idx > 0 {
            out.push_str(", ");
        }
        out.push_str(v.as_str());
    }
    out
}

fn render_context_line(label: &str, field: &ContextField, out: &mut String) {
    match field {
        ContextField::Reported {
            description,
            source_versions,
        } => {
            let _ = writeln!(
                out,
                "- {label}: [REPORTED | sources: {}] {description}",
                format_version_list(source_versions)
            );
        }
        ContextField::Assumed {
            description,
            assumption_id,
        } => {
            let _ = writeln!(
                out,
                "- {label}: [ASSUMED | assumption: {assumption_id}] {description}"
            );
        }
        ContextField::Unknown { reason } => {
            let _ = writeln!(out, "- {label}: [UNKNOWN] {reason}");
        }
    }
}

/// Validates and exports a portable, editable illustrative recreation prompt (`MEDIA-04`, `A10`).
pub fn export_recreation_prompt(
    input: &RecreationPromptInput,
    limits: RecreationLimits,
    asset_policy: AssetExportPolicy,
) -> Result<ExportedRecreationPrompt, RecreationError> {
    if input.shared_reported_details.is_empty() && input.conflicting_variants.is_empty() {
        return Err(RecreationError::EmptyReportedContent);
    }

    if input.assumptions.len() > limits.max_assumptions() {
        return Err(RecreationError::LimitExceeded {
            kind: RecreationLimitKind::Assumptions,
            actual: input.assumptions.len(),
            max: limits.max_assumptions(),
        });
    }
    if input.unknowns.len() > limits.max_unknowns() {
        return Err(RecreationError::LimitExceeded {
            kind: RecreationLimitKind::Unknowns,
            actual: input.unknowns.len(),
            max: limits.max_unknowns(),
        });
    }
    if input.conflicting_variants.len() > limits.max_variants() {
        return Err(RecreationError::LimitExceeded {
            kind: RecreationLimitKind::Variants,
            actual: input.conflicting_variants.len(),
            max: limits.max_variants(),
        });
    }
    if input.reference_assets.len() > limits.max_assets() {
        return Err(RecreationError::LimitExceeded {
            kind: RecreationLimitKind::Assets,
            actual: input.reference_assets.len(),
            max: limits.max_assets(),
        });
    }

    let mut text_budget = 0usize;
    validate_single_line(
        &input.title,
        "title",
        &mut text_budget,
        limits.max_text_bytes(),
    )?;

    let mut assumption_ids = BTreeSet::new();
    for item in &input.assumptions {
        if !assumption_ids.insert(item.assumption_id.clone()) {
            return Err(RecreationError::DuplicateId {
                kind: "assumption",
                id: item.assumption_id.clone(),
            });
        }
        validate_single_line(
            &item.assumption,
            "assumption.assumption",
            &mut text_budget,
            limits.max_text_bytes(),
        )?;
        validate_single_line(
            &item.rationale,
            "assumption.rationale",
            &mut text_budget,
            limits.max_text_bytes(),
        )?;
    }

    let mut unknown_ids = BTreeSet::new();
    for item in &input.unknowns {
        if !unknown_ids.insert(item.unknown_id.clone()) {
            return Err(RecreationError::DuplicateId {
                kind: "unknown",
                id: item.unknown_id.clone(),
            });
        }
        validate_single_line(
            &item.aspect,
            "unknown.aspect",
            &mut text_budget,
            limits.max_text_bytes(),
        )?;
        validate_single_line(
            &item.impact_on_recreation,
            "unknown.impact_on_recreation",
            &mut text_budget,
            limits.max_text_bytes(),
        )?;
    }

    validate_context_field(
        &input.context.scene,
        "context.scene",
        &input.prompt_id,
        &assumption_ids,
        &mut text_budget,
        limits.max_text_bytes(),
    )?;
    validate_context_field(
        &input.context.time_context,
        "context.time_context",
        &input.prompt_id,
        &assumption_ids,
        &mut text_budget,
        limits.max_text_bytes(),
    )?;
    validate_context_field(
        &input.context.viewpoint,
        "context.viewpoint",
        &input.prompt_id,
        &assumption_ids,
        &mut text_budget,
        limits.max_text_bytes(),
    )?;
    validate_context_field(
        &input.context.lighting,
        "context.lighting",
        &input.prompt_id,
        &assumption_ids,
        &mut text_budget,
        limits.max_text_bytes(),
    )?;

    match &input.context.apparent_scale {
        ApparentScale::ReportedAngularOrRelative {
            description,
            source_versions,
            physical_size_unknown_reason,
        } => {
            validate_single_line(
                description,
                "context.apparent_scale.description",
                &mut text_budget,
                limits.max_text_bytes(),
            )?;
            validate_source_versions(&input.prompt_id, source_versions)?;
            validate_single_line(
                physical_size_unknown_reason,
                "context.apparent_scale.physical_size_unknown_reason",
                &mut text_budget,
                limits.max_text_bytes(),
            )?;
        }
        ApparentScale::CalibratedEstimate {
            description,
            calibration_version: _,
            uncertainty_bounds,
        } => {
            validate_single_line(
                description,
                "context.apparent_scale.description",
                &mut text_budget,
                limits.max_text_bytes(),
            )?;
            validate_single_line(
                uncertainty_bounds,
                "context.apparent_scale.uncertainty_bounds",
                &mut text_budget,
                limits.max_text_bytes(),
            )?;
        }
        ApparentScale::Unknown { reason } => {
            validate_single_line(
                reason,
                "context.apparent_scale.reason",
                &mut text_budget,
                limits.max_text_bytes(),
            )?;
        }
    }

    let mut total_reported_details = input.shared_reported_details.len();
    for v in &input.conflicting_variants {
        total_reported_details = total_reported_details.saturating_add(v.reported_details.len());
    }
    if total_reported_details > limits.max_reported_details() {
        return Err(RecreationError::LimitExceeded {
            kind: RecreationLimitKind::ReportedDetails,
            actual: total_reported_details,
            max: limits.max_reported_details(),
        });
    }

    let mut detail_ids = BTreeSet::new();
    for detail in &input.shared_reported_details {
        validate_reported_detail(
            detail,
            &mut detail_ids,
            &mut text_budget,
            limits.max_text_bytes(),
        )?;
    }

    let mut variant_map: BTreeMap<OpaqueId, &AccountVariant> = BTreeMap::new();
    for variant in &input.conflicting_variants {
        if variant_map
            .insert(variant.variant_id.clone(), variant)
            .is_some()
        {
            return Err(RecreationError::DuplicateId {
                kind: "variant",
                id: variant.variant_id.clone(),
            });
        }
        validate_single_line(
            &variant.summary,
            "variant.summary",
            &mut text_budget,
            limits.max_text_bytes(),
        )?;
        if variant.reported_details.is_empty() {
            return Err(RecreationError::MissingSourceVersions {
                item_id: variant.variant_id.clone(),
            });
        }
        for detail in &variant.reported_details {
            validate_reported_detail(
                detail,
                &mut detail_ids,
                &mut text_budget,
                limits.max_text_bytes(),
            )?;
        }
    }

    for variant in &input.conflicting_variants {
        if input.conflicting_variants.len() > 1 && variant.conflicts_with.is_empty() {
            return Err(RecreationError::InvalidVariantConflict {
                variant_id: variant.variant_id.clone(),
                target_id: variant.variant_id.clone(),
                reason: "multiple variants must declare which variant IDs they conflict with",
            });
        }
        let mut seen_targets = BTreeSet::new();
        for target_id in &variant.conflicts_with {
            if target_id == &variant.variant_id {
                return Err(RecreationError::InvalidVariantConflict {
                    variant_id: variant.variant_id.clone(),
                    target_id: target_id.clone(),
                    reason: "variant cannot conflict with itself",
                });
            }
            if !variant_map.contains_key(target_id) {
                return Err(RecreationError::InvalidVariantConflict {
                    variant_id: variant.variant_id.clone(),
                    target_id: target_id.clone(),
                    reason: "target variant is not declared in this prompt",
                });
            }
            if !seen_targets.insert(target_id.clone()) {
                return Err(RecreationError::InvalidVariantConflict {
                    variant_id: variant.variant_id.clone(),
                    target_id: target_id.clone(),
                    reason: "duplicate conflict target",
                });
            }
        }
    }

    let mut asset_ids = BTreeSet::new();
    let mut permitted_assets = Vec::new();
    let mut omitted_assets = Vec::new();

    for asset in &input.reference_assets {
        if !asset_ids.insert(asset.asset_id.clone()) {
            return Err(RecreationError::DuplicateId {
                kind: "reference_asset",
                id: asset.asset_id.clone(),
            });
        }
        validate_sha256_hex(&asset.sha256_hex, &asset.asset_id)?;
        validate_single_line(
            &asset.media_type,
            "reference_asset.media_type",
            &mut text_budget,
            limits.max_text_bytes(),
        )?;
        validate_single_line(
            &asset.role,
            "reference_asset.role",
            &mut text_budget,
            limits.max_text_bytes(),
        )?;
        validate_single_line(
            &asset.usage_conditions,
            "reference_asset.usage_conditions",
            &mut text_budget,
            limits.max_text_bytes(),
        )?;

        if asset.redistribution_permitted {
            permitted_assets.push(PermittedReferenceAsset {
                asset_id: asset.asset_id.clone(),
                source_version: asset.source_version.clone(),
                sha256_hex: asset.sha256_hex.clone(),
                media_type: asset.media_type.clone(),
                role: asset.role.clone(),
                usage_conditions: asset.usage_conditions.clone(),
            });
        } else {
            match asset_policy {
                AssetExportPolicy::RefuseUnpermitted => {
                    return Err(RecreationError::UnpermittedAsset {
                        asset_id: asset.asset_id.clone(),
                    });
                }
                AssetExportPolicy::OmitWithNotice => {
                    omitted_assets.push(OmittedReferenceAsset {
                        asset_id: asset.asset_id.clone(),
                        source_version: asset.source_version.clone(),
                        reason:
                            "omitted from export: source terms do not permit asset redistribution"
                                .to_owned(),
                        usage_conditions: asset.usage_conditions.clone(),
                    });
                }
            }
        }
    }

    let portable_prompt_text =
        render_portable_text(input, &permitted_assets, &omitted_assets, limits)?;

    Ok(ExportedRecreationPrompt {
        schema_version: SchemaVersion::V1,
        prompt_id: input.prompt_id.clone(),
        prompt_version: input.prompt_version.clone(),
        case_id: input.case_id.clone(),
        visible_label: VISIBLE_RECREATION_LABEL,
        fidelity_caveat: FIDELITY_AND_EXTERNAL_LABEL_CAVEAT,
        title: input.title.clone(),
        context: input.context.clone(),
        shared_reported_details: input.shared_reported_details.clone(),
        conflicting_variants: input.conflicting_variants.clone(),
        assumptions: input.assumptions.clone(),
        unknowns: input.unknowns.clone(),
        permitted_assets,
        omitted_assets,
        portable_prompt_text,
    })
}

fn render_portable_text(
    input: &RecreationPromptInput,
    permitted_assets: &[PermittedReferenceAsset],
    omitted_assets: &[OmittedReferenceAsset],
    limits: RecreationLimits,
) -> Result<String, RecreationError> {
    let mut out = String::new();
    let _ = writeln!(out, "# {VISIBLE_RECREATION_LABEL}");
    let _ = writeln!(out, "# Caveat: {FIDELITY_AND_EXTERNAL_LABEL_CAVEAT}");
    let _ = writeln!(out);
    let _ = writeln!(out, "## Prompt Metadata");
    let _ = writeln!(out, "- schema_version: {}", SchemaVersion::V1.as_u32());
    let _ = writeln!(out, "- prompt_id: {}", input.prompt_id);
    let _ = writeln!(out, "- prompt_version: {}", input.prompt_version);
    let _ = writeln!(out, "- case_id: {}", input.case_id);
    let _ = writeln!(out, "- title: {}", input.title);
    let _ = writeln!(out);
    let _ = writeln!(out, "## Scene, Time, Viewpoint, Lighting, and Scale");
    render_context_line("scene", &input.context.scene, &mut out);
    render_context_line("time", &input.context.time_context, &mut out);
    render_context_line("viewpoint", &input.context.viewpoint, &mut out);
    render_context_line("lighting", &input.context.lighting, &mut out);
    match &input.context.apparent_scale {
        ApparentScale::ReportedAngularOrRelative {
            description,
            source_versions,
            physical_size_unknown_reason,
        } => {
            let _ = writeln!(
                out,
                "- apparent_scale: [REPORTED ANGULAR/RELATIVE | sources: {}] {description} (physical size/range unknown: {physical_size_unknown_reason})",
                format_version_list(source_versions)
            );
        }
        ApparentScale::CalibratedEstimate {
            description,
            calibration_version,
            uncertainty_bounds,
        } => {
            let _ = writeln!(
                out,
                "- apparent_scale: [CALIBRATED ESTIMATE | calibration: {calibration_version} | uncertainty: {uncertainty_bounds}] {description}"
            );
        }
        ApparentScale::Unknown { reason } => {
            let _ = writeln!(out, "- apparent_scale: [UNKNOWN] {reason}");
        }
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "## Shared Reported Details (Pinned to Sources)");
    if input.shared_reported_details.is_empty() {
        let _ = writeln!(out, "- (none; see conflicting account variants below)");
    } else {
        for detail in &input.shared_reported_details {
            match &detail.source_note {
                Some(note) => {
                    let _ = writeln!(
                        out,
                        "- [{}] {} (sources: {}; note: {note})",
                        detail.detail_id,
                        detail.statement,
                        format_version_list(&detail.source_versions)
                    );
                }
                None => {
                    let _ = writeln!(
                        out,
                        "- [{}] {} (sources: {})",
                        detail.detail_id,
                        detail.statement,
                        format_version_list(&detail.source_versions)
                    );
                }
            }
        }
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "## Conflicting Account Variants (Do Not Merge)");
    if input.conflicting_variants.is_empty() {
        let _ = writeln!(out, "- (no conflicting variants recorded)");
    } else {
        for variant in &input.conflicting_variants {
            let mut conflicts = String::new();
            for (idx, cid) in variant.conflicts_with.iter().enumerate() {
                if idx > 0 {
                    conflicts.push_str(", ");
                }
                conflicts.push_str(cid.as_str());
            }
            let _ = writeln!(
                out,
                "### Variant `{}` (conflicts_with: [{}])",
                variant.variant_id, conflicts
            );
            let _ = writeln!(out, "- summary: {}", variant.summary);
            for detail in &variant.reported_details {
                match &detail.source_note {
                    Some(note) => {
                        let _ = writeln!(
                            out,
                            "  - [{}] {} (sources: {}; note: {note})",
                            detail.detail_id,
                            detail.statement,
                            format_version_list(&detail.source_versions)
                        );
                    }
                    None => {
                        let _ = writeln!(
                            out,
                            "  - [{}] {} (sources: {})",
                            detail.detail_id,
                            detail.statement,
                            format_version_list(&detail.source_versions)
                        );
                    }
                }
            }
        }
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "## Explicit Assumptions (Not Observed Evidence)");
    if input.assumptions.is_empty() {
        let _ = writeln!(out, "- (none)");
    } else {
        for item in &input.assumptions {
            let _ = writeln!(
                out,
                "- [{}] {} (rationale: {})",
                item.assumption_id, item.assumption, item.rationale
            );
        }
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "## Explicit Unknowns (Do Not Invent Measurements)");
    if input.unknowns.is_empty() {
        let _ = writeln!(out, "- (none)");
    } else {
        for item in &input.unknowns {
            let _ = writeln!(
                out,
                "- [{}] {}: {}",
                item.unknown_id, item.aspect, item.impact_on_recreation
            );
        }
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "## Permitted Reference Asset Manifest");
    if permitted_assets.is_empty() {
        let _ = writeln!(out, "- (no permitted reference assets exported)");
    } else {
        for asset in permitted_assets {
            let _ = writeln!(
                out,
                "- [{}] source_version={} sha256={} media_type={} role=\"{}\" conditions=\"{}\"",
                asset.asset_id,
                asset.source_version,
                asset.sha256_hex,
                asset.media_type,
                asset.role,
                asset.usage_conditions
            );
        }
    }

    if !omitted_assets.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "## Omitted Reference Assets (Redistribution Restricted)"
        );
        for omitted in omitted_assets {
            let _ = writeln!(
                out,
                "- [{}] source_version={} reason=\"{}\" conditions=\"{}\"",
                omitted.asset_id, omitted.source_version, omitted.reason, omitted.usage_conditions
            );
        }
    }

    if out.len() > limits.max_output_bytes() {
        return Err(RecreationError::LimitExceeded {
            kind: RecreationLimitKind::OutputBytes,
            actual: out.len(),
            max: limits.max_output_bytes(),
        });
    }

    Ok(out)
}

/// Validates an imported recreation image against its exported prompt so that it
/// preserves prompt/settings lineage and visible `ILLUSTRATIVE RECREATION` labeling
/// and can never be promoted to observation evidence (`MEDIA-04`, `A10`).
pub fn validate_imported_recreation(
    input: &ImportedRecreationInput,
    exported_prompt: &ExportedRecreationPrompt,
    max_text_bytes: usize,
) -> Result<ValidatedImportedRecreation, RecreationError> {
    if max_text_bytes == 0 {
        return Err(RecreationError::ZeroLimit(RecreationLimitKind::TextBytes));
    }
    if input.attempted_class != AttemptedArtifactClass::IllustrativeRecreation {
        return Err(RecreationError::ForbiddenEvidencePromotion {
            artifact_id: input.artifact_id.clone(),
            attempted: input.attempted_class,
        });
    }
    if input.prompt_id != exported_prompt.prompt_id
        || input.prompt_version != exported_prompt.prompt_version
    {
        return Err(RecreationError::PromptVersionMismatch {
            expected_prompt_id: exported_prompt.prompt_id.clone(),
            expected_version: exported_prompt.prompt_version.clone(),
            actual_prompt_id: input.prompt_id.clone(),
            actual_version: input.prompt_version.clone(),
        });
    }
    if let Some(variant_id) = &input.variant_id
        && !exported_prompt
            .conflicting_variants
            .iter()
            .any(|v| &v.variant_id == variant_id)
    {
        return Err(RecreationError::UnknownVariantReference {
            variant_id: variant_id.clone(),
        });
    }

    validate_sha256_hex(&input.sha256_hex, &input.artifact_id)?;

    let mut text_budget = 0usize;
    validate_single_line(
        &input.media_type,
        "imported_recreation.media_type",
        &mut text_budget,
        max_text_bytes,
    )?;
    validate_single_line(
        &input.generator_model,
        "imported_recreation.generator_model",
        &mut text_budget,
        max_text_bytes,
    )?;
    validate_single_line(
        &input.generator_settings,
        "imported_recreation.generator_settings",
        &mut text_budget,
        max_text_bytes,
    )?;
    validate_single_line(
        &input.visible_label,
        "imported_recreation.visible_label",
        &mut text_budget,
        max_text_bytes,
    )?;

    if !input.visible_label.contains("ILLUSTRATIVE RECREATION") {
        return Err(RecreationError::MissingVisibleRecreationLabel {
            artifact_id: input.artifact_id.clone(),
        });
    }

    Ok(ValidatedImportedRecreation {
        schema_version: SchemaVersion::V1,
        artifact_id: input.artifact_id.clone(),
        artifact_version: input.artifact_version.clone(),
        sha256_hex: input.sha256_hex.clone(),
        media_type: input.media_type.clone(),
        prompt_id: input.prompt_id.clone(),
        prompt_version: input.prompt_version.clone(),
        variant_id: input.variant_id.clone(),
        generator_model: input.generator_model.clone(),
        generator_settings: input.generator_settings.clone(),
        visible_label: input.visible_label.clone(),
        fidelity_caveat: FIDELITY_AND_EXTERNAL_LABEL_CAVEAT,
        can_serve_as_observation_evidence: false,
        can_establish_physical_measurement: false,
    })
}
