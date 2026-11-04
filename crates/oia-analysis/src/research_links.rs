//! Bounded public link analysis, reversible conflicting-identity resolution,
//! structural citation-cycle tracing, and scientific/technical development tracking
//! (`INTEL-02`, `INTEL-04` alternate names, `REASON-03` reversible decisions,
//! `A08` subset, `#36`).
//!
//! This module implements the dependency-independent domain evaluation kernel for:
//! - **Public links (`INTEL-02`, `A08`):** Evaluates typed relationships across
//!   [`PublicEntityKind::Organization`], [`PublicEntityKind::Contract`],
//!   [`PublicEntityKind::Grant`], [`PublicEntityKind::Facility`],
//!   [`PublicEntityKind::Publication`], and [`PublicEntityKind::Event`] nodes with
//!   pinned source versions and explicit alternate names / transliterations (`INTEL-04`).
//!   Keeps [`PublicLinkRelation::ObservedCoOccurrence`] strictly separate from established
//!   funding, contract, facility, and organizational affiliation links
//!   (`docs/architecture.md` §2.3).
//! - **Reversible conflicting-identity resolution (`REASON-03`, `A08`):** Reuses
//!   [`crate::reasoning::DecisionState`] (`Proposed`, `Confirmed`, `Reversed`) over
//!   candidate matches for each [`IdentityMention`]. When a mention has multiple active
//!   (`Proposed` or `Confirmed`) candidates, it resolves to
//!   [`MentionResolution::DisputedIdentity`] rather than silently merging to one entity,
//!   and any link or technical record referencing that mention inherits the disputed state.
//!   Reversing a candidate match (`DecisionState::Reversed`) requires a non-empty rationale,
//!   retains the full candidate list on [`EvaluatedMention::candidates`], and cleanly unlinks
//!   dependent edges when all candidates are reversed. Note: because [`evaluate_research_links`]
//!   is a stateless projection over the current candidate set, if a mention has two candidates
//!   (`A`, `B`) and `A` is later reversed while `B` is confirmed, dependent links resolve to
//!   `B` without a cross-snapshot diff flag.
//! - **Structural circular citation tracing (`INTEL-01`, `INTEL-02`, `A08`):** Traces directed
//!   [`PublicLinkRelation::CitesPublication`] cycles across publications using iterative Tarjan
//!   strongly connected components (for guaranteed $O(V + E)$ cycle-membership detection)
//!   plus bounded canonical elementary-cycle enumeration. Publications that participate in
//!   a citation cycle, or whose outgoing citation paths lead *exclusively* into a citation
//!   cycle without reaching any acyclic publication, are flagged as structure. Corroboration
//!   and independent-root verdicts belong exclusively to `oia_documents::lineage` (`AGENTS.md` §4).
//! - **Proposal and patent vs. demonstrated capability (`INTEL-02`, `#36`):** Tracks
//!   scientific and technical developments across grants, contracts, patents, theoretical
//!   and simulation papers, and laboratory or field test reports. Grant awards, contract
//!   scopes, patent filings/grants, and conceptual or simulation papers are capped at
//!   proposal, patent, or analytical stages and can never establish demonstrated capability
//!   — even if their prose claims a demonstration. Only sources with observational or
//!   source-assertion origins (`AcquiredSourceMaterial`, `NormalizedObservation`,
//!   `ExtractedClaim`, `UserTestimony`, `ConventionalDerivative`) whose entire in-set
//!   upstream lineage (`derived_from_versions`) consists of non-retracted observational or
//!   source-assertion records can establish reported links or reported demonstrations;
//!   `Simulation`, `AnalyticalInference`, `Forecast`, `PredictiveDerivative`, and
//!   `IllustrativeRecreation` sources (or sources with any such ancestor in their lineage)
//!   are excluded (`AGENTS.md` §2: feeding an inference or forecast into a downstream source
//!   never promotes it to an observation). Sources that are [`SourceLifecycleState::Retracted`]
//!   (or have any retracted ancestor in their lineage) are excluded from active maturity and
//!   confirmed link counts, and both direct and transitive upstream `CorrectedBy` / `Retracted`
//!   states propagate `needs_reassessment` to downstream mentions, links, and technical items.
//!
//! ### Relationship to other modules (`AGENTS.md` §4)
//!
//! - **Option (b) boundary with `oia_documents::lineage` (`#19`):** `oia_documents::lineage`
//!   is the sole owner of document-release corroboration verdicts and independent-root counts
//!   (`evaluate_claim_corroboration`, `independent_root_count`). This module stays
//!   dependency-independent of `oia-documents` and reports `CitesPublication` edges, directed
//!   citation cycles, and shared in-set `derived_from_versions` as *structure only*, without
//!   computing a parallel independent-corroboration count.
//! - [`crate::reasoning`] (`#35`) evaluates general `AbstractionSpec` steps inside a
//!   deductive/correlative `ReasoningGraph`. This module reuses [`DecisionState`] and
//!   [`EvidenceOriginClass`] from [`crate::reasoning`] and [`StopReason`] from
//!   [`crate::multilingual`].

pub use crate::multilingual::StopReason;
pub use crate::reasoning::{DecisionState, EvidenceOriginClass};
use oia_domain::text::requires_visible_unicode_escape;
use oia_domain::{OpaqueId, VersionId};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Stable method identifier to pin alongside any persisted or exported result.
pub const METHOD_ID: &str = "oia-public-research-links-v1";

/// Mandatory caveat attached to every [`ResearchLinkReport`] regarding entity resolution.
pub const IDENTITY_RESOLUTION_CAVEAT: &str = "Entity matches and public relationship edges are \
    analyst- or extractor-proposed inferences over cited source text. Conflicting candidates \
    remain DisputedIdentity and are never merged automatically.";

/// Mandatory caveat attached to every [`ResearchLinkReport`] regarding co-occurrence edges.
pub const CO_OCCURRENCE_CAVEAT: &str = "ObservedCoOccurrence records that two entities appear \
    in the same source or event context; it is not an established organizational affiliation, \
    funding award, contract, or causal link.";

/// Mandatory caveat attached to every [`ResearchLinkReport`] regarding technical maturity.
pub const TECHNICAL_MATURITY_CAVEAT: &str = "Patents, grant awards, contract scopes, and \
    conceptual or simulation papers establish proposed, patented, or modelled work only — never \
    demonstrated physical capability. Reported laboratory or field demonstrations in public \
    literature remain source assertions requiring independent empirical verification.";

/// Mandatory caveat attached to every [`ResearchLinkReport`] regarding circular citations.
pub const CIRCULAR_CITATION_CAVEAT: &str = "Citation edges, directed citation cycles, and \
    declared source lineage are reported as graph structure only; document-level independent \
    corroboration verdicts belong to oia_documents::lineage.";

/// Mandatory caveat attached to every [`ResearchLinkReport`] regarding non-empirical or
/// synthetic sources.
pub const SIMULATED_SOURCE_CAVEAT: &str = "Sources with Simulation, AnalyticalInference, \
    Forecast, PredictiveDerivative, or IllustrativeRecreation origin classes (or deriving from \
    any ancestor with such an origin class) never count as reported public links or \
    demonstrated capability.";

/// Default maximum number of source versions in one evaluation request.
pub const MAX_SOURCES: usize = 128;
/// Default maximum number of canonical public entities in one evaluation request.
pub const MAX_ENTITIES: usize = 256;
/// Default maximum number of alternate names or transliterations per entity.
pub const MAX_ALTERNATE_NAMES_PER_ENTITY: usize = 32;
/// Default maximum number of source mentions with candidate entity matches.
pub const MAX_MENTIONS: usize = 256;
/// Default maximum number of candidate matches per mention.
pub const MAX_CANDIDATES_PER_MENTION: usize = 16;
/// Default maximum number of public relationship links in one evaluation request.
pub const MAX_LINKS: usize = 512;
/// Default maximum number of technical development tracks in one evaluation request.
pub const MAX_DEVELOPMENTS: usize = 128;
/// Default maximum number of evidence items per technical development track.
pub const MAX_ITEMS_PER_DEVELOPMENT: usize = 64;
/// Default maximum number of funding/contract entity references per technical item.
pub const MAX_FUNDING_REFS_PER_ITEM: usize = 32;
/// Default maximum number of declared upstream source versions per source record.
pub const MAX_UPSTREAM_VERSIONS_PER_SOURCE: usize = 32;
/// Default maximum UTF-8 bytes for short structural labels (names, topics, script/language tags).
pub const MAX_LABEL_BYTES: usize = 256;
/// Default maximum UTF-8 bytes for verbatim excerpts, rationales, and limitation notes.
pub const MAX_EXCERPT_BYTES: usize = 4_096;
/// Default maximum combined UTF-8 text bytes across the entire request.
pub const MAX_TOTAL_TEXT_BYTES: usize = 1_048_576;
/// Default maximum number of elementary citation cycles enumerated before truncation is flagged.
pub const MAX_REPORTED_CYCLES: usize = 64;
/// Default maximum depth-first search steps during elementary citation-cycle enumeration.
pub const MAX_CYCLE_DFS_STEPS: usize = 16_384;

const CONTROL_CHECK_INTERVAL: usize = 64;

/// Which budget was zero or exceeded in a [`ResearchLinkRequest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResearchLinkLimitKind {
    Sources,
    Entities,
    AlternateNamesPerEntity,
    Mentions,
    CandidatesPerMention,
    Links,
    Developments,
    ItemsPerDevelopment,
    FundingRefsPerItem,
    UpstreamVersionsPerSource,
    LabelBytes,
    ExcerptBytes,
    TotalTextBytes,
    ReportedCycles,
    CycleDfsSteps,
}

impl fmt::Display for ResearchLinkLimitKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sources => f.write_str("max_sources"),
            Self::Entities => f.write_str("max_entities"),
            Self::AlternateNamesPerEntity => f.write_str("max_alternate_names_per_entity"),
            Self::Mentions => f.write_str("max_mentions"),
            Self::CandidatesPerMention => f.write_str("max_candidates_per_mention"),
            Self::Links => f.write_str("max_links"),
            Self::Developments => f.write_str("max_developments"),
            Self::ItemsPerDevelopment => f.write_str("max_items_per_development"),
            Self::FundingRefsPerItem => f.write_str("max_funding_refs_per_item"),
            Self::UpstreamVersionsPerSource => f.write_str("max_upstream_versions_per_source"),
            Self::LabelBytes => f.write_str("max_label_bytes"),
            Self::ExcerptBytes => f.write_str("max_excerpt_bytes"),
            Self::TotalTextBytes => f.write_str("max_total_text_bytes"),
            Self::ReportedCycles => f.write_str("max_reported_cycles"),
            Self::CycleDfsSteps => f.write_str("max_cycle_dfs_steps"),
        }
    }
}

/// Resource budgets for [`evaluate_research_links`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResearchLinkLimits {
    max_sources: usize,
    max_entities: usize,
    max_alternate_names_per_entity: usize,
    max_mentions: usize,
    max_candidates_per_mention: usize,
    max_links: usize,
    max_developments: usize,
    max_items_per_development: usize,
    max_funding_refs_per_item: usize,
    max_upstream_versions_per_source: usize,
    max_label_bytes: usize,
    max_excerpt_bytes: usize,
    max_total_text_bytes: usize,
    max_reported_cycles: usize,
    max_cycle_dfs_steps: usize,
}

impl Default for ResearchLinkLimits {
    fn default() -> Self {
        Self {
            max_sources: MAX_SOURCES,
            max_entities: MAX_ENTITIES,
            max_alternate_names_per_entity: MAX_ALTERNATE_NAMES_PER_ENTITY,
            max_mentions: MAX_MENTIONS,
            max_candidates_per_mention: MAX_CANDIDATES_PER_MENTION,
            max_links: MAX_LINKS,
            max_developments: MAX_DEVELOPMENTS,
            max_items_per_development: MAX_ITEMS_PER_DEVELOPMENT,
            max_funding_refs_per_item: MAX_FUNDING_REFS_PER_ITEM,
            max_upstream_versions_per_source: MAX_UPSTREAM_VERSIONS_PER_SOURCE,
            max_label_bytes: MAX_LABEL_BYTES,
            max_excerpt_bytes: MAX_EXCERPT_BYTES,
            max_total_text_bytes: MAX_TOTAL_TEXT_BYTES,
            max_reported_cycles: MAX_REPORTED_CYCLES,
            max_cycle_dfs_steps: MAX_CYCLE_DFS_STEPS,
        }
    }
}

impl ResearchLinkLimits {
    /// Validates that all configured budgets are strictly positive and within the hard module caps.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        max_sources: usize,
        max_entities: usize,
        max_alternate_names_per_entity: usize,
        max_mentions: usize,
        max_candidates_per_mention: usize,
        max_links: usize,
        max_developments: usize,
        max_items_per_development: usize,
        max_funding_refs_per_item: usize,
        max_upstream_versions_per_source: usize,
        max_label_bytes: usize,
        max_excerpt_bytes: usize,
        max_total_text_bytes: usize,
        max_reported_cycles: usize,
        max_cycle_dfs_steps: usize,
    ) -> Result<Self, ResearchLinkError> {
        let checks = [
            (ResearchLinkLimitKind::Sources, max_sources, MAX_SOURCES),
            (ResearchLinkLimitKind::Entities, max_entities, MAX_ENTITIES),
            (
                ResearchLinkLimitKind::AlternateNamesPerEntity,
                max_alternate_names_per_entity,
                MAX_ALTERNATE_NAMES_PER_ENTITY,
            ),
            (ResearchLinkLimitKind::Mentions, max_mentions, MAX_MENTIONS),
            (
                ResearchLinkLimitKind::CandidatesPerMention,
                max_candidates_per_mention,
                MAX_CANDIDATES_PER_MENTION,
            ),
            (ResearchLinkLimitKind::Links, max_links, MAX_LINKS),
            (
                ResearchLinkLimitKind::Developments,
                max_developments,
                MAX_DEVELOPMENTS,
            ),
            (
                ResearchLinkLimitKind::ItemsPerDevelopment,
                max_items_per_development,
                MAX_ITEMS_PER_DEVELOPMENT,
            ),
            (
                ResearchLinkLimitKind::FundingRefsPerItem,
                max_funding_refs_per_item,
                MAX_FUNDING_REFS_PER_ITEM,
            ),
            (
                ResearchLinkLimitKind::UpstreamVersionsPerSource,
                max_upstream_versions_per_source,
                MAX_UPSTREAM_VERSIONS_PER_SOURCE,
            ),
            (
                ResearchLinkLimitKind::LabelBytes,
                max_label_bytes,
                MAX_LABEL_BYTES,
            ),
            (
                ResearchLinkLimitKind::ExcerptBytes,
                max_excerpt_bytes,
                MAX_EXCERPT_BYTES,
            ),
            (
                ResearchLinkLimitKind::TotalTextBytes,
                max_total_text_bytes,
                MAX_TOTAL_TEXT_BYTES,
            ),
            (
                ResearchLinkLimitKind::ReportedCycles,
                max_reported_cycles,
                MAX_REPORTED_CYCLES,
            ),
            (
                ResearchLinkLimitKind::CycleDfsSteps,
                max_cycle_dfs_steps,
                MAX_CYCLE_DFS_STEPS,
            ),
        ];
        for (kind, value, cap) in checks {
            if value == 0 || value > cap {
                return Err(ResearchLinkError::InvalidLimit {
                    limit: kind,
                    supplied: value,
                    max_allowed: cap,
                });
            }
        }
        Ok(Self {
            max_sources,
            max_entities,
            max_alternate_names_per_entity,
            max_mentions,
            max_candidates_per_mention,
            max_links,
            max_developments,
            max_items_per_development,
            max_funding_refs_per_item,
            max_upstream_versions_per_source,
            max_label_bytes,
            max_excerpt_bytes,
            max_total_text_bytes,
            max_reported_cycles,
            max_cycle_dfs_steps,
        })
    }

    #[must_use]
    pub const fn max_sources(self) -> usize {
        self.max_sources
    }

    #[must_use]
    pub const fn max_entities(self) -> usize {
        self.max_entities
    }

    #[must_use]
    pub const fn max_mentions(self) -> usize {
        self.max_mentions
    }

    #[must_use]
    pub const fn max_links(self) -> usize {
        self.max_links
    }

    #[must_use]
    pub const fn max_developments(self) -> usize {
        self.max_developments
    }

    #[must_use]
    pub const fn max_reported_cycles(self) -> usize {
        self.max_reported_cycles
    }

    #[must_use]
    pub const fn max_cycle_dfs_steps(self) -> usize {
        self.max_cycle_dfs_steps
    }
}

/// Typed validation and execution errors for [`evaluate_research_links`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResearchLinkError {
    InvalidLimit {
        limit: ResearchLinkLimitKind,
        supplied: usize,
        max_allowed: usize,
    },
    BudgetExceeded {
        limit: ResearchLinkLimitKind,
        actual: usize,
        max: usize,
    },
    EmptySources,
    EmptyGraphContent,
    EmptyField {
        context: &'static str,
        field: &'static str,
    },
    ControlOrFormatCharacter {
        context: &'static str,
        field: &'static str,
    },
    DuplicateSourceVersion(VersionId),
    SelfSourceDerivation(VersionId),
    DuplicateSourceDerivation {
        source_version: VersionId,
        upstream_version: VersionId,
    },
    SourceCorrectedBySelf(VersionId),
    DuplicateEntityId(OpaqueId),
    DuplicateMentionId(OpaqueId),
    EmptyMentionCandidates(OpaqueId),
    DuplicateCandidateId {
        mention_id: OpaqueId,
        candidate_id: OpaqueId,
    },
    DuplicateCandidateTargetEntity {
        mention_id: OpaqueId,
        target_entity_id: OpaqueId,
    },
    DuplicateLinkId(OpaqueId),
    DuplicateDevelopmentId(OpaqueId),
    EmptyDevelopmentItems(OpaqueId),
    DuplicateDevelopmentItemId {
        development_id: OpaqueId,
        item_id: OpaqueId,
    },
    DuplicateFundingReference {
        development_id: OpaqueId,
        item_id: OpaqueId,
        funding_entity_id: OpaqueId,
    },
    UnknownSourceVersion {
        context: &'static str,
        version_id: VersionId,
    },
    UnknownEntity {
        context: &'static str,
        entity_id: OpaqueId,
    },
    UnknownMention {
        context: &'static str,
        mention_id: OpaqueId,
    },
    MentionEntityKindMismatch {
        mention_id: OpaqueId,
        expected: PublicEntityKind,
        actual: PublicEntityKind,
        target_entity_id: OpaqueId,
    },
    InvalidLinkEndpointKinds {
        link_id: OpaqueId,
        relation: PublicLinkRelation,
        from_kind: PublicEntityKind,
        to_kind: PublicEntityKind,
    },
    SelfCitationLink {
        link_id: OpaqueId,
        publication_id: OpaqueId,
    },
    DevelopmentSubjectKindInvalid {
        development_id: OpaqueId,
        actual_kind: PublicEntityKind,
    },
    ItemPublicationKindMismatch {
        development_id: OpaqueId,
        item_id: OpaqueId,
        entity_id: OpaqueId,
        actual_kind: PublicEntityKind,
    },
    ItemFundingKindMismatch {
        development_id: OpaqueId,
        item_id: OpaqueId,
        entity_id: OpaqueId,
        actual_kind: PublicEntityKind,
    },
    Stopped(StopReason),
}

impl fmt::Display for ResearchLinkError {
    #[allow(clippy::too_many_lines)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimit {
                limit,
                supplied,
                max_allowed,
            } => write!(
                f,
                "research link limit {limit} must be in 1..={max_allowed} (got {supplied})"
            ),
            Self::BudgetExceeded { limit, actual, max } => {
                write!(f, "research link limit {limit} exceeded ({actual} > {max})")
            }
            Self::EmptySources => f.write_str("research link request requires at least one source"),
            Self::EmptyGraphContent => f.write_str(
                "research link request must include at least one entity, mention, link, or technical development",
            ),
            Self::EmptyField { context, field } => {
                write!(f, "{context}.{field} must not be empty or whitespace-only")
            }
            Self::ControlOrFormatCharacter { context, field } => write!(
                f,
                "{context}.{field} contains disallowed control or invisible Unicode format characters"
            ),
            Self::DuplicateSourceVersion(v) => {
                write!(f, "duplicate source version {v} in research link request")
            }
            Self::SelfSourceDerivation(v) => {
                write!(f, "source version {v} cannot declare derivation from itself")
            }
            Self::DuplicateSourceDerivation {
                source_version,
                upstream_version,
            } => write!(
                f,
                "source version {source_version} declares duplicate upstream version {upstream_version}"
            ),
            Self::SourceCorrectedBySelf(v) => {
                write!(f, "source version {v} cannot be marked CorrectedBy itself")
            }
            Self::DuplicateEntityId(id) => write!(f, "duplicate entity ID {id}"),
            Self::DuplicateMentionId(id) => write!(f, "duplicate mention ID {id}"),
            Self::EmptyMentionCandidates(id) => {
                write!(f, "mention {id} must contain at least one candidate match")
            }
            Self::DuplicateCandidateId {
                mention_id,
                candidate_id,
            } => write!(
                f,
                "mention {mention_id} contains duplicate candidate ID {candidate_id}"
            ),
            Self::DuplicateCandidateTargetEntity {
                mention_id,
                target_entity_id,
            } => write!(
                f,
                "mention {mention_id} contains duplicate candidate target entity {target_entity_id}"
            ),
            Self::DuplicateLinkId(id) => write!(f, "duplicate public link ID {id}"),
            Self::DuplicateDevelopmentId(id) => {
                write!(f, "duplicate technical development ID {id}")
            }
            Self::EmptyDevelopmentItems(id) => write!(
                f,
                "technical development {id} must contain at least one evidence item"
            ),
            Self::DuplicateDevelopmentItemId {
                development_id,
                item_id,
            } => write!(
                f,
                "technical development {development_id} contains duplicate item ID {item_id}"
            ),
            Self::DuplicateFundingReference {
                development_id,
                item_id,
                funding_entity_id,
            } => write!(
                f,
                "technical development {development_id} item {item_id} contains duplicate funding/contract reference {funding_entity_id}"
            ),
            Self::UnknownSourceVersion {
                context,
                version_id,
            } => write!(
                f,
                "{context} cites unregistered source version {version_id}"
            ),
            Self::UnknownEntity { context, entity_id } => {
                write!(f, "{context} references unknown entity {entity_id}")
            }
            Self::UnknownMention {
                context,
                mention_id,
            } => write!(f, "{context} references unknown mention {mention_id}"),
            Self::MentionEntityKindMismatch {
                mention_id,
                expected,
                actual,
                target_entity_id,
            } => write!(
                f,
                "mention {mention_id} expects entity kind {expected}, but candidate target {target_entity_id} has kind {actual}"
            ),
            Self::InvalidLinkEndpointKinds {
                link_id,
                relation,
                from_kind,
                to_kind,
            } => write!(
                f,
                "public link {link_id} relation {relation} does not permit endpoints ({from_kind} -> {to_kind})"
            ),
            Self::SelfCitationLink {
                link_id,
                publication_id,
            } => write!(
                f,
                "public link {link_id} cannot record self-citation on publication {publication_id}"
            ),
            Self::DevelopmentSubjectKindInvalid {
                development_id,
                actual_kind,
            } => write!(
                f,
                "technical development {development_id} subject must be an Organization or Facility, got {actual_kind}"
            ),
            Self::ItemPublicationKindMismatch {
                development_id,
                item_id,
                entity_id,
                actual_kind,
            } => write!(
                f,
                "technical development {development_id} item {item_id} publication reference {entity_id} must have kind Publication, got {actual_kind}"
            ),
            Self::ItemFundingKindMismatch {
                development_id,
                item_id,
                entity_id,
                actual_kind,
            } => write!(
                f,
                "technical development {development_id} item {item_id} funding reference {entity_id} must have kind Grant or Contract, got {actual_kind}"
            ),
            Self::Stopped(reason) => write!(f, "research link evaluation stopped: {reason:?}"),
        }
    }
}

impl std::error::Error for ResearchLinkError {}

/// Lifecycle state of a pinned source version (`COLLAB-03`, `REASON-04`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceLifecycleState {
    /// Version is current and has not been superseded or retracted.
    Current,
    /// Version has been corrected by a newer source version; dependent items need reassessment.
    CorrectedBy {
        replacement_version: VersionId,
        summary: String,
    },
    /// Version has been retracted; dependent items are excluded from active link/maturity
    /// rollups and flagged for reassessment.
    Retracted { reason: String },
}

/// Pinned source record supplying public links, entity mentions, or technical claims.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchSourceRecord {
    pub source_id: OpaqueId,
    pub version_id: VersionId,
    pub origin_class: EvidenceOriginClass,
    pub lifecycle: SourceLifecycleState,
    /// Upstream source versions from which this record declares it derives.
    ///
    /// Upstream versions outside the evaluated request are retained in
    /// [`SourceSummary::external_derived_from_versions`] and surfaced in
    /// [`ResearchLinkLimitation::OutOfSetSourceLineage`].
    pub derived_from_versions: Vec<VersionId>,
    pub citation_label: String,
}

/// Principal node kinds in an `INTEL-02` public research link graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PublicEntityKind {
    Organization,
    Contract,
    Grant,
    Facility,
    Publication,
    Event,
}

impl fmt::Display for PublicEntityKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Organization => f.write_str("organization"),
            Self::Contract => f.write_str("contract"),
            Self::Grant => f.write_str("grant"),
            Self::Facility => f.write_str("facility"),
            Self::Publication => f.write_str("publication"),
            Self::Event => f.write_str("event"),
        }
    }
}

/// Classification of an alternate name, acronym, code, or transliteration (`INTEL-02`, `INTEL-04`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AlternateNameKind {
    Acronym,
    FormerOrTradeName,
    ProgramOrAwardCode,
    Transliteration { script: String },
    TranslatedTitle { language: String },
}

/// Attributed alternate name or transliteration for a [`ResearchEntity`] (`INTEL-04`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlternateName {
    pub name: String,
    pub kind: AlternateNameKind,
    pub source_version: VersionId,
}

/// Canonical node in the public link graph (`INTEL-02`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchEntity {
    pub entity_id: OpaqueId,
    pub kind: PublicEntityKind,
    pub primary_name: String,
    pub alternate_names: Vec<AlternateName>,
    pub defining_source_version: VersionId,
}

/// One reversible candidate match linking an [`IdentityMention`] to a [`ResearchEntity`]
/// (`REASON-03`, `A08`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityCandidateMatch {
    pub candidate_id: OpaqueId,
    pub target_entity_id: OpaqueId,
    pub matching_basis: String,
    pub supporting_source_version: VersionId,
    pub decision: DecisionState,
}

/// A source mention whose entity identity may be uniquely confirmed, proposed, disputed across
/// conflicting candidates, or reversed (`REASON-03`, `A08`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityMention {
    pub mention_id: OpaqueId,
    pub verbatim_text: String,
    pub source_version: VersionId,
    pub expected_kind: PublicEntityKind,
    pub candidates: Vec<IdentityCandidateMatch>,
}

/// Resolved state of an [`IdentityMention`] (`REASON-03`, `A08`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MentionResolution {
    /// Exactly one candidate is `Confirmed` and all other candidates (if any) are `Reversed`.
    ConfirmedUnique {
        entity_id: OpaqueId,
        candidate_id: OpaqueId,
    },
    /// Exactly one candidate is `Proposed` (unconfirmed) and all other candidates are `Reversed`.
    ProposedOnly {
        candidate_entity_id: OpaqueId,
        candidate_id: OpaqueId,
    },
    /// Two or more non-reversed (`Proposed` or `Confirmed`) candidates compete for this mention.
    ///
    /// Conflicting candidates are kept explicitly separate and never merged into one entity.
    DisputedIdentity {
        confirmed_entity_ids: Vec<OpaqueId>,
        proposed_entity_ids: Vec<OpaqueId>,
    },
    /// Every candidate match on this mention has been explicitly `Reversed`.
    AllReversed { reversed_candidate_count: usize },
}

impl MentionResolution {
    /// Returns the uniquely resolved entity ID only when [`Self::ConfirmedUnique`].
    #[must_use]
    pub const fn confirmed_entity_id(&self) -> Option<&OpaqueId> {
        match self {
            Self::ConfirmedUnique { entity_id, .. } => Some(entity_id),
            Self::ProposedOnly { .. }
            | Self::DisputedIdentity { .. }
            | Self::AllReversed { .. } => None,
        }
    }

    /// Returns the single active entity ID when either [`Self::ConfirmedUnique`] or [`Self::ProposedOnly`].
    #[must_use]
    pub const fn single_active_entity_id(&self) -> Option<&OpaqueId> {
        match self {
            Self::ConfirmedUnique { entity_id, .. } => Some(entity_id),
            Self::ProposedOnly {
                candidate_entity_id,
                ..
            } => Some(candidate_entity_id),
            Self::DisputedIdentity { .. } | Self::AllReversed { .. } => None,
        }
    }
}

/// Endpoint of a [`PublicLinkRecord`] or subject of a [`TechnicalDevelopmentInput`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LinkEndpoint {
    /// References a [`ResearchEntity`] directly.
    Entity(OpaqueId),
    /// References an [`IdentityMention`] so reversible or disputed identity decisions propagate
    /// to dependent links and technical development assessments (`A08`).
    Mention(OpaqueId),
}

/// Typed relationship semantics for public research links (`INTEL-02`, `docs/architecture.md` §2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PublicLinkRelation {
    /// An organization or grant awards/provides funding to an organization, grant, facility,
    /// publication, or event.
    GrantsFundingTo,
    /// An organization or contract awards/provides a contract to an organization, contract,
    /// facility, publication, or event.
    AwardsContractTo,
    /// An organization operates or manages a facility.
    OperatesFacility,
    /// Explicit organizational affiliation (parent/subsidiary, joint laboratory, member unit).
    AffiliatedWithOrganization,
    /// An organization publishes, sponsors, or authors a publication.
    PublishesOrAuthors,
    /// A grant or contract is cited as funding/producing a publication, facility, or event.
    LinksGrantOrContractToOutput,
    /// An organization, publication, or facility reports on or participates in an event.
    ReportsOrParticipatesInEvent,
    /// A publication cites another publication (`INTEL-01`, `INTEL-02`, `A08`).
    CitesPublication,
    /// Two entities co-occur in the same source passage or event without established affiliation,
    /// funding, contract, or causation (`docs/architecture.md` §2.3).
    ObservedCoOccurrence,
}

impl PublicLinkRelation {
    /// Returns `true` when the relation represents a substantive typed link rather than
    /// [`Self::ObservedCoOccurrence`] (`docs/architecture.md` §2.3).
    #[must_use]
    pub const fn is_substantive_public_link(self) -> bool {
        !matches!(self, Self::ObservedCoOccurrence)
    }

    /// Returns `true` when the relation represents funding, contract award, or grant-to-output
    /// linkage (`INTEL-02`).
    #[must_use]
    pub const fn is_funding_or_contract_link(self) -> bool {
        matches!(
            self,
            Self::GrantsFundingTo | Self::AwardsContractTo | Self::LinksGrantOrContractToOutput
        )
    }

    /// No public link edge asserts established physical or historical causation (`AGENTS.md` §2).
    #[must_use]
    pub const fn asserts_causation(self) -> bool {
        false
    }

    /// Checks whether `(from_kind, to_kind)` is valid for this relationship type.
    #[must_use]
    pub const fn accepts_endpoint_kinds(
        self,
        from_kind: PublicEntityKind,
        to_kind: PublicEntityKind,
    ) -> bool {
        match self {
            Self::GrantsFundingTo => {
                matches!(
                    from_kind,
                    PublicEntityKind::Organization | PublicEntityKind::Grant
                ) && matches!(
                    to_kind,
                    PublicEntityKind::Organization
                        | PublicEntityKind::Grant
                        | PublicEntityKind::Facility
                        | PublicEntityKind::Publication
                        | PublicEntityKind::Event
                )
            }
            Self::AwardsContractTo => {
                matches!(
                    from_kind,
                    PublicEntityKind::Organization | PublicEntityKind::Contract
                ) && matches!(
                    to_kind,
                    PublicEntityKind::Organization
                        | PublicEntityKind::Contract
                        | PublicEntityKind::Facility
                        | PublicEntityKind::Publication
                        | PublicEntityKind::Event
                )
            }
            Self::OperatesFacility => {
                matches!(from_kind, PublicEntityKind::Organization)
                    && matches!(to_kind, PublicEntityKind::Facility)
            }
            Self::AffiliatedWithOrganization => {
                matches!(from_kind, PublicEntityKind::Organization)
                    && matches!(to_kind, PublicEntityKind::Organization)
            }
            Self::PublishesOrAuthors => {
                matches!(from_kind, PublicEntityKind::Organization)
                    && matches!(to_kind, PublicEntityKind::Publication)
            }
            Self::LinksGrantOrContractToOutput => {
                matches!(
                    from_kind,
                    PublicEntityKind::Grant | PublicEntityKind::Contract
                ) && matches!(
                    to_kind,
                    PublicEntityKind::Publication
                        | PublicEntityKind::Facility
                        | PublicEntityKind::Event
                )
            }
            Self::ReportsOrParticipatesInEvent => matches!(to_kind, PublicEntityKind::Event),
            Self::CitesPublication => {
                matches!(from_kind, PublicEntityKind::Publication)
                    && matches!(to_kind, PublicEntityKind::Publication)
            }
            Self::ObservedCoOccurrence => true,
        }
    }
}

impl fmt::Display for PublicLinkRelation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GrantsFundingTo => f.write_str("grants_funding_to"),
            Self::AwardsContractTo => f.write_str("awards_contract_to"),
            Self::OperatesFacility => f.write_str("operates_facility"),
            Self::AffiliatedWithOrganization => f.write_str("affiliated_with_organization"),
            Self::PublishesOrAuthors => f.write_str("publishes_or_authors"),
            Self::LinksGrantOrContractToOutput => f.write_str("links_grant_or_contract_to_output"),
            Self::ReportsOrParticipatesInEvent => f.write_str("reports_or_participates_in_event"),
            Self::CitesPublication => f.write_str("cites_publication"),
            Self::ObservedCoOccurrence => f.write_str("observed_co_occurrence"),
        }
    }
}

/// A typed, reversible relationship edge in the public research graph (`INTEL-02`, `A08`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicLinkRecord {
    pub link_id: OpaqueId,
    pub from_endpoint: LinkEndpoint,
    pub to_endpoint: LinkEndpoint,
    pub relation: PublicLinkRelation,
    pub source_version: VersionId,
    pub verbatim_excerpt: String,
    pub decision: DecisionState,
}

/// Kind of public record cited in a [`TechnicalDevelopmentInput`] (`INTEL-02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TechnicalArtifactKind {
    GrantApplicationOrAward,
    ContractSolicitationOrAward,
    PatentApplicationOrGrant,
    TheoreticalOrConceptPaper,
    AnalyticalOrSimulationPaper,
    LaboratoryTestReport,
    FieldOrOperationalTestReport,
}

impl TechnicalArtifactKind {
    /// Returns `true` only when the artifact is a laboratory or field test report capable of
    /// reporting empirical test evidence (`INTEL-02`).
    ///
    /// Grants, contracts, patents, theoretical papers, and simulation papers return `false`.
    #[must_use]
    pub const fn can_report_empirical_demonstration(self) -> bool {
        matches!(
            self,
            Self::LaboratoryTestReport | Self::FieldOrOperationalTestReport
        )
    }
}

impl fmt::Display for TechnicalArtifactKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GrantApplicationOrAward => f.write_str("grant_application_or_award"),
            Self::ContractSolicitationOrAward => f.write_str("contract_solicitation_or_award"),
            Self::PatentApplicationOrGrant => f.write_str("patent_application_or_grant"),
            Self::TheoreticalOrConceptPaper => f.write_str("theoretical_or_concept_paper"),
            Self::AnalyticalOrSimulationPaper => f.write_str("analytical_or_simulation_paper"),
            Self::LaboratoryTestReport => f.write_str("laboratory_test_report"),
            Self::FieldOrOperationalTestReport => f.write_str("field_or_operational_test_report"),
        }
    }
}

/// Stage claimed by a source or caller for a [`TechnicalEvidenceItem`] (`INTEL-02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClaimedTechnicalStage {
    ProposalOrConcept,
    PatentSpecification,
    AnalyticalOrSimulatedModel,
    LaboratoryDemonstration,
    FieldOrOperationalDemonstration,
}

impl ClaimedTechnicalStage {
    /// Returns `true` when the claim asserts a laboratory or field/operational demonstration.
    #[must_use]
    pub const fn claims_empirical_demonstration(self) -> bool {
        matches!(
            self,
            Self::LaboratoryDemonstration | Self::FieldOrOperationalDemonstration
        )
    }
}

impl fmt::Display for ClaimedTechnicalStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProposalOrConcept => f.write_str("proposal_or_concept"),
            Self::PatentSpecification => f.write_str("patent_specification"),
            Self::AnalyticalOrSimulatedModel => f.write_str("analytical_or_simulated_model"),
            Self::LaboratoryDemonstration => f.write_str("laboratory_demonstration"),
            Self::FieldOrOperationalDemonstration => {
                f.write_str("field_or_operational_demonstration")
            }
        }
    }
}

/// Empirical scope of a reported demonstration (`INTEL-02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DemonstrationScope {
    Laboratory,
    FieldOrOperational,
}

impl fmt::Display for DemonstrationScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Laboratory => f.write_str("laboratory"),
            Self::FieldOrOperational => f.write_str("field_or_operational"),
        }
    }
}

/// One piece of public evidence in a [`TechnicalDevelopmentInput`] (`INTEL-02`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TechnicalEvidenceItem {
    pub item_id: OpaqueId,
    pub artifact_kind: TechnicalArtifactKind,
    pub claimed_stage: ClaimedTechnicalStage,
    /// Optional reference to a [`PublicEntityKind::Publication`] node so citation cycles
    /// (`CitesPublication`) propagate to this item's structural cycle flags.
    pub publication_entity_id: Option<OpaqueId>,
    /// Optional references to [`PublicEntityKind::Grant`] or [`PublicEntityKind::Contract`]
    /// nodes declared by the item itself (kept distinct from link-confirmed funding in
    /// [`TechnicalDevelopmentAssessment::item_declared_funding_or_contract_ids`]).
    pub funding_or_contract_ids: Vec<OpaqueId>,
    pub source_version: VersionId,
    pub verbatim_claim: String,
    pub stated_conditions_and_limitations: String,
    pub decision: DecisionState,
}

/// A scientific or technical capability track evaluated under `INTEL-02`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TechnicalDevelopmentInput {
    pub development_id: OpaqueId,
    pub topic: String,
    /// Organization or facility to which this technical track is attributed.
    pub subject_endpoint: LinkEndpoint,
    pub items: Vec<TechnicalEvidenceItem>,
}

/// Top-level input to [`evaluate_research_links`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchLinkRequest {
    pub limits: ResearchLinkLimits,
    pub sources: Vec<ResearchSourceRecord>,
    pub entities: Vec<ResearchEntity>,
    pub mentions: Vec<IdentityMention>,
    pub links: Vec<PublicLinkRecord>,
    pub developments: Vec<TechnicalDevelopmentInput>,
}

/// Why an evaluated mention, link, or technical development requires reassessment (`COLLAB-03`, `REASON-04`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReassessmentReason {
    SourceCorrected {
        source_version: VersionId,
        replacement_version: VersionId,
    },
    SourceRetracted {
        source_version: VersionId,
    },
    DisputedMentionIdentity {
        mention_id: OpaqueId,
    },
    ReversedMentionIdentity {
        mention_id: OpaqueId,
    },
    UnsupportedDemonstrationClaim {
        item_id: OpaqueId,
    },
    CircularCitationDependency {
        publication_id: OpaqueId,
    },
}

/// Per-source summary in a [`ResearchLinkReport`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSummary {
    pub source_id: OpaqueId,
    pub version_id: VersionId,
    pub origin_class: EvidenceOriginClass,
    pub lifecycle: SourceLifecycleState,
    /// `true` only when this source and every in-set ancestor in its `derived_from_versions`
    /// graph has a non-retracted lifecycle and an observational or source-assertion origin class
    /// (`!origin_class.requires_non_observational_qualification()`).
    pub counts_as_reported_public_source: bool,
    /// Source versions (including `self` if applicable, plus any in-set ancestors) whose
    /// `origin_class` requires non-observational qualification (`Simulation`,
    /// `AnalyticalInference`, `Forecast`, `PredictiveDerivative`, `IllustrativeRecreation`).
    pub non_observational_ancestor_versions: Vec<VersionId>,
    /// Source versions (including `self` if applicable, plus any in-set ancestors) that are
    /// [`SourceLifecycleState::Retracted`].
    pub retracted_ancestor_versions: Vec<VersionId>,
    /// Upstream source versions that were declared in `derived_from_versions` and are present
    /// in the evaluated request.
    pub in_set_derived_from_versions: Vec<VersionId>,
    /// Upstream source versions that were declared in `derived_from_versions` but are outside
    /// the evaluated request (retained so out-of-set lineage is never silently dropped).
    pub external_derived_from_versions: Vec<VersionId>,
    /// Root source versions in the evaluated request from which this source derives (or itself
    /// when it has no in-set upstream versions).
    pub lineage_root_versions: Vec<VersionId>,
}

/// Evaluated [`IdentityMention`] with its retained candidate history, resolved state, and
/// reassessment notices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluatedMention {
    pub mention_id: OpaqueId,
    pub verbatim_text: String,
    pub source_version: VersionId,
    pub expected_kind: PublicEntityKind,
    /// Full candidate list (including `Reversed` candidates and their reversal rationales)
    /// preserved for auditability (`REASON-03`).
    pub candidates: Vec<IdentityCandidateMatch>,
    pub resolution: MentionResolution,
    pub needs_reassessment: bool,
    pub reassessment_reasons: Vec<ReassessmentReason>,
}

/// Effective status of a [`PublicLinkRecord`] after resolving endpoint mentions, decisions,
/// source lifecycle, and source origin classes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkEffectiveStatus {
    /// Confirmed substantive link (`!ObservedCoOccurrence`) with uniquely confirmed endpoints
    /// and a qualifying non-retracted observational/source-assertion source.
    ConfirmedSubstantive {
        from_entity_id: OpaqueId,
        to_entity_id: OpaqueId,
    },
    /// Substantive link where either the link decision is `Proposed` or an endpoint mention
    /// is `ProposedOnly`.
    ProposedUnconfirmed {
        from_entity_id: OpaqueId,
        to_entity_id: OpaqueId,
    },
    /// Co-occurrence link (`ObservedCoOccurrence`); never counted as established funding,
    /// contract, facility, or organizational affiliation (`docs/architecture.md` §2.3).
    CoOccurrenceOnly {
        from_entity_id: Option<OpaqueId>,
        to_entity_id: Option<OpaqueId>,
    },
    /// Link cannot be resolved to a single pair of entities because at least one endpoint
    /// mention has [`MentionResolution::DisputedIdentity`] (`A08`).
    DisputedEndpointIdentity { disputed_mention_ids: Vec<OpaqueId> },
    /// Link is backed by a `Simulation`, `AnalyticalInference`, `Forecast`,
    /// `PredictiveDerivative`, or `IllustrativeRecreation` source or ancestor
    /// (`!counts_as_reported_public_source`).
    SimulatedSourceOnly {
        from_entity_id: Option<OpaqueId>,
        to_entity_id: Option<OpaqueId>,
    },
    /// Link is inactive because its source version (or any in-set ancestor in its lineage) has
    /// been [`SourceLifecycleState::Retracted`].
    InactiveRetractedSource,
    /// Link is inactive because its own decision is `Reversed` or an endpoint mention is
    /// `AllReversed`.
    InactiveReversed,
}

/// Evaluated [`PublicLinkRecord`] in a [`ResearchLinkReport`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluatedPublicLink {
    pub link_id: OpaqueId,
    pub relation: PublicLinkRelation,
    pub from_endpoint: LinkEndpoint,
    pub to_endpoint: LinkEndpoint,
    pub source_version: VersionId,
    pub verbatim_excerpt: String,
    pub effective_status: LinkEffectiveStatus,
    pub needs_reassessment: bool,
    pub reassessment_reasons: Vec<ReassessmentReason>,
}

/// Directed elementary citation cycle across [`PublicLinkRelation::CitesPublication`] links (`A08`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationCycle {
    /// Ordered publication entity IDs starting at the lexicographically smallest ID in the
    /// cycle and repeating that starting ID at the end (`[A, B, ..., A]`).
    pub publication_chain: Vec<OpaqueId>,
    /// Link IDs forming each step of `publication_chain`.
    pub link_ids: Vec<OpaqueId>,
    /// `true` if any link in the cycle has `ProposedUnconfirmed` status rather than
    /// `ConfirmedSubstantive`.
    pub includes_unconfirmed_link: bool,
    /// `true` if any link in the cycle comes from a non-observational / simulated source.
    pub includes_simulated_source: bool,
}

/// Caveat explaining why a [`TechnicalEvidenceItem`]'s assessed stage was capped or qualified
/// (`INTEL-02`, `AGENTS.md` §2 & §3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DemonstrationCaveat {
    /// A grant, contract, patent, or theoretical paper claimed an empirical demonstration
    /// (`INTEL-02`: proposals and patents are not demonstrated capability).
    ProposalOrPatentCannotEstablishDemonstration {
        artifact_kind: TechnicalArtifactKind,
        claimed_stage: ClaimedTechnicalStage,
    },
    /// An analytical or simulation study claimed a physical laboratory or field demonstration.
    AnalyticalModelCannotEstablishDemonstration {
        claimed_stage: ClaimedTechnicalStage,
    },
    /// A laboratory test report claimed a field or operational demonstration; scope is capped
    /// at [`DemonstrationScope::Laboratory`].
    LaboratoryReportClaimedFieldDemonstration,
    /// The item's source (or one or more ancestors in its `derived_from_versions` lineage) has
    /// a `Simulation`, `AnalyticalInference`, `Forecast`, `PredictiveDerivative`, or
    /// `IllustrativeRecreation` origin class (`AGENTS.md` §2).
    SimulatedSourceCannotEstablishDemonstration {
        qualifying_ancestors: Vec<(VersionId, EvidenceOriginClass)>,
    },
    /// A retracted source (`SourceLifecycleState::Retracted`) in the item's lineage cannot
    /// establish active maturity.
    RetractedSourceCannotEstablishMaturity { source_version: VersionId },
    /// The item's publication participates in or depends exclusively on a directed citation cycle.
    CircularCitationChain { publication_entity_id: OpaqueId },
}

/// Assessed technical stage of a single [`TechnicalEvidenceItem`] after applying `INTEL-02`
/// artifact-kind, source-origin, lifecycle, decision, and circular-citation rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AssessedTechnicalStage {
    ReversedClaim,
    RetractedSourceClaim,
    SimulatedEvidenceOnly,
    ProposalOrFundingScopeOnly,
    PatentSpecificationOnly,
    AnalyticalOrModelledOnly,
    UnconfirmedReportedDemonstration {
        scope: DemonstrationScope,
        in_citation_cycle: bool,
    },
    ConfirmedReportedDemonstration {
        scope: DemonstrationScope,
        in_citation_cycle: bool,
    },
}

impl fmt::Display for AssessedTechnicalStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReversedClaim => f.write_str("reversed_claim"),
            Self::RetractedSourceClaim => f.write_str("retracted_source_claim"),
            Self::SimulatedEvidenceOnly => f.write_str("simulated_evidence_only"),
            Self::ProposalOrFundingScopeOnly => f.write_str("proposal_or_funding_scope_only"),
            Self::PatentSpecificationOnly => f.write_str("patent_specification_only"),
            Self::AnalyticalOrModelledOnly => f.write_str("analytical_or_modelled_only"),
            Self::UnconfirmedReportedDemonstration {
                scope,
                in_citation_cycle,
            } => write!(
                f,
                "unconfirmed_reported_demonstration({scope}, circular={in_citation_cycle})"
            ),
            Self::ConfirmedReportedDemonstration {
                scope,
                in_citation_cycle,
            } => write!(
                f,
                "confirmed_reported_demonstration({scope}, circular={in_citation_cycle})"
            ),
        }
    }
}

/// Evaluated [`TechnicalEvidenceItem`] inside a [`TechnicalDevelopmentAssessment`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssessedTechnicalItem {
    pub item_id: OpaqueId,
    pub artifact_kind: TechnicalArtifactKind,
    pub claimed_stage: ClaimedTechnicalStage,
    pub assessed_stage: AssessedTechnicalStage,
    pub publication_entity_id: Option<OpaqueId>,
    pub funding_or_contract_ids: Vec<OpaqueId>,
    pub source_version: VersionId,
    pub verbatim_claim: String,
    pub stated_conditions_and_limitations: String,
    pub caveats: Vec<DemonstrationCaveat>,
    pub needs_reassessment: bool,
    pub reassessment_reasons: Vec<ReassessmentReason>,
}

/// Overall rolled-up maturity classification for a [`TechnicalDevelopmentAssessment`] (`INTEL-02`).
///
/// Document-level corroboration verdicts and independent-root counts are owned by
/// `oia_documents::lineage` (`AGENTS.md` §4); this enum reports the highest qualifying stage
/// together with structural source/publication metadata on [`TechnicalDevelopmentAssessment`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OverallTechnicalMaturity {
    /// Every item on the track has been reversed or comes from a retracted source.
    NoActiveEvidence,
    /// Only simulated or non-observational/non-source-assertion items are active.
    SimulatedOnly,
    /// Active evidence consists only of grant/contract proposals, awards, theoretical papers,
    /// or patents — never demonstrated capability (`INTEL-02`).
    ProposalOrPatentOnly,
    /// Highest active evidence is an analytical or simulation study without empirical test reports.
    AnalyticalOrModelledOnly,
    /// Empirical test reports exist, but all are either unconfirmed (`Proposed`) or tied
    /// exclusively to a circular citation chain.
    CircularOrUnconfirmedDemonstrationOnly,
    /// At least one confirmed, non-retracted, observational/source-assertion, non-circular
    /// laboratory or field test report supports this capability track. Still subject to
    /// [`TECHNICAL_MATURITY_CAVEAT`] and `oia_documents::lineage` corroboration evaluation.
    ReportedDemonstratedCapability { highest_scope: DemonstrationScope },
}

impl fmt::Display for OverallTechnicalMaturity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoActiveEvidence => f.write_str("no_active_evidence"),
            Self::SimulatedOnly => f.write_str("simulated_only"),
            Self::ProposalOrPatentOnly => f.write_str("proposal_or_patent_only"),
            Self::AnalyticalOrModelledOnly => f.write_str("analytical_or_modelled_only"),
            Self::CircularOrUnconfirmedDemonstrationOnly => {
                f.write_str("circular_or_unconfirmed_demonstration_only")
            }
            Self::ReportedDemonstratedCapability { highest_scope } => {
                write!(f, "reported_demonstrated_capability({highest_scope})")
            }
        }
    }
}

/// Evaluated scientific/technical development track (`INTEL-02`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TechnicalDevelopmentAssessment {
    pub development_id: OpaqueId,
    pub topic: String,
    pub subject_endpoint: LinkEndpoint,
    /// Uniquely resolved subject entity ID when `subject_endpoint` is `Entity` or a
    /// `ConfirmedUnique` mention; `None` when the subject mention is `ProposedOnly`,
    /// `DisputedIdentity`, or `AllReversed`.
    pub resolved_subject_entity_id: Option<OpaqueId>,
    pub subject_identity_disputed: bool,
    pub overall_maturity: OverallTechnicalMaturity,
    pub proposal_or_funding_item_count: usize,
    pub patent_item_count: usize,
    pub analytical_or_model_item_count: usize,
    pub unconfirmed_demonstration_item_count: usize,
    pub confirmed_demonstration_item_count: usize,
    pub circular_demonstration_item_count: usize,
    pub simulated_item_count: usize,
    pub retracted_item_count: usize,
    pub reversed_item_count: usize,
    /// Source versions of confirmed, non-circular demonstration items (reported as structure;
    /// independent-corroboration verdicts belong to `oia_documents::lineage`).
    pub demonstration_source_versions: Vec<VersionId>,
    /// In-set lineage root versions that are shared by two or more confirmed, non-circular
    /// demonstration items on this track (structural shared-lineage indicator).
    pub shared_demonstration_lineage_roots: Vec<VersionId>,
    /// `true` if any item claimed an empirical demonstration from a grant, contract, patent,
    /// conceptual/simulation paper, or non-observational/simulated source.
    pub has_unsupported_demonstration_claim: bool,
    /// Confirmed funding/contract entities established by `ConfirmedSubstantive`
    /// `GrantsFundingTo` / `AwardsContractTo` / `LinksGrantOrContractToOutput` links touching
    /// the resolved subject or item publications.
    pub confirmed_funding_or_contract_ids: Vec<OpaqueId>,
    /// Funding/contract entities declared directly on confirmed items' `funding_or_contract_ids`
    /// fields (kept separate from link-confirmed [`Self::confirmed_funding_or_contract_ids`]).
    pub item_declared_funding_or_contract_ids: Vec<OpaqueId>,
    /// Mentions with `DisputedIdentity` that touch funding/contract links for this subject,
    /// kept separate so disputed organization/grant matches never appear as confirmed funding.
    pub disputed_funding_mention_ids: Vec<OpaqueId>,
    pub items: Vec<AssessedTechnicalItem>,
    pub needs_reassessment: bool,
    pub reassessment_reasons: Vec<ReassessmentReason>,
}

/// Explicit coverage or evaluation limitations recorded in a [`ResearchLinkReport`] (`AGENTS.md` §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResearchLinkLimitation {
    /// A source declared upstream derivation from version IDs outside the evaluated request.
    OutOfSetSourceLineage {
        source_version: VersionId,
        external_upstream_versions: Vec<VersionId>,
    },
    /// Elementary citation-cycle enumeration reached `max_reported_cycles` or
    /// `max_cycle_dfs_steps`. Cycle membership (`publications_with_circular_dependency`) remains
    /// exact via Tarjan strongly connected components, but additional elementary paths may exist.
    CitationCycleSearchTruncated {
        reported_cycles: usize,
        dfs_steps: usize,
    },
    /// At least one source in the request has a non-observational / simulated origin class (or
    /// derives exclusively from such lineage roots).
    ContainsSimulatedSources {
        simulated_source_versions: Vec<VersionId>,
    },
    /// At least one source in the request is marked `CorrectedBy` or `Retracted`.
    ContainsNonCurrentSources {
        non_current_source_versions: Vec<VersionId>,
    },
}

/// Complete output of [`evaluate_research_links`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchLinkReport {
    pub method_id: &'static str,
    pub identity_resolution_caveat: &'static str,
    pub co_occurrence_caveat: &'static str,
    pub technical_maturity_caveat: &'static str,
    pub circular_citation_caveat: &'static str,
    pub simulated_source_caveat: &'static str,
    pub sources: Vec<SourceSummary>,
    pub mentions: Vec<EvaluatedMention>,
    pub links: Vec<EvaluatedPublicLink>,
    /// Substantive links (`!ObservedCoOccurrence`) with `ConfirmedSubstantive` status.
    pub confirmed_substantive_link_count: usize,
    /// Links with `CoOccurrenceOnly` status, kept separate from substantive links.
    pub co_occurrence_link_count: usize,
    /// Links blocked by `DisputedEndpointIdentity`.
    pub disputed_endpoint_link_count: usize,
    /// Canonical elementary directed citation cycles over active `CitesPublication` edges.
    pub citation_cycles: Vec<CitationCycle>,
    /// All publication entity IDs that either belong to a directed citation cycle (exact via
    /// Tarjan SCCs) or whose outgoing `CitesPublication` paths lead *exclusively* into a
    /// citation cycle without reaching any acyclic publication.
    pub publications_with_circular_dependency: Vec<OpaqueId>,
    pub developments: Vec<TechnicalDevelopmentAssessment>,
    pub limitations: Vec<ResearchLinkLimitation>,
}

impl ResearchLinkReport {
    /// Returns `true` when any limitation is present (for example truncated elementary-cycle
    /// enumeration or out-of-set source lineage).
    #[must_use]
    pub fn has_limitations(&self) -> bool {
        !self.limitations.is_empty()
    }
}

/// Internal resolved representation of a [`LinkEndpoint`].
#[derive(Debug, Clone)]
struct ResolvedEndpoint {
    kind: PublicEntityKind,
    entity_id: Option<OpaqueId>,
    is_confirmed_unique: bool,
    is_proposed_only: bool,
    disputed_mention_id: Option<OpaqueId>,
    reversed_mention_id: Option<OpaqueId>,
    reassessment_reasons: Vec<ReassessmentReason>,
}

fn check_stop_tick(
    counter: &mut usize,
    check_stop: &mut impl FnMut() -> Option<StopReason>,
) -> Result<(), ResearchLinkError> {
    *counter = counter.saturating_add(1);
    if (*counter).is_multiple_of(CONTROL_CHECK_INTERVAL)
        && let Some(reason) = check_stop()
    {
        return Err(ResearchLinkError::Stopped(reason));
    }
    Ok(())
}

/// Returns `true` when `origin` is an observational or source-assertion class eligible to
/// back reported public links and reported empirical test records (`AGENTS.md` §2).
///
/// Excludes `Simulation`, `AnalyticalInference`, `Forecast`, `PredictiveDerivative`, and
/// `IllustrativeRecreation`.
const fn can_back_reported_public_evidence(origin: EvidenceOriginClass) -> bool {
    !origin.requires_non_observational_qualification()
}

fn validate_text_field(
    context: &'static str,
    field: &'static str,
    value: &str,
    max_bytes: usize,
    limit_kind: ResearchLinkLimitKind,
    total_text_bytes: &mut usize,
    max_total_text_bytes: usize,
) -> Result<(), ResearchLinkError> {
    if value.trim().is_empty() {
        return Err(ResearchLinkError::EmptyField { context, field });
    }
    if value.len() > max_bytes {
        return Err(ResearchLinkError::BudgetExceeded {
            limit: limit_kind,
            actual: value.len(),
            max: max_bytes,
        });
    }
    // Uses `requires_visible_unicode_escape` from `oia_domain::text`, which rejects control
    // characters and bidi/invisible format characters while preserving ZWNJ (U+200C) and
    // ZWJ (U+200D) for multilingual script shaping (`INTEL-04`).
    if value.chars().any(requires_visible_unicode_escape) {
        return Err(ResearchLinkError::ControlOrFormatCharacter { context, field });
    }
    *total_text_bytes = total_text_bytes.saturating_add(value.len());
    if *total_text_bytes > max_total_text_bytes {
        return Err(ResearchLinkError::BudgetExceeded {
            limit: ResearchLinkLimitKind::TotalTextBytes,
            actual: *total_text_bytes,
            max: max_total_text_bytes,
        });
    }
    Ok(())
}

fn validate_decision_state(
    context: &'static str,
    decision: &DecisionState,
    limits: ResearchLinkLimits,
    total_text_bytes: &mut usize,
) -> Result<(), ResearchLinkError> {
    match decision {
        DecisionState::Proposed { rationale } => validate_text_field(
            context,
            "decision.proposed_rationale",
            rationale,
            limits.max_excerpt_bytes,
            ResearchLinkLimitKind::ExcerptBytes,
            total_text_bytes,
            limits.max_total_text_bytes,
        ),
        DecisionState::Confirmed { rationale, .. } => validate_text_field(
            context,
            "decision.confirmed_rationale",
            rationale,
            limits.max_excerpt_bytes,
            ResearchLinkLimitKind::ExcerptBytes,
            total_text_bytes,
            limits.max_total_text_bytes,
        ),
        DecisionState::Reversed {
            reversal_rationale, ..
        } => validate_text_field(
            context,
            "decision.reversal_rationale",
            reversal_rationale,
            limits.max_excerpt_bytes,
            ResearchLinkLimitKind::ExcerptBytes,
            total_text_bytes,
            limits.max_total_text_bytes,
        ),
    }
}

fn direct_source_reassessment_reason(
    lifecycle: &SourceLifecycleState,
    version_id: &VersionId,
) -> Option<ReassessmentReason> {
    match lifecycle {
        SourceLifecycleState::Current => None,
        SourceLifecycleState::CorrectedBy {
            replacement_version,
            ..
        } => Some(ReassessmentReason::SourceCorrected {
            source_version: version_id.clone(),
            replacement_version: replacement_version.clone(),
        }),
        SourceLifecycleState::Retracted { .. } => Some(ReassessmentReason::SourceRetracted {
            source_version: version_id.clone(),
        }),
    }
}

/// Evaluates a bounded [`ResearchLinkRequest`] with cooperative cancellation checks.
#[allow(clippy::too_many_lines)]
pub fn evaluate_research_links(
    request: &ResearchLinkRequest,
    mut check_stop: impl FnMut() -> Option<StopReason>,
) -> Result<ResearchLinkReport, ResearchLinkError> {
    if let Some(reason) = check_stop() {
        return Err(ResearchLinkError::Stopped(reason));
    }

    let limits = request.limits;
    ResearchLinkLimits::new(
        limits.max_sources,
        limits.max_entities,
        limits.max_alternate_names_per_entity,
        limits.max_mentions,
        limits.max_candidates_per_mention,
        limits.max_links,
        limits.max_developments,
        limits.max_items_per_development,
        limits.max_funding_refs_per_item,
        limits.max_upstream_versions_per_source,
        limits.max_label_bytes,
        limits.max_excerpt_bytes,
        limits.max_total_text_bytes,
        limits.max_reported_cycles,
        limits.max_cycle_dfs_steps,
    )?;

    if request.sources.is_empty() {
        return Err(ResearchLinkError::EmptySources);
    }
    if request.sources.len() > limits.max_sources {
        return Err(ResearchLinkError::BudgetExceeded {
            limit: ResearchLinkLimitKind::Sources,
            actual: request.sources.len(),
            max: limits.max_sources,
        });
    }
    if request.entities.is_empty()
        && request.mentions.is_empty()
        && request.links.is_empty()
        && request.developments.is_empty()
    {
        return Err(ResearchLinkError::EmptyGraphContent);
    }
    if request.entities.len() > limits.max_entities {
        return Err(ResearchLinkError::BudgetExceeded {
            limit: ResearchLinkLimitKind::Entities,
            actual: request.entities.len(),
            max: limits.max_entities,
        });
    }
    if request.mentions.len() > limits.max_mentions {
        return Err(ResearchLinkError::BudgetExceeded {
            limit: ResearchLinkLimitKind::Mentions,
            actual: request.mentions.len(),
            max: limits.max_mentions,
        });
    }
    if request.links.len() > limits.max_links {
        return Err(ResearchLinkError::BudgetExceeded {
            limit: ResearchLinkLimitKind::Links,
            actual: request.links.len(),
            max: limits.max_links,
        });
    }
    if request.developments.len() > limits.max_developments {
        return Err(ResearchLinkError::BudgetExceeded {
            limit: ResearchLinkLimitKind::Developments,
            actual: request.developments.len(),
            max: limits.max_developments,
        });
    }

    let mut tick = 0usize;
    let mut total_text_bytes = 0usize;

    // 1. Validate sources and index by VersionId.
    let mut source_by_version: BTreeMap<VersionId, &ResearchSourceRecord> = BTreeMap::new();
    for src in &request.sources {
        check_stop_tick(&mut tick, &mut check_stop)?;
        if source_by_version
            .insert(src.version_id.clone(), src)
            .is_some()
        {
            return Err(ResearchLinkError::DuplicateSourceVersion(
                src.version_id.clone(),
            ));
        }
        if src.derived_from_versions.len() > limits.max_upstream_versions_per_source {
            return Err(ResearchLinkError::BudgetExceeded {
                limit: ResearchLinkLimitKind::UpstreamVersionsPerSource,
                actual: src.derived_from_versions.len(),
                max: limits.max_upstream_versions_per_source,
            });
        }
        let mut seen_up = BTreeSet::new();
        for up in &src.derived_from_versions {
            if up == &src.version_id {
                return Err(ResearchLinkError::SelfSourceDerivation(
                    src.version_id.clone(),
                ));
            }
            if !seen_up.insert(up.clone()) {
                return Err(ResearchLinkError::DuplicateSourceDerivation {
                    source_version: src.version_id.clone(),
                    upstream_version: up.clone(),
                });
            }
        }
        validate_text_field(
            "source",
            "citation_label",
            &src.citation_label,
            limits.max_excerpt_bytes,
            ResearchLinkLimitKind::ExcerptBytes,
            &mut total_text_bytes,
            limits.max_total_text_bytes,
        )?;
        match &src.lifecycle {
            SourceLifecycleState::Current => {}
            SourceLifecycleState::CorrectedBy {
                replacement_version,
                summary,
            } => {
                if replacement_version == &src.version_id {
                    return Err(ResearchLinkError::SourceCorrectedBySelf(
                        src.version_id.clone(),
                    ));
                }
                validate_text_field(
                    "source",
                    "lifecycle.corrected_by_summary",
                    summary,
                    limits.max_excerpt_bytes,
                    ResearchLinkLimitKind::ExcerptBytes,
                    &mut total_text_bytes,
                    limits.max_total_text_bytes,
                )?;
            }
            SourceLifecycleState::Retracted { reason } => {
                validate_text_field(
                    "source",
                    "lifecycle.retracted_reason",
                    reason,
                    limits.max_excerpt_bytes,
                    ResearchLinkLimitKind::ExcerptBytes,
                    &mut total_text_bytes,
                    limits.max_total_text_bytes,
                )?;
            }
        }
    }

    // Compute source lineage roots, transitive reassessment reasons, retraction status, and
    // observational/source-assertion eligibility across in-set sources.
    let mut source_summaries = Vec::with_capacity(request.sources.len());
    let mut source_summary_by_version: BTreeMap<VersionId, SourceSummary> = BTreeMap::new();
    let mut source_reassessment_by_version: BTreeMap<VersionId, Vec<ReassessmentReason>> =
        BTreeMap::new();
    let mut source_retracted_by_version: BTreeMap<VersionId, bool> = BTreeMap::new();
    let mut limitations = Vec::new();
    let mut simulated_source_versions = Vec::new();
    let mut non_current_source_versions = Vec::new();

    for src in &request.sources {
        check_stop_tick(&mut tick, &mut check_stop)?;
        let mut in_set_up = Vec::new();
        let mut ext_up = Vec::new();
        for up in &src.derived_from_versions {
            if source_by_version.contains_key(up) {
                in_set_up.push(up.clone());
            } else {
                ext_up.push(up.clone());
            }
        }
        if !ext_up.is_empty() {
            limitations.push(ResearchLinkLimitation::OutOfSetSourceLineage {
                source_version: src.version_id.clone(),
                external_upstream_versions: ext_up.clone(),
            });
        }

        // Walk in-set upstream graph: evaluate EVERY visited node (self + intermediate ancestors +
        // roots) for reassessment reasons, retraction, and non-observational qualification
        // (`AGENTS.md` §2: feeding an inference or forecast into a downstream source must not
        // promote it to an observation).
        let mut visited: BTreeSet<VersionId> = BTreeSet::new();
        let mut stack: Vec<VersionId> = vec![src.version_id.clone()];
        let mut roots: BTreeSet<VersionId> = BTreeSet::new();
        let mut lineage_reasons: BTreeSet<ReassessmentReason> = BTreeSet::new();
        let mut non_obs_ancestors: BTreeSet<VersionId> = BTreeSet::new();
        let mut retracted_ancestors: BTreeSet<VersionId> = BTreeSet::new();

        while let Some(curr_ver) = stack.pop() {
            check_stop_tick(&mut tick, &mut check_stop)?;
            if !visited.insert(curr_ver.clone()) {
                continue;
            }
            if let Some(curr_src) = source_by_version.get(&curr_ver) {
                if let Some(r) = direct_source_reassessment_reason(&curr_src.lifecycle, &curr_ver) {
                    lineage_reasons.insert(r);
                }
                if !can_back_reported_public_evidence(curr_src.origin_class) {
                    non_obs_ancestors.insert(curr_ver.clone());
                }
                if matches!(curr_src.lifecycle, SourceLifecycleState::Retracted { .. }) {
                    retracted_ancestors.insert(curr_ver.clone());
                }
                let curr_in_set_up: Vec<VersionId> = curr_src
                    .derived_from_versions
                    .iter()
                    .filter(|u| source_by_version.contains_key(*u))
                    .cloned()
                    .collect();
                if curr_in_set_up.is_empty() {
                    roots.insert(curr_ver.clone());
                } else {
                    for next_up in curr_in_set_up {
                        if !visited.contains(&next_up) {
                            stack.push(next_up);
                        }
                    }
                }
            }
        }
        if roots.is_empty() {
            for v in &visited {
                roots.insert(v.clone());
            }
        }

        let is_retracted = !retracted_ancestors.is_empty();
        let has_qualifying_origin = non_obs_ancestors.is_empty();
        let counts_as_reported_public_source = !is_retracted && has_qualifying_origin;

        if !has_qualifying_origin {
            simulated_source_versions.push(src.version_id.clone());
        }
        if !matches!(src.lifecycle, SourceLifecycleState::Current) {
            non_current_source_versions.push(src.version_id.clone());
        }

        source_reassessment_by_version.insert(
            src.version_id.clone(),
            lineage_reasons.into_iter().collect(),
        );
        source_retracted_by_version.insert(src.version_id.clone(), is_retracted);

        let summary = SourceSummary {
            source_id: src.source_id.clone(),
            version_id: src.version_id.clone(),
            origin_class: src.origin_class,
            lifecycle: src.lifecycle.clone(),
            counts_as_reported_public_source,
            non_observational_ancestor_versions: non_obs_ancestors.into_iter().collect(),
            retracted_ancestor_versions: retracted_ancestors.into_iter().collect(),
            in_set_derived_from_versions: in_set_up,
            external_derived_from_versions: ext_up,
            lineage_root_versions: roots.into_iter().collect(),
        };
        source_summary_by_version.insert(src.version_id.clone(), summary.clone());
        source_summaries.push(summary);
    }

    if !simulated_source_versions.is_empty() {
        limitations.push(ResearchLinkLimitation::ContainsSimulatedSources {
            simulated_source_versions,
        });
    }
    if !non_current_source_versions.is_empty() {
        limitations.push(ResearchLinkLimitation::ContainsNonCurrentSources {
            non_current_source_versions,
        });
    }

    // 2. Validate entities.
    let mut entity_by_id: BTreeMap<OpaqueId, &ResearchEntity> = BTreeMap::new();
    for entity in &request.entities {
        check_stop_tick(&mut tick, &mut check_stop)?;
        if entity_by_id
            .insert(entity.entity_id.clone(), entity)
            .is_some()
        {
            return Err(ResearchLinkError::DuplicateEntityId(
                entity.entity_id.clone(),
            ));
        }
        if !source_by_version.contains_key(&entity.defining_source_version) {
            return Err(ResearchLinkError::UnknownSourceVersion {
                context: "entity.defining_source_version",
                version_id: entity.defining_source_version.clone(),
            });
        }
        validate_text_field(
            "entity",
            "primary_name",
            &entity.primary_name,
            limits.max_label_bytes,
            ResearchLinkLimitKind::LabelBytes,
            &mut total_text_bytes,
            limits.max_total_text_bytes,
        )?;
        if entity.alternate_names.len() > limits.max_alternate_names_per_entity {
            return Err(ResearchLinkError::BudgetExceeded {
                limit: ResearchLinkLimitKind::AlternateNamesPerEntity,
                actual: entity.alternate_names.len(),
                max: limits.max_alternate_names_per_entity,
            });
        }
        for alt in &entity.alternate_names {
            check_stop_tick(&mut tick, &mut check_stop)?;
            if !source_by_version.contains_key(&alt.source_version) {
                return Err(ResearchLinkError::UnknownSourceVersion {
                    context: "entity.alternate_name.source_version",
                    version_id: alt.source_version.clone(),
                });
            }
            validate_text_field(
                "entity.alternate_name",
                "name",
                &alt.name,
                limits.max_label_bytes,
                ResearchLinkLimitKind::LabelBytes,
                &mut total_text_bytes,
                limits.max_total_text_bytes,
            )?;
            match &alt.kind {
                AlternateNameKind::Acronym
                | AlternateNameKind::FormerOrTradeName
                | AlternateNameKind::ProgramOrAwardCode => {}
                AlternateNameKind::Transliteration { script } => {
                    validate_text_field(
                        "entity.alternate_name",
                        "transliteration.script",
                        script,
                        limits.max_label_bytes,
                        ResearchLinkLimitKind::LabelBytes,
                        &mut total_text_bytes,
                        limits.max_total_text_bytes,
                    )?;
                }
                AlternateNameKind::TranslatedTitle { language } => {
                    validate_text_field(
                        "entity.alternate_name",
                        "translated_title.language",
                        language,
                        limits.max_label_bytes,
                        ResearchLinkLimitKind::LabelBytes,
                        &mut total_text_bytes,
                        limits.max_total_text_bytes,
                    )?;
                }
            }
        }
    }

    // 3. Validate and evaluate mentions.
    let mut evaluated_mentions = Vec::with_capacity(request.mentions.len());
    let mut mention_by_id: BTreeMap<OpaqueId, EvaluatedMention> = BTreeMap::new();

    for mention in &request.mentions {
        check_stop_tick(&mut tick, &mut check_stop)?;
        if mention_by_id.contains_key(&mention.mention_id) {
            return Err(ResearchLinkError::DuplicateMentionId(
                mention.mention_id.clone(),
            ));
        }
        if !source_by_version.contains_key(&mention.source_version) {
            return Err(ResearchLinkError::UnknownSourceVersion {
                context: "mention.source_version",
                version_id: mention.source_version.clone(),
            });
        }
        validate_text_field(
            "mention",
            "verbatim_text",
            &mention.verbatim_text,
            limits.max_label_bytes,
            ResearchLinkLimitKind::LabelBytes,
            &mut total_text_bytes,
            limits.max_total_text_bytes,
        )?;
        if mention.candidates.is_empty() {
            return Err(ResearchLinkError::EmptyMentionCandidates(
                mention.mention_id.clone(),
            ));
        }
        if mention.candidates.len() > limits.max_candidates_per_mention {
            return Err(ResearchLinkError::BudgetExceeded {
                limit: ResearchLinkLimitKind::CandidatesPerMention,
                actual: mention.candidates.len(),
                max: limits.max_candidates_per_mention,
            });
        }

        let mut seen_cand_ids = BTreeSet::new();
        let mut seen_target_entities = BTreeSet::new();
        let mut confirmed_cands: Vec<(&OpaqueId, &OpaqueId)> = Vec::new();
        let mut proposed_cands: Vec<(&OpaqueId, &OpaqueId)> = Vec::new();
        let mut reversed_count = 0usize;
        let mut reasons: BTreeSet<ReassessmentReason> = BTreeSet::new();

        if let Some(src_reasons) = source_reassessment_by_version.get(&mention.source_version) {
            for r in src_reasons {
                reasons.insert(r.clone());
            }
        }

        for cand in &mention.candidates {
            check_stop_tick(&mut tick, &mut check_stop)?;
            if !seen_cand_ids.insert(cand.candidate_id.clone()) {
                return Err(ResearchLinkError::DuplicateCandidateId {
                    mention_id: mention.mention_id.clone(),
                    candidate_id: cand.candidate_id.clone(),
                });
            }
            if !seen_target_entities.insert(cand.target_entity_id.clone()) {
                return Err(ResearchLinkError::DuplicateCandidateTargetEntity {
                    mention_id: mention.mention_id.clone(),
                    target_entity_id: cand.target_entity_id.clone(),
                });
            }
            let Some(target_entity) = entity_by_id.get(&cand.target_entity_id) else {
                return Err(ResearchLinkError::UnknownEntity {
                    context: "mention.candidate.target_entity_id",
                    entity_id: cand.target_entity_id.clone(),
                });
            };
            if target_entity.kind != mention.expected_kind {
                return Err(ResearchLinkError::MentionEntityKindMismatch {
                    mention_id: mention.mention_id.clone(),
                    expected: mention.expected_kind,
                    actual: target_entity.kind,
                    target_entity_id: cand.target_entity_id.clone(),
                });
            }
            if !source_by_version.contains_key(&cand.supporting_source_version) {
                return Err(ResearchLinkError::UnknownSourceVersion {
                    context: "mention.candidate.supporting_source_version",
                    version_id: cand.supporting_source_version.clone(),
                });
            }
            validate_text_field(
                "mention.candidate",
                "matching_basis",
                &cand.matching_basis,
                limits.max_excerpt_bytes,
                ResearchLinkLimitKind::ExcerptBytes,
                &mut total_text_bytes,
                limits.max_total_text_bytes,
            )?;
            validate_decision_state(
                "mention.candidate",
                &cand.decision,
                limits,
                &mut total_text_bytes,
            )?;

            if !matches!(cand.decision, DecisionState::Reversed { .. }) {
                if let Some(cand_reasons) =
                    source_reassessment_by_version.get(&cand.supporting_source_version)
                {
                    for r in cand_reasons {
                        reasons.insert(r.clone());
                    }
                }
                if let Some(def_reasons) =
                    source_reassessment_by_version.get(&target_entity.defining_source_version)
                {
                    for r in def_reasons {
                        reasons.insert(r.clone());
                    }
                }
            }

            match &cand.decision {
                DecisionState::Confirmed { .. } => {
                    confirmed_cands.push((&cand.target_entity_id, &cand.candidate_id));
                }
                DecisionState::Proposed { .. } => {
                    proposed_cands.push((&cand.target_entity_id, &cand.candidate_id));
                }
                DecisionState::Reversed { .. } => {
                    reversed_count += 1;
                }
            }
        }

        let resolution = match (confirmed_cands.as_slice(), proposed_cands.as_slice()) {
            ([(entity_id, candidate_id)], []) => MentionResolution::ConfirmedUnique {
                entity_id: (*entity_id).clone(),
                candidate_id: (*candidate_id).clone(),
            },
            ([], [(candidate_entity_id, candidate_id)]) => MentionResolution::ProposedOnly {
                candidate_entity_id: (*candidate_entity_id).clone(),
                candidate_id: (*candidate_id).clone(),
            },
            ([], []) => MentionResolution::AllReversed {
                reversed_candidate_count: reversed_count,
            },
            _ => {
                reasons.insert(ReassessmentReason::DisputedMentionIdentity {
                    mention_id: mention.mention_id.clone(),
                });
                MentionResolution::DisputedIdentity {
                    confirmed_entity_ids: confirmed_cands
                        .iter()
                        .map(|(e, _)| (*e).clone())
                        .collect(),
                    proposed_entity_ids: proposed_cands.iter().map(|(e, _)| (*e).clone()).collect(),
                }
            }
        };

        let reassessment_reasons: Vec<ReassessmentReason> = reasons.into_iter().collect();
        let evaluated = EvaluatedMention {
            mention_id: mention.mention_id.clone(),
            verbatim_text: mention.verbatim_text.clone(),
            source_version: mention.source_version.clone(),
            expected_kind: mention.expected_kind,
            candidates: mention.candidates.clone(),
            resolution,
            needs_reassessment: !reassessment_reasons.is_empty(),
            reassessment_reasons,
        };
        mention_by_id.insert(mention.mention_id.clone(), evaluated.clone());
        evaluated_mentions.push(evaluated);
    }

    let resolve_endpoint = |context: &'static str,
                            endpoint: &LinkEndpoint|
     -> Result<ResolvedEndpoint, ResearchLinkError> {
        match endpoint {
            LinkEndpoint::Entity(entity_id) => {
                let Some(ent) = entity_by_id.get(entity_id) else {
                    return Err(ResearchLinkError::UnknownEntity {
                        context,
                        entity_id: entity_id.clone(),
                    });
                };
                let ep_reasons = source_reassessment_by_version
                    .get(&ent.defining_source_version)
                    .cloned()
                    .unwrap_or_default();
                Ok(ResolvedEndpoint {
                    kind: ent.kind,
                    entity_id: Some(entity_id.clone()),
                    is_confirmed_unique: true,
                    is_proposed_only: false,
                    disputed_mention_id: None,
                    reversed_mention_id: None,
                    reassessment_reasons: ep_reasons,
                })
            }
            LinkEndpoint::Mention(mention_id) => {
                let Some(m) = mention_by_id.get(mention_id) else {
                    return Err(ResearchLinkError::UnknownMention {
                        context,
                        mention_id: mention_id.clone(),
                    });
                };
                let mut ep_reasons = m.reassessment_reasons.clone();
                match &m.resolution {
                    MentionResolution::ConfirmedUnique { entity_id, .. } => Ok(ResolvedEndpoint {
                        kind: m.expected_kind,
                        entity_id: Some(entity_id.clone()),
                        is_confirmed_unique: true,
                        is_proposed_only: false,
                        disputed_mention_id: None,
                        reversed_mention_id: None,
                        reassessment_reasons: ep_reasons,
                    }),
                    MentionResolution::ProposedOnly {
                        candidate_entity_id,
                        ..
                    } => Ok(ResolvedEndpoint {
                        kind: m.expected_kind,
                        entity_id: Some(candidate_entity_id.clone()),
                        is_confirmed_unique: false,
                        is_proposed_only: true,
                        disputed_mention_id: None,
                        reversed_mention_id: None,
                        reassessment_reasons: ep_reasons,
                    }),
                    MentionResolution::DisputedIdentity { .. } => {
                        ep_reasons.push(ReassessmentReason::DisputedMentionIdentity {
                            mention_id: mention_id.clone(),
                        });
                        Ok(ResolvedEndpoint {
                            kind: m.expected_kind,
                            entity_id: None,
                            is_confirmed_unique: false,
                            is_proposed_only: false,
                            disputed_mention_id: Some(mention_id.clone()),
                            reversed_mention_id: None,
                            reassessment_reasons: ep_reasons,
                        })
                    }
                    MentionResolution::AllReversed { .. } => {
                        ep_reasons.push(ReassessmentReason::ReversedMentionIdentity {
                            mention_id: mention_id.clone(),
                        });
                        Ok(ResolvedEndpoint {
                            kind: m.expected_kind,
                            entity_id: None,
                            is_confirmed_unique: false,
                            is_proposed_only: false,
                            disputed_mention_id: None,
                            reversed_mention_id: Some(mention_id.clone()),
                            reassessment_reasons: ep_reasons,
                        })
                    }
                }
            }
        }
    };

    // 4. Validate and evaluate public links.
    let mut seen_link_ids = BTreeSet::new();
    let mut evaluated_links = Vec::with_capacity(request.links.len());
    let mut confirmed_substantive_link_count = 0usize;
    let mut co_occurrence_link_count = 0usize;
    let mut disputed_endpoint_link_count = 0usize;

    let mut citation_edges: Vec<(OpaqueId, OpaqueId, OpaqueId, bool, bool)> = Vec::new();

    for link in &request.links {
        check_stop_tick(&mut tick, &mut check_stop)?;
        if !seen_link_ids.insert(link.link_id.clone()) {
            return Err(ResearchLinkError::DuplicateLinkId(link.link_id.clone()));
        }
        let Some(link_src_summary) = source_summary_by_version.get(&link.source_version) else {
            return Err(ResearchLinkError::UnknownSourceVersion {
                context: "link.source_version",
                version_id: link.source_version.clone(),
            });
        };
        let link_src_retracted = source_retracted_by_version
            .get(&link.source_version)
            .copied()
            .unwrap_or(false);
        validate_text_field(
            "link",
            "verbatim_excerpt",
            &link.verbatim_excerpt,
            limits.max_excerpt_bytes,
            ResearchLinkLimitKind::ExcerptBytes,
            &mut total_text_bytes,
            limits.max_total_text_bytes,
        )?;
        validate_decision_state("link", &link.decision, limits, &mut total_text_bytes)?;

        let from_res = resolve_endpoint("link.from_endpoint", &link.from_endpoint)?;
        let to_res = resolve_endpoint("link.to_endpoint", &link.to_endpoint)?;

        if !link
            .relation
            .accepts_endpoint_kinds(from_res.kind, to_res.kind)
        {
            return Err(ResearchLinkError::InvalidLinkEndpointKinds {
                link_id: link.link_id.clone(),
                relation: link.relation,
                from_kind: from_res.kind,
                to_kind: to_res.kind,
            });
        }

        if link.relation == PublicLinkRelation::CitesPublication
            && let (Some(f_id), Some(t_id)) = (&from_res.entity_id, &to_res.entity_id)
            && f_id == t_id
        {
            return Err(ResearchLinkError::SelfCitationLink {
                link_id: link.link_id.clone(),
                publication_id: f_id.clone(),
            });
        }

        let mut reasons: BTreeSet<ReassessmentReason> = BTreeSet::new();
        if !matches!(link.decision, DecisionState::Reversed { .. }) {
            if let Some(src_reasons) = source_reassessment_by_version.get(&link.source_version) {
                for r in src_reasons {
                    reasons.insert(r.clone());
                }
            }
            for r in from_res.reassessment_reasons {
                reasons.insert(r);
            }
            for r in to_res.reassessment_reasons {
                reasons.insert(r);
            }
        }

        let effective_status = if matches!(link.decision, DecisionState::Reversed { .. })
            || from_res.reversed_mention_id.is_some()
            || to_res.reversed_mention_id.is_some()
        {
            LinkEffectiveStatus::InactiveReversed
        } else if link_src_retracted {
            LinkEffectiveStatus::InactiveRetractedSource
        } else if from_res.disputed_mention_id.is_some() || to_res.disputed_mention_id.is_some() {
            let mut disputed_mention_ids = Vec::new();
            if let Some(m) = from_res.disputed_mention_id {
                disputed_mention_ids.push(m);
            }
            if let Some(m) = to_res.disputed_mention_id
                && !disputed_mention_ids.contains(&m)
            {
                disputed_mention_ids.push(m);
            }
            disputed_endpoint_link_count += 1;
            LinkEffectiveStatus::DisputedEndpointIdentity {
                disputed_mention_ids,
            }
        } else if !link.relation.is_substantive_public_link() {
            co_occurrence_link_count += 1;
            LinkEffectiveStatus::CoOccurrenceOnly {
                from_entity_id: from_res.entity_id,
                to_entity_id: to_res.entity_id,
            }
        } else if !link_src_summary.counts_as_reported_public_source {
            if link.relation == PublicLinkRelation::CitesPublication
                && let (Some(f_id), Some(t_id)) = (&from_res.entity_id, &to_res.entity_id)
            {
                citation_edges.push((
                    f_id.clone(),
                    t_id.clone(),
                    link.link_id.clone(),
                    matches!(link.decision, DecisionState::Proposed { .. })
                        || from_res.is_proposed_only
                        || to_res.is_proposed_only,
                    true,
                ));
            }
            LinkEffectiveStatus::SimulatedSourceOnly {
                from_entity_id: from_res.entity_id,
                to_entity_id: to_res.entity_id,
            }
        } else if let (Some(f_id), Some(t_id)) = (from_res.entity_id, to_res.entity_id) {
            let is_unconfirmed = matches!(link.decision, DecisionState::Proposed { .. })
                || !from_res.is_confirmed_unique
                || !to_res.is_confirmed_unique
                || from_res.is_proposed_only
                || to_res.is_proposed_only;
            if link.relation == PublicLinkRelation::CitesPublication {
                citation_edges.push((
                    f_id.clone(),
                    t_id.clone(),
                    link.link_id.clone(),
                    is_unconfirmed,
                    false,
                ));
            }
            if is_unconfirmed {
                LinkEffectiveStatus::ProposedUnconfirmed {
                    from_entity_id: f_id,
                    to_entity_id: t_id,
                }
            } else {
                confirmed_substantive_link_count += 1;
                LinkEffectiveStatus::ConfirmedSubstantive {
                    from_entity_id: f_id,
                    to_entity_id: t_id,
                }
            }
        } else {
            LinkEffectiveStatus::InactiveReversed
        };

        let reassessment_reasons: Vec<ReassessmentReason> = reasons.into_iter().collect();
        evaluated_links.push(EvaluatedPublicLink {
            link_id: link.link_id.clone(),
            relation: link.relation,
            from_endpoint: link.from_endpoint.clone(),
            to_endpoint: link.to_endpoint.clone(),
            source_version: link.source_version.clone(),
            verbatim_excerpt: link.verbatim_excerpt.clone(),
            effective_status,
            needs_reassessment: !reassessment_reasons.is_empty(),
            reassessment_reasons,
        });
    }

    // 5. Trace circular citations over CitesPublication edges.
    let (citation_cycles, publications_with_circular_dependency, cycle_truncation) =
        trace_citation_cycles(&citation_edges, limits, &mut tick, &mut check_stop)?;
    if let Some(lim) = cycle_truncation {
        limitations.push(lim);
    }
    let circular_pub_set: BTreeSet<OpaqueId> = publications_with_circular_dependency
        .iter()
        .cloned()
        .collect();

    // 6. Validate and evaluate technical developments (`INTEL-02`).
    let mut seen_dev_ids = BTreeSet::new();
    let mut evaluated_developments = Vec::with_capacity(request.developments.len());

    for dev in &request.developments {
        check_stop_tick(&mut tick, &mut check_stop)?;
        if !seen_dev_ids.insert(dev.development_id.clone()) {
            return Err(ResearchLinkError::DuplicateDevelopmentId(
                dev.development_id.clone(),
            ));
        }
        validate_text_field(
            "development",
            "topic",
            &dev.topic,
            limits.max_label_bytes,
            ResearchLinkLimitKind::LabelBytes,
            &mut total_text_bytes,
            limits.max_total_text_bytes,
        )?;
        if dev.items.is_empty() {
            return Err(ResearchLinkError::EmptyDevelopmentItems(
                dev.development_id.clone(),
            ));
        }
        if dev.items.len() > limits.max_items_per_development {
            return Err(ResearchLinkError::BudgetExceeded {
                limit: ResearchLinkLimitKind::ItemsPerDevelopment,
                actual: dev.items.len(),
                max: limits.max_items_per_development,
            });
        }

        let subj_res = resolve_endpoint("development.subject_endpoint", &dev.subject_endpoint)?;

        if !matches!(
            subj_res.kind,
            PublicEntityKind::Organization | PublicEntityKind::Facility
        ) {
            return Err(ResearchLinkError::DevelopmentSubjectKindInvalid {
                development_id: dev.development_id.clone(),
                actual_kind: subj_res.kind,
            });
        }

        let resolved_subject_entity_id = if subj_res.is_confirmed_unique {
            subj_res.entity_id
        } else {
            None
        };
        let subject_identity_disputed = subj_res.disputed_mention_id.is_some();

        let mut dev_reasons: BTreeSet<ReassessmentReason> =
            subj_res.reassessment_reasons.into_iter().collect();
        if let Some(m) = &subj_res.disputed_mention_id {
            dev_reasons.insert(ReassessmentReason::DisputedMentionIdentity {
                mention_id: m.clone(),
            });
        }
        if let Some(m) = &subj_res.reversed_mention_id {
            dev_reasons.insert(ReassessmentReason::ReversedMentionIdentity {
                mention_id: m.clone(),
            });
        }

        let mut seen_item_ids = BTreeSet::new();
        let mut assessed_items = Vec::with_capacity(dev.items.len());
        let mut proposal_or_funding_item_count = 0usize;
        let mut patent_item_count = 0usize;
        let mut analytical_or_model_item_count = 0usize;
        let mut unconfirmed_demonstration_item_count = 0usize;
        let mut confirmed_demonstration_item_count = 0usize;
        let mut circular_demonstration_item_count = 0usize;
        let mut simulated_item_count = 0usize;
        let mut retracted_item_count = 0usize;
        let mut reversed_item_count = 0usize;
        let mut has_unsupported_demonstration_claim = false;
        let mut highest_confirmed_non_circular_scope: Option<DemonstrationScope> = None;

        let mut demo_source_versions: BTreeSet<VersionId> = BTreeSet::new();
        let mut demo_lineage_root_sources: BTreeMap<VersionId, BTreeSet<VersionId>> =
            BTreeMap::new();
        let mut confirmed_funding_set: BTreeSet<OpaqueId> = BTreeSet::new();
        let mut item_declared_funding_set: BTreeSet<OpaqueId> = BTreeSet::new();
        let mut item_publications: BTreeSet<OpaqueId> = BTreeSet::new();

        for item in &dev.items {
            check_stop_tick(&mut tick, &mut check_stop)?;
            if !seen_item_ids.insert(item.item_id.clone()) {
                return Err(ResearchLinkError::DuplicateDevelopmentItemId {
                    development_id: dev.development_id.clone(),
                    item_id: item.item_id.clone(),
                });
            }
            let Some(item_src_summary) = source_summary_by_version.get(&item.source_version) else {
                return Err(ResearchLinkError::UnknownSourceVersion {
                    context: "development.item.source_version",
                    version_id: item.source_version.clone(),
                });
            };
            let item_src_retracted = source_retracted_by_version
                .get(&item.source_version)
                .copied()
                .unwrap_or(false);
            validate_text_field(
                "development.item",
                "verbatim_claim",
                &item.verbatim_claim,
                limits.max_excerpt_bytes,
                ResearchLinkLimitKind::ExcerptBytes,
                &mut total_text_bytes,
                limits.max_total_text_bytes,
            )?;
            validate_text_field(
                "development.item",
                "stated_conditions_and_limitations",
                &item.stated_conditions_and_limitations,
                limits.max_excerpt_bytes,
                ResearchLinkLimitKind::ExcerptBytes,
                &mut total_text_bytes,
                limits.max_total_text_bytes,
            )?;
            validate_decision_state(
                "development.item",
                &item.decision,
                limits,
                &mut total_text_bytes,
            )?;

            if let Some(pub_id) = &item.publication_entity_id {
                let Some(pub_ent) = entity_by_id.get(pub_id) else {
                    return Err(ResearchLinkError::UnknownEntity {
                        context: "development.item.publication_entity_id",
                        entity_id: pub_id.clone(),
                    });
                };
                if pub_ent.kind != PublicEntityKind::Publication {
                    return Err(ResearchLinkError::ItemPublicationKindMismatch {
                        development_id: dev.development_id.clone(),
                        item_id: item.item_id.clone(),
                        entity_id: pub_id.clone(),
                        actual_kind: pub_ent.kind,
                    });
                }
            }

            if item.funding_or_contract_ids.len() > limits.max_funding_refs_per_item {
                return Err(ResearchLinkError::BudgetExceeded {
                    limit: ResearchLinkLimitKind::FundingRefsPerItem,
                    actual: item.funding_or_contract_ids.len(),
                    max: limits.max_funding_refs_per_item,
                });
            }
            let mut seen_funding = BTreeSet::new();
            for fund_id in &item.funding_or_contract_ids {
                if !seen_funding.insert(fund_id.clone()) {
                    return Err(ResearchLinkError::DuplicateFundingReference {
                        development_id: dev.development_id.clone(),
                        item_id: item.item_id.clone(),
                        funding_entity_id: fund_id.clone(),
                    });
                }
                let Some(fund_ent) = entity_by_id.get(fund_id) else {
                    return Err(ResearchLinkError::UnknownEntity {
                        context: "development.item.funding_or_contract_ids",
                        entity_id: fund_id.clone(),
                    });
                };
                if !matches!(
                    fund_ent.kind,
                    PublicEntityKind::Grant | PublicEntityKind::Contract
                ) {
                    return Err(ResearchLinkError::ItemFundingKindMismatch {
                        development_id: dev.development_id.clone(),
                        item_id: item.item_id.clone(),
                        entity_id: fund_id.clone(),
                        actual_kind: fund_ent.kind,
                    });
                }
            }

            let mut item_reasons: BTreeSet<ReassessmentReason> = BTreeSet::new();
            let mut caveats = Vec::new();

            if !matches!(item.decision, DecisionState::Reversed { .. })
                && let Some(src_reasons) = source_reassessment_by_version.get(&item.source_version)
            {
                for r in src_reasons {
                    item_reasons.insert(r.clone());
                }
            }

            let pub_in_cycle = item
                .publication_entity_id
                .as_ref()
                .is_some_and(|pub_id| circular_pub_set.contains(pub_id));

            let is_confirmed_item = matches!(item.decision, DecisionState::Confirmed { .. });

            let assessed_stage = if matches!(item.decision, DecisionState::Reversed { .. }) {
                reversed_item_count += 1;
                AssessedTechnicalStage::ReversedClaim
            } else if item_src_retracted {
                retracted_item_count += 1;
                for ret_ver in &item_src_summary.retracted_ancestor_versions {
                    caveats.push(
                        DemonstrationCaveat::RetractedSourceCannotEstablishMaturity {
                            source_version: ret_ver.clone(),
                        },
                    );
                }
                AssessedTechnicalStage::RetractedSourceClaim
            } else if !item_src_summary.counts_as_reported_public_source {
                simulated_item_count += 1;
                let qualifying_ancestors: Vec<(VersionId, EvidenceOriginClass)> = item_src_summary
                    .non_observational_ancestor_versions
                    .iter()
                    .filter_map(|ver| {
                        source_by_version
                            .get(ver)
                            .map(|s| (ver.clone(), s.origin_class))
                    })
                    .collect();
                caveats.push(
                    DemonstrationCaveat::SimulatedSourceCannotEstablishDemonstration {
                        qualifying_ancestors,
                    },
                );
                if item.claimed_stage.claims_empirical_demonstration() {
                    has_unsupported_demonstration_claim = true;
                    item_reasons.insert(ReassessmentReason::UnsupportedDemonstrationClaim {
                        item_id: item.item_id.clone(),
                    });
                }
                AssessedTechnicalStage::SimulatedEvidenceOnly
            } else {
                if is_confirmed_item {
                    for fund_id in &item.funding_or_contract_ids {
                        item_declared_funding_set.insert(fund_id.clone());
                    }
                    if let Some(pub_id) = &item.publication_entity_id {
                        item_publications.insert(pub_id.clone());
                    }
                }

                match item.artifact_kind {
                    TechnicalArtifactKind::GrantApplicationOrAward
                    | TechnicalArtifactKind::ContractSolicitationOrAward
                    | TechnicalArtifactKind::TheoreticalOrConceptPaper => {
                        proposal_or_funding_item_count += 1;
                        if item.claimed_stage.claims_empirical_demonstration() {
                            has_unsupported_demonstration_claim = true;
                            caveats.push(
                                DemonstrationCaveat::ProposalOrPatentCannotEstablishDemonstration {
                                    artifact_kind: item.artifact_kind,
                                    claimed_stage: item.claimed_stage,
                                },
                            );
                            item_reasons.insert(
                                ReassessmentReason::UnsupportedDemonstrationClaim {
                                    item_id: item.item_id.clone(),
                                },
                            );
                        }
                        AssessedTechnicalStage::ProposalOrFundingScopeOnly
                    }
                    TechnicalArtifactKind::PatentApplicationOrGrant => {
                        patent_item_count += 1;
                        if item.claimed_stage.claims_empirical_demonstration() {
                            has_unsupported_demonstration_claim = true;
                            caveats.push(
                                DemonstrationCaveat::ProposalOrPatentCannotEstablishDemonstration {
                                    artifact_kind: item.artifact_kind,
                                    claimed_stage: item.claimed_stage,
                                },
                            );
                            item_reasons.insert(
                                ReassessmentReason::UnsupportedDemonstrationClaim {
                                    item_id: item.item_id.clone(),
                                },
                            );
                        }
                        AssessedTechnicalStage::PatentSpecificationOnly
                    }
                    TechnicalArtifactKind::AnalyticalOrSimulationPaper => {
                        analytical_or_model_item_count += 1;
                        if item.claimed_stage.claims_empirical_demonstration() {
                            has_unsupported_demonstration_claim = true;
                            caveats.push(
                                DemonstrationCaveat::AnalyticalModelCannotEstablishDemonstration {
                                    claimed_stage: item.claimed_stage,
                                },
                            );
                            item_reasons.insert(
                                ReassessmentReason::UnsupportedDemonstrationClaim {
                                    item_id: item.item_id.clone(),
                                },
                            );
                        }
                        AssessedTechnicalStage::AnalyticalOrModelledOnly
                    }
                    TechnicalArtifactKind::LaboratoryTestReport
                    | TechnicalArtifactKind::FieldOrOperationalTestReport => {
                        match item.claimed_stage {
                            ClaimedTechnicalStage::ProposalOrConcept => {
                                proposal_or_funding_item_count += 1;
                                AssessedTechnicalStage::ProposalOrFundingScopeOnly
                            }
                            ClaimedTechnicalStage::PatentSpecification => {
                                patent_item_count += 1;
                                AssessedTechnicalStage::PatentSpecificationOnly
                            }
                            ClaimedTechnicalStage::AnalyticalOrSimulatedModel => {
                                analytical_or_model_item_count += 1;
                                AssessedTechnicalStage::AnalyticalOrModelledOnly
                            }
                            ClaimedTechnicalStage::LaboratoryDemonstration
                            | ClaimedTechnicalStage::FieldOrOperationalDemonstration => {
                                let scope = if item.artifact_kind
                                    == TechnicalArtifactKind::LaboratoryTestReport
                                    && item.claimed_stage
                                        == ClaimedTechnicalStage::FieldOrOperationalDemonstration
                                {
                                    caveats.push(
                                        DemonstrationCaveat::LaboratoryReportClaimedFieldDemonstration,
                                    );
                                    DemonstrationScope::Laboratory
                                } else if item.claimed_stage
                                    == ClaimedTechnicalStage::FieldOrOperationalDemonstration
                                {
                                    DemonstrationScope::FieldOrOperational
                                } else {
                                    DemonstrationScope::Laboratory
                                };

                                if pub_in_cycle && let Some(pub_id) = &item.publication_entity_id {
                                    caveats.push(DemonstrationCaveat::CircularCitationChain {
                                        publication_entity_id: pub_id.clone(),
                                    });
                                    item_reasons.insert(
                                        ReassessmentReason::CircularCitationDependency {
                                            publication_id: pub_id.clone(),
                                        },
                                    );
                                }

                                if is_confirmed_item {
                                    confirmed_demonstration_item_count += 1;
                                    if pub_in_cycle {
                                        circular_demonstration_item_count += 1;
                                    } else {
                                        highest_confirmed_non_circular_scope =
                                            Some(match highest_confirmed_non_circular_scope {
                                                Some(DemonstrationScope::FieldOrOperational) => {
                                                    DemonstrationScope::FieldOrOperational
                                                }
                                                _ => scope,
                                            });
                                        demo_source_versions.insert(item.source_version.clone());
                                        for root_ver in &item_src_summary.lineage_root_versions {
                                            demo_lineage_root_sources
                                                .entry(root_ver.clone())
                                                .or_default()
                                                .insert(item.source_version.clone());
                                        }
                                    }
                                    AssessedTechnicalStage::ConfirmedReportedDemonstration {
                                        scope,
                                        in_citation_cycle: pub_in_cycle,
                                    }
                                } else {
                                    unconfirmed_demonstration_item_count += 1;
                                    if pub_in_cycle {
                                        circular_demonstration_item_count += 1;
                                    }
                                    AssessedTechnicalStage::UnconfirmedReportedDemonstration {
                                        scope,
                                        in_citation_cycle: pub_in_cycle,
                                    }
                                }
                            }
                        }
                    }
                }
            };

            for r in &item_reasons {
                dev_reasons.insert(r.clone());
            }
            let item_reassessment_list: Vec<ReassessmentReason> =
                item_reasons.into_iter().collect();
            assessed_items.push(AssessedTechnicalItem {
                item_id: item.item_id.clone(),
                artifact_kind: item.artifact_kind,
                claimed_stage: item.claimed_stage,
                assessed_stage,
                publication_entity_id: item.publication_entity_id.clone(),
                funding_or_contract_ids: item.funding_or_contract_ids.clone(),
                source_version: item.source_version.clone(),
                verbatim_claim: item.verbatim_claim.clone(),
                stated_conditions_and_limitations: item.stated_conditions_and_limitations.clone(),
                caveats,
                needs_reassessment: !item_reassessment_list.is_empty(),
                reassessment_reasons: item_reassessment_list,
            });
        }

        let mut disputed_funding_mentions: BTreeSet<OpaqueId> = BTreeSet::new();
        for eval_link in &evaluated_links {
            check_stop_tick(&mut tick, &mut check_stop)?;
            if !eval_link.relation.is_funding_or_contract_link() {
                continue;
            }
            match &eval_link.effective_status {
                LinkEffectiveStatus::ConfirmedSubstantive {
                    from_entity_id,
                    to_entity_id,
                } => {
                    let touches_track = resolved_subject_entity_id
                        .as_ref()
                        .is_some_and(|s| s == from_entity_id || s == to_entity_id)
                        || item_publications.contains(from_entity_id)
                        || item_publications.contains(to_entity_id);
                    if touches_track {
                        if let Some(ent) = entity_by_id.get(from_entity_id)
                            && matches!(
                                ent.kind,
                                PublicEntityKind::Grant | PublicEntityKind::Contract
                            )
                        {
                            confirmed_funding_set.insert(from_entity_id.clone());
                        }
                        if let Some(ent) = entity_by_id.get(to_entity_id)
                            && matches!(
                                ent.kind,
                                PublicEntityKind::Grant | PublicEntityKind::Contract
                            )
                        {
                            confirmed_funding_set.insert(to_entity_id.clone());
                        }
                    }
                }
                LinkEffectiveStatus::DisputedEndpointIdentity {
                    disputed_mention_ids,
                } => {
                    let touches_from = endpoint_may_touch_development(
                        &eval_link.from_endpoint,
                        &dev.subject_endpoint,
                        resolved_subject_entity_id.as_ref(),
                        &item_publications,
                        &request.mentions,
                        &mut tick,
                        &mut check_stop,
                    )?;
                    let touches_to = if touches_from {
                        false
                    } else {
                        endpoint_may_touch_development(
                            &eval_link.to_endpoint,
                            &dev.subject_endpoint,
                            resolved_subject_entity_id.as_ref(),
                            &item_publications,
                            &request.mentions,
                            &mut tick,
                            &mut check_stop,
                        )?
                    };
                    if touches_from || touches_to {
                        for m_id in disputed_mention_ids {
                            disputed_funding_mentions.insert(m_id.clone());
                            dev_reasons.insert(ReassessmentReason::DisputedMentionIdentity {
                                mention_id: m_id.clone(),
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        let shared_demonstration_lineage_roots: Vec<VersionId> = demo_lineage_root_sources
            .into_iter()
            .filter_map(|(root, distinct_sources)| (distinct_sources.len() >= 2).then_some(root))
            .collect();

        let overall_maturity = if let Some(highest_scope) = highest_confirmed_non_circular_scope {
            OverallTechnicalMaturity::ReportedDemonstratedCapability { highest_scope }
        } else if unconfirmed_demonstration_item_count > 0 || circular_demonstration_item_count > 0
        {
            OverallTechnicalMaturity::CircularOrUnconfirmedDemonstrationOnly
        } else if analytical_or_model_item_count > 0 {
            OverallTechnicalMaturity::AnalyticalOrModelledOnly
        } else if proposal_or_funding_item_count > 0 || patent_item_count > 0 {
            OverallTechnicalMaturity::ProposalOrPatentOnly
        } else if simulated_item_count > 0 {
            OverallTechnicalMaturity::SimulatedOnly
        } else {
            OverallTechnicalMaturity::NoActiveEvidence
        };

        let dev_reassessment_list: Vec<ReassessmentReason> = dev_reasons.into_iter().collect();
        evaluated_developments.push(TechnicalDevelopmentAssessment {
            development_id: dev.development_id.clone(),
            topic: dev.topic.clone(),
            subject_endpoint: dev.subject_endpoint.clone(),
            resolved_subject_entity_id,
            subject_identity_disputed,
            overall_maturity,
            proposal_or_funding_item_count,
            patent_item_count,
            analytical_or_model_item_count,
            unconfirmed_demonstration_item_count,
            confirmed_demonstration_item_count,
            circular_demonstration_item_count,
            simulated_item_count,
            retracted_item_count,
            reversed_item_count,
            demonstration_source_versions: demo_source_versions.into_iter().collect(),
            shared_demonstration_lineage_roots,
            has_unsupported_demonstration_claim,
            confirmed_funding_or_contract_ids: confirmed_funding_set.into_iter().collect(),
            item_declared_funding_or_contract_ids: item_declared_funding_set.into_iter().collect(),
            disputed_funding_mention_ids: disputed_funding_mentions.into_iter().collect(),
            items: assessed_items,
            needs_reassessment: !dev_reassessment_list.is_empty(),
            reassessment_reasons: dev_reassessment_list,
        });
    }

    if let Some(reason) = check_stop() {
        return Err(ResearchLinkError::Stopped(reason));
    }

    Ok(ResearchLinkReport {
        method_id: METHOD_ID,
        identity_resolution_caveat: IDENTITY_RESOLUTION_CAVEAT,
        co_occurrence_caveat: CO_OCCURRENCE_CAVEAT,
        technical_maturity_caveat: TECHNICAL_MATURITY_CAVEAT,
        circular_citation_caveat: CIRCULAR_CITATION_CAVEAT,
        simulated_source_caveat: SIMULATED_SOURCE_CAVEAT,
        sources: source_summaries,
        mentions: evaluated_mentions,
        links: evaluated_links,
        confirmed_substantive_link_count,
        co_occurrence_link_count,
        disputed_endpoint_link_count,
        citation_cycles,
        publications_with_circular_dependency,
        developments: evaluated_developments,
        limitations,
    })
}

fn endpoint_may_touch_development(
    endpoint: &LinkEndpoint,
    subject_endpoint: &LinkEndpoint,
    subject_id: Option<&OpaqueId>,
    publications: &BTreeSet<OpaqueId>,
    mentions: &[IdentityMention],
    tick: &mut usize,
    check_stop: &mut impl FnMut() -> Option<StopReason>,
) -> Result<bool, ResearchLinkError> {
    check_stop_tick(tick, check_stop)?;
    if endpoint == subject_endpoint {
        return Ok(true);
    }
    let mut subject_candidates: BTreeSet<OpaqueId> = BTreeSet::new();
    match subject_endpoint {
        LinkEndpoint::Entity(id) => {
            subject_candidates.insert(id.clone());
        }
        LinkEndpoint::Mention(m_id) => {
            if let Some(m) = mentions.iter().find(|m| &m.mention_id == m_id) {
                for c in &m.candidates {
                    check_stop_tick(tick, check_stop)?;
                    if !matches!(c.decision, DecisionState::Reversed { .. }) {
                        subject_candidates.insert(c.target_entity_id.clone());
                    }
                }
            }
        }
    }
    match endpoint {
        LinkEndpoint::Entity(id) => Ok(subject_id.is_some_and(|s| s == id)
            || subject_candidates.contains(id)
            || publications.contains(id)),
        LinkEndpoint::Mention(m_id) => {
            let Some(m) = mentions.iter().find(|m| &m.mention_id == m_id) else {
                return Ok(false);
            };
            for c in &m.candidates {
                check_stop_tick(tick, check_stop)?;
                if !matches!(c.decision, DecisionState::Reversed { .. })
                    && (subject_id.is_some_and(|s| s == &c.target_entity_id)
                        || subject_candidates.contains(&c.target_entity_id)
                        || publications.contains(&c.target_entity_id))
                {
                    return Ok(true);
                }
            }
            Ok(false)
        }
    }
}

/// Traces directed citation cycles and circular dependencies over active `CitesPublication` edges.
#[allow(clippy::type_complexity)]
fn trace_citation_cycles(
    edges: &[(OpaqueId, OpaqueId, OpaqueId, bool, bool)],
    limits: ResearchLinkLimits,
    tick: &mut usize,
    check_stop: &mut impl FnMut() -> Option<StopReason>,
) -> Result<
    (
        Vec<CitationCycle>,
        Vec<OpaqueId>,
        Option<ResearchLinkLimitation>,
    ),
    ResearchLinkError,
> {
    if edges.is_empty() {
        return Ok((Vec::new(), Vec::new(), None));
    }

    let mut nodes_set: BTreeSet<OpaqueId> = BTreeSet::new();
    let mut adj: BTreeMap<OpaqueId, Vec<(OpaqueId, OpaqueId, bool, bool)>> = BTreeMap::new();
    for (from_id, to_id, link_id, is_unconfirmed, is_simulated) in edges {
        nodes_set.insert(from_id.clone());
        nodes_set.insert(to_id.clone());
        adj.entry(from_id.clone()).or_default().push((
            to_id.clone(),
            link_id.clone(),
            *is_unconfirmed,
            *is_simulated,
        ));
    }
    let nodes: Vec<OpaqueId> = nodes_set.into_iter().collect();

    // Iterative Tarjan strongly connected components over `nodes` (`O(V + E)`).
    let mut index_counter = 0usize;
    let mut indices: BTreeMap<OpaqueId, usize> = BTreeMap::new();
    let mut lowlink: BTreeMap<OpaqueId, usize> = BTreeMap::new();
    let mut on_stack: BTreeSet<OpaqueId> = BTreeSet::new();
    let mut tarjan_stack: Vec<OpaqueId> = Vec::new();
    let mut in_cycle_scc: BTreeSet<OpaqueId> = BTreeSet::new();

    for root in &nodes {
        check_stop_tick(tick, check_stop)?;
        if indices.contains_key(root) {
            continue;
        }
        let root_idx = index_counter;
        index_counter += 1;
        indices.insert(root.clone(), root_idx);
        lowlink.insert(root.clone(), root_idx);
        tarjan_stack.push(root.clone());
        on_stack.insert(root.clone());

        let mut call_stack: Vec<(OpaqueId, usize)> = vec![(root.clone(), 0)];
        while let Some((v, neighbor_idx)) = call_stack.pop() {
            check_stop_tick(tick, check_stop)?;
            if let Some((w, _, _, _)) = adj.get(&v).and_then(|nbrs| nbrs.get(neighbor_idx)).cloned()
            {
                call_stack.push((v.clone(), neighbor_idx + 1));
                if !indices.contains_key(&w) {
                    let w_idx = index_counter;
                    index_counter += 1;
                    indices.insert(w.clone(), w_idx);
                    lowlink.insert(w.clone(), w_idx);
                    tarjan_stack.push(w.clone());
                    on_stack.insert(w.clone());
                    call_stack.push((w, 0));
                } else if on_stack.contains(&w) {
                    let iw = indices.get(&w).copied().unwrap_or(0);
                    if let Some(lv) = lowlink.get_mut(&v)
                        && iw < *lv
                    {
                        *lv = iw;
                    }
                }
            } else {
                if let Some((parent, _)) = call_stack.last() {
                    let lv = lowlink.get(&v).copied().unwrap_or(0);
                    if let Some(lp) = lowlink.get_mut(parent)
                        && lv < *lp
                    {
                        *lp = lv;
                    }
                }
                if lowlink.get(&v) == indices.get(&v) {
                    let mut scc = Vec::new();
                    while let Some(w) = tarjan_stack.pop() {
                        on_stack.remove(&w);
                        let done = w == v;
                        scc.push(w);
                        if done {
                            break;
                        }
                    }
                    if scc.len() >= 2 {
                        for member in scc {
                            in_cycle_scc.insert(member);
                        }
                    }
                }
            }
        }
    }

    let mut rev_adj: BTreeMap<OpaqueId, Vec<OpaqueId>> = BTreeMap::new();
    for (from_id, to_id, _, _, _) in edges {
        if in_cycle_scc.contains(from_id) && in_cycle_scc.contains(to_id) {
            rev_adj
                .entry(to_id.clone())
                .or_default()
                .push(from_id.clone());
        }
    }

    let mut cycles: Vec<CitationCycle> = Vec::new();
    let mut seen_cycle_keys: BTreeSet<(Vec<OpaqueId>, Vec<OpaqueId>)> = BTreeSet::new();
    let mut dfs_steps = 0usize;
    let mut truncated = false;

    for start_node in &in_cycle_scc {
        if truncated {
            break;
        }
        // Reverse reachability over nodes `>= start_node` in `in_cycle_scc` so nodes that cannot
        // return to `start_node` without visiting `< start_node` are pruned immediately.
        let mut can_reach_start: BTreeSet<OpaqueId> = BTreeSet::new();
        can_reach_start.insert(start_node.clone());
        let mut rev_queue: Vec<OpaqueId> = vec![start_node.clone()];
        while let Some(curr) = rev_queue.pop() {
            check_stop_tick(tick, check_stop)?;
            if let Some(preds) = rev_adj.get(&curr) {
                for pred in preds {
                    if pred >= start_node && can_reach_start.insert(pred.clone()) {
                        rev_queue.push(pred.clone());
                    }
                }
            }
        }
        if can_reach_start.len() < 2 {
            continue;
        }

        let mut dfs_stack: Vec<(OpaqueId, Vec<OpaqueId>, Vec<OpaqueId>, bool, bool, usize)> =
            vec![(
                start_node.clone(),
                vec![start_node.clone()],
                Vec::new(),
                false,
                false,
                0,
            )];

        while let Some((curr, path_nodes, path_links, unconf, sim, edge_idx)) = dfs_stack.pop() {
            check_stop_tick(tick, check_stop)?;
            dfs_steps = dfs_steps.saturating_add(1);
            if dfs_steps > limits.max_cycle_dfs_steps {
                truncated = true;
                break;
            }

            let Some((next_node, link_id, edge_unconf, edge_sim)) =
                adj.get(&curr).and_then(|nbrs| nbrs.get(edge_idx))
            else {
                continue;
            };
            dfs_stack.push((
                curr.clone(),
                path_nodes.clone(),
                path_links.clone(),
                unconf,
                sim,
                edge_idx + 1,
            ));

            if next_node < start_node || !can_reach_start.contains(next_node) {
                continue;
            }

            let next_unconf = unconf || *edge_unconf;
            let next_sim = sim || *edge_sim;

            if next_node == start_node {
                let mut chain = path_nodes.clone();
                chain.push(start_node.clone());
                let mut links = path_links.clone();
                links.push(link_id.clone());

                if seen_cycle_keys.insert((chain.clone(), links.clone())) {
                    if cycles.len() >= limits.max_reported_cycles {
                        truncated = true;
                        break;
                    }
                    cycles.push(CitationCycle {
                        publication_chain: chain,
                        link_ids: links,
                        includes_unconfirmed_link: next_unconf,
                        includes_simulated_source: next_sim,
                    });
                }
            } else if !path_nodes.contains(next_node) {
                let mut next_path = path_nodes;
                next_path.push(next_node.clone());
                let mut next_links = path_links;
                next_links.push(link_id.clone());
                dfs_stack.push((
                    next_node.clone(),
                    next_path,
                    next_links,
                    next_unconf,
                    next_sim,
                    0,
                ));
            }
        }
    }

    let mut circular_pubs = in_cycle_scc.clone();
    for node in &nodes {
        check_stop_tick(tick, check_stop)?;
        if circular_pubs.contains(node) {
            continue;
        }
        let Some(neighbors) = adj.get(node) else {
            continue;
        };
        if neighbors.is_empty() {
            continue;
        }
        let mut visited: BTreeSet<OpaqueId> = BTreeSet::new();
        let mut queue: Vec<OpaqueId> = vec![node.clone()];
        let mut reaches_acyclic_root = false;
        while let Some(curr) = queue.pop() {
            check_stop_tick(tick, check_stop)?;
            if !visited.insert(curr.clone()) {
                continue;
            }
            if in_cycle_scc.contains(&curr) {
                continue;
            }
            match adj.get(&curr) {
                None => {
                    reaches_acyclic_root = true;
                    break;
                }
                Some(nbrs) if nbrs.is_empty() => {
                    reaches_acyclic_root = true;
                    break;
                }
                Some(nbrs) => {
                    for (nxt, _, _, _) in nbrs {
                        if !visited.contains(nxt) {
                            queue.push(nxt.clone());
                        }
                    }
                }
            }
        }
        if !reaches_acyclic_root {
            circular_pubs.insert(node.clone());
        }
    }

    let limitation = if truncated {
        Some(ResearchLinkLimitation::CitationCycleSearchTruncated {
            reported_cycles: cycles.len(),
            dfs_steps,
        })
    } else {
        None
    };

    Ok((cycles, circular_pubs.into_iter().collect(), limitation))
}
