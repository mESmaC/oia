use crate::{
    AssessmentLimits, AvailabilityState, ClaimCheckError, LimitKind, LineageEdgeInput,
    LineageRelationKind, RetractionStatus, StopReason,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug)]
pub struct Publication<'a> {
    pub record_id: &'a str,
    pub version_id: &'a str,
    pub source_id: &'a str,
    pub availability: AvailabilityState,
    pub retraction_status: RetractionStatus,
    pub predecessor_version_id: Option<&'a str>,
    pub superseded_by_version_id: Option<&'a str>,
}

#[derive(Debug, Eq, PartialEq)]
pub struct SourceInventory<'a> {
    pub source_id: &'a str,
    pub versions: usize,
    pub records: usize,
}

#[derive(Debug, Eq, PartialEq)]
pub struct PublicationInventory<'a> {
    pub versions: usize,
    pub records: usize,
    pub sources: Vec<SourceInventory<'a>>,
}

pub fn publication_inventory<'a>(
    publications: &[Publication<'a>],
    limits: AssessmentLimits,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<PublicationInventory<'a>, ClaimCheckError> {
    check_control().map_err(ClaimCheckError::Stopped)?;
    if publications.len() > limits.max_source_records() {
        return Err(ClaimCheckError::BudgetExceeded {
            kind: LimitKind::SourceRecords,
            actual: publications.len(),
            limit: limits.max_source_records(),
        });
    }
    let mut versions = BTreeSet::new();
    let mut record_sources = BTreeMap::new();
    let mut sources: BTreeMap<&str, (usize, BTreeSet<&str>)> = BTreeMap::new();
    for publication in publications {
        check_control().map_err(ClaimCheckError::Stopped)?;
        if publication.version_id.is_empty() || !versions.insert(publication.version_id) {
            return Err(ClaimCheckError::DuplicateIdentifier {
                kind: "record_version_id",
                id: publication.version_id.to_owned(),
            });
        }
        if publication.record_id.is_empty()
            || publication.source_id.is_empty()
            || record_sources
                .insert(publication.record_id, publication.source_id)
                .is_some_and(|source| source != publication.source_id)
        {
            return Err(ClaimCheckError::InvalidPublication {
                record_version_id: publication.version_id.to_owned(),
                reason: "record/source IDs must be nonempty and record ownership consistent",
            });
        }
        if publication.availability == AvailabilityState::Removed
            && publication.retraction_status == RetractionStatus::Retracted
        {
            return Err(ClaimCheckError::RemovedPageAutoRetracted {
                record_version_id: publication.version_id.to_owned(),
            });
        }
        let source = sources.entry(publication.source_id).or_default();
        source.0 += 1;
        source.1.insert(publication.record_id);
        if sources.len() > limits.max_sources() {
            return Err(ClaimCheckError::BudgetExceeded {
                kind: LimitKind::Sources,
                actual: sources.len(),
                limit: limits.max_sources(),
            });
        }
    }
    check_control().map_err(ClaimCheckError::Stopped)?;
    Ok(PublicationInventory {
        versions: publications.len(),
        records: record_sources.len(),
        sources: sources
            .into_iter()
            .map(|(source_id, (versions, records))| SourceInventory {
                source_id,
                versions,
                records: records.len(),
            })
            .collect(),
    })
}

pub fn validate_correction_links(
    publications: &[Publication<'_>],
    limits: AssessmentLimits,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<(), ClaimCheckError> {
    publication_inventory(publications, limits, &mut check_control)?;
    for publication in publications {
        check_control().map_err(ClaimCheckError::Stopped)?;
        if (publication.retraction_status == RetractionStatus::CorrectedByLaterVersion
            && publication.superseded_by_version_id.is_none())
            || (publication.retraction_status == RetractionStatus::CorrectionIssued
                && publication.predecessor_version_id.is_none())
        {
            return Err(ClaimCheckError::InvalidPublication {
                record_version_id: publication.version_id.to_owned(),
                reason: "declared correction state requires an explicit version link",
            });
        }
        for (target_id, predecessor) in [
            (publication.predecessor_version_id, true),
            (publication.superseded_by_version_id, false),
        ] {
            let Some(target_id) = target_id else { continue };
            let target = publications
                .iter()
                .find(|entry| entry.version_id == target_id)
                .ok_or_else(|| ClaimCheckError::UnknownReference {
                    context: format!("correction link {}", publication.version_id),
                    target: target_id.to_owned(),
                })?;
            let reciprocal = if predecessor {
                target.superseded_by_version_id
            } else {
                target.predecessor_version_id
            };
            if target_id == publication.version_id
                || target.record_id != publication.record_id
                || target.source_id != publication.source_id
                || reciprocal != Some(publication.version_id)
            {
                return Err(ClaimCheckError::InvalidPublication {
                    record_version_id: publication.version_id.to_owned(),
                    reason: "correction links must be reciprocal versions of the same record/source",
                });
            }
        }
        let mut next = publication.predecessor_version_id;
        for _ in 0..publications.len() {
            check_control().map_err(ClaimCheckError::Stopped)?;
            let Some(version_id) = next else { break };
            if version_id == publication.version_id {
                return Err(ClaimCheckError::InvalidPublication {
                    record_version_id: publication.version_id.to_owned(),
                    reason: "correction chain contains a cycle",
                });
            }
            next = publications
                .iter()
                .find(|entry| entry.version_id == version_id)
                .and_then(|entry| entry.predecessor_version_id);
        }
    }
    Ok(())
}

pub fn validate_declared_lineage(
    version_ids: &[&str],
    edges: &[LineageEdgeInput<'_>],
    limits: AssessmentLimits,
    mut check_control: impl FnMut() -> Result<(), StopReason>,
) -> Result<(), ClaimCheckError> {
    check_control().map_err(ClaimCheckError::Stopped)?;
    for (kind, actual, limit) in [
        (
            LimitKind::SourceRecords,
            version_ids.len(),
            limits.max_source_records(),
        ),
        (
            LimitKind::LineageEdges,
            edges.len(),
            limits.max_lineage_edges(),
        ),
    ] {
        if actual > limit {
            return Err(ClaimCheckError::BudgetExceeded {
                kind,
                actual,
                limit,
            });
        }
    }
    for (idx, edge) in edges.iter().enumerate() {
        check_control().map_err(ClaimCheckError::Stopped)?;
        if edge.edge_id.is_empty()
            || edges
                .get(..idx)
                .is_some_and(|prior| prior.iter().any(|e| e.edge_id == edge.edge_id))
        {
            return Err(ClaimCheckError::DuplicateIdentifier {
                kind: "edge_id",
                id: edge.edge_id.to_owned(),
            });
        }
        if !version_ids.contains(&edge.from_version_id) {
            return Err(ClaimCheckError::UnknownReference {
                context: format!("lineage edge {}", edge.edge_id),
                target: edge.from_version_id.to_owned(),
            });
        }
        if !version_ids.contains(&edge.to_version_id) {
            return Err(ClaimCheckError::UnknownReference {
                context: format!("lineage edge {}", edge.edge_id),
                target: edge.to_version_id.to_owned(),
            });
        }
        if edge.from_version_id == edge.to_version_id {
            return Err(ClaimCheckError::InvalidLineageEdge {
                edge_id: edge.edge_id.to_owned(),
                reason: "self-referential lineage edge is not permitted",
            });
        }
        if matches!(
            edge.relation_kind,
            LineageRelationKind::Quotes
                | LineageRelationKind::Republishes
                | LineageRelationKind::Supersedes
        ) && edge.preserves_independent_corroboration
        {
            return Err(ClaimCheckError::InvalidLineageEdge {
                edge_id: edge.edge_id.to_owned(),
                reason: "quotes, republishes, and supersedes edges must not count as independent corroboration",
            });
        }
    }

    check_control().map_err(ClaimCheckError::Stopped)?;
    Ok(())
}
