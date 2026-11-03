# Reversible Event Clustering and Distinct Evidence Counts (`#23`)

## Problem / Goal

Implements the narrowed `#23` slice in `crates/oia-query` (`oia_query::geo_workspace`):

- **Reversible event clustering (`EventClusterLedger`, `ProposedEventCluster`, `ClusterMatchCriteria`):** Candidate groupings remain explicit, inspectable, and version-pinned (`VersionId`) with append-only merge, split, and revert decision history (`ClusterDecisionRecord`). Reverting a merge restores the pre-merge cluster versions from `archived_cluster_versions` and records the revert in `decision_history` without deleting prior decision records.
- **Lineage-adjusted `DistinctEvidenceCounts`:** Reuses `oia_news_claims::LineageRelationKind` (`Quotes`, `Republishes`, `Supersedes`, `Supports`) and `ClaimAssessmentState` so syndication chains, quote loops, and superseded editions collapse into a single independent lineage root (`independent_source_count`) while separating `supportive_independent_source_count` from `mixed_stance_independent_source_count` and `disputed_or_retracted_count`.
- **Sub-day ambiguity preservation (`ReportedClusteringTime`, `TemporalComparisonOutcome`):** Never fabricates midnight UTC for `CalendarDateOnly` timestamps (`TemporalComparisonOutcome::CalendarDateOnlyCannotResolveSubDayGap`).
- **Downstream reassessment (`EventClusterLedger::revise_item`):** Preserves prior item revisions and marks any cluster containing the revised item with `needs_reassessment = true` without mutating `pinned_item_versions` in place (`AGENTS.md` §4).

## Architectural Boundary

- **Shared geographic/temporal primitives (`#12`):** Coverage envelopes, bounding-box/polygon geometry, and viewing-cone footprint math are intentionally deferred to `oia-time-geo` (`#12`) so `oia-query` does not duplicate coordinate or calibration primitives.
- **Workspace shell navigation (`#16`, `#14`):** Linked multi-surface selection and local view filters belong to `oia-app` / query-service shell state (`#16`, `#14`) rather than `oia-query`.

## Verification

- `cargo +1.99.0 fmt --all -- --check`
- `cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings`
- `cargo +1.99.0 test --workspace --locked --offline`
- `cargo +1.99.0 run -p oia-query --example inspect-a04-geo-workspace --locked --offline`
