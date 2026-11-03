# OIA: delivery and verification

Version: 0.1. Date: 2026-11-01. Status: proposed implementation sequence and acceptance plan for the [product specification](spec.md) and [architecture](architecture.md).

This plan sets scope and dependencies, not calendar commitments. Requirement IDs refer to intended behavior. No acceptance scenario below has been executed against an OIA implementation; the current work is documentation.

Track implementation in the [GitHub backlog](backlog.md), which links the Project, phase epics, milestones, and work packages. GitHub issue state is the live progress record; this document defines the delivery and validation expectations.

## 1. Delivery strategy

Ship coherent investigation workflows on a shared evidence foundation. A phase can introduce an early subset of a later capability, but it must not claim the whole capability is complete until its requirements and acceptance criteria pass. An unavailable provider must be reported as unavailable rather than replaced with unlabeled sample data.

### P0: evidence foundation

Build the Rust workspace boundaries, common records, version graph, time/geometry primitives, transactional storage, bounded job lifecycle, source registry, application interfaces, and minimal workspace shell. Establish original/derivative separation, citation anchors, migration/backup fixtures, and private-by-default cases.

Exit: a fixture can be imported, queried, annotated, versioned, exported, reimported, and restored after interruption without losing provenance. A source correction marks a dependent result stale. A cancelled job cannot overwrite a newer result. Complete the initial UI/runtime, storage, and portability ADRs before their implementation becomes coupled to the domain core.

### P1: document and news research release

Deliver local document import, one verified remote acquisition adapter, searchable text/OCR, cited AI drafts through at least one evaluated model option, document revisions, claim/source lineage, news event grouping, and an editable report. Introduce report and portable-bundle export before collaborative hosting.

The first usable release must let a researcher answer a bounded question from a small collection, inspect every substantive generated statement, correct it, and export an auditable result. It must show failed/partial extraction, duplicate coverage, conflicting claims, and corrections. The terminal visual system must be recognizable and usable without a globe being required for every task.

Initial scope is a declared corpus/source set, not comprehensive worldwide collection. Geography can be recorded as metadata before the full map experience. Remote AI can be unavailable without preventing import, reading, editing, and export of retained material.

### P2: UAP, geographic, and space reconstruction

Add linked globe, sky, solar system, historical timeline, witness capture, and event queries over selected weather, flight/orbital, imagery, and camera-catalog sources. Implement source-specific historical availability, appropriate ephemerides, and exact source selection rather than assuming every live source has an archive.

Exit: a fixed event can be explored across documents, map, sky, weather, imagery, and observations without losing context. Missing data remains explicit. A historical sky result is checked against an independent reference. Earliest-known-report and source-lineage views preserve their coverage qualifications.

### P3: media and connected observation

Add original/processed media comparison, initial forensic methods, predictive upscaling labels, recreation-prompt export, camera anomaly candidates, radio streams/receivers, one supported drone video/telemetry path, and a personal observation station. Publish a tested hardware/source compatibility list.

Exit: a captured event passes from live reception through timestamped evidence and analysis into a report. Stream interruptions, clock offsets, source fallbacks, moving-camera artifacts, and missing hardware capabilities are represented correctly. Detector performance is measured on the declared domain; no universal detection claim is made.

### P4: deeper analysis and collaborative fieldwork

Deliver composed reasoning, competing hypotheses, entity/link analysis, collection planning, research leads, calibration, multi-observer reconstruction, multilingual/public-record research workflows, and expanded reproducibility. Add shared roles, offline conflict handling, and validation/simulation tools.

Exit: a reviewer can inspect the reasoning chain, reproduce a supported analysis, challenge a claim, and merge a contribution without overwriting originals or another user's conclusions. Calibrated multi-view reconstruction reports uncertainty and rejects unsupported measurements.

### P5: markets and general forecasts

Add point-in-time market data, event studies, exposure relationships, conditional scenarios, historical analogue retrieval, issued forecast records, outcome resolution, and evaluation dashboards. Lock targets and resolution criteria before prospective evaluation.

Exit: historical evaluation excludes known future information, original forecasts survive revisions, failures remain visible, and comparisons against baselines are reproducible. If a model fails to improve on its baseline or is uncalibrated, the UI must reflect that limitation; useful research/scenario tools can still ship without predictive superiority claims.

### P6: personal privacy and coverage expansion

Complete personal exposure, file metadata cleanup, infrastructure observation, and extended local situation/change monitoring. Expand providers, instruments, hardware, and model choices through the same adapter and evaluation gates. Improve scale only with measured workloads.

Exit: privacy tools preserve originals, document the limits of supported formats and identity matches, and produce verifiable changes. New integrations pass source-contract and failure tests rather than merely appearing in a catalog.

### Requirement coverage and primary delivery

| Requirement family | Primary stage | Acceptance scenario |
| --- | --- | --- |
| CASE-01–CASE-05 | P0/P1 | A01 |
| UX-01–UX-05 | P1, extended by each view | A02 |
| DOC-01–DOC-05 | P1 | A03 |
| GEO-01–GEO-05 | P2 | A04 |
| SPACE-01–SPACE-05 | P2 | A05 |
| NEWS-01–NEWS-05 | P1 | A06 |
| REASON-01–REASON-04 | P4 | A07 |
| INTEL-01–INTEL-04 | P4; source lineage begins P1 | A08 |
| CAM-01–CAM-05 | P3; catalog begins P2 | A09 |
| MEDIA-01–MEDIA-04 | P3 | A10 |
| RADIO-01–RADIO-03 | P3 | A11 |
| DEVICE-01–DEVICE-04 | P3 | A12 |
| FIELD-01–FIELD-04 | P2–P4 | A13 |
| MARKET-01–MARKET-05 | P5 | A14 |
| FORECAST-01–FORECAST-04 | P5 | A15 |
| COLLAB-01–COLLAB-03 | P4; local reports begin P1 | A16 |
| REPRO-01–REPRO-03 | P1 foundation, P4 replay breadth | A17 |
| PRIVACY-01–PRIVACY-03 | P6; local alerts may begin P2 | A18 |

## 2. Acceptance scenarios

Each scenario is a required verification category, not a single test that establishes an entire capability. Its implementation checklist must cover every referenced requirement. Use fixture-driven tests for deterministic contracts, evaluated datasets for model quality, and end-to-end checks for user workflows.

### A01: persistent investigation and asynchronous work

Create a case with approximate location/time, import records, save a query, annotate, and switch among available views. Restart and verify the same context and version links. Cancel and replace an acquisition while the old adapter deliberately completes late; only the current operation may update the selected view. Verify partial results, scoped search, retries, and permission-aware results.

### A02: cinematic but usable workspace

Complete import, evidence inspection, event selection, report editing, and export with keyboard navigation. Verify visible focus, contrast, non-color status cues, reduced motion/transparency, quiet defaults, and readable layouts over bright imagery. A lower-effects setting must preserve data/analysis results. Historical and live modes must remain distinguishable; technical readouts must match domain state.

### A03: cited document research

Use a corpus containing a scanned document, unreadable page, duplicate bytes under two sources, revised paper, differently redacted release, and conflicting claims. Verify extraction coverage, anchored citations, exact source-version access, distinct releases, and citation invalidation after correction. A partial summary must identify omitted pages. Inject instructions into source text and verify they cannot initiate tools or change source access.

### A04: geographic reconstruction

Use a known historical event with uncertain witness time, a dateline-crossing query, incomplete tracks, stale imagery, and missing camera history. Verify bounds/units, source coverage, reversible event clustering, distinct report/source/event counts, and observation versus interpolated positions. A missing feed must not become a clear-sky or empty-scene conclusion. Match physical calculations to independently established references and tolerances.

### A05: space observations and planetary context

Import a scientific image and a processed composite with known instrument metadata. Verify target/pointing, exposure, processing, and preview provenance. Scrub Earth/sky/solar views through a reference date and compare positions/orientation with an independent ephemeris implementation or authoritative reference data. Verify reference frames, supported date ranges, true/illustrative scale labeling, and live/delayed/archive/simulated media states.

### A06: news lineage and claim assessment

Provide syndicated articles, an original account, an independent report, a correction, a deleted page, an attributed allegation, and a misleading headline. Verify source independence, uncertainty in grouping/location, claim assessment criteria, article-version evidence, and correction propagation. A deleted page must not be auto-marked retracted. Publisher summaries must reveal sample/method; news article counts must not masquerade as crime rates.

### A07: inspectable reasoning

Compose abstraction, correlation, deduction, and extrapolation over fixed evidence. Inspect every intermediate input, premise, rule, parameter, and uncertainty. Modify a premise and verify downstream reassessment. Reject unsupported unit/domain inputs. A generated inference must retain its type through later steps. Competing hypotheses and conflicting evidence must remain visible rather than being forced into one answer.

### A08: additional intelligence methods

Trace circular citations and organization/grant relationships with conflicting identity matches. Verify reversible entity decisions and typed relationships. Compare imagery containing a seasonal change and a real construction change. Review a multilingual passage against its original. Draft a records request without submitting it. Trigger an indicator rule and verify its rationale, source limits, false-positive evaluation record, and linked research lead.

### A09: camera source and detector integrity

Use controlled test feeds representing ordinarily available sources, restricted sources, home-interior exclusions, frozen frames, still/video substitutions, timing drift, and repeated rebroadcasts. Verify access policy and no alternate access attempt after restriction. Test daylight/night/weather/scene shifts, buffer expiry, pre/post capture, deduplication, and review history. Measure false alerts per camera-hour, labeled-event recall, latency, processing cost, and missing coverage; do not substitute raw classifier accuracy for operational performance.

### A10: media provenance and reconstruction prompts

Inspect known originals and transformed copies; run supported forensic operations, upscaling, and frame alignment. Verify saved settings, source hashes, overlays, and comparison views. Generated detail must not become a measurement or independent detection feature. Export a prompt from conflicting accounts; verify assumptions, variants, source notes, permitted asset manifest, and uncertain scale. Imported recreations and exported images/reports must retain visible labels.

### A11: radio sources and audio evidence

Exercise an internet stream, a simulated receiver, and an actual supported receiver before claiming hardware support. Verify capability-dependent controls, true RF versus audio spectra, source/receiver locations, delayed playback, disconnection, and no transmission command in the initial interface. Preserve clip timing and uncertain transcription. An unavailable frequency must not be represented as receivable audio.

### A12: drone video and telemetry

Replay a recorded flight and exercise the supported live connection. Introduce clock drift, dropped frames, missing telemetry, and independent gimbal motion. Verify HUD fields/units, stale indicators, calibrated footprint limitations, responsive telemetry during analysis, and unaltered originals. Attach a bookmark to a case and later link onboard footage without overwriting earlier evidence. The initial adapter must not emit flight-control commands.

### A13: fieldwork and multi-observer research

Capture an initial witness account before displaying explanations, then retain revisions. Calibrate two known views, reconstruct a reference trajectory with declared error bounds, and test a degenerate geometry that cannot uniquely resolve range. A calibration invalidated by camera movement must be flagged. Record offline observations and conflicting edits, then synchronize without erasing either. Keep simulations labeled throughout search, reports, and evaluation.

### A14: market analysis and point-in-time prediction

Use data with trading closures, corporate actions, delayed quotes, an instrument rename, a revised economic release, and news published after an event. Verify stable identity, timestamp selection, adjustments, benchmark calculations, event exposure citations, and strict information cutoffs. Save scenario assumptions and forecast vintages; compare actual outcomes with intervals and baseline forecasts. Separate event occurrence from conditional market response and expose leakage/uncalibrated results.

### A15: historical analogue forecasting

Use comparable events, counterexamples, dependent episodes, documented non-response, and incomplete follow-up. Verify editable matching criteria, source lineage, institutional differences, censored cases, and scenario dependencies. Issue a forecast before resolving it, revise it, and confirm the original remains scored under its original horizon and outcome definition. Report baseline comparison, calibration where applicable, timing error, and unresolved outcomes.

### A16: report collaboration and sharing

Exercise owner/editor/reviewer/viewer permissions with sensitive material in a shared case. Verify source access through search, subscriptions, model prompts, report generation, and direct artifact requests. Concurrent edits must retain conflict history. Review a cited report, preview redactions, and export privately without publishing. A source correction must flag the report while its issued version remains identifiable.

### A17: portable evidence and replay

Export and import a case with originals, restricted references, missing assets, calibration, analyses, forecasts, and reports. Verify hashes, version mapping, citations, omissions, and a supported analysis replay. Reject corrupted hashes, path traversal, oversized/decompression-bomb inputs, and unsupported required schemas without mutating existing cases. Opening a bundle must not execute code, connect devices, or fetch undeclared remote assets.

### A18: privacy and change monitoring

Inspect a supported file with known identifying metadata, produce a cleaned copy, and re-inspect it. Preserve the original and disclose content-level identifiers that metadata removal cannot address. Evaluate ambiguous account matches without asserting identity. Track a user's declared domain scope and a public-page revision; verify dated source evidence, supported-format limits, and explained notifications.

## 3. Cross-cutting release gates

Every stage must pass the relevant contracts, integration tests, and end-to-end scenarios. Broader testing is warranted by changed code paths, failures, or unresolved concerns, not by a desire to accumulate redundant checks.

- **Integrity:** No silent original mutation, lost provenance, or guessed numerical defaults in evidence records. Citation anchors and dependency invalidation pass representative fixtures.
- **Isolation:** Unauthorized case/blob/model-input access fails. Exports contain no credentials or unintentionally retained private fields. Resource-limited parsers and adapters handle malformed input.
- **Recovery:** Interrupted acquisition/import/analysis, expired leases, storage exhaustion, cancellation, and restart preserve consistent state. Backup/restore is demonstrated on a representative case.
- **Compatibility:** Schema migrations and bundle version handling are tested; provider and hardware support claims name versions and tested configurations.
- **Usability:** Critical workflows pass keyboard and contrast checks, show honest source/job states, and remain usable with reduced effects.
- **Scientific/numerical validity:** Units, coordinate/time conventions, numeric edge cases, and supported accuracy are documented and checked independently where needed.
- **Model validity:** Each shipped model-assisted capability has a dataset/method description, held-out evaluation, known failures, and a visible insufficient-evidence path. No unmeasured probability-calibration claim is allowed.
- **Documentation:** Source licenses/notices, user-facing limits, adapter configuration, proof claims, and capability status match the implemented release.

## 4. Proposed performance and resource targets

These are design targets to validate in P0/P1, not measurements or promises. Record the benchmark commit, data hashes, hardware, OS, renderer, worker settings, and cold/warm state. Adjust a target only through a documented decision with supporting measurements.

Initial reference profile: an 8-core-class CPU, 16 GB RAM, SSD storage, integrated graphics, and a 1920×1080 display. Representative local corpus: 10,000 document metadata records and 100,000 extracted passages, with a fixed fixture distribution and separately declared media sizes.

- Local UI acknowledgment of ordinary actions: p95 within 100 ms, excluding acquisition and computation completion.
- Warm local filtered text queries over the reference corpus: p95 within 2 seconds. Semantic queries and cold indexing require separately reported benchmarks.
- Cancellation: UI acknowledgment within 250 ms and no publication of superseded results; resource release deadlines must be defined per adapter, with a proposed 5-second default before worker termination where safe.
- Globe interaction: target 30 FPS on the reference profile with a fixed 5,000-marker scene and reduced-effects mode. Numerical results must be identical across visual quality settings.
- Ingestion, video decoding, inference, storage, and live feeds: enforce configurable hard budgets and report queue/drop/coverage effects. Throughput claims must name stream count, codec, resolution, frame sampling, hardware, and model; no universal real-time detector target is assumed.

External provider latency and model-service latency must be reported separately from OIA processing. Load tests must establish where backpressure activates and whether case editing remains responsive under exhausted budgets.

## 5. Lean and Rust verification plan

[ADR 0004](adr/0004-d13-lean-correspondence-target.md) selects the implemented forecast cutoff contract as the first Rust correspondence target, with frame-selection and dependency-propagation model proofs alongside it. The [proof manifest](../proofs/README.md) records theorem scope, assumptions, source revisions, numeric limits, and separate local Lean/Rust commands.

For every proof-backed claim, record specification, theorem, assumptions, pinned Lean toolchain, Rust revision, correspondence method, fixture/property tests, and numeric limits. Local validation must check Lean artifacts and relevant Rust behavior independently; hosted CI is not enabled by this requirement. Admitted proofs or project-specific axioms cannot support a production verification claim unless explicitly disclosed and accepted as assumptions.

Tests connecting Rust to a Lean model establish evidence of correspondence, not automatically a formal implementation proof. Verification labels in the product must reflect that distinction. Source truth, model calibration, and forecasting skill require empirical evaluation.

## 6. Decision register

Decisions belong to the project owner and implementing maintainers. Each resolution must record rationale, alternatives considered, validation, and affected requirements. The proposals below allow specification work to proceed without pretending the choices are already approved.

| ID | Decision | Proposed direction or open question | Needed before |
| --- | --- | --- | --- |
| D01 | Deployment and UI | Accepted: local-first Rust boundary ([ADR 0001](adr/0001-d01-deployment-and-ui-boundary.md)) and owner-selected Tauri 2 renderer ([ADR 0008](adr/0008-d01-workspace-renderer.md)), including the WebView2 traffic disclosure condition. Local file workbench implemented; persistent cases/services remain #14/#16. | P0/P1 application shell |
| D02 | Storage/indexes | Accepted in [ADR 0002](adr/0002-d02-storage-and-indexing.md) ([owner decision](https://github.com/mESmaC/oia/pull/56#issuecomment-6568675966)): embedded SQLite (`rusqlite` in WAL mode with FTS5 and R-Tree as needed) plus content-addressed managed blobs; runtime implementation in #11 | P0 persistence (decided; #11 in progress) |
| D03 | Seed corpus and news sources | [ADR 0005](adr/0005-d03-seed-corpus-and-news-sources.md): small, documented UAP/disclosure and regional collection with verified source access | P1 adapters |
| D04 | AI execution | [ADR 0007](adr/0007-d04-ai-execution-and-grounding.md): provider-neutral local/remote interface; select evaluated summarization/OCR/embedding options and data-handling defaults | P1 AI features |
| D05 | Export formats | [ADR 0006](adr/0006-d05-report-and-bundle-formats.md): Markdown, self-contained HTML, uncompressed JSON directory bundle selected for implementation; wire schema pending shared contracts; PDF after render QA | P1 report exporter |
| D06 | Region and geo/space sources | Choose a first region, weather/track/imagery providers, ephemerides, and historical depth | P2 integrations |
| D07 | Cameras and retention | Choose eligible directories/feeds, automated-use conditions, buffer/storage limits, and excluded views | P3 live monitoring |
| D08 | Devices and radio | Select first drone video/telemetry interface, SDR, remote receiver, and stream integrations | P3 hardware claims |
| D09 | Detection and restoration | Select methods/models, evaluation corpora, acceptance thresholds, hardware budget, and error presentation | P3 model release |
| D10 | Collaboration and sync | Choose hosted transport/identity, capability matrix, merge rules, and sensitive-data defaults | P4 shared service |
| D11 | Markets | Choose instruments, providers, calendars, corporate-action handling, historical depth, and licensed uses | P5 market ingestion |
| D12 | Forecasting | Choose first targets/horizons, baselines, point-in-time datasets, calibration and resolution methods | P5 issued forecasts |
| D13 | Lean correspondence | [ADR 0004](adr/0004-d13-lean-correspondence-target.md): Lean 4.35.0 model proofs and shared forecast-cutoff fixture; finite Rust evidence, not formal implementation equivalence | First proof claim (implemented scope in #15) |
| D14 | Distribution and project licensing | Accepted in [ADR 0003](adr/0003-d14-licensing-and-distribution.md) ([owner decision](https://github.com/mESmaC/oia/pull/56#issuecomment-6568675966)): dual `MIT OR Apache-2.0` repository code license ([`LICENSE-MIT`](../LICENSE-MIT), [`LICENSE-APACHE`](../LICENSE-APACHE)) with third-party datasets/fixtures excluded and tracked in [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md); release installer/signing packaging deferred to #49 | Public release (code license decided; #49 open) |

## 7. Risk and dependency register

- **Source access changes:** Adapter isolation, explicit unavailable states, capability manifests, and permitted cached evidence reduce disruption. A fallback must not masquerade as the requested source.
- **Incomplete historical coverage:** Query results and forecasts expose coverage and availability timestamps; a demonstration is scoped to sources actually acquired.
- **Scope breadth:** Phases share contracts and deliver usable workflows; feature names alone do not count as completion. UI, providers, and hardware remain replaceable.
- **False associations and generated detail:** Typed evidence, independent-source tracing, review, and evaluated models keep uncertainty inspectable throughout reports.
- **Numeric and temporal errors:** Shared types, independent references, fixture suites, and selected Lean proofs target high-impact assumptions.
- **Resource pressure:** Bounded jobs, ingestion budgets, separate telemetry handling, retention controls, and degraded visual modes preserve useful behavior.
- **Ported legacy behavior:** Differential tests must distinguish intentional contracts from display shortcuts and documented defects. Independent references take precedence over blind parity.
- **Licensing and portability:** Code notices and third-party data/assets are tracked separately; export manifests record omissions and permitted reuse.

## 8. Implementation handoff

The next implementation task should establish P0's domain records and a minimal case/document round trip, resolving D01/D02 only to the extent needed for that slice. It should include a versioned source record, retained original, anchored extraction, case reference, and report/bundle skeleton before adding many source adapters.

For each subsequent task, name the requirement IDs, source fixtures, decision prerequisites, intended user workflow, acceptance scenarios, and validation evidence. Keep the [product specification](spec.md) and [architecture](architecture.md) synchronized with resolved decisions rather than allowing a second implicit specification to accumulate in implementation notes.
