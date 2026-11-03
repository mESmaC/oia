//! Portable illustrative recreation-prompt export and imported-recreation labeling (`MEDIA-04`, `A10`).
//!
//! Exports editable, portable image-generation prompts that keep reported details,
//! explicit assumptions, unknowns, and conflicting witness/source variants separate,
//! and validates imported recreations so generated imagery retains its prompt/settings
//! lineage and visible `ILLUSTRATIVE RECREATION` metadata labeling without ever being
//! promoted to observation evidence (`docs/spec.md` `MEDIA-04`, `docs/architecture.md` §§2.2–2.3, §8).
//!
//! Note on scope: this kernel validates in-memory metadata, structural prompt boundaries,
//! and 64-character lowercase hexadecimal SHA-256 digest strings supplied by the caller.
//! It does not decode image pixels, verify visual watermarks inside rendered pixel buffers,
//! or compute cryptographic digests over external files.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use oia_domain::{IdError, OpaqueId, SchemaVersion, VersionId};

/// Mandatory visible label applied to exported recreation prompts and imported recreations.
pub const VISIBLE_RECREATION_LABEL: &str =
    "ILLUSTRATIVE RECREATION — GENERATED FROM PROMPT (NOT OBSERVATION EVIDENCE)";

/// Mandatory caveat explaining that prompt wording alone neither proves physical fidelity
/// nor guarantees that an external image generator embeds provenance labels (`MEDIA-04`).
pub const FIDELITY_AND_EXTERNAL_LABEL_CAVEAT: &str = "Prompt wording organizes reported details, assumptions, unknowns, and conflicting variants for an illustrative recreation; it does not establish physical fidelity or guarantee that an external image generator preserves labeling.";

/// Default per-item cap on `source_versions` references when `RecreationLimits::new` is used.
pub const DEFAULT_MAX_SOURCE_VERSIONS_PER_ITEM: usize = 64;

/// Default cap on `negative_keywords` entries when `RecreationLimits::new` is used.
pub const DEFAULT_MAX_NEGATIVE_KEYWORDS: usize = 64;

/// Which budget was zero or exceeded during recreation-prompt validation or rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecreationLimitKind {
    ReportedDetails,
    Assumptions,
    Unknowns,
    Variants,
    NegativeKeywords,
    Assets,
    SourceVersionsPerItem,
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
            Self::NegativeKeywords => f.write_str("max_negative_keywords"),
            Self::Assets => f.write_str("max_assets"),
            Self::SourceVersionsPerItem => f.write_str("max_source_versions_per_item"),
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
    max_negative_keywords: usize,
    max_assets: usize,
    max_source_versions_per_item: usize,
    max_text_bytes: usize,
    max_output_bytes: usize,
}

impl RecreationLimits {
    /// Validates that all recreation-prompt budgets are non-zero, using
    /// [`DEFAULT_MAX_SOURCE_VERSIONS_PER_ITEM`] for per-item source-version lists and
    /// [`DEFAULT_MAX_NEGATIVE_KEYWORDS`] for negative keywords.
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
            max_negative_keywords: max_unknowns.max(DEFAULT_MAX_NEGATIVE_KEYWORDS),
            max_assets,
            max_source_versions_per_item: DEFAULT_MAX_SOURCE_VERSIONS_PER_ITEM,
            max_text_bytes,
            max_output_bytes,
        })
    }

    /// Overrides the per-item limit on `source_versions` references (`>= 1`).
    pub fn with_max_source_versions_per_item(
        mut self,
        max_source_versions_per_item: usize,
    ) -> Result<Self, RecreationError> {
        if max_source_versions_per_item == 0 {
            return Err(RecreationError::ZeroLimit(
                RecreationLimitKind::SourceVersionsPerItem,
            ));
        }
        self.max_source_versions_per_item = max_source_versions_per_item;
        Ok(self)
    }

    /// Overrides the limit on `negative_keywords` entries (`>= 1`).
    pub fn with_max_negative_keywords(
        mut self,
        max_negative_keywords: usize,
    ) -> Result<Self, RecreationError> {
        if max_negative_keywords == 0 {
            return Err(RecreationError::ZeroLimit(
                RecreationLimitKind::NegativeKeywords,
            ));
        }
        self.max_negative_keywords = max_negative_keywords;
        Ok(self)
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
    pub const fn max_negative_keywords(self) -> usize {
        self.max_negative_keywords
    }

    #[must_use]
    pub const fn max_assets(self) -> usize {
        self.max_assets
    }

    #[must_use]
    pub const fn max_source_versions_per_item(self) -> usize {
        self.max_source_versions_per_item
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
///
/// This local guard is scoped to `oia-media::recreation` and will be consolidated with
/// the shared `oia-domain` origin-class vocabulary in `#10`.
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
    EmptyVariantDetails {
        variant_id: OpaqueId,
    },
    SingleVariantRequiresSharedDetails {
        variant_id: OpaqueId,
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
    MissingNegativeKeywordOrigin {
        negative_id: OpaqueId,
    },
    UnknownNegativeOriginReference {
        negative_id: OpaqueId,
        kind: &'static str,
        target_id: OpaqueId,
    },
    ContradictedDetailInReportedDetails {
        negative_id: OpaqueId,
        detail_id: OpaqueId,
    },
    UnpinnedContradictingSourceVersion {
        negative_id: OpaqueId,
        source_version: VersionId,
    },
    SelfContradictingSourceVersion {
        negative_id: OpaqueId,
        source_version: VersionId,
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
    MissingVariantSelection {
        prompt_id: OpaqueId,
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
    AllocationFailed,
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
                write!(
                    f,
                    "field `{field}` contains disallowed control, line-break, bidi, or zero-width characters"
                )
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
                    "recreation prompt requires at least one reported detail or conflicting account variant"
                )
            }
            Self::MissingSourceVersions { item_id } => {
                write!(
                    f,
                    "reported item `{item_id}` must cite at least one source version"
                )
            }
            Self::EmptyVariantDetails { variant_id } => {
                write!(
                    f,
                    "account variant `{variant_id}` must include at least one reported detail"
                )
            }
            Self::SingleVariantRequiresSharedDetails { variant_id } => {
                write!(
                    f,
                    "single variant `{variant_id}` has no competing variant; place unconflicted details in shared_reported_details or supply at least two conflicting variants"
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
            Self::MissingNegativeKeywordOrigin { negative_id } => {
                write!(
                    f,
                    "negative keyword `{negative_id}` must carry an explicit evidence-derived origin"
                )
            }
            Self::UnknownNegativeOriginReference {
                negative_id,
                kind,
                target_id,
            } => {
                write!(
                    f,
                    "negative keyword `{negative_id}` references undeclared {kind} `{target_id}`"
                )
            }
            Self::ContradictedDetailInReportedDetails {
                negative_id,
                detail_id,
            } => {
                write!(
                    f,
                    "negative keyword `{negative_id}` marks detail `{detail_id}` as contradicted, but it appears as a reported detail in this prompt"
                )
            }
            Self::UnpinnedContradictingSourceVersion {
                negative_id,
                source_version,
            } => {
                write!(
                    f,
                    "negative keyword `{negative_id}` cites contradicting source version `{source_version}` that is not pinned in this prompt"
                )
            }
            Self::SelfContradictingSourceVersion {
                negative_id,
                source_version,
            } => {
                write!(
                    f,
                    "negative keyword `{negative_id}` cites source version `{source_version}` as both the contradicted statement source and a contradicting source"
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
            Self::MissingVariantSelection { prompt_id } => {
                write!(
                    f,
                    "imported recreation for prompt `{prompt_id}` with conflicting variants must specify which variant_id was rendered"
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
                    "recreation artifact `{artifact_id}` must carry the exact VISIBLE_RECREATION_LABEL"
                )
            }
            Self::AllocationFailed => write!(f, "memory allocation failed while building prompt"),
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

/// Evidence-derived origin required for every [`NegativeKeyword`] (`MEDIA-04`, `#140`).
///
/// Negative keywords may only exclude aspects justified by a declared [`ExplicitUnknown`]
/// (or an unknown [`SceneContext`] attribute), a verbatim statement from a source version
/// contradicted by one or more pinned source versions in this prompt, or a declared
/// [`AccountVariant`] whose elements must not be merged into another variant's image
/// (`AGENTS.md` §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NegativeKeywordOrigin {
    /// Derived from a declared [`ExplicitUnknown`] or an unknown [`SceneContext`] attribute
    /// in the same prompt, indicating that a specific value for the aspect is unspecified
    /// (not established) rather than absent.
    Unknown { unknown_id: OpaqueId },
    /// Derived from a verbatim statement from `contradicted_source_version` that is
    /// contradicted by one or more source versions pinned in this prompt.
    ContradictedDetail {
        detail_id: OpaqueId,
        contradicted_statement: String,
        contradicted_source_version: VersionId,
        contradicting_source_versions: Vec<VersionId>,
    },
    /// Derived from a competing [`AccountVariant`] (`excluded_variant_id`) whose elements
    /// must not be merged when rendering `for_variant_id`.
    Variant {
        for_variant_id: OpaqueId,
        excluded_variant_id: OpaqueId,
    },
}

/// Input negative keyword excluding an unestablished, contradicted, or competing-variant
/// element from an illustrative recreation (`MEDIA-04`, `#140`).
///
/// `origin` is `Option<NegativeKeywordOrigin>` at the input boundary so callers or
/// deserialized payloads that omit an origin are explicitly refused with
/// [`RecreationError::MissingNegativeKeywordOrigin`] rather than silently accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NegativeKeyword {
    pub negative_id: OpaqueId,
    pub keyword: String,
    pub rationale: String,
    pub origin: Option<NegativeKeywordOrigin>,
}

impl NegativeKeyword {
    /// Constructs a negative keyword grounded in a declared [`ExplicitUnknown`] or an
    /// unknown [`SceneContext`] attribute (`context:scene`, `context:time`,
    /// `context:viewpoint`, `context:lighting`, or `context:apparent_scale`).
    #[must_use]
    pub const fn from_unknown(
        negative_id: OpaqueId,
        keyword: String,
        rationale: String,
        unknown_id: OpaqueId,
    ) -> Self {
        Self {
            negative_id,
            keyword,
            rationale,
            origin: Some(NegativeKeywordOrigin::Unknown { unknown_id }),
        }
    }

    /// Constructs a negative keyword grounded in a verbatim contradicted statement, its
    /// source version, and the pinned source versions in this prompt that contradict it.
    #[must_use]
    pub const fn from_contradicted_detail(
        negative_id: OpaqueId,
        keyword: String,
        rationale: String,
        detail_id: OpaqueId,
        contradicted_statement: String,
        contradicted_source_version: VersionId,
        contradicting_source_versions: Vec<VersionId>,
    ) -> Self {
        Self {
            negative_id,
            keyword,
            rationale,
            origin: Some(NegativeKeywordOrigin::ContradictedDetail {
                detail_id,
                contradicted_statement,
                contradicted_source_version,
                contradicting_source_versions,
            }),
        }
    }

    /// Constructs a negative keyword scoped to `for_variant_id` that excludes a
    /// competing [`AccountVariant`] (`excluded_variant_id`) so the two variants are
    /// not merged into a single image.
    #[must_use]
    pub const fn from_variant(
        negative_id: OpaqueId,
        keyword: String,
        rationale: String,
        for_variant_id: OpaqueId,
        excluded_variant_id: OpaqueId,
    ) -> Self {
        Self {
            negative_id,
            keyword,
            rationale,
            origin: Some(NegativeKeywordOrigin::Variant {
                for_variant_id,
                excluded_variant_id,
            }),
        }
    }
}

/// Validated negative keyword with a verified evidence-derived [`NegativeKeywordOrigin`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedNegativeKeyword {
    negative_id: OpaqueId,
    keyword: String,
    rationale: String,
    origin: NegativeKeywordOrigin,
}

impl ValidatedNegativeKeyword {
    #[must_use]
    pub const fn negative_id(&self) -> &OpaqueId {
        &self.negative_id
    }

    #[must_use]
    pub fn keyword(&self) -> &str {
        &self.keyword
    }

    #[must_use]
    pub fn rationale(&self) -> &str {
        &self.rationale
    }

    #[must_use]
    pub const fn origin(&self) -> &NegativeKeywordOrigin {
        &self.origin
    }
}

/// Derives bounded [`NegativeKeyword`] entries from a prompt's unknown [`SceneContext`]
/// attributes and declared [`ExplicitUnknown`] items (`MEDIA-04`, `#140`).
///
/// Single-witness prompts have no cross-source contradiction matrix or competing account
/// variants, so their negative keywords are derived from unknown context attributes and
/// declared unknowns (`NegativeKeywordOrigin::Unknown`). Each derived keyword is phrased as
/// `"a specific value for <aspect>"` to record that a specific value is unspecified rather
/// than suppressing the attribute altogether. When `oia-app` derives negatives from a
/// simulated session witness account, the caller must include the simulated account's
/// source label in the unknown impact/rationale so derived negatives preserve that
/// qualification.
pub fn derive_unknown_negative_keywords(
    context: &SceneContext,
    unknowns: &[ExplicitUnknown],
    limits: RecreationLimits,
) -> Result<Vec<NegativeKeyword>, RecreationError> {
    let declared_unknown_ids: BTreeSet<&str> =
        unknowns.iter().map(|u| u.unknown_id.as_str()).collect();
    let mut context_items: Vec<(&'static str, &'static str, &str)> = Vec::new();
    for (id_str, aspect, field) in [
        ("context:scene", "scene", &context.scene),
        ("context:time", "time", &context.time_context),
        ("context:viewpoint", "viewpoint", &context.viewpoint),
        ("context:lighting", "lighting", &context.lighting),
    ] {
        if let ContextField::Unknown { reason } = field
            && !declared_unknown_ids.contains(id_str)
        {
            context_items.push((id_str, aspect, reason.as_str()));
        }
    }
    if !declared_unknown_ids.contains("context:apparent_scale") {
        match &context.apparent_scale {
            ApparentScale::Unknown { reason } => {
                context_items.push(("context:apparent_scale", "apparent scale", reason.as_str()));
            }
            ApparentScale::ReportedAngularOrRelative {
                physical_size_unknown_reason,
                ..
            } => {
                context_items.push((
                    "context:apparent_scale",
                    "physical size and slant range",
                    physical_size_unknown_reason.as_str(),
                ));
            }
            ApparentScale::CalibratedEstimate { .. } => {}
        }
    }

    let total_count = context_items.len().saturating_add(unknowns.len());
    if total_count > limits.max_negative_keywords() {
        return Err(RecreationError::LimitExceeded {
            kind: RecreationLimitKind::NegativeKeywords,
            actual: total_count,
            max: limits.max_negative_keywords(),
        });
    }

    let mut out = Vec::new();
    out.try_reserve_exact(total_count)
        .map_err(|_| RecreationError::AllocationFailed)?;
    let mut text_budget = 0usize;
    let mut used_ids = BTreeSet::new();
    let mut fallback_seq = 1usize;

    let mut next_negative_id = |raw_unknown_id: &str| -> Result<OpaqueId, RecreationError> {
        let candidate = format!("neg:{raw_unknown_id}");
        if candidate.len() <= oia_domain::MAX_ID_LEN
            && let Ok(id) = OpaqueId::new(candidate)
            && used_ids.insert(id.clone())
        {
            return Ok(id);
        }
        loop {
            let fallback = OpaqueId::new(format!("neg:u{fallback_seq}"))?;
            fallback_seq = fallback_seq.saturating_add(1);
            if used_ids.insert(fallback.clone()) {
                return Ok(fallback);
            }
        }
    };

    for (id_str, aspect, reason) in context_items {
        let unknown_id = OpaqueId::new(id_str)?;
        let negative_id = next_negative_id(unknown_id.as_str())?;
        charge_text_bytes(
            negative_id.as_str().len(),
            &mut text_budget,
            limits.max_text_bytes(),
        )?;
        charge_text_bytes(
            unknown_id.as_str().len(),
            &mut text_budget,
            limits.max_text_bytes(),
        )?;
        validate_single_line(
            aspect,
            "unknown.aspect",
            &mut text_budget,
            limits.max_text_bytes(),
        )?;
        let keyword = format!("a specific value for {aspect}");
        charge_text_bytes(
            "a specific value for ".len(),
            &mut text_budget,
            limits.max_text_bytes(),
        )?;
        validate_single_line(
            reason,
            "unknown.impact_on_recreation",
            &mut text_budget,
            limits.max_text_bytes(),
        )?;
        out.push(NegativeKeyword::from_unknown(
            negative_id,
            keyword,
            reason.to_owned(),
            unknown_id,
        ));
    }

    for unknown in unknowns {
        let negative_id = next_negative_id(unknown.unknown_id.as_str())?;
        charge_text_bytes(
            negative_id.as_str().len(),
            &mut text_budget,
            limits.max_text_bytes(),
        )?;
        charge_text_bytes(
            unknown.unknown_id.as_str().len(),
            &mut text_budget,
            limits.max_text_bytes(),
        )?;
        validate_single_line(
            &unknown.aspect,
            "unknown.aspect",
            &mut text_budget,
            limits.max_text_bytes(),
        )?;
        let keyword = format!("a specific value for {}", unknown.aspect);
        charge_text_bytes(
            "a specific value for ".len(),
            &mut text_budget,
            limits.max_text_bytes(),
        )?;
        validate_single_line(
            &unknown.impact_on_recreation,
            "unknown.impact_on_recreation",
            &mut text_budget,
            limits.max_text_bytes(),
        )?;
        out.push(NegativeKeyword::from_unknown(
            negative_id,
            keyword,
            unknown.impact_on_recreation.clone(),
            unknown.unknown_id.clone(),
        ));
    }
    Ok(out)
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
    pub negative_keywords: Vec<NegativeKeyword>,
    pub reference_assets: Vec<ReferenceAssetInput>,
}

/// Validated, portable recreation-prompt export with its editable text representation.
///
/// Fields are private so callers cannot construct an unvalidated prompt or mutate its
/// labels, caveat, or provenance after export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportedRecreationPrompt {
    schema_version: SchemaVersion,
    prompt_id: OpaqueId,
    prompt_version: VersionId,
    case_id: OpaqueId,
    title: String,
    context: SceneContext,
    shared_reported_details: Vec<ReportedDetail>,
    conflicting_variants: Vec<AccountVariant>,
    assumptions: Vec<ExplicitAssumption>,
    unknowns: Vec<ExplicitUnknown>,
    negative_keywords: Vec<ValidatedNegativeKeyword>,
    permitted_assets: Vec<PermittedReferenceAsset>,
    omitted_assets: Vec<OmittedReferenceAsset>,
    portable_prompt_text: String,
}

impl ExportedRecreationPrompt {
    #[must_use]
    pub const fn schema_version(&self) -> SchemaVersion {
        self.schema_version
    }

    #[must_use]
    pub const fn prompt_id(&self) -> &OpaqueId {
        &self.prompt_id
    }

    #[must_use]
    pub const fn prompt_version(&self) -> &VersionId {
        &self.prompt_version
    }

    #[must_use]
    pub const fn case_id(&self) -> &OpaqueId {
        &self.case_id
    }

    #[must_use]
    pub const fn visible_label(&self) -> &'static str {
        VISIBLE_RECREATION_LABEL
    }

    #[must_use]
    pub const fn fidelity_caveat(&self) -> &'static str {
        FIDELITY_AND_EXTERNAL_LABEL_CAVEAT
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub const fn context(&self) -> &SceneContext {
        &self.context
    }

    #[must_use]
    pub fn shared_reported_details(&self) -> &[ReportedDetail] {
        &self.shared_reported_details
    }

    #[must_use]
    pub fn conflicting_variants(&self) -> &[AccountVariant] {
        &self.conflicting_variants
    }

    #[must_use]
    pub fn assumptions(&self) -> &[ExplicitAssumption] {
        &self.assumptions
    }

    #[must_use]
    pub fn unknowns(&self) -> &[ExplicitUnknown] {
        &self.unknowns
    }

    #[must_use]
    pub fn negative_keywords(&self) -> &[ValidatedNegativeKeyword] {
        &self.negative_keywords
    }

    #[must_use]
    pub fn permitted_assets(&self) -> &[PermittedReferenceAsset] {
        &self.permitted_assets
    }

    #[must_use]
    pub fn omitted_assets(&self) -> &[OmittedReferenceAsset] {
        &self.omitted_assets
    }

    #[must_use]
    pub fn portable_prompt_text(&self) -> &str {
        &self.portable_prompt_text
    }
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
///
/// Fields are private and evidentiary flags are returned by constant methods (`false`) so
/// a validated recreation cannot be mutated into observation or measurement evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedImportedRecreation {
    schema_version: SchemaVersion,
    artifact_id: OpaqueId,
    artifact_version: VersionId,
    sha256_hex: String,
    media_type: String,
    prompt_id: OpaqueId,
    prompt_version: VersionId,
    variant_id: Option<OpaqueId>,
    generator_model: String,
    generator_settings: String,
    negative_keywords: Vec<ValidatedNegativeKeyword>,
}

impl ValidatedImportedRecreation {
    #[must_use]
    pub const fn schema_version(&self) -> SchemaVersion {
        self.schema_version
    }

    #[must_use]
    pub const fn artifact_id(&self) -> &OpaqueId {
        &self.artifact_id
    }

    #[must_use]
    pub const fn artifact_version(&self) -> &VersionId {
        &self.artifact_version
    }

    #[must_use]
    pub fn sha256_hex(&self) -> &str {
        &self.sha256_hex
    }

    #[must_use]
    pub fn media_type(&self) -> &str {
        &self.media_type
    }

    #[must_use]
    pub const fn prompt_id(&self) -> &OpaqueId {
        &self.prompt_id
    }

    #[must_use]
    pub const fn prompt_version(&self) -> &VersionId {
        &self.prompt_version
    }

    #[must_use]
    pub const fn variant_id(&self) -> Option<&OpaqueId> {
        self.variant_id.as_ref()
    }

    #[must_use]
    pub fn generator_model(&self) -> &str {
        &self.generator_model
    }

    #[must_use]
    pub fn generator_settings(&self) -> &str {
        &self.generator_settings
    }

    #[must_use]
    pub fn negative_keywords(&self) -> &[ValidatedNegativeKeyword] {
        &self.negative_keywords
    }

    #[must_use]
    pub const fn visible_label(&self) -> &'static str {
        VISIBLE_RECREATION_LABEL
    }

    #[must_use]
    pub const fn fidelity_caveat(&self) -> &'static str {
        FIDELITY_AND_EXTERNAL_LABEL_CAVEAT
    }

    #[must_use]
    pub const fn can_serve_as_observation_evidence(&self) -> bool {
        false
    }

    #[must_use]
    pub const fn can_establish_physical_measurement(&self) -> bool {
        false
    }

    /// Exports a plain UTF-8 metadata sidecar with the recreation label and pinned
    /// lineage (`MEDIA-04`, portable-label subset of `A10`, `#140`).
    ///
    /// Every write, including quote/Unicode escape expansion, counts toward the
    /// byte limit. Failure returns no partial document. This does not write a file,
    /// embed a pixel watermark, verify image bytes, or export the referenced prompt;
    /// distribute that prompt separately to retain its source notes and assumptions.
    pub fn portable_metadata_text(
        &self,
        max_output_bytes: usize,
    ) -> Result<String, RecreationError> {
        if max_output_bytes == 0 {
            return Err(RecreationError::ZeroLimit(RecreationLimitKind::OutputBytes));
        }
        let mut out = String::new();
        push_str_bounded(&mut out, VISIBLE_RECREATION_LABEL, max_output_bytes)?;
        push_str_bounded(
            &mut out,
            "\nRecreation metadata sidecar\n",
            max_output_bytes,
        )?;
        for (name, value) in [
            ("artifact_id", self.artifact_id.as_str()),
            ("artifact_version", self.artifact_version.as_str()),
            ("declared_sha256_hex", self.sha256_hex.as_str()),
            ("media_type", self.media_type.as_str()),
            ("prompt_id", self.prompt_id.as_str()),
            ("prompt_version", self.prompt_version.as_str()),
            (
                "variant_id",
                self.variant_id.as_ref().map_or("", OpaqueId::as_str),
            ),
            ("generator_model", self.generator_model.as_str()),
            ("generator_settings", self.generator_settings.as_str()),
        ] {
            push_str_bounded(&mut out, name, max_output_bytes)?;
            push_str_bounded(&mut out, "=", max_output_bytes)?;
            push_quoted_escaped(&mut out, value, max_output_bytes)?;
            push_str_bounded(&mut out, "\n", max_output_bytes)?;
        }
        if !self.negative_keywords.is_empty() {
            push_str_bounded(&mut out, "negative_keyword_count=", max_output_bytes)?;
            push_usize_bounded(&mut out, self.negative_keywords.len(), max_output_bytes)?;
            push_str_bounded(&mut out, "\n", max_output_bytes)?;
            for neg in &self.negative_keywords {
                render_negative_keyword_line("negative_keyword=", neg, &mut out, max_output_bytes)?;
            }
        }
        push_str_bounded(
            &mut out,
            "An empty variant_id means the prompt has no conflicting variants.\n\
             Observation evidence: false\nPhysical measurement: false\n\
             Digest is caller-declared; image bytes have not been verified.\n\
             This sidecar does not embed or verify a visible label in image pixels.\n\
             Distribute the referenced prompt separately for source notes, assumptions and unknowns.\n",
            max_output_bytes,
        )?;
        push_str_bounded(
            &mut out,
            FIDELITY_AND_EXTERNAL_LABEL_CAVEAT,
            max_output_bytes,
        )?;
        push_str_bounded(&mut out, "\n", max_output_bytes)?;
        Ok(out)
    }
}

fn is_disallowed_control_or_format_char(ch: char) -> bool {
    ch.is_control()
        || matches!(
            ch,
            '\u{0085}'
                | '\u{061C}'
                | '\u{200B}'
                | '\u{2028}'..='\u{202E}'
                | '\u{2060}'..='\u{2069}'
                | '\u{FEFF}'
        )
}

fn charge_text_bytes(
    len: usize,
    text_budget: &mut usize,
    max_text_bytes: usize,
) -> Result<(), RecreationError> {
    *text_budget = text_budget.saturating_add(len);
    if *text_budget > max_text_bytes {
        return Err(RecreationError::LimitExceeded {
            kind: RecreationLimitKind::TextBytes,
            actual: *text_budget,
            max: max_text_bytes,
        });
    }
    Ok(())
}

fn validate_single_line(
    value: &str,
    field: &'static str,
    text_budget: &mut usize,
    max_text_bytes: usize,
) -> Result<(), RecreationError> {
    if value.chars().any(is_disallowed_control_or_format_char) {
        return Err(RecreationError::ControlCharacterInField { field });
    }
    if value.trim().is_empty() {
        return Err(RecreationError::EmptyField { field });
    }
    charge_text_bytes(value.len(), text_budget, max_text_bytes)
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
    limits: RecreationLimits,
    text_budget: &mut usize,
) -> Result<(), RecreationError> {
    if versions.is_empty() {
        return Err(RecreationError::MissingSourceVersions {
            item_id: owner_id.clone(),
        });
    }
    if versions.len() > limits.max_source_versions_per_item() {
        return Err(RecreationError::LimitExceeded {
            kind: RecreationLimitKind::SourceVersionsPerItem,
            actual: versions.len(),
            max: limits.max_source_versions_per_item(),
        });
    }
    let mut seen = BTreeSet::new();
    for version in versions {
        charge_text_bytes(version.as_str().len(), text_budget, limits.max_text_bytes())?;
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
    limits: RecreationLimits,
    text_budget: &mut usize,
) -> Result<(), RecreationError> {
    charge_text_bytes(
        detail.detail_id.as_str().len(),
        text_budget,
        limits.max_text_bytes(),
    )?;
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
        limits.max_text_bytes(),
    )?;
    validate_source_versions(
        &detail.detail_id,
        &detail.source_versions,
        limits,
        text_budget,
    )?;
    if let Some(note) = &detail.source_note {
        validate_single_line(
            note,
            "reported_detail.source_note",
            text_budget,
            limits.max_text_bytes(),
        )?;
    }
    Ok(())
}

fn validate_context_field(
    field_value: &ContextField,
    field_name: &'static str,
    owner_id: &OpaqueId,
    declared_assumptions: &BTreeSet<OpaqueId>,
    limits: RecreationLimits,
    text_budget: &mut usize,
) -> Result<(), RecreationError> {
    match field_value {
        ContextField::Reported {
            description,
            source_versions,
        } => {
            validate_single_line(
                description,
                field_name,
                text_budget,
                limits.max_text_bytes(),
            )?;
            validate_source_versions(owner_id, source_versions, limits, text_budget)?;
        }
        ContextField::Assumed {
            description,
            assumption_id,
        } => {
            charge_text_bytes(
                assumption_id.as_str().len(),
                text_budget,
                limits.max_text_bytes(),
            )?;
            validate_single_line(
                description,
                field_name,
                text_budget,
                limits.max_text_bytes(),
            )?;
            if !declared_assumptions.contains(assumption_id) {
                return Err(RecreationError::UnknownAssumptionReference {
                    field: field_name,
                    assumption_id: assumption_id.clone(),
                });
            }
        }
        ContextField::Unknown { reason } => {
            validate_single_line(reason, field_name, text_budget, limits.max_text_bytes())?;
        }
    }
    Ok(())
}

fn push_str_bounded(out: &mut String, text: &str, max_out: usize) -> Result<(), RecreationError> {
    let new_len = out.len().saturating_add(text.len());
    if new_len > max_out {
        return Err(RecreationError::LimitExceeded {
            kind: RecreationLimitKind::OutputBytes,
            actual: new_len,
            max: max_out,
        });
    }
    out.try_reserve(text.len())
        .map_err(|_| RecreationError::AllocationFailed)?;
    out.push_str(text);
    Ok(())
}

fn push_usize_bounded(
    out: &mut String,
    value: usize,
    max_out: usize,
) -> Result<(), RecreationError> {
    let mut digits = [0u8; 20];
    let mut n = value;
    let mut pos = digits.len();
    if n == 0 {
        pos = pos.saturating_sub(1);
        if let Some(slot) = digits.get_mut(pos) {
            *slot = b'0';
        }
    } else {
        while n > 0 {
            pos = pos.saturating_sub(1);
            let rem = (n % 10) as u8;
            if let Some(slot) = digits.get_mut(pos) {
                *slot = b'0'.saturating_add(rem);
            }
            n /= 10;
        }
    }
    let slice = digits.get(pos..).unwrap_or(&[]);
    let text = std::str::from_utf8(slice).unwrap_or("0");
    push_str_bounded(out, text, max_out)
}

fn push_u32_bounded(out: &mut String, value: u32, max_out: usize) -> Result<(), RecreationError> {
    push_usize_bounded(out, value as usize, max_out)
}

/// Appends `"..."` with `\` and `"` escaped as `\\` and `\"` so free text can never
/// break out of its quoted value or forge structural section/manifest delimiters. Invisible
/// formatting/tag characters are rendered as visible `\u{...}` ASCII escapes; ZWNJ/ZWJ and
/// ordinary Unicode text remain verbatim.
fn push_quoted_escaped(
    out: &mut String,
    text: &str,
    max_out: usize,
) -> Result<(), RecreationError> {
    push_str_bounded(out, "\"", max_out)?;
    for ch in text.chars() {
        match ch {
            '\\' => push_str_bounded(out, "\\\\", max_out)?,
            '"' => push_str_bounded(out, "\\\"", max_out)?,
            c if oia_domain::text::requires_visible_unicode_escape(c) => {
                for escaped in c.escape_unicode() {
                    let mut buf = [0u8; 4];
                    let encoded = escaped.encode_utf8(&mut buf);
                    push_str_bounded(out, encoded, max_out)?;
                }
            }
            c => {
                let mut buf = [0u8; 4];
                let encoded = c.encode_utf8(&mut buf);
                push_str_bounded(out, encoded, max_out)?;
            }
        }
    }
    push_str_bounded(out, "\"", max_out)
}

fn push_version_list(
    out: &mut String,
    versions: &[VersionId],
    max_out: usize,
) -> Result<(), RecreationError> {
    for (idx, v) in versions.iter().enumerate() {
        if idx > 0 {
            push_str_bounded(out, ", ", max_out)?;
        }
        push_str_bounded(out, v.as_str(), max_out)?;
    }
    Ok(())
}

fn render_context_line(
    label: &str,
    field: &ContextField,
    out: &mut String,
    max_out: usize,
) -> Result<(), RecreationError> {
    push_str_bounded(out, "- ", max_out)?;
    push_str_bounded(out, label, max_out)?;
    match field {
        ContextField::Reported {
            description,
            source_versions,
        } => {
            push_str_bounded(out, ": [REPORTED | sources: ", max_out)?;
            push_version_list(out, source_versions, max_out)?;
            push_str_bounded(out, "] text=", max_out)?;
            push_quoted_escaped(out, description, max_out)?;
            push_str_bounded(out, "\n", max_out)
        }
        ContextField::Assumed {
            description,
            assumption_id,
        } => {
            push_str_bounded(out, ": [ASSUMED | assumption: ", max_out)?;
            push_str_bounded(out, assumption_id.as_str(), max_out)?;
            push_str_bounded(out, "] text=", max_out)?;
            push_quoted_escaped(out, description, max_out)?;
            push_str_bounded(out, "\n", max_out)
        }
        ContextField::Unknown { reason } => {
            push_str_bounded(out, ": [UNKNOWN] reason=", max_out)?;
            push_quoted_escaped(out, reason, max_out)?;
            push_str_bounded(out, "\n", max_out)
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
    if let Some(only_variant) = input.conflicting_variants.first()
        && input.conflicting_variants.len() == 1
    {
        return Err(RecreationError::SingleVariantRequiresSharedDetails {
            variant_id: only_variant.variant_id.clone(),
        });
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
    if input.negative_keywords.len() > limits.max_negative_keywords() {
        return Err(RecreationError::LimitExceeded {
            kind: RecreationLimitKind::NegativeKeywords,
            actual: input.negative_keywords.len(),
            max: limits.max_negative_keywords(),
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
    charge_text_bytes(
        input.prompt_id.as_str().len(),
        &mut text_budget,
        limits.max_text_bytes(),
    )?;
    charge_text_bytes(
        input.prompt_version.as_str().len(),
        &mut text_budget,
        limits.max_text_bytes(),
    )?;
    charge_text_bytes(
        input.case_id.as_str().len(),
        &mut text_budget,
        limits.max_text_bytes(),
    )?;
    validate_single_line(
        &input.title,
        "title",
        &mut text_budget,
        limits.max_text_bytes(),
    )?;

    let mut assumption_ids = BTreeSet::new();
    for item in &input.assumptions {
        charge_text_bytes(
            item.assumption_id.as_str().len(),
            &mut text_budget,
            limits.max_text_bytes(),
        )?;
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
        charge_text_bytes(
            item.unknown_id.as_str().len(),
            &mut text_budget,
            limits.max_text_bytes(),
        )?;
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
        limits,
        &mut text_budget,
    )?;
    validate_context_field(
        &input.context.time_context,
        "context.time_context",
        &input.prompt_id,
        &assumption_ids,
        limits,
        &mut text_budget,
    )?;
    validate_context_field(
        &input.context.viewpoint,
        "context.viewpoint",
        &input.prompt_id,
        &assumption_ids,
        limits,
        &mut text_budget,
    )?;
    validate_context_field(
        &input.context.lighting,
        "context.lighting",
        &input.prompt_id,
        &assumption_ids,
        limits,
        &mut text_budget,
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
            validate_source_versions(&input.prompt_id, source_versions, limits, &mut text_budget)?;
            validate_single_line(
                physical_size_unknown_reason,
                "context.apparent_scale.physical_size_unknown_reason",
                &mut text_budget,
                limits.max_text_bytes(),
            )?;
        }
        ApparentScale::CalibratedEstimate {
            description,
            calibration_version,
            uncertainty_bounds,
        } => {
            charge_text_bytes(
                calibration_version.as_str().len(),
                &mut text_budget,
                limits.max_text_bytes(),
            )?;
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
        validate_reported_detail(detail, &mut detail_ids, limits, &mut text_budget)?;
    }

    let mut variant_map: BTreeMap<OpaqueId, &AccountVariant> = BTreeMap::new();
    for variant in &input.conflicting_variants {
        charge_text_bytes(
            variant.variant_id.as_str().len(),
            &mut text_budget,
            limits.max_text_bytes(),
        )?;
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
            return Err(RecreationError::EmptyVariantDetails {
                variant_id: variant.variant_id.clone(),
            });
        }
        for detail in &variant.reported_details {
            validate_reported_detail(detail, &mut detail_ids, limits, &mut text_budget)?;
        }
    }

    for variant in &input.conflicting_variants {
        if variant.conflicts_with.is_empty() {
            return Err(RecreationError::InvalidVariantConflict {
                variant_id: variant.variant_id.clone(),
                target_id: variant.variant_id.clone(),
                reason: "conflicting variants must declare which variant IDs they conflict with",
            });
        }
        if variant.conflicts_with.len() > limits.max_variants() {
            return Err(RecreationError::LimitExceeded {
                kind: RecreationLimitKind::Variants,
                actual: variant.conflicts_with.len(),
                max: limits.max_variants(),
            });
        }
        let mut seen_targets = BTreeSet::new();
        for target_id in &variant.conflicts_with {
            charge_text_bytes(
                target_id.as_str().len(),
                &mut text_budget,
                limits.max_text_bytes(),
            )?;
            if target_id == &variant.variant_id {
                return Err(RecreationError::InvalidVariantConflict {
                    variant_id: variant.variant_id.clone(),
                    target_id: target_id.clone(),
                    reason: "variant cannot conflict with itself",
                });
            }
            let Some(target_variant) = variant_map.get(target_id) else {
                return Err(RecreationError::InvalidVariantConflict {
                    variant_id: variant.variant_id.clone(),
                    target_id: target_id.clone(),
                    reason: "target variant is not declared in this prompt",
                });
            };
            if !seen_targets.insert(target_id.clone()) {
                return Err(RecreationError::InvalidVariantConflict {
                    variant_id: variant.variant_id.clone(),
                    target_id: target_id.clone(),
                    reason: "duplicate conflict target",
                });
            }
            if !target_variant.conflicts_with.contains(&variant.variant_id) {
                return Err(RecreationError::InvalidVariantConflict {
                    variant_id: variant.variant_id.clone(),
                    target_id: target_id.clone(),
                    reason: "conflict links between variants must be symmetric",
                });
            }
        }
    }

    let mut context_unknown_ids = BTreeSet::new();
    for (id_str, field) in [
        ("context:scene", &input.context.scene),
        ("context:time", &input.context.time_context),
        ("context:viewpoint", &input.context.viewpoint),
        ("context:lighting", &input.context.lighting),
    ] {
        if matches!(field, ContextField::Unknown { .. }) {
            context_unknown_ids.insert(OpaqueId::new(id_str)?);
        }
    }
    if matches!(
        &input.context.apparent_scale,
        ApparentScale::Unknown { .. } | ApparentScale::ReportedAngularOrRelative { .. }
    ) {
        context_unknown_ids.insert(OpaqueId::new("context:apparent_scale")?);
    }

    let mut pinned_source_versions = BTreeSet::new();
    for field in [
        &input.context.scene,
        &input.context.time_context,
        &input.context.viewpoint,
        &input.context.lighting,
    ] {
        if let ContextField::Reported {
            source_versions, ..
        } = field
        {
            for v in source_versions {
                pinned_source_versions.insert(v.clone());
            }
        }
    }
    match &input.context.apparent_scale {
        ApparentScale::ReportedAngularOrRelative {
            source_versions, ..
        } => {
            for v in source_versions {
                pinned_source_versions.insert(v.clone());
            }
        }
        ApparentScale::CalibratedEstimate {
            calibration_version,
            ..
        } => {
            pinned_source_versions.insert(calibration_version.clone());
        }
        ApparentScale::Unknown { .. } => {}
    }
    for detail in &input.shared_reported_details {
        for v in &detail.source_versions {
            pinned_source_versions.insert(v.clone());
        }
    }
    for variant in &input.conflicting_variants {
        for detail in &variant.reported_details {
            for v in &detail.source_versions {
                pinned_source_versions.insert(v.clone());
            }
        }
    }

    let mut negative_ids = BTreeSet::new();
    let mut validated_negatives = Vec::new();
    validated_negatives
        .try_reserve_exact(input.negative_keywords.len())
        .map_err(|_| RecreationError::AllocationFailed)?;

    for neg in &input.negative_keywords {
        charge_text_bytes(
            neg.negative_id.as_str().len(),
            &mut text_budget,
            limits.max_text_bytes(),
        )?;
        if !negative_ids.insert(neg.negative_id.clone()) {
            return Err(RecreationError::DuplicateId {
                kind: "negative_keyword",
                id: neg.negative_id.clone(),
            });
        }
        validate_single_line(
            &neg.keyword,
            "negative_keyword.keyword",
            &mut text_budget,
            limits.max_text_bytes(),
        )?;
        validate_single_line(
            &neg.rationale,
            "negative_keyword.rationale",
            &mut text_budget,
            limits.max_text_bytes(),
        )?;
        let Some(origin) = &neg.origin else {
            return Err(RecreationError::MissingNegativeKeywordOrigin {
                negative_id: neg.negative_id.clone(),
            });
        };
        match origin {
            NegativeKeywordOrigin::Unknown { unknown_id } => {
                charge_text_bytes(
                    unknown_id.as_str().len(),
                    &mut text_budget,
                    limits.max_text_bytes(),
                )?;
                if !unknown_ids.contains(unknown_id) && !context_unknown_ids.contains(unknown_id) {
                    return Err(RecreationError::UnknownNegativeOriginReference {
                        negative_id: neg.negative_id.clone(),
                        kind: "unknown",
                        target_id: unknown_id.clone(),
                    });
                }
            }
            NegativeKeywordOrigin::ContradictedDetail {
                detail_id,
                contradicted_statement,
                contradicted_source_version,
                contradicting_source_versions,
            } => {
                charge_text_bytes(
                    detail_id.as_str().len(),
                    &mut text_budget,
                    limits.max_text_bytes(),
                )?;
                if detail_ids.contains(detail_id) {
                    return Err(RecreationError::ContradictedDetailInReportedDetails {
                        negative_id: neg.negative_id.clone(),
                        detail_id: detail_id.clone(),
                    });
                }
                validate_single_line(
                    contradicted_statement,
                    "negative_keyword.contradicted_statement",
                    &mut text_budget,
                    limits.max_text_bytes(),
                )?;
                charge_text_bytes(
                    contradicted_source_version.as_str().len(),
                    &mut text_budget,
                    limits.max_text_bytes(),
                )?;
                validate_source_versions(
                    &neg.negative_id,
                    contradicting_source_versions,
                    limits,
                    &mut text_budget,
                )?;
                for v in contradicting_source_versions {
                    if v == contradicted_source_version {
                        return Err(RecreationError::SelfContradictingSourceVersion {
                            negative_id: neg.negative_id.clone(),
                            source_version: v.clone(),
                        });
                    }
                    if !pinned_source_versions.contains(v) {
                        return Err(RecreationError::UnpinnedContradictingSourceVersion {
                            negative_id: neg.negative_id.clone(),
                            source_version: v.clone(),
                        });
                    }
                }
            }
            NegativeKeywordOrigin::Variant {
                for_variant_id,
                excluded_variant_id,
            } => {
                charge_text_bytes(
                    for_variant_id.as_str().len(),
                    &mut text_budget,
                    limits.max_text_bytes(),
                )?;
                charge_text_bytes(
                    excluded_variant_id.as_str().len(),
                    &mut text_budget,
                    limits.max_text_bytes(),
                )?;
                let Some(for_variant) = variant_map.get(for_variant_id) else {
                    return Err(RecreationError::UnknownNegativeOriginReference {
                        negative_id: neg.negative_id.clone(),
                        kind: "variant",
                        target_id: for_variant_id.clone(),
                    });
                };
                if !variant_map.contains_key(excluded_variant_id) {
                    return Err(RecreationError::UnknownNegativeOriginReference {
                        negative_id: neg.negative_id.clone(),
                        kind: "variant",
                        target_id: excluded_variant_id.clone(),
                    });
                }
                if for_variant_id == excluded_variant_id
                    || !for_variant.conflicts_with.contains(excluded_variant_id)
                {
                    return Err(RecreationError::InvalidVariantConflict {
                        variant_id: for_variant_id.clone(),
                        target_id: excluded_variant_id.clone(),
                        reason: "variant negative must exclude a declared conflicting variant",
                    });
                }
            }
        }
        validated_negatives.push(ValidatedNegativeKeyword {
            negative_id: neg.negative_id.clone(),
            keyword: neg.keyword.clone(),
            rationale: neg.rationale.clone(),
            origin: origin.clone(),
        });
    }

    let mut asset_ids = BTreeSet::new();
    let mut permitted_assets = Vec::new();
    let mut omitted_assets = Vec::new();

    for asset in &input.reference_assets {
        charge_text_bytes(
            asset.asset_id.as_str().len(),
            &mut text_budget,
            limits.max_text_bytes(),
        )?;
        charge_text_bytes(
            asset.source_version.as_str().len(),
            &mut text_budget,
            limits.max_text_bytes(),
        )?;
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

    let portable_prompt_text = render_portable_text(
        input,
        &validated_negatives,
        &permitted_assets,
        &omitted_assets,
        limits,
    )?;

    Ok(ExportedRecreationPrompt {
        schema_version: SchemaVersion::V1,
        prompt_id: input.prompt_id.clone(),
        prompt_version: input.prompt_version.clone(),
        case_id: input.case_id.clone(),
        title: input.title.clone(),
        context: input.context.clone(),
        shared_reported_details: input.shared_reported_details.clone(),
        conflicting_variants: input.conflicting_variants.clone(),
        assumptions: input.assumptions.clone(),
        unknowns: input.unknowns.clone(),
        negative_keywords: validated_negatives,
        permitted_assets,
        omitted_assets,
        portable_prompt_text,
    })
}

fn render_reported_detail_line(
    indent: &str,
    detail: &ReportedDetail,
    out: &mut String,
    max_out: usize,
) -> Result<(), RecreationError> {
    push_str_bounded(out, indent, max_out)?;
    push_str_bounded(out, "- [", max_out)?;
    push_str_bounded(out, detail.detail_id.as_str(), max_out)?;
    push_str_bounded(out, "] sources=[", max_out)?;
    push_version_list(out, &detail.source_versions, max_out)?;
    push_str_bounded(out, "] statement=", max_out)?;
    push_quoted_escaped(out, &detail.statement, max_out)?;
    if let Some(note) = &detail.source_note {
        push_str_bounded(out, " note=", max_out)?;
        push_quoted_escaped(out, note, max_out)?;
    }
    push_str_bounded(out, "\n", max_out)
}

fn render_negative_keyword_line(
    prefix: &str,
    neg: &ValidatedNegativeKeyword,
    out: &mut String,
    max_out: usize,
) -> Result<(), RecreationError> {
    push_str_bounded(out, prefix, max_out)?;
    push_str_bounded(out, "[", max_out)?;
    push_str_bounded(out, neg.negative_id.as_str(), max_out)?;
    push_str_bounded(out, "] ", max_out)?;
    match &neg.origin {
        NegativeKeywordOrigin::Unknown { unknown_id } => {
            push_str_bounded(out, "origin=unknown unknown_id=", max_out)?;
            push_str_bounded(out, unknown_id.as_str(), max_out)?;
        }
        NegativeKeywordOrigin::ContradictedDetail {
            detail_id,
            contradicted_statement,
            contradicted_source_version,
            contradicting_source_versions,
        } => {
            push_str_bounded(out, "origin=contradicted_detail detail_id=", max_out)?;
            push_str_bounded(out, detail_id.as_str(), max_out)?;
            push_str_bounded(out, " contradicted_source=", max_out)?;
            push_str_bounded(out, contradicted_source_version.as_str(), max_out)?;
            push_str_bounded(out, " contradicted_statement=", max_out)?;
            push_quoted_escaped(out, contradicted_statement, max_out)?;
            push_str_bounded(out, " contradicting_sources=[", max_out)?;
            push_version_list(out, contradicting_source_versions, max_out)?;
            push_str_bounded(out, "]", max_out)?;
        }
        NegativeKeywordOrigin::Variant {
            for_variant_id,
            excluded_variant_id,
        } => {
            push_str_bounded(out, "origin=variant for_variant=", max_out)?;
            push_str_bounded(out, for_variant_id.as_str(), max_out)?;
            push_str_bounded(out, " excludes_variant=", max_out)?;
            push_str_bounded(out, excluded_variant_id.as_str(), max_out)?;
        }
    }
    push_str_bounded(out, " keyword=", max_out)?;
    push_quoted_escaped(out, &neg.keyword, max_out)?;
    push_str_bounded(out, " rationale=", max_out)?;
    push_quoted_escaped(out, &neg.rationale, max_out)?;
    push_str_bounded(out, "\n", max_out)
}

fn render_positive_context_entry<'a>(
    indent: &str,
    label: &str,
    field: &'a ContextField,
    assumption_map: &BTreeMap<&OpaqueId, &ExplicitAssumption>,
    emitted_assumptions: &mut BTreeSet<&'a OpaqueId>,
    out: &mut String,
    max_out: usize,
) -> Result<(), RecreationError> {
    match field {
        ContextField::Reported {
            description,
            source_versions,
        } => {
            push_str_bounded(out, indent, max_out)?;
            push_str_bounded(out, "- [context:", max_out)?;
            push_str_bounded(out, label, max_out)?;
            push_str_bounded(out, "] origin=source_context sources=[", max_out)?;
            push_version_list(out, source_versions, max_out)?;
            push_str_bounded(out, "] keyword=", max_out)?;
            push_quoted_escaped(out, description, max_out)?;
            push_str_bounded(out, "\n", max_out)
        }
        ContextField::Assumed {
            description,
            assumption_id,
        } => {
            emitted_assumptions.insert(assumption_id);
            push_str_bounded(out, indent, max_out)?;
            push_str_bounded(out, "- [context:", max_out)?;
            push_str_bounded(out, label, max_out)?;
            push_str_bounded(out, "] origin=assumption assumption_id=", max_out)?;
            push_str_bounded(out, assumption_id.as_str(), max_out)?;
            push_str_bounded(out, " keyword=", max_out)?;
            push_quoted_escaped(out, description, max_out)?;
            if let Some(assumption) = assumption_map.get(assumption_id) {
                push_str_bounded(out, " assumption=", max_out)?;
                push_quoted_escaped(out, &assumption.assumption, max_out)?;
                push_str_bounded(out, " rationale=", max_out)?;
                push_quoted_escaped(out, &assumption.rationale, max_out)?;
            }
            push_str_bounded(out, "\n", max_out)
        }
        ContextField::Unknown { .. } => Ok(()),
    }
}

fn render_positive_scale_entry(
    indent: &str,
    apparent_scale: &ApparentScale,
    out: &mut String,
    max_out: usize,
) -> Result<(), RecreationError> {
    match apparent_scale {
        ApparentScale::ReportedAngularOrRelative {
            description,
            source_versions,
            physical_size_unknown_reason,
        } => {
            push_str_bounded(out, indent, max_out)?;
            push_str_bounded(
                out,
                "- [context:apparent_scale] origin=source_angular_scale sources=[",
                max_out,
            )?;
            push_version_list(out, source_versions, max_out)?;
            push_str_bounded(out, "] keyword=", max_out)?;
            push_quoted_escaped(out, description, max_out)?;
            push_str_bounded(out, " physical_size_unknown_reason=", max_out)?;
            push_quoted_escaped(out, physical_size_unknown_reason, max_out)?;
            push_str_bounded(out, "\n", max_out)
        }
        ApparentScale::CalibratedEstimate {
            description,
            calibration_version,
            uncertainty_bounds,
        } => {
            push_str_bounded(out, indent, max_out)?;
            push_str_bounded(
                out,
                "- [context:apparent_scale] origin=calibrated_scale calibration_version=",
                max_out,
            )?;
            push_str_bounded(out, calibration_version.as_str(), max_out)?;
            push_str_bounded(out, " keyword=", max_out)?;
            push_quoted_escaped(out, description, max_out)?;
            push_str_bounded(out, " uncertainty_bounds=", max_out)?;
            push_quoted_escaped(out, uncertainty_bounds, max_out)?;
            push_str_bounded(out, "\n", max_out)
        }
        ApparentScale::Unknown { .. } => Ok(()),
    }
}

fn render_positive_entries_for_scope(
    input: &RecreationPromptInput,
    variant: Option<&AccountVariant>,
    assumption_map: &BTreeMap<&OpaqueId, &ExplicitAssumption>,
    indent: &str,
    out: &mut String,
    max_out: usize,
) -> Result<(), RecreationError> {
    push_str_bounded(out, indent, max_out)?;
    push_str_bounded(out, "- [prompt:title] title=", max_out)?;
    push_quoted_escaped(out, &input.title, max_out)?;
    push_str_bounded(out, "\n", max_out)?;

    let mut emitted_assumptions = BTreeSet::new();
    render_positive_context_entry(
        indent,
        "scene",
        &input.context.scene,
        assumption_map,
        &mut emitted_assumptions,
        out,
        max_out,
    )?;
    render_positive_context_entry(
        indent,
        "time",
        &input.context.time_context,
        assumption_map,
        &mut emitted_assumptions,
        out,
        max_out,
    )?;
    render_positive_context_entry(
        indent,
        "viewpoint",
        &input.context.viewpoint,
        assumption_map,
        &mut emitted_assumptions,
        out,
        max_out,
    )?;
    render_positive_context_entry(
        indent,
        "lighting",
        &input.context.lighting,
        assumption_map,
        &mut emitted_assumptions,
        out,
        max_out,
    )?;
    render_positive_scale_entry(indent, &input.context.apparent_scale, out, max_out)?;

    for detail in &input.shared_reported_details {
        push_str_bounded(out, indent, max_out)?;
        push_str_bounded(out, "- [", max_out)?;
        push_str_bounded(out, detail.detail_id.as_str(), max_out)?;
        push_str_bounded(out, "] origin=source_detail sources=[", max_out)?;
        push_version_list(out, &detail.source_versions, max_out)?;
        push_str_bounded(out, "] keyword=", max_out)?;
        push_quoted_escaped(out, &detail.statement, max_out)?;
        if let Some(note) = &detail.source_note {
            push_str_bounded(out, " note=", max_out)?;
            push_quoted_escaped(out, note, max_out)?;
        }
        push_str_bounded(out, "\n", max_out)?;
    }

    if let Some(v) = variant {
        for detail in &v.reported_details {
            push_str_bounded(out, indent, max_out)?;
            push_str_bounded(out, "- [", max_out)?;
            push_str_bounded(out, detail.detail_id.as_str(), max_out)?;
            push_str_bounded(out, "] origin=variant_source_detail variant_id=", max_out)?;
            push_str_bounded(out, v.variant_id.as_str(), max_out)?;
            push_str_bounded(out, " sources=[", max_out)?;
            push_version_list(out, &detail.source_versions, max_out)?;
            push_str_bounded(out, "] keyword=", max_out)?;
            push_quoted_escaped(out, &detail.statement, max_out)?;
            if let Some(note) = &detail.source_note {
                push_str_bounded(out, " note=", max_out)?;
                push_quoted_escaped(out, note, max_out)?;
            }
            push_str_bounded(out, "\n", max_out)?;
        }
    }

    for item in &input.assumptions {
        if emitted_assumptions.contains(&item.assumption_id) {
            continue;
        }
        push_str_bounded(out, indent, max_out)?;
        push_str_bounded(out, "- [", max_out)?;
        push_str_bounded(out, item.assumption_id.as_str(), max_out)?;
        push_str_bounded(out, "] origin=assumption keyword=", max_out)?;
        push_quoted_escaped(out, &item.assumption, max_out)?;
        push_str_bounded(out, " rationale=", max_out)?;
        push_quoted_escaped(out, &item.rationale, max_out)?;
        push_str_bounded(out, "\n", max_out)?;
    }

    Ok(())
}

fn negative_applies_to_variant(neg: &ValidatedNegativeKeyword, variant_id: &OpaqueId) -> bool {
    match &neg.origin {
        NegativeKeywordOrigin::Unknown { .. }
        | NegativeKeywordOrigin::ContradictedDetail { .. } => true,
        NegativeKeywordOrigin::Variant { for_variant_id, .. } => for_variant_id == variant_id,
    }
}

fn render_portable_text(
    input: &RecreationPromptInput,
    validated_negatives: &[ValidatedNegativeKeyword],
    permitted_assets: &[PermittedReferenceAsset],
    omitted_assets: &[OmittedReferenceAsset],
    limits: RecreationLimits,
) -> Result<String, RecreationError> {
    let max_out = limits.max_output_bytes();
    let mut out = String::new();

    push_str_bounded(&mut out, "# ", max_out)?;
    push_str_bounded(&mut out, VISIBLE_RECREATION_LABEL, max_out)?;
    push_str_bounded(&mut out, "\n# Caveat: ", max_out)?;
    push_str_bounded(&mut out, FIDELITY_AND_EXTERNAL_LABEL_CAVEAT, max_out)?;
    push_str_bounded(
        &mut out,
        "\n\n## Prompt Metadata\n- schema_version: ",
        max_out,
    )?;
    push_u32_bounded(&mut out, SchemaVersion::V1.as_u32(), max_out)?;
    push_str_bounded(&mut out, "\n- prompt_id: ", max_out)?;
    push_str_bounded(&mut out, input.prompt_id.as_str(), max_out)?;
    push_str_bounded(&mut out, "\n- prompt_version: ", max_out)?;
    push_str_bounded(&mut out, input.prompt_version.as_str(), max_out)?;
    push_str_bounded(&mut out, "\n- case_id: ", max_out)?;
    push_str_bounded(&mut out, input.case_id.as_str(), max_out)?;
    push_str_bounded(&mut out, "\n- title: ", max_out)?;
    push_quoted_escaped(&mut out, &input.title, max_out)?;
    push_str_bounded(
        &mut out,
        "\n\n## Scene, Time, Viewpoint, Lighting, and Scale\n",
        max_out,
    )?;

    render_context_line("scene", &input.context.scene, &mut out, max_out)?;
    render_context_line("time", &input.context.time_context, &mut out, max_out)?;
    render_context_line("viewpoint", &input.context.viewpoint, &mut out, max_out)?;
    render_context_line("lighting", &input.context.lighting, &mut out, max_out)?;

    match &input.context.apparent_scale {
        ApparentScale::ReportedAngularOrRelative {
            description,
            source_versions,
            physical_size_unknown_reason,
        } => {
            push_str_bounded(
                &mut out,
                "- apparent_scale: [REPORTED ANGULAR/RELATIVE | sources: ",
                max_out,
            )?;
            push_version_list(&mut out, source_versions, max_out)?;
            push_str_bounded(&mut out, "] description=", max_out)?;
            push_quoted_escaped(&mut out, description, max_out)?;
            push_str_bounded(&mut out, " physical_size_unknown_reason=", max_out)?;
            push_quoted_escaped(&mut out, physical_size_unknown_reason, max_out)?;
            push_str_bounded(&mut out, "\n", max_out)?;
        }
        ApparentScale::CalibratedEstimate {
            description,
            calibration_version,
            uncertainty_bounds,
        } => {
            push_str_bounded(
                &mut out,
                "- apparent_scale: [CALIBRATED ESTIMATE | calibration: ",
                max_out,
            )?;
            push_str_bounded(&mut out, calibration_version.as_str(), max_out)?;
            push_str_bounded(&mut out, "] description=", max_out)?;
            push_quoted_escaped(&mut out, description, max_out)?;
            push_str_bounded(&mut out, " uncertainty_bounds=", max_out)?;
            push_quoted_escaped(&mut out, uncertainty_bounds, max_out)?;
            push_str_bounded(&mut out, "\n", max_out)?;
        }
        ApparentScale::Unknown { reason } => {
            push_str_bounded(&mut out, "- apparent_scale: [UNKNOWN] reason=", max_out)?;
            push_quoted_escaped(&mut out, reason, max_out)?;
            push_str_bounded(&mut out, "\n", max_out)?;
        }
    }

    push_str_bounded(
        &mut out,
        "\n## Shared Reported Details (Pinned to Sources)\n",
        max_out,
    )?;
    if input.shared_reported_details.is_empty() {
        push_str_bounded(
            &mut out,
            "- (none; see conflicting account variants below)\n",
            max_out,
        )?;
    } else {
        for detail in &input.shared_reported_details {
            render_reported_detail_line("", detail, &mut out, max_out)?;
        }
    }

    push_str_bounded(
        &mut out,
        "\n## Conflicting Account Variants (Do Not Merge)\n",
        max_out,
    )?;
    if input.conflicting_variants.is_empty() {
        push_str_bounded(&mut out, "- (no conflicting variants recorded)\n", max_out)?;
    } else {
        for variant in &input.conflicting_variants {
            push_str_bounded(&mut out, "### Variant `", max_out)?;
            push_str_bounded(&mut out, variant.variant_id.as_str(), max_out)?;
            push_str_bounded(&mut out, "` (conflicts_with: [", max_out)?;
            for (idx, cid) in variant.conflicts_with.iter().enumerate() {
                if idx > 0 {
                    push_str_bounded(&mut out, ", ", max_out)?;
                }
                push_str_bounded(&mut out, cid.as_str(), max_out)?;
            }
            push_str_bounded(&mut out, "])\n- summary: ", max_out)?;
            push_quoted_escaped(&mut out, &variant.summary, max_out)?;
            push_str_bounded(&mut out, "\n", max_out)?;
            for detail in &variant.reported_details {
                render_reported_detail_line("  ", detail, &mut out, max_out)?;
            }
        }
    }

    push_str_bounded(
        &mut out,
        "\n## Explicit Assumptions (Not Observed Evidence)\n",
        max_out,
    )?;
    if input.assumptions.is_empty() {
        push_str_bounded(&mut out, "- (none)\n", max_out)?;
    } else {
        for item in &input.assumptions {
            push_str_bounded(&mut out, "- [", max_out)?;
            push_str_bounded(&mut out, item.assumption_id.as_str(), max_out)?;
            push_str_bounded(&mut out, "] assumption=", max_out)?;
            push_quoted_escaped(&mut out, &item.assumption, max_out)?;
            push_str_bounded(&mut out, " rationale=", max_out)?;
            push_quoted_escaped(&mut out, &item.rationale, max_out)?;
            push_str_bounded(&mut out, "\n", max_out)?;
        }
    }

    push_str_bounded(
        &mut out,
        "\n## Explicit Unknowns (Do Not Invent Measurements)\n",
        max_out,
    )?;
    if input.unknowns.is_empty() {
        push_str_bounded(&mut out, "- (none)\n", max_out)?;
    } else {
        for item in &input.unknowns {
            push_str_bounded(&mut out, "- [", max_out)?;
            push_str_bounded(&mut out, item.unknown_id.as_str(), max_out)?;
            push_str_bounded(&mut out, "] aspect=", max_out)?;
            push_quoted_escaped(&mut out, &item.aspect, max_out)?;
            push_str_bounded(&mut out, " impact=", max_out)?;
            push_quoted_escaped(&mut out, &item.impact_on_recreation, max_out)?;
            push_str_bounded(&mut out, "\n", max_out)?;
        }
    }

    if !validated_negatives.is_empty() {
        let assumption_map: BTreeMap<&OpaqueId, &ExplicitAssumption> = input
            .assumptions
            .iter()
            .map(|a| (&a.assumption_id, a))
            .collect();

        push_str_bounded(
            &mut out,
            "\n## Positive Prompt Keywords (Source-Pinned Details and Declared Assumptions)\n",
            max_out,
        )?;
        if input.conflicting_variants.is_empty() {
            render_positive_entries_for_scope(input, None, &assumption_map, "", &mut out, max_out)?;
        } else {
            for variant in &input.conflicting_variants {
                push_str_bounded(&mut out, "### Variant `", max_out)?;
                push_str_bounded(&mut out, variant.variant_id.as_str(), max_out)?;
                push_str_bounded(&mut out, "`\n", max_out)?;
                render_positive_entries_for_scope(
                    input,
                    Some(variant),
                    &assumption_map,
                    "  ",
                    &mut out,
                    max_out,
                )?;
            }
        }

        push_str_bounded(
            &mut out,
            "\n## Negative Prompt Keywords (Evidence-Derived Exclusions)\n",
            max_out,
        )?;
        if input.conflicting_variants.is_empty() {
            for neg in validated_negatives {
                render_negative_keyword_line("- ", neg, &mut out, max_out)?;
            }
        } else {
            for variant in &input.conflicting_variants {
                push_str_bounded(&mut out, "### Variant `", max_out)?;
                push_str_bounded(&mut out, variant.variant_id.as_str(), max_out)?;
                push_str_bounded(&mut out, "`\n", max_out)?;
                let mut any_for_variant = false;
                for neg in validated_negatives {
                    if negative_applies_to_variant(neg, &variant.variant_id) {
                        any_for_variant = true;
                        render_negative_keyword_line("  - ", neg, &mut out, max_out)?;
                    }
                }
                if !any_for_variant {
                    push_str_bounded(&mut out, "  - (none)\n", max_out)?;
                }
            }
        }
    }

    push_str_bounded(
        &mut out,
        "\n## Permitted Reference Asset Manifest\n",
        max_out,
    )?;
    if permitted_assets.is_empty() {
        push_str_bounded(
            &mut out,
            "- (no permitted reference assets exported)\n",
            max_out,
        )?;
    } else {
        for asset in permitted_assets {
            push_str_bounded(&mut out, "- [", max_out)?;
            push_str_bounded(&mut out, asset.asset_id.as_str(), max_out)?;
            push_str_bounded(&mut out, "] source_version=", max_out)?;
            push_str_bounded(&mut out, asset.source_version.as_str(), max_out)?;
            push_str_bounded(&mut out, " sha256=", max_out)?;
            push_str_bounded(&mut out, &asset.sha256_hex, max_out)?;
            push_str_bounded(&mut out, " media_type=", max_out)?;
            push_quoted_escaped(&mut out, &asset.media_type, max_out)?;
            push_str_bounded(&mut out, " role=", max_out)?;
            push_quoted_escaped(&mut out, &asset.role, max_out)?;
            push_str_bounded(&mut out, " conditions=", max_out)?;
            push_quoted_escaped(&mut out, &asset.usage_conditions, max_out)?;
            push_str_bounded(&mut out, "\n", max_out)?;
        }
    }

    if !omitted_assets.is_empty() {
        push_str_bounded(
            &mut out,
            "\n## Omitted Reference Assets (Redistribution Restricted)\n",
            max_out,
        )?;
        for omitted in omitted_assets {
            push_str_bounded(&mut out, "- [", max_out)?;
            push_str_bounded(&mut out, omitted.asset_id.as_str(), max_out)?;
            push_str_bounded(&mut out, "] source_version=", max_out)?;
            push_str_bounded(&mut out, omitted.source_version.as_str(), max_out)?;
            push_str_bounded(&mut out, " reason=", max_out)?;
            push_quoted_escaped(&mut out, &omitted.reason, max_out)?;
            push_str_bounded(&mut out, " conditions=", max_out)?;
            push_quoted_escaped(&mut out, &omitted.usage_conditions, max_out)?;
            push_str_bounded(&mut out, "\n", max_out)?;
        }
    }

    Ok(out)
}

/// Validates an imported recreation image against its exported prompt so that it
/// preserves prompt/settings lineage and exact `VISIBLE_RECREATION_LABEL` metadata
/// labeling and can never be promoted to observation evidence (`MEDIA-04`, `A10`).
///
/// This function checks in-memory metadata and 64-character lowercase hexadecimal
/// SHA-256 format; it does not decode image pixels or verify visual watermarks inside
/// rendered pixel buffers.
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
    match (
        exported_prompt.conflicting_variants.is_empty(),
        &input.variant_id,
    ) {
        (false, None) => {
            return Err(RecreationError::MissingVariantSelection {
                prompt_id: exported_prompt.prompt_id.clone(),
            });
        }
        (false, Some(variant_id)) => {
            if !exported_prompt
                .conflicting_variants
                .iter()
                .any(|v| &v.variant_id == variant_id)
            {
                return Err(RecreationError::UnknownVariantReference {
                    variant_id: variant_id.clone(),
                });
            }
        }
        (true, Some(variant_id)) => {
            return Err(RecreationError::UnknownVariantReference {
                variant_id: variant_id.clone(),
            });
        }
        (true, None) => {}
    }

    validate_sha256_hex(&input.sha256_hex, &input.artifact_id)?;

    let mut text_budget = 0usize;
    charge_text_bytes(
        input.artifact_id.as_str().len(),
        &mut text_budget,
        max_text_bytes,
    )?;
    charge_text_bytes(
        input.artifact_version.as_str().len(),
        &mut text_budget,
        max_text_bytes,
    )?;
    charge_text_bytes(
        input.prompt_id.as_str().len(),
        &mut text_budget,
        max_text_bytes,
    )?;
    charge_text_bytes(
        input.prompt_version.as_str().len(),
        &mut text_budget,
        max_text_bytes,
    )?;
    if let Some(variant_id) = &input.variant_id {
        charge_text_bytes(variant_id.as_str().len(), &mut text_budget, max_text_bytes)?;
    }

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

    if input.visible_label != VISIBLE_RECREATION_LABEL {
        return Err(RecreationError::MissingVisibleRecreationLabel {
            artifact_id: input.artifact_id.clone(),
        });
    }

    let applicable_negatives: Vec<ValidatedNegativeKeyword> = match &input.variant_id {
        Some(selected_variant_id) => exported_prompt
            .negative_keywords
            .iter()
            .filter(|neg| negative_applies_to_variant(neg, selected_variant_id))
            .cloned()
            .collect(),
        None => exported_prompt.negative_keywords.clone(),
    };

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
        negative_keywords: applicable_negatives,
    })
}
