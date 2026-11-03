# Reversible Event Clustering and Distinct Evidence Counts (`#23`)

## Problem / Goal

Implements the narrowed `#23` slice in `crates/oia-query` (`oia_query::geo_workspace`):

- **Reversible event clustering (`EventClusterLedger`, `ProposedEventCluster`, `ClusterMatchCriteria`):** Candidate groupings remain explicit, inspectable, and version-pinned (`VersionId`) with append-only merge, split, reassess, and revert decision history (`ClusterDecisionRecord`). Reverting a merge, split, or reassess restores the pre-decision cluster snapshots (preserving their `pinned_item_versions`, `counts`, `conflicts`, and `rationale`) under new `VersionId`s, carries forward relevant reassessment reasons, rejects destination `cluster_id` collisions (`RevertDestinationClusterInUse`), and records the revert in `decision_history` without deleting prior decision records.
- **Lineage-adjusted `DistinctEvidenceCounts`:** Reuses `oia_news_claims::LineageRelationKind` (`Quotes`, `Republishes`, `Supersedes`, `Supports`) and `ClaimAssessmentState` so syndication chains, quote loops, shared `source_id`s along a collapsing lineage chain, and identical `content_hash` values collapse into a single independent-source equivalence class (`independent_source_count`) while separating `supportive_independent_source_count`, `contradictory_independent_source_count`, and `mixed_stance_independent_source_count` (and excluding `AccountOrigin::Simulated` items into `simulated_item_count`).
- **Explicit spatial coverage gaps (`ClusterConflict::SpatialSeparationNotEvaluated`):** When a multi-item cluster has no recorded pairwise distance between two members, `SpatialSeparationNotEvaluated` is emitted rather than treating missing distance data as spatial agreement.
- **Sub-day ambiguity preservation (`ReportedClusteringTime`, `TemporalComparisonOutcome`):** Never fabricates midnight UTC for `CalendarDateOnly` timestamps (`TemporalComparisonOutcome::CalendarDateOnlyCannotResolveSubDayGap`), using a timezone-safe lower bound (`raw_diff.saturating_sub(2)`) for `minimum_whole_day_separation`.
- **Downstream reassessment without silent truncation or in-place snapshot mutation (`EventClusterLedger::revise_item`, `EventClusterLedger::reassess_cluster`, `cluster_needs_reassessment`, `cluster_reassessment_reasons`):** Preserves prior item revisions (`max_revisions_per_item` budget) and records reassessment reasons in `reassessment_by_cluster_version` (`max_reassessment_reasons_per_cluster` budget, never silently truncating) for every active cluster that contains the item or reaches it transitively through lineage, without mutating frozen snapshots in `archived_cluster_versions` (`AGENTS.md` §4). Calling `reassess_cluster` acknowledges pending reasons under a new `VersionId` and records `ClusterDecisionKind::Reassess`.

## Architectural Boundary & `#12` Consolidation Notes

- **Shared geographic/temporal primitives (`#12`):** Coverage envelopes, bounding-box/polygon geometry, and viewing-cone footprint math are deferred to `oia-time-geo` (`#12`). `ClusteringTimePrecision` and `LocationQualification` (`LocationQualificationKind`) are provisional clustering-boundary types pending `#12` consolidation with `oia-export::TimePrecision`.
- **Cross-crate lineage consolidation (`#23` / `#12`):** `EventClusterLedger` computes graph-traversed lineage equivalence classes over `LineageRelationKind`, whereas `oia-news-claims::verify_corpus` uses caller-supplied `independent_lineage_root_id`; these should be unified when `#12` and shared application services (`#14`) land.
- **Workspace shell navigation (`#16`, `#14`):** Linked multi-surface selection and local view filters are deferred to `oia-app` (`#16`) and shared application services (`#14`) rather than `oia-query`.

## Verification

- `cargo +1.99.0 fmt --all -- --check` (exit 0)
- `cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings` (exit 0)
- `cargo +1.99.0 test -p oia-query --locked --offline` (exit 0, 4 contract tests pass)
- `cargo +1.99.0 run -p oia-query --example inspect-a04-geo-workspace --locked --offline` (exit 0)
