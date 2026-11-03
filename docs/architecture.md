# OIA: architecture and data contracts

Version: 0.1. Date: 2026-11-01. Status: proposed implementation architecture accompanying the [product specification](spec.md).

Rust and selective Lean verification are established direction. The process layout, storage choices, interfaces, and defaults below are proposed implementation decisions. They are concrete enough to guide a first implementation without implying that dependencies or providers have already been selected or validated.

## 1. Architecture boundaries

### 1.1 Proposed deployment model

Begin with a local-first application: a Rust domain/service runtime, local evidence storage, background workers, and a UI connected through typed interfaces. Basic document import, case editing, offline search over downloaded material, and report export must work without an OIA account. Provider integrations and remote AI require their respective access and connectivity.

Separate these responsibilities even if the first release packages them in one process:

1. **Presentation:** Workspace navigation, globe/sky/charts, document/media readers, HUD, and review interfaces.
2. **Application services:** Cases, permissions, queries, source registration, evidence ingestion, reports, forecasts, and job orchestration.
3. **Domain core:** Typed records, version relationships, time/geometry contracts, calculations, and validation, with no UI or provider dependency.
4. **Adapters:** Data providers, model providers, hardware, storage, indexing, and import/export formats.
5. **Workers:** Bounded document parsing, OCR, media analysis, model inference, and other resource-intensive tasks.

A shared hosted service and multi-user synchronization can be added through the same application contracts. Remote clients must not gain direct access to local devices, files, or arbitrary network destinations by connecting to a shared case.

The UI/rendering ADR must resolve a Rust-native renderer versus an adapter around an established rendering stack. Reusing Cesium from prior work is an option to evaluate, not an assumed decision or a requirement to port its engine into Rust. Production analytical logic must remain in Rust; external numerical/media/model dependencies need version and capability records.

### 1.2 Proposed Rust workspace

These names describe intended boundaries; creating crates is outside this documentation task.

| Boundary | Responsibility |
| --- | --- |
| `oia-domain` | IDs, evidence types, version graph, claim/review states, domain validation |
| `oia-time-geo` | Time intervals, coordinate/height references, transforms, observation geometry |
| `oia-store` | Transactions, migrations, blob references, case access, durable jobs |
| `oia-ingest` | Source registry, adapters, normalized batches, freshness and retention |
| `oia-query` | Text/spatial/time queries, coverage envelopes, saved queries |
| `oia-analysis` | Versioned analysis definitions, dependency graph, deterministic calculations |
| `oia-models` | Inference-provider adapters, citations, model/run metadata, evaluation hooks |
| `oia-media` | Original/derivative media handling, transforms, calibration, clip records |
| `oia-devices` | Explicit hardware sessions, capability negotiation, video/telemetry/audio |
| `oia-forecast` | Forecast vintages, scenarios, outcome resolution, evaluation |
| `oia-market` | Stable instruments, calendars, quotes, corporate actions, point-in-time release vintages |
| `oia-export` | Reports, recreation prompts, manifests, bundle validation/import |
| `oia-app` | Composition, commands/queries, UI boundary, optional API/CLI/MCP adapters |
| `proofs/` | Lean specifications, proof sources, correspondence notes, reference fixtures |

Dependencies point toward the domain core. Core types must not depend on a renderer, HTTP client, database engine, model service, or vendor SDK. Provider-specific errors and payloads terminate at adapter boundaries; normalized results retain links to the original source payload where permitted.

## 2. Identity, versions, and evidence

### 2.1 Common envelope

Every persisted domain object must carry:

- Stable `id`, immutable `version_id`, `schema_version`, object type, creation time, and creating actor/process.
- Owning workspace/case association or library visibility, plus access policy references.
- Predecessor/supersession links where revised; versions are not reused or silently overwritten.
- Provenance links to source records or declared manual/generated origin.
- Applicable quality, uncertainty, and review attributes; unknown values are explicit, not invented defaults.

Use opaque IDs independent of mutable names, URLs, ticker symbols, and database row positions. Content hashes identify retained bytes; they do not replace semantic IDs. Two publishers distributing the same bytes remain distinct source records even if their blobs are deduplicated.

### 2.2 Principal objects

| Object | Required content beyond the common envelope |
| --- | --- |
| `Source` | Provider/listing references, access method, capability set, attribution, per-use conditions, credential reference, health and coverage metadata |
| `SourceRecord` | Exact provider identity/version, source times, acquisition attempt, source URL/reference, retained blob or explicit omission reason |
| `Artifact` | Blob hash, media type, byte length, storage status, origin class, source/derivative links, processing history |
| `Document` | Source record, bibliographic metadata, edition/release links, extracted-text versions and page/region anchors |
| `Observation` | Observed/reported property, value/unit, time and geometry, instrument or reporter, quality, measurement/estimation status |
| `Event` | Proposed type, temporal/spatial bounds, linked observations/accounts, grouping rationale and split/merge history |
| `Claim` | Exact asserted proposition, claimant, supporting span or analysis, temporal scope, evidence links, assessment/review history |
| `Investigation` | Question, context, evidence references, hypotheses, notes, saved queries, leads, visibility and collaborators |
| `AnalysisRun` | Method/version, input version IDs, parameters, model/calibration versions, runtime environment, outputs, status and reproducibility limits |
| `Forecast` | Origin/cutoff/horizon, outcome definition, target, baseline, assumptions, comparison set, distribution/scenarios, model version and resolution rules |
| `Device` | Explicit registration, adapter, capabilities, local access reference, calibration, session state and health |
| `Report` | Structured blocks, statement-to-evidence links, selected versions, draft/review/published state, export history |
| `Review` | Target version, reviewer, assessment, rationale, cited evidence, creation time and supersession |

Additional typed records cover instrument/organization identity, source lineage, relationships, annotations, calibration, telemetry samples, alert rules, jobs, outcome resolutions, and export manifests. They must follow the common envelope rather than encode state in display strings.

### 2.3 Evidence classes and graph

Use explicit origin classes: acquired source material, user testimony, normalized observation, extracted claim, analytical inference, forecast, conventional derivative, predictive derivative, simulation, and illustrative recreation.

Relations must specify meaning: `derived_from`, `quotes`, `republishes`, `supports`, `contradicts`, `possible_same_event`, `supersedes`, `calibrated_by`, or another versioned relation type. A relationship edge has its own evidence and review status. A co-occurrence edge must not be rendered as established organizational affiliation or causation.

Lineage can contain citation cycles; analysis derivation dependencies must be acyclic. Copied accounts remain linked to their common origin. Corroboration metrics must expose their definition of independence rather than simply count nodes or publishers.

Version replacement, source correction, retraction, or changed calibration triggers a dependency walk that marks downstream results `needs_reassessment`. It does not erase prior versions or silently recompute issued reports/forecasts. A re-run produces new objects linked to the originals.

### 2.4 Claim and review states

Claim assessment and review status are separate:

- Assessment: supported, contradicted, unsupported-by-available-evidence, materially-misleading, disputed, unresolved.
- Review: unreviewed, in-review, reviewed, challenged, superseded.
- Availability: available, reference-only, missing, restricted, removed, expired, corrupt.

An unsupported claim is not necessarily false. A reviewed claim is not necessarily supported. A removed page is not necessarily a retraction. Claims about what a person said must remain distinct from the truth of the quoted assertion.

## 3. Time, geometry, and numerical contracts

### 3.1 Time model

Represent exact instants, bounded intervals, calendar-only dates, approximate dates, and unknown times. Preserve original text and precision. Date-only historical records must not appear as accurately observed midnight timestamps.

Normalize ordinary timeline comparisons to UTC while preserving the original timezone, offset, and ambiguity. Calendar filtering uses half-open intervals `[start, end)`; instantaneous samples are explicit points. Scientific time values must carry their time scale and conversion metadata. Leap seconds and UTC/TAI/TT/TDB conversions must be handled through a versioned, tested facility rather than naive string arithmetic.

Separate at least:

- Event/capture interval and source-clock uncertainty.
- First public availability or publication time, with evidence and precision.
- Provider revision and retrieval times.
- OIA ingestion and analysis times.
- Forecast origin, information cutoff, target interval, and resolution date.

For historical forecasts, admissibility must distinguish public availability from OIA's own knowledge. Prospective evaluation uses only versions ingested and usable at forecast issuance. Retrospective reconstruction may use externally documented historical availability, but must identify that mode and its limitations. Unknown availability cannot silently qualify a record for a strict cutoff test.

A clock alignment record must contain offset/drift estimates, reference clock, method, validity interval, and uncertainty. Loss of synchronization must degrade confidence in alignment rather than align frames by receipt time as if capture times were known.

### 3.2 Geometry and measurements

Geographic input must state coordinate system and units. WGS84 longitude/latitude is the interchange default; altitude must separately identify ellipsoid, orthometric/MSL, above-ground, barometric, or unknown reference. Barometric altitude must not be treated as physically corrected by merely adding geoid undulation.

Represent locations as points, regions, tracks, viewing cones, footprints, or uncertainty bounds. A gazetteer centroid must be labeled as an approximate location. Support dateline-crossing regions, poles, negative heights, missing height, and camera orientation changes explicitly.

Celestial positions must retain reference frame, epoch/equinox as applicable, time scale, observer, and geometric/apparent status. Ephemerides, Earth-orientation tables, geoid grids, and calibrations are versioned dependencies.

Missing numeric values are absent with a reason, not zero or a plausible visual default. Reject non-finite values at persistence and API boundaries. Bound inputs before expensive calculations. Store units with quantities or enforce them through typed contracts; financial values needing exact decimal arithmetic must not rely on binary floating-point by default.

Each numerical algorithm must declare tolerances, supported domain, and treatment of uncertainty. A small root-finding tolerance must not be advertised as physical prediction accuracy.

## 4. Source adapters and ingestion

### 4.1 Source registry

A source registration must declare provider identity, source class, retrieval method, permitted endpoint family, credential reference if needed, attribution, data-use conditions, supported capabilities, expected cadence, geographic/temporal coverage, limits, and schema/adapter version.

Supported capabilities may include catalog search, historical query, live polling, streaming, media retrieval, and hardware telemetry. A source can support some without the others. The application must not manufacture history from a currently live feed.

Source operations must receive an explicit bounded query, cancellation/deadline, and resource budget and return a normalized batch plus source/version/coverage metadata. Network transport is injected so fixtures can exercise the same parser and normalization logic offline.

### 4.2 Result envelope

Every query result must include:

- Request/query ID, normalized criteria, cutoff/time context, and result version.
- Records with provenance and unit/time/geometry semantics.
- Coverage: requested versus actually queried area/time/providers, pagination/truncation, and known blind spots.
- Freshness and source status, including cached/stale use.
- Per-source failures, stable machine-readable error codes, retry information, and safe human explanations.

Distinguish `success`, `partial`, `empty`, `unavailable`, `unauthorized`, `rate_limited`, `malformed`, and `cancelled` outcomes. Completeness is a separate attribute: an apparently successful request may still cover only a subset of a catalog. Credentials, private endpoint details, and raw provider errors must not leak into public reports.

### 4.3 Ingestion lifecycle

Use: register source → schedule bounded acquisition → validate payload → retain permitted bytes/reference → normalize records → commit version/provenance → index → publish domain event.

Deduplicate identical acquisition attempts without conflating distinct releases. Commit source identity, blob reference, and normalized records atomically where possible; staged blobs require cleanup after failed transactions. Reprocessing retained input creates a new extraction/normalization version and retains the old one.

Provider-specific budgets must cover concurrency, requests/time, response size, retries/backoff, and retention. Apply backpressure and bounded queues. Cancellation, supersession, or teardown must prevent late responses from publishing into a newer session. Retries must be idempotent.

Source health must distinguish unconfigured, connecting, available, degraded, stale, unavailable, and disabled states. Health changes and actual data observations remain separate records.

### 4.4 Camera-specific admission

Apply `CAM-01` exactly: ordinary viewing through existing listings and supported interfaces, without extra access attempts. Intentional publication by the owner is not a universal prerequisite. Record listing provenance, access method, actual view, and per-use conditions; exclude home interiors.

A listed camera can be cataloged separately from enabling automated recording. Use source-specific frame rates, concurrency, and retention. No-source, stale still, fallback street imagery, and synthetic placeholders must remain different from live video.

## 5. Storage, indexing, and retention

### 5.1 Proposed initial stores

Use a transactional local relational store, initially SQLite as a proposed baseline, for cases, versions, references, permissions, and durable job metadata. Store large originals and derivatives in a managed content-addressed blob directory. Start with full-text indexing appropriate to the selected engine; spatial and vector indexes are replaceable accelerators.

An embedding index is derived state, not the authoritative record. Record model/dimensions/version and rebuildability. Queries must permission-filter results and snippets; cross-workspace deduplication must not reveal private content or its existence.

A hosted deployment may use another relational/spatial store and object storage, but must preserve contracts and import/export semantics. Do not choose a distributed architecture before workload measurements justify it.

### 5.2 Retention and deletion

Separate routine buffers, temporary extraction/cache data, pinned case evidence, and issued report snapshots. Each class has configured limits and expiry behavior. If a license or user action requires deletion of retained bytes, keep only metadata that may legitimately remain and mark affected references unavailable.

An immutable version means it is not edited in place; it does not prohibit deletion. Tombstones and dependency notices must support deletion without pretending the original is still available. Garbage collection must respect live references and retention rules, with staged cleanup recoverable after interruption.

Backups must include database state, referenced blobs, model/calibration manifests where applicable, and an integrity index. Restore must validate consistency and report missing objects. Credentials must not be embedded in ordinary investigation backups or exports.

## 6. Jobs, analysis, and model execution

### 6.1 Durable jobs

Job states are queued, running, succeeded, partially-succeeded, failed, cancelled, or superseded. Paused/retry-waiting may be represented explicitly if supported. Each attempt must record its input versions, execution lease, resource limits, progress, error, and outputs.

Use bounded workers for ingestion, parsing, indexing, inference, and media tasks, with independent scheduling for device telemetry. Workers must honor deadlines/cancellation and not publish after lease revocation. Restart recovery must not duplicate side effects or mark incomplete work successful.

Return partial output only when the output contract declares its extent and validity. Publishing a run's output and its provenance must be atomic. Completion events must be durable enough to recover UI state after a restart.

### 6.2 Analysis contracts

An analysis definition must specify accepted input types, method/version, parameters and units, output types, preconditions, resource limits, determinism, and validation evidence. A run pins exact input/calibration/model versions and records any random seed, runtime, relevant dependencies, and hardware sensitivity.

Source updates invalidate dependent runs for reassessment; they do not mutate a frozen run. Visual transformations and predictive transformations must declare different output classes. Analysis on predictive derivatives must not silently promote invented detail into independent observations.

### 6.3 AI provider abstraction

Allow local and remote providers through a common interface. No single provider is mandatory for core case storage or original-document viewing. Record model/provider identity, configuration/prompt-template version, input selection, output, token/resource usage when available, and truncation/coverage.

The workspace must expose when material is sent to a remote provider and honor configured workspace data-handling rules. Secrets and private fields outside the selected task must be excluded. Retrieved content cannot grant tool permissions or override task instructions.

Generated citations must reference allowed input versions and valid anchors. Structural citation validation can detect nonexistent anchors; semantic support still needs assessment. Distinguish generated, structurally validated, and human-reviewed status. Retrieval and generation must report insufficient material rather than fabricate a citation.

### 6.4 Forecast records

Freeze outcome definition, target universe, time horizon, information cutoff, sources, features, model, comparison set, assumptions, probability/interval interpretation, baseline, and resolution method at issuance. Store revisions as linked forecasts and score original vintages separately.

Define outcome resolution independently from model predictions. Later corrections to outcomes must create a new resolution version and evaluation run. Unresolved, censored, invalidated, and resolved outcomes remain distinguishable.

Backtest manifests must record sampling, split dates, embargo/gap rules where applicable, missing-data treatment, corporate actions, transaction assumptions if relevant, and tuning lineage. A language model's post-cutoff knowledge is a potential leakage source even when retrieved documents are filtered.

## 7. Application interfaces and events

UI, CLI, API, and optional MCP/function-calling clients must call the same application services. Define read-only queries separately from state-changing commands. Client visibility does not confer capabilities absent from its authorization context.

Initial interface families:

- Case/library operations: create case, import source, search, inspect lineage, annotate, assess claim, save query.
- Observation operations: query area/time, register device, open session, bookmark event, inspect calibration.
- Analysis operations: validate inputs, start/cancel run, inspect progress/results, request reassessment.
- Forecast operations: prepare scenario, issue immutable forecast, revise, resolve outcome, evaluate.
- Report operations: assemble draft, validate citations, preview export, export bundle, import validated bundle.

Commands need request IDs and idempotency where retries could duplicate effects. Updates need expected-version checks; concurrent edits must not silently overwrite accepted versions. Transport schemas must be versioned and share generated types or equivalent compatibility checks.

Domain events include source acquired, source corrected, artifact available, analysis completed, dependency stale, candidate detected, review changed, forecast issued/resolved, and report exported. Events must carry IDs/versions and minimal context, not unbounded media payloads or secrets.

A subscription must support bounded buffering and a resynchronization mechanism when the client misses events. Device sample time, source event time, and event-delivery time must not be conflated.

## 8. Media, devices, and rendering

[Still-image forensic cue kernel: implemented scope and integration limits](implementation/image-forensics.md).

Video/audio packets may be transient; retained evidence clips must have manifests referencing original bytes, sample/frame timing, drop information, and source session. HUD and annotation rendering belongs in separate derivatives. A camera player's apparent smoothness must not conceal missing frames in analysis.

Hardware sessions are explicitly registered and scoped to declared capabilities. Local endpoints may be allowed for a registered device without allowing arbitrary remote-source requests into the local network. Device configuration changes, disconnections, and calibration invalidation must be recorded.

Rendering consumes domain state; it must not write calculated display defaults back into observations. Camera center, ground footprint, physical scale, or trajectory estimates require explicit geometry/model inputs. Lower graphical quality must not alter scientific calculations.

Globe, sky, and solar system transformations must use shared time/geometry services. True-scale and illustrative views differ in rendering transforms only; measurement readouts must retain physical units and identify any approximate calculation.

## 9. Access, collaboration, and local operation

Roles are proposed as owner, editor, reviewer, and viewer. Ownership controls sharing/deletion; editors add case material; reviewers submit assessments; viewers inspect allowed content. Final permissions need a capabilities matrix, with access to sensitive artifacts independent of permission to see a case title.

Authorization must apply at queries, blob reads, exports, subscriptions, model-input assembly, and shared indexes. Report redaction must remove hidden payloads and metadata, not merely hide a UI element. Server-side jobs act with the initiating workspace's authorized scope.

Offline edits must retain actor, base version, and local clock quality. Synchronization exchanges versioned changes and content manifests, verifies hashes, and exposes conflicts requiring a merge. Exact-byte equality can deduplicate storage; semantic claims must not auto-merge solely from similar text.

Use OS-protected credential storage where available. A localhost service must authenticate its UI clients and resist cross-origin requests; loopback binding alone is not authentication. Remote deployment requires authenticated encrypted transport and explicit workspace isolation. Device and provider credentials remain outside case exports.

Untrusted files require resource-bounded parsers and isolated workers where appropriate. Source adapters must constrain destinations, redirects, response sizes, and content types; registered local device access is a distinct capability. Imported bundles cannot choose executable adapters, arbitrary credential headers, or filesystem paths outside the bundle destination.

## 10. Exports and portability

[D05 / ADR 0006](adr/0006-d05-report-and-bundle-formats.md) selects the initial
formats. The [report and bundle contract](contracts/report-export.md) defines
snapshot, citation, redaction, bounded import, and atomic publication behavior.
These are implementation designs, not implemented export/import capabilities.

### 10.1 Report and prompt formats

Selected initial outputs are Markdown and self-contained HTML reports, plus a JSON investigation directory bundle. PDF is a later output once render QA is available. Plain-text recreation prompts must include evidence/assumption sections and a reference-asset manifest; actual asset inclusion follows use conditions.

Statement anchors must link to exact source versions. External URLs can be included, but a report must state whether cited material is bundled, locally referenced, or externally dependent. Offline opening must not silently fetch remote resources or leak case details.

### 10.2 Investigation bundle

Selected initial portable directory layout (wire schema pending shared domain contracts):

```text
manifest.json
records/          # Versioned domain records and relation records
assets/           # Permitted original/derivative bytes, content-addressed
reports/          # Rendered report versions
reproduction/     # Run manifests, parameters, calibration and model references
```

The manifest must declare format/schema versions, package ID, creation time, entry inventory with hashes/sizes/types, included/omitted assets and reasons, licensing/attribution, and required analysis capabilities. A checksum is not a signature; signatures, if later added, must name the signer and trust basis.

Import stages must validate limits, paths, entry uniqueness, types, references, and hashes before making the case visible. Reject traversal, symlinks escaping the destination, decompression bombs, unsupported required schemas, and duplicate conflicting IDs. Generate local mappings for imported IDs without losing origin IDs. Failed import leaves existing cases untouched.

## 11. Rust and Lean verification boundary

[ADR 0004](adr/0004-d13-lean-correspondence-target.md) and the [proof manifest](../proofs/README.md) document the implemented Lean models and the first finite Rust cutoff correspondence fixture, including assumptions and remaining gaps.

Candidate proofs include interval selection, bounded searches, ordering/deduplication, structured privacy filtering, diff reconstruction, soundness of a restricted rule engine, and selected coordinate/abstraction properties.

Each proof-backed component must have:

1. A written specification and precise input domain.
2. A Lean model, named theorem statements, explicit assumptions, and a pinned toolchain.
3. A link from the model to a particular Rust module/revision and the method used to establish correspondence.
4. Differential/property tests or a stronger refinement/code-generation bridge as appropriate.
5. Numeric and implementation limitations, including rounding, overflow, approximations, and external dependencies.

Candidate claims must be labeled at their actual level: model property proved; Rust behavior tested against the model; or implementation correspondence formally established. Passing tests alone do not constitute a formal proof of the Rust implementation. Proofs over ideal real arithmetic do not automatically cover floating-point calculations.

Production proof artifacts must contain no unacknowledged admitted goals or unproved project-specific axioms. Changes to the specification, Rust implementation, or numeric representation require the correspondence claim to be revisited. Model validity, source truth, detector quality, and forecast accuracy are empirical questions outside these proofs.

Lean runs in development/CI and need not be linked into the deployed executable. An application may expose a proof manifest without implying every displayed conclusion is formally verified.

## 12. Selective port from God's Eye View

Use the pinned inspection in [prior-art.md](prior-art.md). Preserve useful behavior rather than its full UI or legacy compatibility surface.

For each selected port, record source revision/files, notices, data licenses, behavioral contract, known limitations, fixtures, and independent reference expectations. Then implement the Rust adapter/algorithm behind OIA contracts, compare behavior, document intended differences, and integrate its acceptance scenario.

Priorities to evaluate include provider normalization/request handling, flight-record state transitions, weather time selection, camera source provenance, geoid/coordinate semantics, satellite pass calculations, structured queries, and bounded scene/asset import patterns.

Do not carry rendering defaults into measured evidence, live-only assumptions into historical queries, or third-party assets under the code's MIT license. Existing tests are useful regression inputs, not a guarantee that every legacy behavior is correct. Current source/API availability must be verified when an adapter is implemented.

## 13. Operational quality

Structured logs and metrics must identify request/job/source versions without exposing credentials or private case content by default. Track provider latency/errors, queue depth, dropped samples, source freshness, analysis duration, storage growth, and model usage.

Configuration must support bounded concurrency, cache/buffer limits, per-source polling, model budgets, retention, and effect quality. Source/model exhaustion should degrade that capability with an explanation while preserving case editing and access to retained evidence.

Schema migrations must be versioned, tested against fixture stores, and backed up before destructive conversion. Unknown future schemas must produce an explicit compatibility error. Backup/restore and crash recovery are release gates, not solely deployment documentation.

Numerical performance and resource targets, acceptance fixtures, and decisions that must precede implementation are defined in [delivery and verification](delivery.md).
