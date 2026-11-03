# Public link analysis, reversible conflicting-identity resolution, circular citation tracing, and technical development tracking

This implements the dependency-independent `INTEL-02` / `A08` public-link and technical-research kernel slice of [#36](https://github.com/mESmaC/oia/issues/36) in [`oia_analysis::research_links`](../../crates/oia-analysis/src/research_links.rs), with a read-only CLI inspection example in [`inspect-research-links.rs`](../../crates/oia-analysis/examples/inspect-research-links.rs) and contract tests in [`research_links_contract.rs`](../../crates/oia-analysis/tests/research_links_contract.rs).

It covers:
- **`INTEL-02` & `A08` public links:** Typed links across organizations, contracts, grants, facilities, publications, and events (`PublicEntityKind`), preserving pinned source versions, verbatim excerpts, and explicit separation between substantive links and `ObservedCoOccurrence` (`docs/architecture.md` §2.3).
- **`INTEL-04` alternate names & transliterations:** Explicit `AlternateName` entries (`Acronym`, `FormerOrTradeName`, `ProgramOrAwardCode`, `Transliteration { script }`, `TranslatedTitle { language }`) pinned to source versions. Preserves ZWNJ (`U+200C`) and ZWJ (`U+200D`) for multilingual shaping via `oia_domain::text::requires_visible_unicode_escape` while rejecting control and invisible format/bidi characters.
- **`REASON-03` & `A08` reversible conflicting-identity resolution:** Reuses `DecisionState` (`Proposed`, `Confirmed`, `Reversed`) from [`oia_analysis::reasoning`](../../crates/oia-analysis/src/reasoning.rs). Mentions with multiple active (`Proposed` or `Confirmed`) candidates remain `MentionResolution::DisputedIdentity` rather than silently collapsing to one entity; reversing a candidate (`DecisionState::Reversed`) requires a non-empty rationale, preserves the full candidate list on `EvaluatedMention::candidates`, and cleanly unlinks dependent edges when all candidates are reversed.
- **`INTEL-01`, `INTEL-02` & `A08` structural circular citation tracing:** Traces directed `CitesPublication` cycles using iterative Tarjan strongly connected components ($O(V + E)$ exact cycle membership) plus bounded canonical elementary-cycle enumeration, and flags publications that participate in or whose outgoing citation paths lead *exclusively* into a citation cycle without reaching any acyclic publication.
- **`INTEL-02` proposal/patent vs. demonstrated capability:** Tracks scientific/technical developments across grants, contracts, patents, theoretical/simulation papers, and laboratory or field test reports (`TechnicalArtifactKind`), capping proposals, grants, contracts, patents, and simulation studies below demonstrated capability even when their prose claims an empirical demonstration.

## Relationship to other modules (`AGENTS.md` §4)

- **Option (b) boundary with [`oia_documents::lineage`](../../crates/oia-documents/src/lineage.rs) (`#19`):** `oia_documents::lineage` is the sole owner of document-release corroboration verdicts and independent-root counts (`evaluate_claim_corroboration`, `independent_root_count`). `oia_analysis::research_links` stays dependency-independent of `oia-documents` and reports `CitesPublication` edges, directed citation cycles, and shared in-set `derived_from_versions` as *structure only* (`demonstration_source_versions`, `shared_demonstration_lineage_roots`), without computing a parallel independent-corroboration count.
- [`oia_analysis::reasoning`](../../crates/oia-analysis/src/reasoning.rs) (`#35`) evaluates general `AbstractionSpec` steps inside a deductive/correlative `ReasoningGraph`. `oia_analysis::research_links` reuses `DecisionState` and `EvidenceOriginClass` from `oia_analysis::reasoning`, `StopReason` from [`oia_analysis::multilingual`](../../crates/oia-analysis/src/multilingual.rs), and `requires_visible_unicode_escape` from [`oia_domain::text`](../../crates/oia-domain/src/text.rs).

## Method (`oia-public-research-links-v1`)

1. **Source origin, lifecycle, and lineage gating:**
   - Every `ResearchSourceRecord` carries an `EvidenceOriginClass`, a `SourceLifecycleState` (`Current`, `CorrectedBy`, `Retracted`), and optional `derived_from_versions`.
   - Upstream source versions outside the evaluated request are retained in `SourceSummary::external_derived_from_versions` and surfaced in `ResearchLinkLimitation::OutOfSetSourceLineage`.
   - Only non-retracted sources with observational or source-assertion origins (`AcquiredSourceMaterial`, `NormalizedObservation`, `ExtractedClaim`, `UserTestimony`, `ConventionalDerivative`) whose in-set lineage has at least one non-retracted observational/source-assertion root have `counts_as_reported_public_source = true`. Sources with `Simulation`, `AnalyticalInference`, `Forecast`, `PredictiveDerivative`, or `IllustrativeRecreation` origins — or deriving exclusively from such roots — never count as reported public links or demonstrated capability.
   - `Retracted` sources (or sources whose in-set lineage roots are all `Retracted`) produce `LinkEffectiveStatus::InactiveRetractedSource` and `AssessedTechnicalStage::RetractedSourceClaim`, and never contribute to active maturity or confirmed link counts.
   - Both direct and transitive upstream `CorrectedBy` and `Retracted` states propagate `ReassessmentReason` flags through `derived_from_versions` to downstream mentions, links, and technical development items.
2. **Reversible mention resolution (`MentionResolution`):**
   - `ConfirmedUnique`: exactly one candidate is `Confirmed` and all others are `Reversed`.
   - `ProposedOnly`: exactly one candidate is `Proposed` and all others are `Reversed`.
   - `DisputedIdentity`: two or more active (`Proposed` or `Confirmed`) candidates compete for the mention. Links referencing a `DisputedIdentity` mention have `LinkEffectiveStatus::DisputedEndpointIdentity` and are excluded from confirmed funding/contract rollups.
   - `AllReversed`: all candidates are `Reversed`; dependent links become `LinkEffectiveStatus::InactiveReversed`.
   - *Stateless snapshot note:* Because `evaluate_research_links` evaluates a single snapshot of candidate states, if a mention has candidates `A` and `B` and `A` is later reversed while `B` is confirmed, dependent links resolve to `B` without a cross-snapshot diff flag.
3. **Typed public links (`PublicLinkRelation` & `LinkEffectiveStatus`):**
   - Enforces domain endpoint rules per relation (`GrantsFundingTo`, `AwardsContractTo`, `OperatesFacility`, `AffiliatedWithOrganization`, `PublishesOrAuthors`, `LinksGrantOrContractToOutput`, `ReportsOrParticipatesInEvent`, `CitesPublication`, `ObservedCoOccurrence`) and rejects self-citations on `CitesPublication`.
   - `ObservedCoOccurrence` is classified as `LinkEffectiveStatus::CoOccurrenceOnly` and never counted in `confirmed_substantive_link_count`.
4. **Structural circular citation tracing (`CitesPublication`):**
   - Over active `CitesPublication` edges, iterative Tarjan SCC discovery identifies all publications in cyclic strongly connected components in linear time, and bounded DFS enumerates canonical elementary cycles (`[A, B, ..., A]` starting at the lexicographically smallest `OpaqueId`).
   - Any publication that belongs to a cycle, or whose outgoing citation paths lead *exclusively* into a cycle without reaching any acyclic publication (so a paper that cites both a cyclic pair and an acyclic paper is not over-flagged), is listed in `publications_with_circular_dependency`.
5. **Technical maturity assessment (`TechnicalDevelopmentAssessment`):**
   - Items backed by `GrantApplicationOrAward`, `ContractSolicitationOrAward`, `TheoreticalOrConceptPaper`, `PatentApplicationOrGrant`, or `AnalyticalOrSimulationPaper` can never establish `ConfirmedReportedDemonstration` or `UnconfirmedReportedDemonstration`; if their `claimed_stage` asserts a laboratory or field demonstration, the kernel records `DemonstrationCaveat::ProposalOrPatentCannotEstablishDemonstration` or `AnalyticalModelCannotEstablishDemonstration` and sets `has_unsupported_demonstration_claim = true`.
   - `LaboratoryTestReport` items claiming `FieldOrOperationalDemonstration` are capped at `DemonstrationScope::Laboratory` with `DemonstrationCaveat::LaboratoryReportClaimedFieldDemonstration`.
   - Link-confirmed funding/contract IDs (`confirmed_funding_or_contract_ids`) are kept strictly separate from item-declared funding references (`item_declared_funding_or_contract_ids`).

## Bounds, determinism, and failure behavior

Inputs exceeding configured `ResearchLinkLimits` (or containing empty strings, control characters, or invisible Unicode format/bidi controls per `oia_domain::text::requires_visible_unicode_escape`) fail closed with a typed `ResearchLinkError`:
- `MAX_SOURCES = 128`, `MAX_ENTITIES = 256`, `MAX_ALTERNATE_NAMES_PER_ENTITY = 32`
- `MAX_MENTIONS = 256`, `MAX_CANDIDATES_PER_MENTION = 16`, `MAX_LINKS = 512`
- `MAX_DEVELOPMENTS = 128`, `MAX_ITEMS_PER_DEVELOPMENT = 64`, `MAX_FUNDING_REFS_PER_ITEM = 32`
- `MAX_LABEL_BYTES = 256`, `MAX_EXCERPT_BYTES = 4_096`, `MAX_TOTAL_TEXT_BYTES = 1_048_576`
- `MAX_REPORTED_CYCLES = 64`, `MAX_CYCLE_DFS_STEPS = 16_384`

If elementary cycle enumeration reaches `max_reported_cycles` or `max_cycle_dfs_steps`, the report records `ResearchLinkLimitation::CitationCycleSearchTruncated` while preserving exact cycle-membership flagging via iterative Tarjan SCCs.

## Local commands

```text
cargo +1.99.0 run -p oia-analysis --example inspect-research-links --locked --offline
cargo +1.99.0 test -p oia-analysis --test research_links_contract --locked --offline
```

## Validation (2026-11-03, Windows 11 x86_64 MSVC, `rustc 1.99.0`)

- **Tested:** `cargo +1.99.0 test -p oia-analysis --test research_links_contract --locked --offline` (3 contract tests covering conflicting and reversed identity resolution, candidate history retention, co-occurrence separation, structural circular citation detection, proposal/patent vs. demonstration capping, non-observational origin exclusion (`Simulation`, `Forecast`, `AnalyticalInference`, `IllustrativeRecreation`, `PredictiveDerivative`), retracted source exclusion, transitive upstream `CorrectedBy`/`Retracted` reassessment propagation, link-confirmed vs. item-declared funding separation, ZWNJ preservation in multilingual alternate names, out-of-set lineage retention, fail-closed validation, cancellation, and SCC soundness under elementary-cycle truncation).
- **Exercised:** `cargo +1.99.0 run -p oia-analysis --example inspect-research-links --locked --offline` over the synthetic `A08` fixture (Pass 1 disputed identity, Pass 2 reversed conflicting candidate).
- **Workspace baseline:** `cargo +1.99.0 fmt --all -- --check`, `cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings`, and `cargo +1.99.0 test --workspace --locked --offline`.

## Remaining `#36` scope

- User-defined indicator rules, alert trigger rationales, false-positive evaluation records, and collection-planning / research-lead workflows (`INTEL-01`, `INTEL-03`, `A08`).
- Persistent storage (`#10`, `#11`), application service integration (`#14`), and desktop graph/dossier views (`#16`).
