# Personal Exposure and Owned Infrastructure Audit (`oia-analysis::exposure_audit`)

This implementation reference covers backlog item `#47` (`[P6] Audit personal public exposure and owned infrastructure`), spanning requirement `PRIVACY-02` and the personal-exposure, ambiguous-account-match, and declared-domain-scope slices of acceptance criterion `A18`.

## Scope and Non-Goals

In scope:
- Explicit, versioned user-declared audit scope (`DeclaredAuditScope`, `DeclaredAccountHandle`, `OwnershipBasis::SelfOwnedPersonalScope`, `OwnershipBasis::AuthorizedAssessmentScope`, `ScopeRevisionInput`) with explicit scope revision history (`PersonalExposureAudit::revise_declared_scope` tracking `predecessor_scope_version` and `revision_rationale`).
- Strict refusal of out-of-scope expansion or fetching (`HandleOutsideDeclaredScope`, `DomainOutsideDeclaredScope`), refusal of IPv4/IPv6 literals (`InvalidDomainSyntax`) and bare multi-label public suffixes (`PublicSuffixRootDisallowed`), refusal of transitive pivoting from external profiles (`PrimaryProfileMustBeInDeclaredScope`), and time-bounded authorization verification (`AuthorizationExpiredOrNotYetValid`) and actor checks (`UnauthorizedActor`).
- Strict service-qualified profile claiming (`PublicProfileRecord`, `PublicProfileInput`, `ObservationTimeEnvelope`) reusing `oia_domain::witness::AccountOrigin` (`Reported` or `Simulated`) and carrying typed source citations (`SourceCitation`, `ExposureCitationAnchor`):
  - A profile is accepted with `claimed_in_declared_scope: true` **only** when its `(service_name, handle)` matches a `DeclaredAccountHandle` in `current_scope`. Listing an in-scope domain in `listed_website_domain` is recorded as a candidate signal (`MatchSignalKind::CrossLinkToDeclaredDomain`) and never makes a third-party profile in-scope.
  - Unclaimed candidate profiles (`claimed_in_declared_scope: false`) require an `anchor_primary_profile_id` pointing to a currently in-scope primary profile (`UnclaimedProfileRequiresInScopePrimaryAnchor`).
  - Narrowing `DeclaredAuditScope` via `revise_declared_scope` re-evaluates existing profiles, candidate matches, and infrastructure entries, marking any record that fell outside the narrowed scope with `outside_current_scope() == true` and `needs_reassessment() == true` while retaining its history.
- Ambiguous cross-platform account matching (`AccountMatchCandidate`, `AccountMatchSignal`, `MatchAssessmentState`, `AccountMatchReviewEntry`, `ACCOUNT_MATCH_UNCERTAINTY_NOTICE`) with explicitly uncalibrated heuristic scores (`uncalibrated_heuristic_score` in `[0.0, 1.0]`), candidate-state gating (`ProposedCandidate`, `AmbiguousRetained`) prior to user confirmation (`UserConfirmedSameOwner`), and retention of rejected candidates (`UserRejectedDifferentEntity`) with their full review audit history.
- Passive public-source owned-domain exposure findings (`InfrastructureHistoryEntry`, `InfrastructureArtifactKind::DnsRecord`, `InfrastructureArtifactKind::CertificateHistory`, `InfrastructureArtifactKind::ExposedContact`, `DnsRecordType`, `ExposedContactKind`) and source query coverage records (`SourceCoverageRecord`, `SourceQueryCoverage::Complete`, `SourceQueryCoverage::PartialBudgetReached`, `SourceQueryCoverage::SourceUnavailable`, `SourceQueryCoverage::ObservedAbsenceInQueriedSource`).
- Outbound query preparation and collaboration access evaluation (`OutboundSourceQueryCandidate`, `PermittedSourceQuery`, `LOCAL_ASSESSMENT_PRIVACY_NOTICE`, `evaluate_surface_access`):
  - `prepare_read_only_source_query` verifies that a proposed lookup is read-only (`mutates_remote_account_or_dns == false`), within the active authorization window, scoped to a single declared domain or `(service_name, handle)` target, and does not attach private notes/hypotheses or export the full scope manifest (`PrivateAssessmentLeakBlocked`).
  - `evaluate_surface_access` delegates local audit surface access checks to `oia_domain::collaboration::evaluate_proposed_access_decision` (`#38`, proposed default pending `D10`), treating local exposure-audit state as an unpublished draft with sensitive artifacts (`PublicPublication` is always denied).
- Source-version reassessment propagation (`flag_source_version_updated`) and summary synthesis (`PersonalExposureAudit::summary`, `ExposureAuditSummary`).

## Design Notes and Boundaries

- **Owned citation anchors vs. `oia_export::CitationAnchor`:** `oia-export` sits above `oia-analysis` in the workspace dependency graph and defines a borrowed `CitationAnchor<'input>` over report slices (`ByteRange`, `Passage`, `Region`, `TimestampRange`). `ExposureCitationAnchor` provides the owned equivalent for retained audit records (`Utf8ByteRange`, `ExtractedPassage`, `StructuredRecordField`).
- **Caller-supplied timestamps:** Because `oia-analysis` is a pure deterministic analysis kernel with no wall-clock I/O, all timestamps (`declared_at_unix_ms`, `revised_at_unix_ms`, `requested_at_unix_ms`, and `ObservationTimeEnvelope` fields) are provided by the caller. The kernel enforces monotonicity across revisions and retrieval/audit ordering; wall-clock attestation belongs to the outer ingestion and job services (`#13`, `#14`).
- **Deferred sibling work packages:**
  - Local file metadata inspection and removal is covered by `#46` (`PRIVACY-01`).
  - Public-source change and local-situation monitoring is covered by `#48` (`PRIVACY-03`, `A18` monitoring slice).
  - Live network source adapters (`#13`), link-analysis graph service integration (`#36`), SQLite persistence (`#11`, `#14`), and workspace UI (`#16`) remain open on `#47`.

## Key Types and Invariants

| Type / Function | Module | Role |
| --- | --- | --- |
| `DeclaredAuditScope` / `DeclaredAccountHandle` | `oia_analysis::exposure_audit` | Captures user-declared `(service_name, handle)` pairs, owned root domains, ownership basis (`SelfOwnedPersonalScope` or time-bounded `AuthorizedAssessmentScope`), and scope revision metadata |
| `PersonalExposureAudit::revise_declared_scope` | `oia_analysis::exposure_audit` | Verifies actor and active authorization window, produces a new versioned scope linked to `predecessor_scope_version` with a required `revision_rationale`, and reconciles existing records (`outside_current_scope`) |
| `PublicProfileRecord` | `oia_analysis::exposure_audit` | Records an observed public profile anchored to `oia_domain::witness::AccountOrigin` (`Reported` / `Simulated`), `ObservationTimeEnvelope` (`observed_at`, `published_or_issued_at`, `retrieved_at`, `audited_at`), and `SourceCitation` |
| `AccountMatchCandidate` | `oia_analysis::exposure_audit` | Represents a cross-platform account match in `ProposedCandidate`, `AmbiguousRetained`, `UserConfirmedSameOwner`, or `UserRejectedDifferentEntity` state with an `uncalibrated_heuristic_score` (`[0.0, 1.0]`) and uncertainty notice; retains rejected candidates and their review history |
| `InfrastructureHistoryEntry` | `oia_analysis::exposure_audit` | Captures passive DNS (`DnsRecord`), certificate transparency (`CertificateHistory` with validated SHA-256 hex fingerprint, CN, and SANs), or exposed contact (`ExposedContact`) observations within a declared owned domain |
| `PermittedSourceQuery` / `prepare_read_only_source_query` | `oia_analysis::exposure_audit` | Gates read-only passive queries to declared domain or `(service_name, handle)` targets without leaking local scope manifests or private notes |
| `PersonalExposureAudit::evaluate_surface_access` | `oia_analysis::exposure_audit` | Delegates local audit surface access checks to `oia_domain::collaboration::evaluate_proposed_access_decision` |
| `PersonalExposureAudit::flag_source_version_updated` | `oia_analysis::exposure_audit` | Flags profiles, candidate matches, and infrastructure entries citing `(source_id, superseded_source_version)` as needing reassessment |

## Verification

Run the contract suite and the runnable self-audit example:

```bash
cargo +1.99.0 test -p oia-analysis --test exposure_audit_contract --locked --offline
cargo +1.99.0 run -p oia-analysis --example audit-personal-exposure --locked --offline
```
