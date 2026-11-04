//! Append-only evidence ledger with bounded, deterministic reassessment propagation.
//!
//! Every operation validates fully and plans its reassessment flags before mutating anything,
//! so a rejected operation leaves the ledger unchanged.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

use super::{
    AttributeName, AttributeValue, EVIDENCE_SCHEMA_VERSION, FieldValue, InheritedLabels,
    MAX_UTC_MS, MIN_UTC_MS, ObjectType, OriginClass, Provenance, RecordFields, RecordVersion,
    Relation, Revision, RevisionReason,
};
use crate::{OpaqueId, VersionId};

/// Resource limits for one ledger. Each limit is checked before any allocation that it bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LedgerLimits {
    pub max_versions: usize,
    pub max_relations: usize,
    /// Maximum stored reassessment flags across the ledger.
    pub max_flags: usize,
    /// Maximum derivation inputs per version.
    pub max_inputs: usize,
    pub max_attributes: usize,
    pub max_access_policies: usize,
    pub max_relation_evidence: usize,
    /// Maximum UTF-8 bytes in any retained text value.
    pub max_text_bytes: usize,
}

impl Default for LedgerLimits {
    fn default() -> Self {
        Self {
            max_versions: 65_536,
            max_relations: 262_144,
            max_flags: 1_048_576,
            max_inputs: 64,
            max_attributes: 64,
            max_access_policies: 16,
            max_relation_evidence: 64,
            max_text_bytes: 8_192,
        }
    }
}

/// Which [`LedgerLimits`] entry was exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitKind {
    Versions,
    Relations,
    Flags,
    Inputs,
    Attributes,
    AccessPolicies,
    RelationEvidence,
    TextBytes,
}

impl fmt::Display for LimitKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Versions => "max_versions",
            Self::Relations => "max_relations",
            Self::Flags => "max_flags",
            Self::Inputs => "max_inputs",
            Self::Attributes => "max_attributes",
            Self::AccessPolicies => "max_access_policies",
            Self::RelationEvidence => "max_relation_evidence",
            Self::TextBytes => "max_text_bytes",
        })
    }
}

/// Typed rejection reasons. A rejected operation never changes the ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceError {
    InvalidAttributeName(String),
    DuplicateAttribute(AttributeName),
    LimitExceeded {
        kind: LimitKind,
        limit: usize,
    },
    TimeOutOfRange(i64),
    EmptyText {
        field: &'static str,
    },
    DuplicateVersion(VersionId),
    /// An initial version was issued for an object that already exists.
    DuplicateObject(OpaqueId),
    DuplicateRelation(OpaqueId),
    DuplicateReference(VersionId),
    DuplicateAccessPolicy(OpaqueId),
    UnknownVersion(VersionId),
    UnknownObject(OpaqueId),
    /// The predecessor is no longer the head of its object.
    StaleRevision {
        id: OpaqueId,
        head: VersionId,
        predecessor: VersionId,
    },
    /// Revisions cannot change identity, object type, origin class or scope.
    ImmutableFieldChanged {
        field: &'static str,
    },
    /// The provenance kind is not permitted for this origin class.
    ProvenanceMismatch {
        origin: OriginClass,
    },
    /// A generated input would produce a source-tier version.
    OriginPromotion {
        input: VersionId,
        input_origin: OriginClass,
        origin: OriginClass,
    },
    /// A derivation relation would add simulation or illustration lineage that the existing
    /// dependent version does not carry. Issue a new version with that input instead.
    LabelNotInherited {
        upstream: VersionId,
    },
    /// A version cannot derive from a version of its own object.
    SelfDerivation(VersionId),
    ClaimFieldsMismatch {
        object_type: ObjectType,
    },
    SelfRelation(VersionId),
    DerivationCycle {
        dependent: VersionId,
        upstream: VersionId,
    },
}

impl fmt::Display for EvidenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAttributeName(name) => write!(
                f,
                "attribute name {name:?} must be 1-64 bytes of [a-z0-9_] starting with a letter"
            ),
            Self::DuplicateAttribute(name) => write!(f, "attribute {name} appears more than once"),
            Self::LimitExceeded { kind, limit } => write!(f, "{kind} limit of {limit} exceeded"),
            Self::TimeOutOfRange(ms) => {
                write!(f, "time {ms} ms is outside {MIN_UTC_MS}..={MAX_UTC_MS}")
            }
            Self::EmptyText { field } => write!(f, "{field} must not be empty"),
            Self::DuplicateVersion(v) => write!(f, "version {v} already exists"),
            Self::DuplicateObject(id) => write!(f, "object {id} already exists; revise it instead"),
            Self::DuplicateRelation(id) => write!(f, "relation {id} already exists"),
            Self::DuplicateReference(v) => write!(f, "version {v} is referenced more than once"),
            Self::DuplicateAccessPolicy(id) => {
                write!(f, "access policy {id} is listed more than once")
            }
            Self::UnknownVersion(v) => write!(f, "version {v} does not exist"),
            Self::UnknownObject(id) => write!(f, "object {id} does not exist"),
            Self::StaleRevision {
                id,
                head,
                predecessor,
            } => write!(
                f,
                "cannot revise {id} from {predecessor}: the current version is {head}"
            ),
            Self::ImmutableFieldChanged { field } => {
                write!(f, "a revision cannot change {field}")
            }
            Self::ProvenanceMismatch { origin } => {
                write!(
                    f,
                    "provenance kind is not permitted for origin class {origin}"
                )
            }
            Self::OriginPromotion {
                input,
                input_origin,
                origin,
            } => write!(
                f,
                "{input_origin} input {input} cannot produce a {origin} version"
            ),
            Self::LabelNotInherited { upstream } => write!(
                f,
                "upstream {upstream} carries simulation or illustration lineage that the dependent lacks"
            ),
            Self::SelfDerivation(v) => {
                write!(
                    f,
                    "version {v} belongs to the same object and cannot be an input"
                )
            }
            Self::ClaimFieldsMismatch { object_type } => write!(
                f,
                "claim fields are required for claim objects and rejected for {object_type}"
            ),
            Self::SelfRelation(v) => write!(f, "relation joins version {v} to itself"),
            Self::DerivationCycle {
                dependent,
                upstream,
            } => write!(
                f,
                "derivation edge {dependent} <- {upstream} would create a cycle"
            ),
        }
    }
}

impl std::error::Error for EvidenceError {}

/// A pending reassessment caused by the replacement of an upstream version.
///
/// Flags are stored beside versions; the flagged version itself is never modified. A re-run
/// produces a new version, which starts without flags unless its own inputs carry them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReassessmentFlag {
    /// The version that was replaced.
    pub replaced: VersionId,
    /// The version that replaced it.
    pub replacement: VersionId,
    pub reason: RevisionReason,
    /// The immediate upstream on the path from `replaced` to the flagged version.
    pub via: VersionId,
    /// Edges from `replaced` to the flagged version along the first path found (breadth-first).
    pub hops: u32,
}

/// Result of a revision: the versions newly flagged, in deterministic breadth-first order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReassessmentReport {
    pub replaced: VersionId,
    pub replacement: VersionId,
    pub reason: RevisionReason,
    pub flagged: Vec<VersionId>,
}

/// One accepted operation, in acceptance order. Replaying entries reproduces the ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LedgerEntry {
    Record(VersionId),
    Relation(OpaqueId),
}

/// Append-only, bounded, in-memory evidence ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceLedger {
    limits: LedgerLimits,
    versions: BTreeMap<VersionId, RecordVersion>,
    heads: BTreeMap<OpaqueId, VersionId>,
    successors: BTreeMap<VersionId, VersionId>,
    relations: BTreeMap<OpaqueId, Relation>,
    /// Upstream -> dependents along edges that propagate reassessment.
    propagating: BTreeMap<VersionId, BTreeSet<VersionId>>,
    /// Upstream -> dependents along derivation edges only (kept acyclic).
    derivation: BTreeMap<VersionId, BTreeSet<VersionId>>,
    flags: BTreeMap<VersionId, Vec<ReassessmentFlag>>,
    flag_count: usize,
    entries: Vec<LedgerEntry>,
}

type PlannedFlags = Vec<(VersionId, ReassessmentFlag)>;

impl EvidenceLedger {
    /// Creates an empty ledger with the given limits.
    #[must_use]
    pub fn new(limits: LedgerLimits) -> Self {
        Self {
            limits,
            versions: BTreeMap::new(),
            heads: BTreeMap::new(),
            successors: BTreeMap::new(),
            relations: BTreeMap::new(),
            propagating: BTreeMap::new(),
            derivation: BTreeMap::new(),
            flags: BTreeMap::new(),
            flag_count: 0,
            entries: Vec::new(),
        }
    }

    #[must_use]
    pub fn limits(&self) -> LedgerLimits {
        self.limits
    }

    /// Accepted operations in order.
    #[must_use]
    pub fn entries(&self) -> &[LedgerEntry] {
        &self.entries
    }

    #[must_use]
    pub fn version(&self, version: &VersionId) -> Option<&RecordVersion> {
        self.versions.get(version)
    }

    /// The current version of an object.
    #[must_use]
    pub fn head(&self, id: &OpaqueId) -> Option<&RecordVersion> {
        self.heads.get(id).and_then(|v| self.versions.get(v))
    }

    /// The version that replaced `version`, if it has been revised.
    #[must_use]
    pub fn successor(&self, version: &VersionId) -> Option<&VersionId> {
        self.successors.get(version)
    }

    #[must_use]
    pub fn relation(&self, id: &OpaqueId) -> Option<&Relation> {
        self.relations.get(id)
    }

    /// Pending reassessment flags on a version, oldest first.
    #[must_use]
    pub fn reassessment_flags(&self, version: &VersionId) -> &[ReassessmentFlag] {
        self.flags.get(version).map_or(&[], Vec::as_slice)
    }

    /// Whether any upstream replacement affects this version.
    #[must_use]
    pub fn needs_reassessment(&self, version: &VersionId) -> bool {
        !self.reassessment_flags(version).is_empty()
    }

    #[must_use]
    pub fn version_count(&self) -> usize {
        self.versions.len()
    }

    /// Issues the initial version of a new object.
    ///
    /// If an input has been replaced or is itself flagged, the new version inherits a flag, so
    /// deriving from outdated material stays visible.
    pub fn issue(&mut self, fields: RecordFields) -> Result<(), EvidenceError> {
        if self.heads.contains_key(&fields.id) {
            return Err(EvidenceError::DuplicateObject(fields.id));
        }
        self.check_new_version(&fields)?;
        let labels = self.validate_fields(&fields)?;
        let planned = self.inherited_for_new(&fields)?;
        self.commit_version(fields, None, None, labels, planned);
        Ok(())
    }

    /// Appends a revision of an object and flags downstream versions for reassessment.
    ///
    /// `predecessor` must be the object's current head, which guards against lost updates.
    pub fn revise(
        &mut self,
        fields: RecordFields,
        predecessor: &VersionId,
        revision: Revision,
    ) -> Result<ReassessmentReport, EvidenceError> {
        let head = self
            .heads
            .get(&fields.id)
            .ok_or_else(|| EvidenceError::UnknownObject(fields.id.clone()))?;
        if head != predecessor {
            return Err(EvidenceError::StaleRevision {
                id: fields.id.clone(),
                head: head.clone(),
                predecessor: predecessor.clone(),
            });
        }
        let previous = self
            .versions
            .get(predecessor)
            .ok_or_else(|| EvidenceError::UnknownVersion(predecessor.clone()))?;
        let prior = &previous.fields;
        if fields.object_type != prior.object_type {
            return Err(EvidenceError::ImmutableFieldChanged {
                field: "object_type",
            });
        }
        if fields.origin != prior.origin {
            return Err(EvidenceError::ImmutableFieldChanged { field: "origin" });
        }
        if fields.scope != prior.scope {
            return Err(EvidenceError::ImmutableFieldChanged { field: "scope" });
        }
        self.check_text("rationale", &revision.rationale, false)?;
        self.check_new_version(&fields)?;
        let labels = self.validate_fields(&fields)?;

        let mut planned = self.inherited_for_new(&fields)?;
        let seeds = self
            .propagating
            .get(predecessor)
            .into_iter()
            .flatten()
            .map(|dependent| {
                (
                    dependent.clone(),
                    ReassessmentFlag {
                        replaced: predecessor.clone(),
                        replacement: fields.version.clone(),
                        reason: revision.reason,
                        via: predecessor.clone(),
                        hops: 1,
                    },
                )
            })
            .collect();
        let downstream = self.plan_propagation(seeds, planned.len())?;
        let report = ReassessmentReport {
            replaced: predecessor.clone(),
            replacement: fields.version.clone(),
            reason: revision.reason,
            flagged: downstream.iter().map(|(v, _)| v.clone()).collect(),
        };
        planned.extend(downstream);
        let predecessor = predecessor.clone();
        self.successors
            .insert(predecessor.clone(), fields.version.clone());
        self.commit_version(fields, Some(predecessor), Some(revision), labels, planned);
        Ok(report)
    }

    /// Adds an immutable typed relation between two existing versions.
    pub fn relate(&mut self, relation: Relation) -> Result<(), EvidenceError> {
        if self.relations.contains_key(&relation.id) {
            return Err(EvidenceError::DuplicateRelation(relation.id));
        }
        if self.relations.len() >= self.limits.max_relations {
            return Err(self.limit(LimitKind::Relations));
        }
        check_time(relation.created_at_ms)?;
        let dependent = self.existing(&relation.dependent)?;
        let upstream = self.existing(&relation.upstream)?;
        if relation.dependent == relation.upstream {
            return Err(EvidenceError::SelfRelation(relation.dependent));
        }
        if relation.evidence.len() > self.limits.max_relation_evidence {
            return Err(self.limit(LimitKind::RelationEvidence));
        }
        self.check_references(&relation.evidence)?;
        if let FieldValue::Known(text) = &relation.rationale {
            self.check_text("rationale", text, true)?;
        }

        if relation.kind.is_derivation() {
            let (dep, up) = (&dependent.fields, &upstream.fields);
            if dep.id == up.id {
                return Err(EvidenceError::SelfDerivation(relation.upstream));
            }
            if matches!(
                dep.origin,
                OriginClass::AcquiredSourceMaterial | OriginClass::UserTestimony
            ) {
                return Err(EvidenceError::ProvenanceMismatch { origin: dep.origin });
            }
            if !dep.origin.admits_input(up.origin) {
                return Err(EvidenceError::OriginPromotion {
                    input: relation.upstream,
                    input_origin: up.origin,
                    origin: dep.origin,
                });
            }
            if !dependent.labels.covers(upstream.labels) {
                return Err(EvidenceError::LabelNotInherited {
                    upstream: relation.upstream,
                });
            }
            if reaches(&self.derivation, &relation.dependent, &relation.upstream) {
                return Err(EvidenceError::DerivationCycle {
                    dependent: relation.dependent,
                    upstream: relation.upstream,
                });
            }
        }

        let planned = if relation.kind.propagates_reassessment() {
            let seeds = self
                .inherited_from(&relation.upstream)
                .into_iter()
                .map(|flag| (relation.dependent.clone(), flag))
                .collect();
            self.plan_propagation(seeds, 0)?
        } else {
            Vec::new()
        };

        if relation.kind.propagates_reassessment() {
            add_edge(
                &mut self.propagating,
                &relation.upstream,
                &relation.dependent,
            );
        }
        if relation.kind.is_derivation() {
            add_edge(
                &mut self.derivation,
                &relation.upstream,
                &relation.dependent,
            );
        }
        self.apply_flags(planned);
        self.entries
            .push(LedgerEntry::Relation(relation.id.clone()));
        self.relations.insert(relation.id.clone(), relation);
        Ok(())
    }

    fn limit(&self, kind: LimitKind) -> EvidenceError {
        let limit = match kind {
            LimitKind::Versions => self.limits.max_versions,
            LimitKind::Relations => self.limits.max_relations,
            LimitKind::Flags => self.limits.max_flags,
            LimitKind::Inputs => self.limits.max_inputs,
            LimitKind::Attributes => self.limits.max_attributes,
            LimitKind::AccessPolicies => self.limits.max_access_policies,
            LimitKind::RelationEvidence => self.limits.max_relation_evidence,
            LimitKind::TextBytes => self.limits.max_text_bytes,
        };
        EvidenceError::LimitExceeded { kind, limit }
    }

    fn existing(&self, version: &VersionId) -> Result<&RecordVersion, EvidenceError> {
        self.versions
            .get(version)
            .ok_or_else(|| EvidenceError::UnknownVersion(version.clone()))
    }

    fn check_new_version(&self, fields: &RecordFields) -> Result<(), EvidenceError> {
        if self.versions.contains_key(&fields.version) {
            return Err(EvidenceError::DuplicateVersion(fields.version.clone()));
        }
        if self.versions.len() >= self.limits.max_versions {
            return Err(self.limit(LimitKind::Versions));
        }
        Ok(())
    }

    fn check_text(
        &self,
        field: &'static str,
        text: &str,
        allow_empty: bool,
    ) -> Result<(), EvidenceError> {
        if text.len() > self.limits.max_text_bytes {
            return Err(self.limit(LimitKind::TextBytes));
        }
        if !allow_empty && text.trim().is_empty() {
            return Err(EvidenceError::EmptyText { field });
        }
        Ok(())
    }

    fn check_references(&self, refs: &[VersionId]) -> Result<(), EvidenceError> {
        let mut seen = BTreeSet::new();
        for version in refs {
            self.existing(version)?;
            if !seen.insert(version) {
                return Err(EvidenceError::DuplicateReference(version.clone()));
            }
        }
        Ok(())
    }

    /// Validates content and returns the lineage labels the version will carry.
    fn validate_fields(&self, fields: &RecordFields) -> Result<InheritedLabels, EvidenceError> {
        check_time(fields.created_at_ms)?;

        if fields.access_policies.len() > self.limits.max_access_policies {
            return Err(self.limit(LimitKind::AccessPolicies));
        }
        let mut policies = BTreeSet::new();
        for policy in &fields.access_policies {
            if !policies.insert(policy) {
                return Err(EvidenceError::DuplicateAccessPolicy(policy.clone()));
            }
        }

        if fields.attributes.len() > self.limits.max_attributes {
            return Err(self.limit(LimitKind::Attributes));
        }
        for value in fields.attributes.values() {
            if let FieldValue::Known(AttributeValue::Text(text)) = value {
                self.check_text("attribute", text, true)?;
            }
        }

        match (&fields.claim, fields.object_type) {
            (Some(claim), ObjectType::Claim) => {
                self.check_text("proposition", &claim.proposition, false)?;
                if let FieldValue::Known(claimant) = &claim.claimant {
                    self.check_text("claimant", claimant, false)?;
                }
            }
            (None, ObjectType::Claim) | (Some(_), _) => {
                return Err(EvidenceError::ClaimFieldsMismatch {
                    object_type: fields.object_type,
                });
            }
            (None, _) => {}
        }

        let origin = fields.origin;
        let provenance_allowed = match &fields.provenance {
            Provenance::Acquired { .. } => origin == OriginClass::AcquiredSourceMaterial,
            Provenance::Manual => matches!(
                origin,
                OriginClass::UserTestimony
                    | OriginClass::AnalyticalInference
                    | OriginClass::Forecast
                    | OriginClass::Simulation
                    | OriginClass::IllustrativeRecreation
            ),
            Provenance::Derived { inputs, .. } => {
                !inputs.is_empty()
                    && !matches!(
                        origin,
                        OriginClass::AcquiredSourceMaterial | OriginClass::UserTestimony
                    )
            }
        };
        if !provenance_allowed {
            return Err(EvidenceError::ProvenanceMismatch { origin });
        }

        let inputs = fields.provenance.inputs();
        if inputs.len() > self.limits.max_inputs {
            return Err(self.limit(LimitKind::Inputs));
        }
        self.check_references(inputs)?;
        let mut labels = InheritedLabels::own(origin);
        for input in inputs {
            let upstream = self.existing(input)?;
            if upstream.fields.id == fields.id {
                return Err(EvidenceError::SelfDerivation(input.clone()));
            }
            if !origin.admits_input(upstream.fields.origin) {
                return Err(EvidenceError::OriginPromotion {
                    input: input.clone(),
                    input_origin: upstream.fields.origin,
                    origin,
                });
            }
            labels = labels.union(upstream.labels);
        }
        Ok(labels)
    }

    /// Flags a dependent of `upstream` inherits: one for each flag on `upstream`, plus one if
    /// `upstream` has already been replaced.
    fn inherited_from(&self, upstream: &VersionId) -> Vec<ReassessmentFlag> {
        let mut inherited: Vec<ReassessmentFlag> = self
            .reassessment_flags(upstream)
            .iter()
            .map(|flag| ReassessmentFlag {
                replaced: flag.replaced.clone(),
                replacement: flag.replacement.clone(),
                reason: flag.reason,
                via: upstream.clone(),
                hops: flag.hops.saturating_add(1),
            })
            .collect();
        if let Some(replacement) = self.successors.get(upstream)
            && let Some(reason) = self
                .versions
                .get(replacement)
                .and_then(|v| v.revision.as_ref())
                .map(|r| r.reason)
        {
            inherited.push(ReassessmentFlag {
                replaced: upstream.clone(),
                replacement: replacement.clone(),
                reason,
                via: upstream.clone(),
                hops: 1,
            });
        }
        inherited
    }

    /// Flags for a version that is not yet in the graph (it has no dependents to walk).
    fn inherited_for_new(&self, fields: &RecordFields) -> Result<PlannedFlags, EvidenceError> {
        let mut planned: PlannedFlags = Vec::new();
        for input in fields.provenance.inputs() {
            for flag in self.inherited_from(input) {
                let duplicate = planned.iter().any(|(_, existing)| {
                    existing.replaced == flag.replaced && existing.replacement == flag.replacement
                });
                if !duplicate {
                    planned.push((fields.version.clone(), flag));
                }
            }
        }
        self.check_flag_capacity(planned.len())?;
        Ok(planned)
    }

    fn check_flag_capacity(&self, additional: usize) -> Result<(), EvidenceError> {
        if self.flag_count.saturating_add(additional) > self.limits.max_flags {
            return Err(self.limit(LimitKind::Flags));
        }
        Ok(())
    }

    fn has_flag(&self, version: &VersionId, flag: &ReassessmentFlag) -> bool {
        self.reassessment_flags(version)
            .iter()
            .any(|f| f.replaced == flag.replaced && f.replacement == flag.replacement)
    }

    /// Breadth-first walk along propagating edges. Each (version, replaced, replacement) is
    /// flagged at most once, so the walk terminates on cycles and its work is bounded by the
    /// flag limit. Neighbours are visited in `VersionId` order for deterministic output.
    fn plan_propagation(
        &self,
        seeds: Vec<(VersionId, ReassessmentFlag)>,
        already_planned: usize,
    ) -> Result<PlannedFlags, EvidenceError> {
        let mut queue: VecDeque<(VersionId, ReassessmentFlag)> = seeds.into();
        let mut seen: BTreeSet<(VersionId, VersionId, VersionId)> = BTreeSet::new();
        let mut planned: PlannedFlags = Vec::new();
        while let Some((node, flag)) = queue.pop_front() {
            let key = (
                node.clone(),
                flag.replaced.clone(),
                flag.replacement.clone(),
            );
            if self.has_flag(&node, &flag) || !seen.insert(key) {
                continue;
            }
            self.check_flag_capacity(already_planned + planned.len() + 1)?;
            for dependent in self.propagating.get(&node).into_iter().flatten() {
                queue.push_back((
                    dependent.clone(),
                    ReassessmentFlag {
                        replaced: flag.replaced.clone(),
                        replacement: flag.replacement.clone(),
                        reason: flag.reason,
                        via: node.clone(),
                        hops: flag.hops.saturating_add(1),
                    },
                ));
            }
            planned.push((node, flag));
        }
        Ok(planned)
    }

    fn apply_flags(&mut self, planned: PlannedFlags) {
        for (version, flag) in planned {
            self.flags.entry(version).or_default().push(flag);
            self.flag_count += 1;
        }
    }

    fn commit_version(
        &mut self,
        fields: RecordFields,
        predecessor: Option<VersionId>,
        revision: Option<Revision>,
        labels: InheritedLabels,
        planned: PlannedFlags,
    ) {
        for input in fields.provenance.inputs() {
            add_edge(&mut self.propagating, input, &fields.version);
            add_edge(&mut self.derivation, input, &fields.version);
        }
        self.apply_flags(planned);
        self.heads.insert(fields.id.clone(), fields.version.clone());
        self.entries
            .push(LedgerEntry::Record(fields.version.clone()));
        self.versions.insert(
            fields.version.clone(),
            RecordVersion {
                schema_version: EVIDENCE_SCHEMA_VERSION,
                predecessor,
                revision,
                labels,
                fields,
            },
        );
    }
}

fn check_time(ms: i64) -> Result<(), EvidenceError> {
    if (MIN_UTC_MS..=MAX_UTC_MS).contains(&ms) {
        Ok(())
    } else {
        Err(EvidenceError::TimeOutOfRange(ms))
    }
}

fn add_edge(
    graph: &mut BTreeMap<VersionId, BTreeSet<VersionId>>,
    upstream: &VersionId,
    dependent: &VersionId,
) {
    graph
        .entry(upstream.clone())
        .or_default()
        .insert(dependent.clone());
}

/// Whether `target` is reachable from `start` along upstream -> dependent edges.
fn reaches(
    graph: &BTreeMap<VersionId, BTreeSet<VersionId>>,
    start: &VersionId,
    target: &VersionId,
) -> bool {
    let mut visited: BTreeSet<&VersionId> = BTreeSet::new();
    let mut queue: VecDeque<&VersionId> = VecDeque::from([start]);
    while let Some(node) = queue.pop_front() {
        if node == target {
            return true;
        }
        if visited.insert(node) {
            queue.extend(graph.get(node).into_iter().flatten());
        }
    }
    false
}
