//! Transient evidence-set recreation composition (MEDIA-04, COLLAB-02/03, #139).
//! Application orchestration only: no extraction, generation, persistence or I/O.

use oia_analysis::statement_comparison::{
    AspectComparison, CodedValue, ComparisonError, ComparisonRequest, ExplanationExposure,
    SourceClass, SourceSummary, StatementComparison, StatementSource, StopReason, TextCapture,
    compare_statements,
};
use oia_domain::witness::AccountOrigin;
use oia_domain::{IdError, OpaqueId, VersionId};
use oia_media::recreation::{
    AccountVariant, ApparentScale, AssetExportPolicy, ContextField, ExplicitAssumption,
    ExplicitUnknown, ExportedRecreationPrompt, RecreationError, RecreationLimits,
    RecreationPromptInput, ReferenceAssetInput, ReportedDetail, SceneContext,
    export_recreation_prompt,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt;

pub const MAX_ASPECTS: usize = 128;
pub const MAX_OPTIONS: usize = 32;
pub const MAX_INPUT_TEXT_BYTES: usize = 262_144;
pub const MAX_PORTABLE_BYTES: usize = 1_048_576;
pub const MAX_PREDECESSORS: usize = 64;
pub const COMPOSER_METHOD: &str = "cited-statement-recreation-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContextChoice<'a> {
    #[default]
    Unknown,
    Aspect(&'a str),
    Assumption(&'a OpaqueId),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContextBindings<'a> {
    pub scene: ContextChoice<'a>,
    pub time: ContextChoice<'a>,
    pub viewpoint: ContextChoice<'a>,
    pub lighting: ContextChoice<'a>,
    /// Analyst-selected angular/relative appearance only; never physical size.
    pub apparent_scale_aspect: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositionRequest<'a> {
    pub comparison: ComparisonRequest<'a>,
    pub prompt_id: OpaqueId,
    pub prompt_version: VersionId,
    pub case_id: OpaqueId,
    pub title: &'a str,
    pub context: ContextBindings<'a>,
    /// Explicitly requested aspects with no coded reports become unknowns.
    pub required_aspects: &'a [&'a str],
    pub assumptions: &'a [ExplicitAssumption],
    pub reference_assets: &'a [ReferenceAssetInput],
    pub reason: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangedSection {
    ComparisonMethod,
    CandidateReview,
    Context,
    Assumptions,
    Unknowns,
    ReferenceAssets,
    AssetPolicy,
    Title,
    ConflictingVariants,
    ExportLimits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositionChanges {
    pub predecessor: Option<VersionId>,
    pub reason: String,
    pub added_sources: Vec<VersionId>,
    pub removed_sources: Vec<VersionId>,
    pub changed_accounts: Vec<OpaqueId>,
    pub added_details: Vec<OpaqueId>,
    pub removed_details: Vec<OpaqueId>,
    pub changed_details: Vec<OpaqueId>,
    pub sections: Vec<ChangedSection>,
}

/// Private state prevents relabeling an issued composition or mutating its predecessors.
/// Borrowed source/coding inputs must remain retained by the caller for this lifetime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposedRecreationPrompt<'a> {
    request: CompositionRequest<'a>,
    comparison: StatementComparison<'a>,
    input: RecreationPromptInput,
    exported: ExportedRecreationPrompt,
    changes: CompositionChanges,
    predecessors: Vec<VersionId>,
    asset_policy: AssetExportPolicy,
    limits: RecreationLimits,
    portable_text: String,
}

impl<'a> ComposedRecreationPrompt<'a> {
    pub fn comparison(&self) -> &StatementComparison<'a> {
        &self.comparison
    }
    pub fn input(&self) -> &RecreationPromptInput {
        &self.input
    }
    pub fn exported(&self) -> &ExportedRecreationPrompt {
        &self.exported
    }
    pub fn changes(&self) -> &CompositionChanges {
        &self.changes
    }
    pub fn portable_text(&self) -> &str {
        &self.portable_text
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompositionError {
    Comparison(ComparisonError),
    Recreation(RecreationError),
    Identifier(IdError),
    Limit(&'static str),
    InvalidOption(&'static str),
    DifferentPromptOrCase,
    ReusedPromptVersion,
    ChangedPinnedSource(VersionId),
    ChangedCodingVersion,
    Stopped(StopReason),
}

impl fmt::Display for CompositionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Comparison(error) => write!(formatter, "comparison: {error}"),
            Self::Recreation(error) => write!(formatter, "recreation: {error}"),
            Self::Identifier(error) => write!(formatter, "identifier: {error}"),
            Self::Limit(field) => write!(formatter, "composition budget exceeded: {field}"),
            Self::InvalidOption(field) => write!(formatter, "invalid composition option: {field}"),
            Self::DifferentPromptOrCase => {
                formatter.write_str("predecessor belongs to another prompt or case")
            }
            Self::ReusedPromptVersion => {
                formatter.write_str("regeneration requires a new prompt version")
            }
            Self::ChangedPinnedSource(version) => {
                write!(formatter, "source changed without a new version: {version}")
            }
            Self::ChangedCodingVersion => {
                formatter.write_str("changed coding requires a new coding version")
            }
            Self::Stopped(reason) => write!(formatter, "composition stopped: {reason:?}"),
        }
    }
}

impl std::error::Error for CompositionError {}

impl From<IdError> for CompositionError {
    fn from(error: IdError) -> Self {
        Self::Identifier(error)
    }
}

fn charge(total: &mut usize, bytes: usize) -> Result<(), CompositionError> {
    *total = total
        .checked_add(bytes)
        .ok_or(CompositionError::Limit("input text"))?;
    if *total > MAX_INPUT_TEXT_BYTES {
        return Err(CompositionError::Limit("input text"));
    }
    Ok(())
}

fn option_text(text: &str, total: &mut usize) -> Result<(), CompositionError> {
    charge(total, text.len())?;
    if text.trim().is_empty() || text.len() > 4_096 {
        return Err(CompositionError::InvalidOption("empty or oversized text"));
    }
    Ok(())
}

fn preflight(request: &CompositionRequest<'_>) -> Result<usize, CompositionError> {
    if request.required_aspects.len() > MAX_ASPECTS
        || request.assumptions.len() > MAX_OPTIONS
        || request.reference_assets.len() > MAX_OPTIONS
    {
        return Err(CompositionError::Limit("options"));
    }
    let mut bytes = 0;
    option_text(request.title, &mut bytes)?;
    option_text(request.reason, &mut bytes)?;
    for aspect in request.required_aspects {
        option_text(aspect, &mut bytes)?;
    }
    for choice in [
        request.context.scene,
        request.context.time,
        request.context.viewpoint,
        request.context.lighting,
    ] {
        if let ContextChoice::Aspect(aspect) = choice {
            option_text(aspect, &mut bytes)?;
        }
    }
    if let Some(aspect) = request.context.apparent_scale_aspect {
        option_text(aspect, &mut bytes)?;
    }
    for assumption in request.assumptions {
        option_text(&assumption.assumption, &mut bytes)?;
        option_text(&assumption.rationale, &mut bytes)?;
    }
    for asset in request.reference_assets {
        for text in [
            &asset.sha256_hex,
            &asset.media_type,
            &asset.role,
            &asset.usage_conditions,
        ] {
            option_text(text, &mut bytes)?;
        }
    }
    Ok(bytes)
}

fn stable_id(kind: &str, aspect: &str, value: &str) -> Result<OpaqueId, CompositionError> {
    let mut digest = Sha256::new();
    digest.update((aspect.len() as u64).to_le_bytes());
    digest.update(aspect.as_bytes());
    digest.update(value.as_bytes());
    Ok(OpaqueId::new(format!("{kind}:{:x}", digest.finalize()))?)
}

fn source_label(source: &SourceSummary<'_>) -> String {
    let origin = match (source.origin, source.class) {
        (AccountOrigin::Simulated, _) => "simulated account",
        (AccountOrigin::Reported, SourceClass::Testimony) => "reported testimony",
        (AccountOrigin::Reported, _) => "reported source assertion",
    };
    let qualification = crate::recreation::qualified_source_label(
        source.version,
        origin,
        source.exposure == ExplanationExposure::AfterExplanations,
        None,
    );
    format!(
        "{} @ {} ({:?}, {:?}, revision={}, coverage={:?}, exposure={:?}, attribution={:?}; revision rationale not supplied)",
        source.account,
        qualification,
        source.class,
        source.origin,
        source.revision,
        source.coverage,
        source.exposure,
        source.attribution
    )
}

fn detail(
    comparison: &StatementComparison<'_>,
    aspect: &AspectComparison<'_>,
    value: &str,
) -> Result<ReportedDetail, CompositionError> {
    let mut statements = Vec::new();
    let mut versions = BTreeSet::new();
    let mut declared = BTreeSet::new();
    let mut adjusted = BTreeSet::new();
    let mut qualifications = Vec::new();
    for report in &aspect.reports {
        if report.value != CodedValue::Reported(value) {
            continue;
        }
        if let Some(source) = comparison.sources.get(report.source)
            && source.counts_as_reported
        {
            versions.insert(source.version.clone());
            declared.insert(source.declared_group);
            adjusted.insert(source.candidate_adjusted_group);
            qualifications.push(source_label(source));
            statements.push(format!(
                "{} bytes {}..{}: {:?}",
                source_label(source),
                report.wording.start,
                report.wording.end,
                report.wording_text
            ));
        }
    }
    Ok(ReportedDetail {
        detail_id: stable_id("detail", aspect.aspect, value)?,
        statement: format!(
            "Aspect {:?}; analyst code {:?}; cited reports: {}",
            aspect.aspect,
            value,
            statements.join("; ")
        ),
        source_versions: versions.into_iter().collect(),
        source_note: Some(format!(
            "Comparison status={:?}; declared reported groups={}; candidate-adjusted reported groups={} (copy candidates are not established dependence). Counts describe support groups, not truth or probability. Source qualifications: {}",
            aspect.status,
            declared.len(),
            adjusted.len(),
            qualifications.join("; ")
        )),
    })
}

fn unknown(aspect: &str, reason: String) -> Result<ExplicitUnknown, CompositionError> {
    Ok(ExplicitUnknown {
        unknown_id: stable_id("unknown", aspect, "")?,
        aspect: aspect.to_owned(),
        impact_on_recreation: reason,
    })
}

fn compose_aspects(
    comparison: &StatementComparison<'_>,
    input: &mut RecreationPromptInput,
    control: &mut impl FnMut() -> Result<(), StopReason>,
) -> Result<(), CompositionError> {
    for aspect in &comparison.aspects {
        control().map_err(CompositionError::Stopped)?;
        let supported: Vec<_> = aspect
            .variants
            .iter()
            .filter(|variant| variant.reported_accounts > 0)
            .collect();
        let mut alternatives = Vec::new();
        for variant in &supported {
            alternatives.push(AccountVariant {
                variant_id: stable_id("variant", aspect.aspect, variant.value)?,
                summary: format!("Alternative for aspect {:?}; analyst code {:?}. Alternatives for different aspects are not reconstructed whole accounts.", aspect.aspect, variant.value),
                reported_details: vec![detail(comparison, aspect, variant.value)?],
                conflicts_with: Vec::new(),
            });
        }
        if alternatives.len() > 1 {
            for alternative in &mut alternatives {
                alternative.conflicts_with = supported
                    .iter()
                    .map(|variant| stable_id("variant", aspect.aspect, variant.value))
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .filter(|id| id != &alternative.variant_id)
                    .collect();
            }
            input.conflicting_variants.extend(alternatives);
        } else if let Some(alternative) = alternatives.pop() {
            input
                .shared_reported_details
                .extend(alternative.reported_details);
        }
        let mut gaps = Vec::new();
        for report in &aspect.reports {
            if let Some(source) = comparison.sources.get(report.source) {
                if report.value == CodedValue::StatedUnknown {
                    gaps.push(format!(
                        "Stated unknown by {} bytes {}..{}: {:?}",
                        source_label(source),
                        report.wording.start,
                        report.wording.end,
                        report.wording_text
                    ));
                } else if !source.counts_as_reported {
                    gaps.push(format!("Not included as reported evidence: {} bytes {}..{}: {:?}; simulated or declared lineage only simulated", source_label(source), report.wording.start, report.wording.end, report.wording_text));
                }
            }
        }
        for omission in &aspect.not_coded_in {
            if let Some(source) = comparison.sources.get(omission.source) {
                gaps.push(format!(
                    "Not coded in {}; not observed absence",
                    source_label(source)
                ));
            }
        }
        if !gaps.is_empty() || supported.is_empty() {
            gaps.push("Do not fill missing information with invented measurements.".to_owned());
            input
                .unknowns
                .push(unknown(aspect.aspect, gaps.join("; "))?);
        }
    }
    Ok(())
}

fn context_field(
    choice: ContextChoice<'_>,
    comparison: &StatementComparison<'_>,
    assumptions: &[ExplicitAssumption],
) -> Result<ContextField, CompositionError> {
    match choice {
        ContextChoice::Unknown => Ok(ContextField::Unknown {
            reason: "No reported context selected; no default inferred.".to_owned(),
        }),
        ContextChoice::Assumption(id) => {
            let assumption = assumptions
                .iter()
                .find(|entry| &entry.assumption_id == id)
                .ok_or(CompositionError::InvalidOption(
                    "undeclared context assumption",
                ))?;
            Ok(ContextField::Assumed {
                description: assumption.assumption.clone(),
                assumption_id: id.clone(),
            })
        }
        ContextChoice::Aspect(key) => {
            if let Some(aspect) = comparison
                .aspects
                .iter()
                .find(|aspect| aspect.aspect == key)
            {
                let supported: Vec<_> = aspect
                    .variants
                    .iter()
                    .filter(|variant| variant.reported_accounts > 0)
                    .collect();
                if supported.len() == 1
                    && let Some(variant) = supported.first()
                {
                    let reported = detail(comparison, aspect, variant.value)?;
                    return Ok(ContextField::Reported {
                        description: reported.statement,
                        source_versions: reported.source_versions,
                    });
                }
            }
            Ok(ContextField::Unknown {
                reason: format!(
                    "Aspect {key:?} is uncoded, unsupported by reported sources, or conflicting; no context variant selected."
                ),
            })
        }
    }
}

fn context(
    request: &CompositionRequest<'_>,
    comparison: &StatementComparison<'_>,
) -> Result<SceneContext, CompositionError> {
    let apparent_scale = match request.context.apparent_scale_aspect {
        Some(aspect) => match context_field(ContextChoice::Aspect(aspect), comparison, request.assumptions)? {
            ContextField::Reported { description, source_versions } => ApparentScale::ReportedAngularOrRelative {
                description, source_versions,
                physical_size_unknown_reason: "Reported appearance alone establishes neither physical dimensions nor range; no calibration is performed.".to_owned(),
            },
            ContextField::Unknown { reason } => ApparentScale::Unknown { reason },
            ContextField::Assumed { .. } => return Err(CompositionError::InvalidOption("assumed scale")),
        },
        None => ApparentScale::Unknown { reason: "No apparent scale selected; physical dimensions and range remain unknown.".to_owned() },
    };
    Ok(SceneContext {
        scene: context_field(request.context.scene, comparison, request.assumptions)?,
        time_context: context_field(request.context.time, comparison, request.assumptions)?,
        viewpoint: context_field(request.context.viewpoint, comparison, request.assumptions)?,
        lighting: context_field(request.context.lighting, comparison, request.assumptions)?,
        apparent_scale,
    })
}

fn all_details(input: &RecreationPromptInput) -> Vec<&ReportedDetail> {
    input
        .shared_reported_details
        .iter()
        .chain(
            input
                .conflicting_variants
                .iter()
                .flat_map(|variant| variant.reported_details.iter()),
        )
        .collect()
}

fn account_sources<'a, 'b>(
    sources: &'b [StatementSource<'a>],
    account: &OpaqueId,
) -> Vec<&'b StatementSource<'a>> {
    let mut selected: Vec<_> = sources
        .iter()
        .filter(|source| &source.account == account)
        .collect();
    selected.sort_by_key(|source| source.revision);
    selected
}

fn changes(
    request: &CompositionRequest<'_>,
    input: &RecreationPromptInput,
    asset_policy: AssetExportPolicy,
    limits: RecreationLimits,
    previous: Option<&ComposedRecreationPrompt<'_>>,
) -> CompositionChanges {
    let before_sources: BTreeSet<_> = previous
        .into_iter()
        .flat_map(|previous| {
            previous
                .request
                .comparison
                .sources
                .iter()
                .map(|source| source.version.clone())
        })
        .collect();
    let after_sources: BTreeSet<_> = request
        .comparison
        .sources
        .iter()
        .map(|source| source.version.clone())
        .collect();
    let before = previous
        .map(|previous| all_details(&previous.input))
        .unwrap_or_default();
    let after = all_details(input);
    let mut result = CompositionChanges {
        predecessor: previous.map(|previous| previous.input.prompt_version.clone()),
        reason: request.reason.to_owned(),
        added_sources: after_sources.difference(&before_sources).cloned().collect(),
        removed_sources: before_sources.difference(&after_sources).cloned().collect(),
        changed_accounts: Vec::new(),
        added_details: after
            .iter()
            .filter(|detail| !before.iter().any(|old| old.detail_id == detail.detail_id))
            .map(|detail| detail.detail_id.clone())
            .collect(),
        removed_details: before
            .iter()
            .filter(|detail| !after.iter().any(|new| new.detail_id == detail.detail_id))
            .map(|detail| detail.detail_id.clone())
            .collect(),
        changed_details: after
            .iter()
            .filter(|detail| {
                before
                    .iter()
                    .any(|old| old.detail_id == detail.detail_id && old != *detail)
            })
            .map(|detail| detail.detail_id.clone())
            .collect(),
        sections: Vec::new(),
    };
    if let Some(previous) = previous {
        let accounts: BTreeSet<_> = previous
            .request
            .comparison
            .sources
            .iter()
            .chain(request.comparison.sources)
            .map(|source| source.account.clone())
            .collect();
        result.changed_accounts = accounts
            .into_iter()
            .filter(|account| {
                account_sources(previous.request.comparison.sources, account)
                    != account_sources(request.comparison.sources, account)
            })
            .collect();
        for (changed, section) in [
            (
                previous.request.comparison.parameters != request.comparison.parameters
                    || previous.request.comparison.coding_version
                        != request.comparison.coding_version
                    || previous.request.comparison.coded_by != request.comparison.coded_by,
                ChangedSection::ComparisonMethod,
            ),
            (
                previous.request.comparison.dismissals != request.comparison.dismissals,
                ChangedSection::CandidateReview,
            ),
            (
                previous.input.context != input.context,
                ChangedSection::Context,
            ),
            (
                previous.input.assumptions != input.assumptions,
                ChangedSection::Assumptions,
            ),
            (
                previous.input.unknowns != input.unknowns,
                ChangedSection::Unknowns,
            ),
            (
                previous.input.reference_assets != input.reference_assets,
                ChangedSection::ReferenceAssets,
            ),
            (
                previous.asset_policy != asset_policy,
                ChangedSection::AssetPolicy,
            ),
            (previous.input.title != input.title, ChangedSection::Title),
            (
                previous.input.conflicting_variants != input.conflicting_variants,
                ChangedSection::ConflictingVariants,
            ),
            (previous.limits != limits, ChangedSection::ExportLimits),
        ] {
            if changed {
                result.sections.push(section);
            }
        }
    }
    result
}

fn append(output: &mut String, text: &str) -> Result<(), CompositionError> {
    if text.len() > MAX_PORTABLE_BYTES.saturating_sub(output.len()) {
        return Err(CompositionError::Limit("portable text"));
    }
    output.push_str(text);
    Ok(())
}

fn portable(result: &ComposedRecreationPrompt<'_>) -> Result<String, CompositionError> {
    let mut output = String::new();
    append(&mut output, result.exported.portable_prompt_text())?;
    append(
        &mut output,
        &format!(
            "\nExport limits: {:?}; asset policy: {:?}\n",
            result.limits, result.asset_policy
        ),
    )?;
    append(
        &mut output,
        &format!(
            "\n## Composition provenance\nMethod: {COMPOSER_METHOD}\nStatus: generated, unreviewed; not observation evidence.\nChanges: {:?}\n",
            result.changes
        ),
    )?;
    append(
        &mut output,
        &format!(
            "Comparison method: {:?}\nPartial comparison: {}; contains simulated input: {}\nLimitations: {:?}\nDeclared reported groups: {}; candidate-adjusted reported groups: {}\n",
            result.comparison.method,
            result.comparison.is_partial(),
            result.comparison.contains_simulated,
            result.comparison.limitations,
            result.comparison.declared.reported_group_count,
            result.comparison.candidate_adjusted.reported_group_count
        ),
    )?;
    append(
        &mut output,
        "Shared-list placement includes unique and dependent reports; it does not establish corroboration. Alternatives conflict only within their named aspect. Source wording and analyst codes are not verified observations.\n",
    )?;
    for source in &result.comparison.sources {
        append(
            &mut output,
            &format!(
                "- Source {}: {}; representative={}; counts_as_reported={}; declared_group={}; candidate_adjusted_group={}\n",
                source.index,
                source_label(source),
                source.representative,
                source.counts_as_reported,
                source.declared_group,
                source.candidate_adjusted_group
            ),
        )?;
    }
    for source in result.request.comparison.sources {
        match source.text {
            TextCapture::Partial { reason, .. } | TextCapture::Unavailable { reason } => append(
                &mut output,
                &format!("- Capture limitation for {}: {reason:?}\n", source.version),
            )?,
            TextCapture::Complete(_) => {}
        }
        append(
            &mut output,
            &format!(
                "- Declared derivations for {}: {:?}\n",
                source.version, source.derived_from
            ),
        )?;
    }
    for candidate in &result.comparison.copy_candidates {
        append(
            &mut output,
            &format!(
                "- Copy candidate: sources {} bytes {:?}, {} bytes {:?}; words={}; dismissal_index={:?}\n",
                candidate.a,
                candidate.a_range,
                candidate.b,
                candidate.b_range,
                candidate.words,
                candidate.dismissal
            ),
        )?;
    }
    for dismissal in result.request.comparison.dismissals {
        append(&mut output, &format!("- Candidate review: {dismissal:?}\n"))?;
    }
    Ok(output)
}

/// Recomputes a validated comparison rather than accepting a caller-forged result.
/// Regeneration creates a new transient version; the predecessor remains untouched.
pub fn compose_recreation_prompt<'a>(
    request: CompositionRequest<'a>,
    previous: Option<&ComposedRecreationPrompt<'_>>,
    limits: RecreationLimits,
    asset_policy: AssetExportPolicy,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<ComposedRecreationPrompt<'a>, CompositionError> {
    check_control().map_err(CompositionError::Stopped)?;
    let mut bytes = preflight(&request)?;
    if let Some(previous) = previous {
        if request.prompt_id != previous.input.prompt_id
            || request.case_id != previous.input.case_id
        {
            return Err(CompositionError::DifferentPromptOrCase);
        }
        if request.prompt_version == previous.input.prompt_version
            || previous.predecessors.contains(&request.prompt_version)
        {
            return Err(CompositionError::ReusedPromptVersion);
        }
        if previous.predecessors.len() >= MAX_PREDECESSORS {
            return Err(CompositionError::Limit("predecessors"));
        }
    }
    let comparison = compare_statements(&request.comparison, &mut check_control)
        .map_err(CompositionError::Comparison)?;
    if let Some(previous) = previous {
        for source in request.comparison.sources {
            if let Some(old) = previous
                .request
                .comparison
                .sources
                .iter()
                .find(|old| old.version == source.version)
            {
                let mut original = (*old).clone();
                original.assertions = source.assertions;
                if &original != source {
                    return Err(CompositionError::ChangedPinnedSource(
                        source.version.clone(),
                    ));
                }
                if old.assertions != source.assertions
                    && previous.request.comparison.coding_version
                        == request.comparison.coding_version
                {
                    return Err(CompositionError::ChangedCodingVersion);
                }
            }
        }
        if previous.request.comparison.coded_by != request.comparison.coded_by
            && previous.request.comparison.coding_version == request.comparison.coding_version
        {
            return Err(CompositionError::ChangedCodingVersion);
        }
    }
    if comparison.aspects.len() > MAX_ASPECTS {
        return Err(CompositionError::Limit("aspects"));
    }
    for aspect in &comparison.aspects {
        charge(&mut bytes, aspect.aspect.len())?;
        for report in &aspect.reports {
            charge(
                &mut bytes,
                report.wording_text.len() * 6 + aspect.aspect.len() * 6 + 512,
            )?;
            if let CodedValue::Reported(value) = report.value {
                charge(&mut bytes, value.len() * 6)?;
            }
            if let Some(source) = comparison.sources.get(report.source) {
                charge(&mut bytes, source_label(source).len() * 2)?;
            }
        }
        for omission in &aspect.not_coded_in {
            if let Some(source) = comparison.sources.get(omission.source) {
                charge(&mut bytes, source_label(source).len() + 128)?;
            }
        }
    }
    let mut input = RecreationPromptInput {
        prompt_id: request.prompt_id.clone(),
        prompt_version: request.prompt_version.clone(),
        case_id: request.case_id.clone(),
        title: request.title.to_owned(),
        context: context(&request, &comparison)?,
        shared_reported_details: Vec::new(),
        conflicting_variants: Vec::new(),
        assumptions: request.assumptions.to_vec(),
        unknowns: Vec::new(),
        reference_assets: request.reference_assets.to_vec(),
    };
    compose_aspects(&comparison, &mut input, &mut check_control)?;
    let mut missing = BTreeSet::new();
    for aspect in request.required_aspects {
        if !comparison
            .aspects
            .iter()
            .any(|entry| entry.aspect == *aspect)
            && missing.insert(*aspect)
        {
            input.unknowns.push(unknown(aspect, "No source codes this aspect. Missing coding is not evidence of observed absence; do not invent a value.".to_owned())?);
        }
    }
    check_control().map_err(CompositionError::Stopped)?;
    let exported = export_recreation_prompt(&input, limits, asset_policy)
        .map_err(CompositionError::Recreation)?;
    let changes = changes(&request, &input, asset_policy, limits, previous);
    let mut predecessors = previous
        .map(|previous| previous.predecessors.clone())
        .unwrap_or_default();
    if let Some(previous) = previous {
        predecessors.push(previous.input.prompt_version.clone());
    }
    let mut result = ComposedRecreationPrompt {
        request,
        comparison,
        input,
        exported,
        changes,
        predecessors,
        asset_policy,
        limits,
        portable_text: String::new(),
    };
    result.portable_text = portable(&result)?;
    check_control().map_err(CompositionError::Stopped)?;
    Ok(result)
}
