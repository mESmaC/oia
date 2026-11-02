//! Versioned evidence graph, lineage cycle detection, independent-source tracing,
//! and dependency reassessment propagation (`docs/architecture.md` Section 2.3).
//!
//! - Lineage (`Quotes`, `Republishes`) may contain citation cycles, which are detected
//!   and exposed (`find_citation_cycles`).
//! - Analysis derivation (`DerivedFrom`) must remain strictly acyclic (DAG).
//! - Multiple articles repeating a single upstream source trace back to the same origin root
//!   and do not count as independent corroboration (`independent_source_roots`).
//! - Source correction, replacement, retraction, or calibration change triggers a dependency
//!   walk (`propagate_reassessment`) that marks downstream objects `NeedsReassessment`
//!   without overwriting or deleting issued versions.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::ids::{OpaqueId, VersionId};
use crate::records::{DomainError, DomainObject, ReassessmentState, ReviewStatus};

/// Semantic type of a directed edge in the [`EvidenceGraph`].
///
/// Direction convention: `from_version` depends on, cites, or supersedes `to_version`.
/// For example, if analysis `A` is derived from observation `O`, then
/// `from_version = A` and `to_version = O` with `RelationKind::DerivedFrom`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RelationKind {
    /// `from_version` was computed or extracted from `to_version` (must be acyclic).
    DerivedFrom,
    /// `from_version` quotes a passage from `to_version` (lineage may contain cycles).
    Quotes,
    /// `from_version` republishes or syndicates `to_version`.
    Republishes,
    /// `from_version` provides evidentiary support for `to_version`.
    Supports,
    /// `from_version` contradicts `to_version`.
    Contradicts,
    /// `from_version` and `to_version` may describe the same underlying event.
    PossibleSameEvent,
    /// `from_version` is a newer revision superseding `to_version`.
    Supersedes,
    /// `from_version` relies on camera/sensor calibration `to_version`.
    CalibratedBy,
    /// `from_version` co-occurs in text with `to_version` (never implies affiliation or causation).
    CoOccursWith,
}

impl RelationKind {
    /// Returns `true` if changes to `to_version` invalidate `from_version` for reassessment.
    #[must_use]
    pub const fn propagates_reassessment_to_from(self) -> bool {
        matches!(
            self,
            Self::DerivedFrom | Self::Quotes | Self::Republishes | Self::CalibratedBy
        )
    }

    /// Returns `true` if `from_version` supports `to_version`, meaning a change to `from_version`
    /// invalidates `to_version` for reassessment.
    #[must_use]
    pub const fn propagates_reassessment_to_target(self) -> bool {
        matches!(self, Self::Supports)
    }
}

/// A versioned relationship edge with its own provenance and review status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationEdge {
    /// Unique edge identifier.
    pub edge_id: OpaqueId,
    /// Source node version ID.
    pub from_version: VersionId,
    /// Target node version ID.
    pub to_version: VersionId,
    /// Semantic relation kind.
    pub kind: RelationKind,
    /// Rationale or evidence supporting this link.
    pub rationale: String,
    /// Review status of this relationship edge.
    pub review_status: ReviewStatus,
}

/// Versioned evidence and dependency graph.
#[derive(Debug, Clone, Default)]
pub struct EvidenceGraph {
    objects: BTreeMap<VersionId, DomainObject>,
    latest_by_id: BTreeMap<OpaqueId, VersionId>,
    edges: Vec<RelationEdge>,
}

impl EvidenceGraph {
    /// Creates an empty [`EvidenceGraph`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a validated [`DomainObject`] version without overwriting any prior version (`B5`, `B6`, `B7`, `B9`).
    ///
    /// - Rejects duplicate `version_id` with [`DomainError::DuplicateVersion`].
    /// - Validates all `supersedes` invariants and edge IDs before mutating any state (`B6`).
    /// - Preserves prior `Challenged` review status in `processing_history` when superseding (`B6`).
    /// - Verifies that any referenced `source_version_ids` already in the graph do not launder
    ///   non-observational ancestors into observational target classes (`B9`).
    pub fn insert_object(
        &mut self,
        object: DomainObject,
        reason_if_superseding: Option<&str>,
    ) -> Result<VersionId, DomainError> {
        object.validate()?;
        let env = object.envelope().clone();
        let version_id = env.version_id.clone();

        if self.objects.contains_key(&version_id) {
            return Err(DomainError::DuplicateVersion(version_id));
        }

        // Check origin-class inheritance against existing upstream provenance versions (`B9`).
        let mut upstream_classes = Vec::new();
        for src_ver in &env.provenance.source_version_ids {
            if let Some(src_obj) = self.objects.get(src_ver) {
                upstream_classes.push(src_obj.envelope().origin_class);
                if self.has_non_observational_ancestor(src_ver) {
                    upstream_classes.push(crate::records::OriginClass::AnalyticalInference);
                }
            }
        }
        crate::records::OriginClass::validate_derivation(&upstream_classes, env.origin_class)?;

        let pending_supersede = if let Some(prev_version) = &env.supersedes {
            let prev_obj = self
                .objects
                .get(prev_version)
                .ok_or_else(|| DomainError::SupersedesMissingVersion(prev_version.clone()))?;
            if prev_obj.envelope().id != env.id {
                return Err(DomainError::SupersedesMismatchedObjectId {
                    expected_id: env.id.clone(),
                    actual_id: prev_obj.envelope().id.clone(),
                });
            }
            let edge_id = OpaqueId::new(format!(
                "supersedes:{}:{}",
                version_id.as_str(),
                prev_version.as_str()
            ))?;
            Some((prev_version.clone(), edge_id))
        } else {
            None
        };

        // All validations passed; now apply mutations atomically (`B6`).
        if let Some((prev_version, edge_id)) = pending_supersede {
            if let Some(prev_obj) = self.objects.get_mut(&prev_version) {
                let prev_status = prev_obj.envelope().review_status;
                if prev_status == ReviewStatus::Challenged {
                    prev_obj
                        .envelope_mut()
                        .provenance
                        .processing_history
                        .push("preserved_prior_review_status:challenged".to_owned());
                }
                prev_obj.envelope_mut().review_status = ReviewStatus::Superseded;
            }
            self.edges.push(RelationEdge {
                edge_id,
                from_version: version_id.clone(),
                to_version: prev_version.clone(),
                kind: RelationKind::Supersedes,
                rationale: reason_if_superseding
                    .unwrap_or("version superseded")
                    .to_owned(),
                review_status: ReviewStatus::Reviewed,
            });
            let reason = reason_if_superseding.unwrap_or("upstream dependency superseded");
            self.propagate_reassessment(&prev_version, reason, env.created_at_ms);
        }

        self.objects.insert(version_id.clone(), object);
        self.recompute_latest_for_id(&env.id);
        Ok(version_id)
    }

    fn recompute_latest_for_id(&mut self, id: &OpaqueId) {
        let mut superseded_versions = BTreeSet::new();
        let mut candidates = Vec::new();
        for (ver, obj) in &self.objects {
            if &obj.envelope().id == id {
                if let Some(prev) = &obj.envelope().supersedes {
                    superseded_versions.insert(prev.clone());
                }
                candidates.push((ver.clone(), obj.envelope().created_at_ms));
            }
        }
        let mut best: Option<(VersionId, i64)> = None;
        for (ver, ts) in &candidates {
            if !superseded_versions.contains(ver) {
                match &best {
                    None => best = Some((ver.clone(), *ts)),
                    Some((b_ver, b_ts)) if ts > b_ts || (ts == b_ts && ver > b_ver) => {
                        best = Some((ver.clone(), *ts));
                    }
                    _ => {}
                }
            }
        }
        if let Some((latest_ver, _)) = best {
            self.latest_by_id.insert(id.clone(), latest_ver);
        }
    }

    /// Returns `true` if `version_id` or any of its transitive `DerivedFrom` / `source_version_ids`
    /// ancestors has a non-observational [`crate::records::OriginClass`] (`B9`).
    #[must_use]
    pub fn has_non_observational_ancestor(&self, start: &VersionId) -> bool {
        let mut visited = BTreeSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(start.clone());
        while let Some(curr) = queue.pop_front() {
            if !visited.insert(curr.clone()) {
                continue;
            }
            if let Some(obj) = self.objects.get(&curr) {
                if obj.envelope().origin_class.is_non_observational() {
                    return true;
                }
                for parent in &obj.envelope().provenance.source_version_ids {
                    queue.push_back(parent.clone());
                }
            }
            for edge in &self.edges {
                if edge.from_version == curr && edge.kind == RelationKind::DerivedFrom {
                    queue.push_back(edge.to_version.clone());
                }
            }
        }
        false
    }

    /// Adds a [`RelationEdge`], enforcing that `RelationKind::DerivedFrom` remains a DAG and
    /// does not launder a non-observational input into an observational target (`B9`).
    pub fn add_edge(&mut self, edge: RelationEdge) -> Result<(), DomainError> {
        if edge.kind == RelationKind::DerivedFrom {
            if edge.from_version == edge.to_version
                || self.has_derivation_path(&edge.to_version, &edge.from_version)
            {
                return Err(DomainError::CyclicDerivation {
                    from: edge.from_version,
                    to: edge.to_version,
                });
            }
            if let Some(from_obj) = self.objects.get(&edge.from_version) {
                let target_class = from_obj.envelope().origin_class;
                if self.has_non_observational_ancestor(&edge.to_version) {
                    crate::records::OriginClass::validate_derivation(
                        &[crate::records::OriginClass::AnalyticalInference],
                        target_class,
                    )?;
                }
            }
        }
        self.edges.push(edge);
        Ok(())
    }

    /// Restores a previously persisted [`DomainObject`] into the graph without re-emitting
    /// duplicate `Supersedes` edges or re-mutating processing history (`A1`).
    pub fn restore_object_unchecked(&mut self, object: DomainObject) -> Result<(), DomainError> {
        object.validate()?;
        let env = object.envelope().clone();
        self.objects.insert(env.version_id, object);
        self.recompute_latest_for_id(&env.id);
        Ok(())
    }

    /// Restores a previously persisted [`RelationEdge`] into the graph (`A1`).
    pub fn restore_edge_unchecked(&mut self, edge: RelationEdge) {
        self.edges.push(edge);
    }

    /// Sets the [`crate::records::AvailabilityState`] of a stored version (used by `oia-store`
    /// when tombstoning or expiring an artifact's bytes, `A3`).
    pub fn set_version_availability(
        &mut self,
        version_id: &VersionId,
        availability: crate::records::AvailabilityState,
    ) -> Result<(), DomainError> {
        let obj = self
            .objects
            .get_mut(version_id)
            .ok_or_else(|| DomainError::UnknownInputVersion(version_id.clone()))?;
        obj.envelope_mut().availability = availability;
        Ok(())
    }

    /// Returns a stored [`DomainObject`] by its exact immutable [`VersionId`].
    #[must_use]
    pub fn get_version(&self, version_id: &VersionId) -> Option<&DomainObject> {
        self.objects.get(version_id)
    }

    /// Returns the latest [`DomainObject`] version for a semantic [`OpaqueId`].
    #[must_use]
    pub fn get_latest(&self, id: &OpaqueId) -> Option<&DomainObject> {
        let ver = self.latest_by_id.get(id)?;
        self.objects.get(ver)
    }

    /// Returns all stored versions across the graph in deterministic order.
    pub fn all_versions(&self) -> impl Iterator<Item = (&VersionId, &DomainObject)> {
        self.objects.iter()
    }

    /// Returns all relationship edges.
    #[must_use]
    pub fn edges(&self) -> &[RelationEdge] {
        &self.edges
    }

    /// Checks whether a directed `DerivedFrom` path exists from `start` to `goal`.
    #[must_use]
    pub fn has_derivation_path(&self, start: &VersionId, goal: &VersionId) -> bool {
        let mut visited = BTreeSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(start.clone());
        while let Some(curr) = queue.pop_front() {
            if &curr == goal {
                return true;
            }
            if !visited.insert(curr.clone()) {
                continue;
            }
            for edge in &self.edges {
                if edge.kind == RelationKind::DerivedFrom && edge.from_version == curr {
                    queue.push_back(edge.to_version.clone());
                }
            }
        }
        false
    }

    /// Marks all direct and transitive downstream dependents of `changed_version` as
    /// [`ReassessmentState::NeedsReassessment`] without mutating their content or deleting them.
    ///
    /// Returns the sorted list of downstream [`VersionId`]s that were flagged.
    pub fn propagate_reassessment(
        &mut self,
        changed_version: &VersionId,
        reason: &str,
        flagged_at_ms: i64,
    ) -> Vec<VersionId> {
        let mut flagged = BTreeSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(changed_version.clone());
        let mut visited = BTreeSet::new();
        visited.insert(changed_version.clone());

        while let Some(curr) = queue.pop_front() {
            for edge in &self.edges {
                let dependent = if edge.to_version == curr
                    && edge.kind.propagates_reassessment_to_from()
                {
                    Some(edge.from_version.clone())
                } else if edge.from_version == curr && edge.kind.propagates_reassessment_to_target()
                {
                    Some(edge.to_version.clone())
                } else {
                    None
                };
                if let Some(dep) = dependent {
                    flagged.insert(dep.clone());
                    if visited.insert(dep.clone()) {
                        queue.push_back(dep);
                    }
                }
            }
            // Also check envelope provenance links (`source_version_ids`).
            for (ver, obj) in &self.objects {
                if obj.envelope().provenance.source_version_ids.contains(&curr) {
                    flagged.insert(ver.clone());
                    if visited.insert(ver.clone()) {
                        queue.push_back(ver.clone());
                    }
                }
            }
        }

        for dep_ver in &flagged {
            if let Some(obj) = self.objects.get_mut(dep_ver) {
                let env = obj.envelope_mut();
                match &mut env.reassessment {
                    ReassessmentState::Current => {
                        env.reassessment = ReassessmentState::NeedsReassessment {
                            upstream_versions: vec![changed_version.clone()],
                            reasons: vec![reason.to_owned()],
                            flagged_at_ms,
                        };
                    }
                    ReassessmentState::NeedsReassessment {
                        upstream_versions,
                        reasons,
                        flagged_at_ms: ts,
                    } => {
                        if !upstream_versions.contains(changed_version) {
                            upstream_versions.push(changed_version.clone());
                        }
                        if !reasons.iter().any(|r| r == reason) {
                            reasons.push(reason.to_owned());
                        }
                        *ts = (*ts).max(flagged_at_ms);
                    }
                }
            }
        }

        flagged.into_iter().collect()
    }

    /// Traces upstream `Quotes` and `Republishes` lineage from `version_ids` to find independent
    /// origin roots (`INTEL-01`, `NEWS-01`, `B8`).
    ///
    /// Computes the reachable `Quotes` / `Republishes` subgraph and condenses strongly connected
    /// components (SCCs) so that:
    /// - Diamond DAGs (`X` quotes `A` and `B`, and both republish `W`) resolve to the single root `{W}`.
    /// - Terminal citation cycles (`A <-> B` with no external parent) collapse to a single canonical
    ///   representative (`min(SCC)`) rather than inflating corroboration counts.
    #[must_use]
    pub fn independent_source_roots(&self, version_ids: &[VersionId]) -> BTreeSet<VersionId> {
        // 1. Collect all nodes reachable from `version_ids` via `Quotes` / `Republishes`.
        let mut reachable = BTreeSet::new();
        let mut queue: VecDeque<VersionId> = version_ids.iter().cloned().collect();
        while let Some(curr) = queue.pop_front() {
            if !reachable.insert(curr.clone()) {
                continue;
            }
            for edge in &self.edges {
                if edge.from_version == curr
                    && matches!(edge.kind, RelationKind::Quotes | RelationKind::Republishes)
                {
                    queue.push_back(edge.to_version.clone());
                }
            }
        }

        if reachable.is_empty() {
            return BTreeSet::new();
        }

        // 2. Partition `reachable` into strongly connected components (mutual reachability in the
        //    Quotes/Republishes subgraph) and identify terminal SCCs (zero outgoing edges to other SCCs).
        let can_reach = |from: &VersionId, target: &VersionId| -> bool {
            let mut seen = BTreeSet::new();
            let mut q = VecDeque::new();
            q.push_back(from.clone());
            while let Some(n) = q.pop_front() {
                if &n == target {
                    return true;
                }
                if !seen.insert(n.clone()) {
                    continue;
                }
                for e in &self.edges {
                    if e.from_version == n
                        && matches!(e.kind, RelationKind::Quotes | RelationKind::Republishes)
                        && reachable.contains(&e.to_version)
                    {
                        q.push_back(e.to_version.clone());
                    }
                }
            }
            false
        };

        let mut assigned = BTreeSet::new();
        let mut roots = BTreeSet::new();
        for node in &reachable {
            if assigned.contains(node) {
                continue;
            }
            let mut scc = BTreeSet::new();
            for other in &reachable {
                if can_reach(node, other) && can_reach(other, node) {
                    scc.insert(other.clone());
                    assigned.insert(other.clone());
                }
            }
            // Check if any node in `scc` has a Quotes/Republishes edge to a node outside `scc`.
            let has_external_parent = self.edges.iter().any(|e| {
                scc.contains(&e.from_version)
                    && matches!(e.kind, RelationKind::Quotes | RelationKind::Republishes)
                    && reachable.contains(&e.to_version)
                    && !scc.contains(&e.to_version)
            });
            if !has_external_parent && let Some(rep) = scc.into_iter().next() {
                roots.insert(rep);
            }
        }

        roots
    }

    /// Detects any directed `Quotes` / `Republishes` citation cycles reachable from `start` (`A08`).
    #[must_use]
    pub fn find_citation_cycles(&self, start: &VersionId) -> Vec<Vec<VersionId>> {
        let mut cycles = Vec::new();
        let mut path = Vec::new();
        let mut on_path = BTreeSet::new();
        self.dfs_citation_cycles(start, &mut path, &mut on_path, &mut cycles);
        cycles
    }

    fn dfs_citation_cycles(
        &self,
        curr: &VersionId,
        path: &mut Vec<VersionId>,
        on_path: &mut BTreeSet<VersionId>,
        cycles: &mut Vec<Vec<VersionId>>,
    ) {
        path.push(curr.clone());
        on_path.insert(curr.clone());

        for edge in &self.edges {
            if &edge.from_version == curr
                && matches!(edge.kind, RelationKind::Quotes | RelationKind::Republishes)
            {
                let next = &edge.to_version;
                if on_path.contains(next) {
                    if let Some(pos) = path.iter().position(|v| v == next) {
                        let mut cycle = path[pos..].to_vec();
                        cycle.push(next.clone());
                        cycles.push(cycle);
                    }
                } else if path.len() < 64 {
                    self.dfs_citation_cycles(next, path, on_path, cycles);
                }
            }
        }

        on_path.remove(curr);
        path.pop();
    }
}
