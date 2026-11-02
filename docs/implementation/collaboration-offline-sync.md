# Shared-case collaboration, sensitive-material isolation, and offline field synchronization kernel

Implemented slice of [#38](https://github.com/mESmaC/oia/issues/38), supporting
[COLLAB-01](../spec.md#61-shared-cases-and-roles),
[COLLAB-03](../spec.md#63-publication-and-portability), and
[FIELD-04](../spec.md#55-personal-observatories-and-field-research), and the
in-memory authorization, report-delivery, and offline conflict-synchronization
subsets of [A16](../delivery.md#a16-report-collaboration-and-sharing) and
[A13](../delivery.md#a13-fieldwork-and-multi-observer-research).
The Rust entry points live in [`crates/oia-domain/src/collaboration.rs`](../../crates/oia-domain/src/collaboration.rs).

## Contract

`SharedCaseWorkspace` enforces role capabilities, independent sensitivity
clearances, author- and scope-attributed reviews, distinct report delivery
transitions, and version-aware offline synchronization under explicit
`CollaborationLimits`:

1. **Roles and independent sensitivity clearances (`COLLAB-01`, `A16`, `docs/architecture.md` §9):**
   - `CaseRole` separates `Owner`, `Editor`, `Reviewer`, and `Viewer` responsibilities:
     - Only `Owner` can assign roles/clearances or issue a public report snapshot (`publish_report_snapshot`), and the last owner cannot be demoted (`CannotRemoveLastOwner`).
     - `Owner` and `Editor` can add/edit case items, prepare offline field packages, synchronize offline captures, resolve conflicts, save report drafts, and share reports with collaborators.
     - `Owner`, `Editor`, and `Reviewer` can submit scoped reviews, record competing interpretations, and export privately.
     - `Viewer` has read-only access to permitted items.
   - `ClearanceGrant` (`allow_unpublished_drafts`, `allow_sensitive_artifacts`, `allow_sensitive_locations`) is evaluated independently of `CaseRole` across every `AccessSurface` (`Search`, `BlobRead`, `Subscription`, `ModelInput`, `ReportGeneration`, `DirectArtifactRequest`, `OfflinePreparation`, and `Export`).
   - When an actor lacks `allow_sensitive_locations` on an otherwise accessible item, `PermittedItemView` physically strips `sensitive_location_detail` (`None`), sets `redacted_sensitive_location = true`, and emits a `RedactionNotice` (`SensitiveLocationStripped`) rather than merely hiding a UI field (`AGENTS.md` §5).
2. **Author- and scope-attributed reviews, assignments, and competing interpretations (`COLLAB-01`):**
   - `submit_scoped_review` records `author`, `author_role`, `target_item_id`, `target_version`, explicit `ReviewScope` (`WholeTargetVersion`, `PassageByteRange`, `TemporalWindow`, `CalibrationAndGeometryOnly`, `MethodologyOnly`), `ReviewDisposition`, `rationale`, `cited_evidence_versions`, and optional `supersedes_review_version`.
   - `ScopedReviewRecord::implies_universal_truth()` always returns `false` and carries `MANDATORY_REVIEW_SCOPE_CAVEAT`. Superseding reviews append to history without overwriting prior reviews, and open `ReviewAssignment` records for that actor and target version transition to `CompletedWithReview`.
   - `add_competing_interpretation` retains multiple analyst hypotheses side-by-side with explicit supporting/contradicting versions and assumptions.
3. **Distinct save, private-export, collaborator-share, and publish transitions (`COLLAB-03`, `A16`, `AGENTS.md` §6):**
   - `save_report_draft` (`SavedDraftOnly`), `export_report_privately` (`PrivateExportOnly`), `share_report_with_collaborators` (`SharedWithCollaborators`), and `publish_report_snapshot` (`PublishedSnapshot`) are separate methods producing distinct `ReportDeliveryMode` records.
   - `ReportSnapshotRecord::is_publicly_published()` returns `true` only for `PublishedSnapshot`, which is restricted to `CaseRole::Owner` and filtered through `ClearanceGrant::standard_only()` so unpublished drafts, sensitive artifacts, and sensitive location coordinates are omitted with an explicit `redaction_preview`.
   - `flag_dependent_reports_on_correction` flags any report snapshot citing a superseded version with `needs_reassessment = true` and appends a `DependencyChangeFlag` without overwriting the snapshot's content, title, or `report_version`.
4. **Offline field preparation, local capture, and version-aware conflict synchronization (`FIELD-04`, `A13`, `A16`):**
   - `prepare_offline_package` filters items by the actor's clearance and records `[prepared_at_unix_ms, expires_at_unix_ms]`. At synchronization time, `PreparedOfflinePackage::evaluate_freshness` marks reference usage as `FreshWithinWindow`, `CachedExpired { expired_by_ms }`, or `ClockUncertainCannotVerifyExpiry`.
   - `OfflineFieldCapture` preserves `DeviceClockQuality` (`SynchronizedUtc`, `DriftingLocalClock`, or `UnreferencedLocalClock`) and fixed `EvidenceOriginKind` (`ObservedOrReported` vs `LabeledSimulation { simulation_parameters }`). Simulated items carry `VISIBLE_SIMULATION_LABEL` across search, reports, and exports and cannot be promoted to observed evidence (`OriginPromotionForbidden`).
   - `synchronize_offline_capture` checks `base_version` against the item's current head version:
     - Matching `base_version` advances the head cleanly (`SyncOutcome::AppliedCleanly`).
     - Divergent `base_version` with identical SHA-256, text, location detail, and origin deduplicates (`SyncOutcome::DeduplicatedExactBytes`, `docs/architecture.md` §9).
     - Divergent `base_version` with differing content never overwrites the current head or auto-merges semantic text; it retains both versions in the item's history and opens an unresolved `SyncConflictRecord` (`SyncOutcome::ConflictPreserved`).
   - `resolve_sync_conflict` requires both conflicting parent `VersionId`s (`ConflictParentMismatch` otherwise) and appends a merged version while preserving both conflicting parent versions in `inspect_item_history`.

## Limitations and remaining `#38` scope

- This module is an in-memory domain kernel; it validates 64-character lowercase hexadecimal SHA-256 strings supplied by the caller, but does not read files, persist state to SQLite or content-addressed blob storage (`#11`), schedule background jobs (`#14`), or render/package HTML/Markdown/JSON bundle files (`#22`).
- Decision `D10` (hosted transport, identity provider, and network synchronization protocol) remains open and is not settled by this kernel.
- Interactive workspace UI controls for role management, review threads, redaction previews, and conflict resolution (`#16`) and full end-to-end `A13`/`A16` integration acceptance remain open on `#38`.
- No Lean theorem or formal Rust-to-Lean correspondence claim is made for this slice.

## Local verification

Run the synthetic workflow and workspace checks with pinned `rustc 1.99.0`:

```text
cargo +1.99.0 fmt --all -- --check
cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings
cargo +1.99.0 test --workspace --locked --offline
cargo +1.99.0 run -p oia-domain --example sync-offline-case --locked --offline
```

Two targeted contract tests in [`crates/oia-domain/tests/collaboration_contract.rs`](../../crates/oia-domain/tests/collaboration_contract.rs) verify:
1. `role_and_sensitivity_isolation_across_surfaces_and_distinct_report_delivery`: verifies role boundaries, independent sensitivity clearances across all `AccessSurface`s, physical stripping of `sensitive_location_detail`, scoped review attribution (`implies_universal_truth() == false`), competing interpretations, distinct `SaveDraftOnly`/`PrivateExportOnly`/`SharedWithCollaborators`/`PublishedSnapshot` transitions, and non-destructive reassessment flagging on source correction.
2. `offline_sync_preserves_cached_expiry_clock_uncertainty_simulations_and_conflicts`: verifies offline package expiry marking (`CachedExpired`), drifting and unreferenced device clock preservation, fixed simulation labeling (`VISIBLE_SIMULATION_LABEL`) and promotion refusal (`OriginPromotionForbidden`), exact-byte deduplication, non-destructive conflict preservation (`SyncOutcome::ConflictPreserved`), parent-checked conflict resolution (`resolve_sync_conflict`), and budget/hash input validation.
