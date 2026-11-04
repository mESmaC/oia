//! Canonical evidence records, origin classes, relations and claim states (`#10`).
//!
//! This module implements the domain side of `docs/architecture.md` §2:
//!
//! - **Common envelope (§2.1):** [`RecordFields`] carries the stable [`OpaqueId`], immutable
//!   [`VersionId`], [`ObjectType`], creation time and actor, case/library [`Scope`], access-policy
//!   references, [`Provenance`], review and availability states. The ledger adds the schema
//!   version, predecessor link and inherited lineage labels when a version is issued.
//! - **Evidence classes (§2.3):** [`OriginClass`] names the ten canonical classes. A derivation
//!   cannot promote generated material into observation-tier evidence, and simulation or
//!   illustration lineage is inherited and cannot be cleared ([`InheritedLabels`]).
//! - **Relations (§2.3):** [`Relation`] edges have their own review status and evidence. The
//!   derivation edges (`derived_from` and `calibrated_by`) must be acyclic. Lineage edges such as
//!   `quotes` and `republishes` may form citation cycles.
//! - **Claim and review states (§2.4):** [`ClaimAssessment`], [`ReviewStatus`] and
//!   [`Availability`] are separate vocabularies, so an unsupported claim is not recorded as
//!   false and a reviewed claim is not recorded as supported.
//! - **Unknown values:** [`FieldValue::Unknown`] carries an [`UnknownReason`] instead of a
//!   plausible default.
//!
//! [`EvidenceLedger`] is the append-only, bounded, in-memory owner of issued versions. A
//! revision, source correction, retraction or calibration change walks the dependency graph and
//! flags downstream versions as needing reassessment. Issued versions are never rewritten.
//! [`EvidenceLedger::to_json`] and [`EvidenceLedger::from_json`] provide the strictly validated
//! JSON form selected by ADR 0006.
//!
//! **Scope:** this is a domain kernel. Persistence (`#11`), application services and
//! authorization (`#14`), UI (`#16`), typed payloads for every principal object, and the formal
//! Rust–Lean bridge for `DependencyInvalidation` are not implemented here.

mod json;
mod ledger;

pub use json::{CodecError, LEDGER_FORMAT, MAX_LEDGER_JSON_BYTES};
pub use ledger::{
    EvidenceError, EvidenceLedger, LedgerEntry, LedgerLimits, LimitKind, ReassessmentFlag,
    ReassessmentReport,
};

use std::collections::BTreeMap;
use std::fmt;

use crate::witness::AccountOrigin;
use crate::{ActorId, OpaqueId, SchemaVersion, VersionId};

/// Earliest accepted creation time: `0001-01-01T00:00:00Z` in Unix milliseconds.
pub const MIN_UTC_MS: i64 = -62_135_596_800_000;
/// Latest accepted creation time: `9999-12-31T23:59:59.999Z` in Unix milliseconds.
pub const MAX_UTC_MS: i64 = 253_402_300_799_999;
/// Maximum byte length of an [`AttributeName`].
pub const MAX_ATTRIBUTE_NAME_LEN: usize = 64;
/// Schema version written for every version issued by this kernel.
pub const EVIDENCE_SCHEMA_VERSION: SchemaVersion = SchemaVersion::V1;

/// Defines a closed vocabulary with stable snake_case codes.
macro_rules! vocabulary {
    (
        $(#[$meta:meta])*
        $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident => $code:literal ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum $name {
            $( $(#[$vmeta])* $variant ),+
        }

        impl $name {
            /// Every value in declaration order.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// Stable code used in persisted and exported records.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $( Self::$variant => $code ),+
                }
            }

            /// Parses a stable code. Codes are exact and case-sensitive.
            #[must_use]
            pub fn parse(code: &str) -> Option<Self> {
                match code {
                    $( $code => Some(Self::$variant), )+
                    _ => None,
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

vocabulary! {
    /// Canonical evidence origin classes (`docs/architecture.md` §2.3).
    OriginClass {
        /// Bytes or records retained from an external source.
        AcquiredSourceMaterial => "acquired_source_material",
        /// An account entered by or on behalf of a person.
        UserTestimony => "user_testimony",
        /// A structured observation normalized from source material or testimony.
        NormalizedObservation => "normalized_observation",
        /// A proposition extracted from source material, with its source retained.
        ExtractedClaim => "extracted_claim",
        /// A conclusion produced by analysis, whether by a person or a method.
        AnalyticalInference => "analytical_inference",
        /// A forecast about an outcome that has not been observed.
        Forecast => "forecast",
        /// A deterministic, non-predictive transform such as a crop, transcode or extraction.
        ConventionalDerivative => "conventional_derivative",
        /// A model-predicted derivative, such as an upscaled, restored or interpolated output.
        PredictiveDerivative => "predictive_derivative",
        /// Generated or controlled simulation input or output.
        Simulation => "simulation",
        /// An illustration or recreation. It is never evidence of what occurred.
        IllustrativeRecreation => "illustrative_recreation",
    }
}

/// Whether an origin class is source-side evidence or generated material.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceTier {
    /// Source material, testimony, observations and claims normalized or extracted from them, and
    /// conventional derivatives.
    Source,
    /// Inferences, forecasts, predictive derivatives, simulations and recreations.
    Generated,
}

impl OriginClass {
    /// Returns the evidence tier used by the no-promotion rule.
    #[must_use]
    pub const fn tier(self) -> EvidenceTier {
        match self {
            Self::AcquiredSourceMaterial
            | Self::UserTestimony
            | Self::NormalizedObservation
            | Self::ExtractedClaim
            | Self::ConventionalDerivative => EvidenceTier::Source,
            Self::AnalyticalInference
            | Self::Forecast
            | Self::PredictiveDerivative
            | Self::Simulation
            | Self::IllustrativeRecreation => EvidenceTier::Generated,
        }
    }

    /// Whether a version of this class may be produced from `input` by derivation.
    ///
    /// Generated inputs can only feed generated outputs, so an inference fed into another step
    /// cannot become an observation, claim or conventional derivative. Simulation and
    /// illustration lineage is tracked separately by [`InheritedLabels`].
    #[must_use]
    pub const fn admits_input(self, input: Self) -> bool {
        !matches!(
            (input.tier(), self.tier()),
            (EvidenceTier::Generated, EvidenceTier::Source)
        )
    }
}

impl From<AccountOrigin> for OriginClass {
    /// Maps the transient witness-capture origin (`#27`) onto the canonical classes.
    fn from(origin: AccountOrigin) -> Self {
        match origin {
            AccountOrigin::Reported => Self::UserTestimony,
            AccountOrigin::Simulated => Self::Simulation,
        }
    }
}

vocabulary! {
    /// Principal and supporting object types (`docs/architecture.md` §2.2).
    ObjectType {
        Source => "source",
        SourceRecord => "source_record",
        Artifact => "artifact",
        Document => "document",
        Observation => "observation",
        Event => "event",
        Claim => "claim",
        Investigation => "investigation",
        AnalysisRun => "analysis_run",
        Forecast => "forecast",
        Device => "device",
        Report => "report",
        Review => "review",
        Annotation => "annotation",
    }
}

vocabulary! {
    /// Claim assessment, kept separate from review status (`docs/architecture.md` §2.4).
    ClaimAssessment {
        Supported => "supported",
        Contradicted => "contradicted",
        /// Not supported by the evidence available. This does not mean false.
        UnsupportedByAvailableEvidence => "unsupported_by_available_evidence",
        MateriallyMisleading => "materially_misleading",
        Disputed => "disputed",
        Unresolved => "unresolved",
    }
}

vocabulary! {
    /// Review status, kept separate from assessment (`docs/architecture.md` §2.4).
    ReviewStatus {
        Unreviewed => "unreviewed",
        InReview => "in_review",
        Reviewed => "reviewed",
        Challenged => "challenged",
        Superseded => "superseded",
    }
}

vocabulary! {
    /// Availability of retained or referenced material (`docs/architecture.md` §2.4).
    Availability {
        Available => "available",
        ReferenceOnly => "reference_only",
        Missing => "missing",
        Restricted => "restricted",
        /// Removed by the publisher. This is not a retraction.
        Removed => "removed",
        Expired => "expired",
        Corrupt => "corrupt",
    }
}

vocabulary! {
    /// Typed relation meanings (`docs/architecture.md` §2.3).
    ///
    /// Each relation joins a `dependent` version to an `upstream` version. For `supports` and
    /// `contradicts`, the dependent is the claim or hypothesis being assessed and the upstream
    /// is the evidence. For `possible_same_event`, the direction carries no meaning.
    RelationKind {
        DerivedFrom => "derived_from",
        Quotes => "quotes",
        Republishes => "republishes",
        Supports => "supports",
        Contradicts => "contradicts",
        /// A proposed grouping, not an established identity.
        PossibleSameEvent => "possible_same_event",
        /// Cross-object replacement. Revisions of one object use predecessor links instead.
        Supersedes => "supersedes",
        CalibratedBy => "calibrated_by",
    }
}

impl RelationKind {
    /// Whether this is a derivation dependency. Derivation edges must be acyclic and must
    /// satisfy the same origin and lineage rules as derivation inputs.
    #[must_use]
    pub const fn is_derivation(self) -> bool {
        matches!(self, Self::DerivedFrom | Self::CalibratedBy)
    }

    /// Whether replacing the upstream marks the dependent for reassessment.
    ///
    /// Derivations and evidential edges propagate. Lineage and grouping edges (`quotes`,
    /// `republishes`, `possible_same_event`, `supersedes`) only record a relationship.
    #[must_use]
    pub const fn propagates_reassessment(self) -> bool {
        matches!(
            self,
            Self::DerivedFrom | Self::CalibratedBy | Self::Supports | Self::Contradicts
        )
    }
}

vocabulary! {
    /// Why a value is absent. Absence is never encoded as zero, an empty string or a default.
    UnknownReason {
        /// The source did not record or provide the value.
        NotRecordedBySource => "not_recorded_by_source",
        /// A person giving an account did not report the value.
        NotReported => "not_reported",
        /// Processing that would produce the value has not run.
        NotYetProcessed => "not_yet_processed",
        /// Processing that would produce the value failed.
        ProcessingFailed => "processing_failed",
        /// The value exists but is withheld or access-restricted.
        Restricted => "restricted",
        /// The field does not apply to this record.
        NotApplicable => "not_applicable",
    }
}

vocabulary! {
    /// Why a new version replaced its predecessor.
    RevisionReason {
        /// An ordinary edit or re-run.
        Revision => "revision",
        /// The source published a correction.
        SourceCorrection => "source_correction",
        /// The source or author retracted the content.
        Retraction => "retraction",
        /// A calibration on which the record depends changed.
        CalibrationChange => "calibration_change",
    }
}

/// A value that is either known or explicitly unknown with a reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue<T> {
    Known(T),
    Unknown(UnknownReason),
}

impl<T> FieldValue<T> {
    /// Returns the known value, if any.
    pub fn known(&self) -> Option<&T> {
        match self {
            Self::Known(value) => Some(value),
            Self::Unknown(_) => None,
        }
    }
}

/// A typed attribute value. Text is retained verbatim, including line endings.
///
/// Physical quantities with units are not represented here; they wait for the shared quantity
/// model in `#12` rather than adding a second unit vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttributeValue {
    Text(String),
    Integer(i64),
    Boolean(bool),
}

/// Validated attribute name: 1–64 bytes of `[a-z0-9_]`, starting with a lowercase letter.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AttributeName(String);

impl AttributeName {
    /// Validates and constructs an attribute name.
    pub fn new(raw: impl Into<String>) -> Result<Self, EvidenceError> {
        let name = raw.into();
        let valid_start = name.bytes().next().is_some_and(|b| b.is_ascii_lowercase());
        let valid_rest = name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
        if name.len() > MAX_ATTRIBUTE_NAME_LEN || !valid_start || !valid_rest {
            return Err(EvidenceError::InvalidAttributeName(name));
        }
        Ok(Self(name))
    }

    /// Returns the attribute name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AttributeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Owning case or shared library visibility.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    Case(OpaqueId),
    Library,
}

/// Declared provenance of a version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provenance {
    /// Retained from a registered source. Only valid for [`OriginClass::AcquiredSourceMaterial`].
    Acquired { source: OpaqueId },
    /// Entered by the creating actor without upstream versions.
    Manual,
    /// Produced by a method from exact upstream versions.
    Derived {
        method: OpaqueId,
        method_version: VersionId,
        inputs: Vec<VersionId>,
    },
}

impl Provenance {
    /// Exact upstream versions for derived provenance; empty otherwise.
    #[must_use]
    pub fn inputs(&self) -> &[VersionId] {
        match self {
            Self::Derived { inputs, .. } => inputs,
            Self::Acquired { .. } | Self::Manual => &[],
        }
    }
}

/// Claim content. A claim about what someone said is not an assessment of the quoted assertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimFields {
    /// The exact asserted proposition, verbatim.
    pub proposition: String,
    /// Who made the assertion, if known.
    pub claimant: FieldValue<String>,
    pub assessment: ClaimAssessment,
}

/// Caller-supplied content of a version. Issued versions are wrapped in [`RecordVersion`] and
/// cannot be modified afterwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordFields {
    pub id: OpaqueId,
    pub version: VersionId,
    pub object_type: ObjectType,
    pub origin: OriginClass,
    /// OIA creation time in Unix milliseconds (UTC). This is not a source, event or capture time.
    pub created_at_ms: i64,
    pub created_by: ActorId,
    pub scope: Scope,
    pub access_policies: Vec<OpaqueId>,
    pub provenance: Provenance,
    pub review: ReviewStatus,
    pub availability: Availability,
    pub attributes: BTreeMap<AttributeName, FieldValue<AttributeValue>>,
    /// Required for [`ObjectType::Claim`] and rejected for every other type.
    pub claim: Option<ClaimFields>,
}

/// Reason and rationale for a revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revision {
    pub reason: RevisionReason,
    /// Non-empty explanation, retained verbatim.
    pub rationale: String,
}

/// Lineage labels inherited through derivation inputs and derivation relations.
///
/// They are set by the record's own class or any derivation input, and a derived version
/// cannot clear them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InheritedLabels {
    /// The version is, or derives from, a simulation.
    pub simulated: bool,
    /// The version is, or derives from, an illustrative recreation.
    pub illustrative: bool,
}

impl InheritedLabels {
    pub(crate) const fn own(origin: OriginClass) -> Self {
        Self {
            simulated: matches!(origin, OriginClass::Simulation),
            illustrative: matches!(origin, OriginClass::IllustrativeRecreation),
        }
    }

    pub(crate) const fn union(self, other: Self) -> Self {
        Self {
            simulated: self.simulated || other.simulated,
            illustrative: self.illustrative || other.illustrative,
        }
    }

    /// Whether `self` carries every label set in `other`.
    #[must_use]
    pub const fn covers(self, other: Self) -> bool {
        (self.simulated || !other.simulated) && (self.illustrative || !other.illustrative)
    }
}

/// An issued, immutable version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordVersion {
    pub(crate) schema_version: SchemaVersion,
    pub(crate) predecessor: Option<VersionId>,
    pub(crate) revision: Option<Revision>,
    pub(crate) labels: InheritedLabels,
    pub(crate) fields: RecordFields,
}

impl RecordVersion {
    #[must_use]
    pub fn schema_version(&self) -> SchemaVersion {
        self.schema_version
    }

    /// The version this one replaced. `None` for the initial version.
    #[must_use]
    pub fn predecessor(&self) -> Option<&VersionId> {
        self.predecessor.as_ref()
    }

    /// Revision reason and rationale. `None` for the initial version.
    #[must_use]
    pub fn revision(&self) -> Option<&Revision> {
        self.revision.as_ref()
    }

    #[must_use]
    pub fn labels(&self) -> InheritedLabels {
        self.labels
    }

    #[must_use]
    pub fn fields(&self) -> &RecordFields {
        &self.fields
    }

    #[must_use]
    pub fn id(&self) -> &OpaqueId {
        &self.fields.id
    }

    #[must_use]
    pub fn version(&self) -> &VersionId {
        &self.fields.version
    }
}

/// An immutable typed relation with its own review status and evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    pub id: OpaqueId,
    pub kind: RelationKind,
    pub dependent: VersionId,
    pub upstream: VersionId,
    pub created_at_ms: i64,
    pub created_by: ActorId,
    pub review: ReviewStatus,
    /// Versions cited as evidence for the relation itself.
    pub evidence: Vec<VersionId>,
    pub rationale: FieldValue<String>,
}
