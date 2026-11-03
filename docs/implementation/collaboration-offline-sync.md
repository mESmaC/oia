# Proposed access decision policy and offline field conflict-synchronization kernel

Implemented slice of [#38](https://github.com/mESmaC/oia/issues/38), supporting
[COLLAB-01](../spec.md#61-shared-cases-and-roles),
[COLLAB-03](../spec.md#63-publication-and-portability), and
[FIELD-04](../spec.md#55-personal-observatories-and-field-research), and the
in-memory policy/synchronization subset of
[A16](../delivery.md#a16-report-collaboration-and-sharing) and
[A13](../delivery.md#a13-fieldwork-and-multi-observer-research).
The Rust entry points live in [`crates/oia-domain/src/collaboration.rs`](../../crates/oia-domain/src/collaboration.rs).

## Status: proposed policy, not yet wired into runtime services (`AGENTS.md` §3, §4)

- **Proposed default pending `D10`:** In [delivery.md](../delivery.md), decision `D10` covers hosted transport/identity, the capability matrix, merge rules, and sensitive-data defaults. All four parts of `D10` remain open. `PROPOSED_ROLE_SURFACE_TABLE`, `evaluate_proposed_access_decision`, and the conflict-merge rules in `OfflineSyncItem` are **proposed defaults** accompanying [architecture.md §9](../architecture.md#9-access-collaboration-and-local-operation), not settled product policy.
- **Not yet called by application services:** Nothing in the product calls `evaluate_proposed_access_decision` yet. Search, blob reads, subscriptions, model prompts, offline preparation, and report export have **no** runtime access control until `#14` (application services), `#22` (`oia-export`), and `#16` (workspace UI) wire this policy into their execution paths.
- **Existence hiding requires collection-layer filtering (`#14`, `#22`):** While `AccessDenialReason` omits item and version identifiers so error payloads do not name restricted artifacts, a single-item check that returns `Err(SyncError::AccessDenied(_))` to a caller who already holds a concrete `OfflineSyncItem` or `ItemId` still reveals that a restricted item exists at that handle. True existence hiding (omitting restricted items from search/list results and returning uniform not-found responses on direct lookups) must be enforced by the collection/repository and export layers in `#14` and `#22`.
- **Single owner per concern:** Report snapshot rendering, existence-sensitive omission disclosures, and dependency reassessment walks live in `oia-export` (`#22`) and `oia-documents` (`#19`). Origin classification reuses `oia_domain::witness::AccountOrigin` (`Reported` vs `Simulated`).

## Contract

1. **Pure proposed access decision function (`evaluate_proposed_access_decision`):**
   - Evaluates `(CaseRole, ClearanceGrant, AccessSurface, SensitivityFlags) -> AccessDecision` from a single table (`PROPOSED_ROLE_SURFACE_TABLE`):
     - `Owner`: permitted by role across all `AccessSurface`s (`Search`, `BlobRead`, `Subscription`, `DirectArtifactRequest`, `ReviewSubmission`, `PrivateExport`, `ModelInput`, `ReportDraftSave`, `OfflinePreparation`, `EvidenceMutation`, `CollaboratorShare`, `PublicPublication`).
     - `Editor`: permitted by role on read, review, private export, model input, draft save, offline preparation, and evidence mutation; denied on `CollaboratorShare` and `PublicPublication` (`docs/architecture.md` §9: "Ownership controls sharing").
     - `Reviewer`: permitted by role on passive read surfaces (`Search`, `BlobRead`, `Subscription`, `DirectArtifactRequest`), `ReviewSubmission`, and `PrivateExport`; denied on `ModelInput`, `ReportDraftSave`, `OfflinePreparation`, `EvidenceMutation`, `CollaboratorShare`, and `PublicPublication`.
     - `Viewer`: permitted by role only on passive read surfaces (`Search`, `BlobRead`, `Subscription`, `DirectArtifactRequest`); denied on all review, export, model-input, offline-preparation, mutation, share, and publication surfaces even if holding `ClearanceGrant::full()`.
   - `ClearanceGrant` (`allow_unpublished_drafts`, `allow_sensitive_artifacts`, `allow_sensitive_locations`) is checked independently of `CaseRole`:
     - On `AccessSurface::PublicPublication`, `ClearanceGrant::standard_only()` is enforced regardless of the caller's personal clearance.
     - On `AccessSurface::CollaboratorShare`, callers must pass the intersection of the sharing owner's clearance and every target recipient's clearance (`ClearanceGrant::intersect`) as the `clearance` argument so an owner cannot share an item with a collaborator who lacks clearance for its sensitivity flags.
   - Returns `AccessDecision::AllowFull`, `AccessDecision::AllowWithStrippedLocation` (instructing the caller to physically strip `sensitive_location_detail`), or `AccessDecision::Deny(AccessDenialReason)`.

2. **Offline field synchronization and conflict kernel (`OfflineSyncItem`, `FIELD-04`, `A13`, `A16`):**
   - Every read (`read_head_on_surface`, `inspect_history_on_surface`, `inspect_conflicts_on_surface`) and write (`new`, `prepare_offline_window`, `synchronize_offline_capture`, `resolve_sync_conflict`) calls `evaluate_proposed_access_decision`. When `AllowWithStrippedLocation` is returned, `PermittedVersionView` physically strips `sensitive_location_detail` (`None`) and sets `location_stripped = true` across both head and historical versions. Custom `fmt::Debug` implementations on `StoredItemVersion` and `OfflineSyncItem` redact `sensitive_location_detail` (`Some("<redacted>")`) so diagnostic logs do not leak restricted coordinates.
   - **Location-restricted editor protections:** A caller whose write decision is `AllowWithStrippedLocation` (or who lacks `allow_sensitive_locations` when creating a new item) is refused with `SyncError::AccessDenied(AccessDenialReason::SensitiveLocationRestricted)` if they supply `sensitive_location_detail.is_some()` or `sensitivity.sensitive_location == true`. When such an editor cleanly updates or resolves an item that already carries a hidden sensitive location, the existing hidden `sensitive_location_detail` on the head is preserved; on divergent base versions, content deduplication is bypassed (`ConflictPreserved`) so an un-cleared editor cannot use `DeduplicatedByDeclaredDigest` as a location-guessing oracle.
   - **Monotonic sensitivity flags (`SensitivityFlags::union`):** Sensitivity flags—including `unpublished_draft`—are combined monotonically across clean advances, deduplication, and conflict resolution so a stricter sensitivity marking is never silently dropped during offline sync. Explicit draft publication (transitioning `unpublished_draft` from `true` to `false`) is out of scope for this offline sync kernel and belongs to the publication workflow in `D10` / `#22`.
   - `prepare_offline_window` requires `AccessSurface::OfflinePreparation` (`Owner` or `Editor`) and enforces `SyncLimits::max_prepared_packages`.
   - Every stored version retains three distinct timestamps paired with their own `DeviceClockQuality` (`SynchronizedUtc`, `DriftingLocalClock { estimated_offset_ms, uncertainty_ms }`, or `UnreferencedLocalClock`): `capture_time`, `receipt_time`, and `sync_time`. `DriftingLocalClock::estimated_offset_ms` is the signed offset (`utc_ms - device_ms`) to add to `recorded_unix_ms` to estimate UTC, and `uncertainty_ms` is bounded by `MAX_CLOCK_UNCERTAINTY_MS` (`31_622_400_000` ms, or 366 days).
   - `PreparedOfflineWindow::evaluate_freshness` computes the conservative UTC interval `[earliest_utc_ms, latest_utc_ms]` spanning both raw and offset-adjusted device readings plus `±uncertainty_ms`:
     - Returns `FreshWithinWindow` only when the entire uncertainty interval lies within `[prepared_at_unix_ms, expires_at_unix_ms]`.
     - Returns `BeforePreparationWindow { min_before_by_ms }` when `latest_utc_ms < prepared_at_unix_ms` (entire interval precedes package preparation).
     - Returns `CachedExpired { min_expired_by_ms }` when `earliest_utc_ms > expires_at_unix_ms` (entire interval is past expiration).
     - Returns `ExpiryUncertain` whenever the clock is unreferenced or its offset/uncertainty interval straddles `prepared_at_unix_ms` or `expires_at_unix_ms`.
   - `synchronize_offline_capture` checks `expected_base_version` against the current `head_version`:
     - Matching base version advances the head cleanly (`SyncOutcome::AppliedCleanly`).
     - Divergent base version with matching caller-declared SHA-256 digest, summary text, location detail, and simulation parameters returns `SyncOutcome::DeduplicatedByDeclaredDigest` while unioning `SensitivityFlags` (`SensitivityFlags::union`).
     - Divergent base version with differing content preserves both versions in history and opens a `SyncConflictRecord` (`SyncOutcome::ConflictPreserved`). Only `ConflictResolutionState::Unresolved` conflicts count toward `SyncLimits::max_conflicts`, so resolving conflicts frees capacity for subsequent field syncs while retaining resolved records in the audit trail.
   - `resolve_sync_conflict` verifies that `self.head_version` is either `conflict.head_version_at_conflict` or a linear/merged descendant of it (`version_descends_from`), and requires `parent_versions` to match `(self.head_version, conflict.incoming_offline_version)` (in either order):
     - If `self.head_version` has advanced past `conflict.head_version_at_conflict` (for example, after resolving an earlier conflict in a multi-observer sequence) and the caller still passes the stale `(conflict.head_version_at_conflict, conflict.incoming_offline_version)` pair, `resolve_sync_conflict` returns `Err(SyncError::StaleConflictHeadMoved)`.
     - When the caller re-evaluates the conflict against the descended current head and passes `(self.head_version, conflict.incoming_offline_version)`, resolution succeeds and records a new merged head version descending from both.

## Limitations and remaining `#38` scope

- In-memory domain kernel only; `declared_content_sha256` validates 64-character lowercase hex syntax and compares caller-supplied digest strings without hashing external files.
- Decision `D10` (hosted transport/identity, final capability matrix, merge rules, and sensitive-data defaults), runtime service wiring (`#14`), persistent storage (`#11`), `oia-export` integration (`#22`), and workspace UI (`#16`) remain open on `#38`.
- No Lean theorem or formal Rust-to-Lean correspondence claim is made for this slice.

## Local verification

Run with pinned `rustc 1.99.0`:

```text
cargo +1.99.0 fmt --all -- --check
cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings
cargo +1.99.0 test --workspace --locked --offline
cargo +1.99.0 run -p oia-domain --example sync-offline-case --locked --offline
```

Two targeted contract tests in [`crates/oia-domain/tests/collaboration_contract.rs`](../../crates/oia-domain/tests/collaboration_contract.rs) verify:
1. `proposed_access_decision_enforces_role_surface_table_and_independent_sensitivity`: checks `PROPOSED_ROLE_SURFACE_TABLE` across roles and surfaces, independent sensitivity clearance, `CollaboratorShare` recipient clearance intersection (`ClearanceGrant::intersect`), public publication forcing `standard_only()`, location stripping across full version history, redacting `fmt::Debug` output (`<redacted>`), refusal of sensitive location writes by un-cleared editors, preservation of hidden head locations without exposing a deduplication oracle, and role/count bounds on `prepare_offline_window`.
2. `offline_sync_preserves_three_clocks_uncertainty_sensitivity_and_refuses_stale_conflict_head`: checks separate `capture_time`/`receipt_time`/`sync_time` retention, conservative clock offset/uncertainty freshness (`BeforePreparationWindow` before `prepared_at_unix_ms`, `ExpiryUncertain` on boundary straddles, `CachedExpired` past expiry) and `MAX_CLOCK_UNCERTAINTY_MS` enforcement, sensitivity-flag preservation on `DeduplicatedByDeclaredDigest`, multi-observer conflict resolution across two concurrent offline conflicts (`StaleConflictHeadMoved` when passing the stale conflict head, followed by clean resolution against the descended `(current_head, incoming)` pair and unresolved-only `max_conflicts` accounting), and fixed `AccountOrigin::Simulated` labeling.
