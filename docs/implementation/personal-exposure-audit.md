# Personal Exposure and Owned Infrastructure Audit (`oia-analysis::exposure_audit`)

This implementation reference covers backlog item `#47` (`[P6] Audit personal public exposure and owned infrastructure`), spanning requirement `PRIVACY-02` and the personal-exposure, ambiguous-account-match, and declared-domain-scope slices of acceptance criterion `A18`.

## Scope and Non-Goals

In scope:
- Explicit, versioned user-declared audit scope (`DeclaredAuditScope`, `OwnershipBasis`, `ScopeRevisionInput`) with explicit scope revision history (`revise_declared_scope` tracking `predecessor_scope_version_id` and `revision_rationale`).
- Strict refusal of out-of-scope expansion or fetching (`HandleOutsideDeclaredScope`, `DomainOutsideDeclaredScope`), refusal of transitive pivoting from external profiles (`PrimaryProfileMustBeInDeclaredScope`), and time-bounded authorization verification (`AuthorizationExpiredOrNotYetValid`).
- Public profile and account discovery records (`PublicProfileRecord`, `PublicProfileInput`, `ObservationTimeEnvelope`) reusing `oia_domain::witness::AccountOrigin` (`PlatformIdentifier`, `PlatformName`, `AccountHandle`, `ProfileUrl`) and carrying typed source citations (`SourceCitation`, `ExposureCitationAnchor`).
- Ambiguous cross-platform account matching (`AccountMatchCandidate`, `AccountMatchSignal`, `MatchAssessmentState`, `AccountMatchReviewEntry`, `ACCOUNT_MATCH_UNCERTAINTY_NOTICE`) with explicitly uncalibrated heuristic scores (`uncalibrated_heuristic_score`), candidate-state gating (`ProposedCandidate`, `AmbiguousRetained`) prior to user confirmation (`UserConfirmedMatch`), and retention of rejected candidates (`UserRejectedMatch`) with their full review audit history.
- Passive public-source owned-domain exposure findings (`InfrastructureHistoryEntry`, `InfrastructureArtifactKind`, `DnsRecordType`, `ExposedContactKind`) and source query coverage records (`SourceCoverageRecord`, `SourceQueryCoverage`).
- Policy-gated outbound queries and access evaluation (`OutboundSourceQueryCandidate`, `PermittedSourceQuery`, `LOCAL_ASSESSMENT_PRIVACY_NOTICE`) delegating access evaluation to `oia_domain::collaboration::evaluate_proposed_access_decision` (`evaluate_surface_access`) and refusing credentialed access, non-read-only queries, private note leakage, and out-of-scope targets.
- Audit assembly, source-version reassessment propagation, and summary synthesis (`PersonalExposureAudit`, `ExposureAuditSummary`).

Out of scope (deferred to sibling backlog items):
- Third-party outreach draft preparation and transmission controls (`#45` / `PRIVACY-01`).
- Sanitized export bundle linting and external publication gates (`#46` / `PRIVACY-03`).
- Longitudinal diffing across historical snapshots (`#48` / `TIME-01`, `TIME-02`).

## Key Types and Invariants

| Type / Function | Module | Role |
| --- | --- | --- |
| `DeclaredAuditScope` | `oia_analysis::exposure_audit` | Captures user-declared handles, owned domains, ownership basis (`PersonalSelfOwned` or time-bounded `AuthorizedAssessment`), and scope revision history |
| `PersonalExposureAudit::revise_declared_scope` | `oia_analysis::exposure_audit` | Produces a new versioned scope linked to `predecessor_scope_version_id` with a required `revision_rationale` |
| `PublicProfileRecord` | `oia_analysis::exposure_audit` | Records an observed public profile anchored to `oia_domain::witness::AccountOrigin`, `ObservationTimeEnvelope`, and non-empty `SourceCitation` anchors |
| `AccountMatchCandidate` | `oia_analysis::exposure_audit` | Represents a cross-platform account match in `ProposedCandidate`, `AmbiguousRetained`, `UserConfirmedMatch`, or `UserRejectedMatch` state with an `uncalibrated_heuristic_score` and uncertainty notice; retains rejected candidates and their review history |
| `InfrastructureHistoryEntry` | `oia_analysis::exposure_audit` | Captures passive DNS (`DnsHistory`), certificate transparency (`CertificateTransparency`), or exposed contact (`ExposedContactMetadata`) observations within a declared owned domain |
| `PermittedSourceQuery` / `evaluate_surface_access` | `oia_analysis::exposure_audit` | Evaluates surface access via `oia_domain::collaboration::evaluate_proposed_access_decision` and gates read-only passive queries to declared targets without leaking local scope or private notes |
| `PersonalExposureAudit` | `oia_analysis::exposure_audit` | Enforces resource limits (`ExposureAuditLimits`), scope boundaries, source-version reassessment propagation (`record_source_revision`), and summary synthesis (`ExposureAuditSummary`) |

## Verification

Run the contract suite and the runnable self-audit example:

```bash
cargo +1.99.0 test -p oia-analysis --test exposure_audit_contract --locked --offline
cargo +1.99.0 run -p oia-analysis --example audit-personal-exposure --locked --offline
```
