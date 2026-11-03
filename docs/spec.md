# Open Intelligence Agency: product specification

Version: 0.1. Date: 2026-11-01. Status: complete specification draft for review; not an implementation or release claim.

This document consolidates the agreed product direction and feature scope. Requirements describe the intended suite, not what must ship simultaneously. Technical defaults, release sequencing, and numerical targets are proposals unless identified as established direction. Unresolved implementation choices are tracked explicitly.

## Specification set

- This document defines product behavior, user journeys, and functional requirements.
- [Architecture and data contracts](architecture.md) define the Rust system, evidence model, integrations, and Lean boundary.
- [Delivery and verification](delivery.md) define proposed milestones, acceptance scenarios, release gates, and open decisions.
- [Selective reuse inventory](prior-art.md) identifies concrete groundwork in God's Eye View.
- [Exploration archive](spec-draft.md) preserves the preceding brainstorming. This specification set supersedes its provisional wording where they differ.

Requirement identifiers such as `DOC-01` are stable references. **Must** is required for the associated capability to be considered implemented. **Should** is a strong design preference with a documented exception allowed. **May** denotes an optional extension. A deferred capability is not a waiver of its requirements when it is delivered.

## 1. Product purpose

OIA is a Rust-based suite of public intelligence and research tools that helps people understand their surroundings, investigate events, evaluate information, and improve their individual privacy. It also demonstrates how much information and analytical capability is already accessible to the public.

OIA serves civilians, former military personnel, researchers, journalists, and other independent investigators. Public internet collection is a core product capability: external APIs, websites, feeds, archives, and streams supply evidence that users can connect, analyze, and reconstruct into new intelligence. Local-first operation preserves user control and access to retained work; it does not limit OIA to local files or offline development.

UAP research and disclosure are a primary organizing use case. The same infrastructure supports regional awareness, public-record research, space observation, media forensics, financial research, and historical forecasting.

The central product outcome is a traceable investigation: a user can move from a question to relevant observations, inspect their origins, test explanations, and produce a report that another person can examine. Collection, analysis, visualization, and reporting must feel like one workspace.

### 1.1 Established direction

- Rust is the implementation language for the application's core, services, algorithms, and integrations. Specific UI and rendering technology remains an explicit decision.
- Lean supports selected formal specifications and proofs during development; it need not be present in distributed Rust binaries.
- God's Eye View is prior implementation work to reuse selectively, including contracts, algorithms, tests, and operational lessons.
- The visual identity combines a futuristic military terminal, Call of Duty-style cinematic atmosphere, modern glass materials, and Apple-like polish. The workspace should evoke a SCIF workstation or satellite intelligence console through real public-source capabilities, connected evidence, and analytical depth. Source and access labels must remain accurate.
- Camera scope includes ordinarily accessible feeds and existing business/street camera directories without requiring proof of intentional publication by the owner. OIA must not attempt to gain additional access. Home interiors are excluded.
- All capabilities recorded below belong to the intended product scope. A proposed implementation order appears in the delivery document.

### 1.2 Intended users and outcomes

- **Individual researcher:** Find original documents, inspect claims, and assemble a cited investigation.
- **UAP investigator or witness:** Record an account, reconstruct the sky and surroundings, inspect media, and evaluate explanations.
- **Local-awareness user:** Understand reported events, infrastructure disruptions, weather, and public information in an area.
- **Observer with personal hardware:** Connect cameras, radios, or drones and preserve observations with device context.
- **Market or geopolitical researcher:** Study event responses, construct conditional forecasts, and evaluate predictions over time.
- **Collaborating reviewer:** Inspect another researcher's evidence and reasoning, reproduce supported analyses, and contribute corrections.

### 1.3 Boundaries

OIA does not assume continuous global imagery, complete tracking coverage, universal historical archives, or support for every hardware device. Provider access, data quality, calibration, and local resources constrain each integration and must be visible.

Brokerage execution, drone flight control, radio transmission, and direct image generation are separate future decisions. The specified initial capabilities are market analysis, drone video/telemetry, radio reception, and recreation-prompt export. Technical adapters must not silently expand those boundaries.

## 2. Shared research rules

These rules apply to every module and every export.

1. **Provenance:** Every substantive claim or derived result must identify its source records or explicitly state that it is an assumption or unsupported proposal.
2. **Evidence identity:** Source material, reported claims, normalized observations, inferences, forecasts, processed derivatives, and recreations must retain distinct types through subsequent operations.
3. **Time:** Event, capture, publication, retrieval, revision, and analysis times are distinct. Approximation, timezones, clock offsets, and unknown times must be representable.
4. **Coverage:** Missing coverage, failed retrieval, delayed data, empty results, and observations of no relevant activity must remain distinguishable.
5. **Integrity:** Original retained bytes must not be overwritten by analysis. Hashes establish integrity after acquisition, not authenticity or truth.
6. **Uncertainty:** A matching record, high detector score, or plausible explanation does not establish identity or causation. Confidence measures need defined meanings and validation.
7. **Revision:** New evidence, corrected sources, and changed models must produce versioned results and flag affected conclusions for reassessment.
8. **Human review:** Generated summaries, correlations, claims, and reports must be inspectable and correctable. Model agreement is not independent corroboration.
9. **Reuse conditions:** Source access, automated processing, retention, and redistribution conditions must be recorded separately. A public URL is not a blanket license.
10. **Reproducibility:** Preserve inputs, versions, methods, parameters, and limitations sufficient to repeat supported analyses or explain why repetition is unavailable.

## 3. Core experience and visual design

### 3.1 Investigation workspace

- **CASE-01:** Users must be able to create an investigation with a question, title, optional geographic area, time interval, topics, and visibility. Saving a case must not publish it.
- **CASE-02:** The workspace must preserve selected investigation, area, time, evidence, and navigation history across globe, sky, documents, media, graphs, market charts, and report views. Each view must expose its local filters when they differ from shared context.
- **CASE-03:** Users must be able to save sources, observations, queries, annotations, reasoning results, hypotheses, and research leads. Source records must remain unchanged by user annotations.
- **CASE-04:** Long-running operations must show progress, partial results, failures, cancellation, and retry options. Changing a query or selected event must not allow late results to replace newer context.
- **CASE-05:** Search must combine exact identifiers and text with optional semantic retrieval, showing scope, filters, pagination, and coverage limitations. Saved searches and alert rules must retain their definitions and revisions.

### 3.2 Visual identity

The intended emotional response is: "I feel like I shouldn't have access to this." Capability, precision, and connected evidence should create that atmosphere. OIA remains an original identity with accurate source and access labels.

- **UX-01:** The default visual system must use a dark canvas, smoked glass surfaces, restrained borders and depth, readable sans-serif text, and monospace instrumentation. Green, cyan, and amber are candidate accents; semantic meanings must be consistent.
- **UX-02:** The workspace should use a central evidence canvas, compact navigation, contextual inspector, shared timeline, and persistent command/search field. Selecting an event should coordinate the view, timeline, and dossier without losing the user's route back.
- **UX-03:** Coordinates, status indicators, orbital paths, and telemetry must represent actual data or explicitly identified estimates. Live, historical, simulated, and generated views must be recognizable without relying only on color.
- **UX-04:** Users must have keyboard navigation, visible focus, readable contrast, reduced-motion and reduced-transparency settings, clean-evidence and focused-reading views. Routine sound must be off by default. Dense controls must reveal explanations without obscuring evidence.
- **UX-05:** Visual effects must degrade independently of data and analysis capability. Reports must retain legibility and attribution in print and plain export formats. An uncertain conclusion must remain visibly uncertain despite polished presentation.

### 3.3 Representative journeys

**UAP observation:** Create a case from an account or location/time; preserve the initial testimony; inspect historical reports and independent source lineage; reconstruct the local sky; query available weather, tracks, cameras, and imagery; analyze media; compare hypotheses; export a cited report and optionally a recreation prompt.

**Regional report:** Select area, period, and topics; collect news and public records; inspect event grouping and disagreements; relate relevant environmental or transport context; draft a report; inspect citations; save a revision and optionally configure update notifications.

**Connected observation:** Register a personal device; inspect its capabilities and clock status; record or monitor permitted data; bookmark an event; open original footage with telemetry; run an analysis; attach the result to a case; review and export.

**Forecast:** Select a current or hypothetical event; define outcome, target, horizon, and information cutoff; inspect comparable history or market exposure; adjust assumptions; save a forecast; track new evidence and revisions; evaluate the original forecast after resolution.

## 4. Research and information capabilities

### 4.0 Internet discovery and collection

These requirements apply across document, news, geographic, camera, space, market, and research tools. They define intended capability, not a claim that collection adapters already exist.

- **SOURCE-01:** Support discovering and registering public internet sources, including user-supplied URLs, external search/API results, linked public records, catalogs, RSS/Atom feeds, archives, downloads, and streams. Support keyless sources and providers requiring ordinary user-authorized accounts, API keys, or subscriptions. Prefer free sources where suitable; a fixed seed corpus or provider list must not become a permanent product boundary.
- **SOURCE-02:** Support interactive and scheduled acquisition through external APIs, feed polling, direct downloads, and bounded web scraping/crawling where appropriate to the source. Public HTML pages are valid sources even without an official API. Each collection job must declare its scope, page/depth/item/byte or stream-duration limits as applicable, cadence, concurrency, deadline, and cancellation. Record visited sources, retrieval/version metadata, extraction method, failures, and limits reached; expose incomplete coverage and stop or back off on restrictions and rate limits.
- **SOURCE-03:** Record source-specific access, automated processing, retention, and redistribution conditions separately. Unknown conditions must remain explicit and be resolved for the requested operation; they must not be converted into a blanket ban on internet research or a universal requirement for individual owner permission. Reading, indexing, retaining originals, and exporting copies are separate uses. Where a use is unsupported or restricted, explain it and preserve permitted references/metadata. Source registration and configured collection rules must permit routine authorized research without repeated approval screens. Camera access continues to follow CAM-01/CAM-02.
- **SOURCE-04:** Acquired internet material must enter the shared evidence and application contracts. Support constructing linked events, maps, timelines, source graphs, reconstructions, and forecasts from acquired information, with inspectable inputs, methods, assumptions, uncertainty, and source versions. Derived intelligence must remain distinct from observations; live acquisition and retained historical evidence must remain recognizable throughout analysis and export.

### 4.1 Public document library

- **DOC-01:** Import files and URLs and support scheduled source adapters for public government records, declassified archives, university publications, arXiv, and other open research. Preserve originals where permitted, otherwise references and available metadata. Record publication type, authors, publisher, dates, version, language, and known review status.
- **DOC-02:** Extract text with page or passage anchors and OCR where necessary. Identify unreadable portions, missing pages, and partial processing. Exact-byte duplicates may share storage, but releases, revisions, differently redacted copies, and arXiv versions must retain separate identities and relationships.
- **DOC-03:** Produce labeled AI drafts covering document identity, subject, claims/findings, methods, limitations, and unresolved questions. Substantive statements must cite source passages; a document's assertion must not automatically become OIA's established fact. Partial summaries must state coverage.
- **DOC-04:** Support filtered text and semantic search, cited question answering, saved collections, release comparisons, and references across documents. Answers must state when retrieved material is insufficient. Citation links must resolve to the exact source version used.
- **DOC-05:** Track model/configuration versions, generation times, human review, source corrections, and stale summaries. Treat document text as untrusted research input, not instructions to execute actions.

### 4.2 Geographic events and UAP investigations

- **GEO-01:** Support areas, points, approximate locations, and time intervals on a globe/map and timeline. Preserve reported local time, timezone, uncertainty, and the earliest-known-source qualification for historical accounts.
- **GEO-02:** Query available satellite imagery, public camera records, weather, aircraft/vessel activity, astronomical context, reports, and documents for the selected event. Display capture time, spatial/temporal coverage, source status, and relevant resolution or calibration limits.
- **GEO-03:** Support editable clustering by proximity, time, reported properties, and source relationships. Show matching criteria, conflicts, and reasons. Preserve separate counts of articles, accounts, independent sources, and proposed events; allow reversible split/merge decisions.
- **GEO-04:** Present imagery and camera views as a navigable mosaic with footprints/viewing cones where supported. Continuous seamless reconstruction must not be implied. Cross-view alignment must retain calibration, clock, and correspondence assumptions.
- **GEO-05:** Reconstruct context using weather at relevant altitudes, recorded aircraft/vessel tracks, appropriate orbital elements, and observer-specific celestial positions. An absent track or recording must not be presented as evidence of an empty scene. Display interpolation must remain distinct from observations.

### 4.3 Space imagery and solar system

- **SPACE-01:** Catalog public Earth-observation, Hubble/James Webb and other telescope, solar/heliospheric, planetary mission, and spacecraft imagery. Support mission, instrument, target, wavelength, observation interval, and footprint/field-of-view filters where source metadata permits. NASA, ESA, STScI/MAST, and mission archives are candidate providers, not confirmed integrations.
- **SPACE-02:** Preserve scientific products such as FITS where supplied, alongside previews. Record exposure, calibration/processing level, source version, color mapping, stacking/mosaic history, and publication references. Outreach renderings and false-color composites must be identified.
- **SPACE-03:** Provide public space-station and mission feeds, launch/docking broadcasts, and archives where available. Distinguish live camera imagery, delayed transmission, replay, commentary, and simulation. Bookmarks must retain source and timing limitations.
- **SPACE-04:** Link Earth, celestial sky, and solar system views using one investigation clock. Show Earth's rotation, tilt, orbit and illumination; other planets' positions, rotations and orbits; and lunar motion/phases. Support pause, scrubbing, accelerated playback, reference-frame selection, and clearly labeled illustrative versus true scale.
- **SPACE-05:** Use versioned ephemerides and orientation models with documented ranges and accuracy. JPL Horizons/SPICE are candidates. Separate geometric from apparent positions, including observer location and relevant light-time, time-standard, orientation, and refraction assumptions. Telescope pointing must not imply coverage of a local atmospheric event.

### 4.4 News, regional intelligence, and claim assessment

- **NEWS-01:** Ingest permitted local, national, international, wire, specialist, and official reporting. Preserve article versions, publication/update dates, event time, language, translations, supporting passages, and geographic uncertainty. Syndication and common underlying sources must be represented.
- **NEWS-02:** Construct regional accounts of reported military movements, exercises, deployments, criminal activity, public safety incidents, transport disruption, and other developments. Distinguish plans, observations, allegations, arrests, charges, judgments, and attributed statements. Movement alone must not establish intent, mission, or destination.
- **NEWS-03:** Evaluate specific claims or presentations for factual contradictions, unsupported certainty, headline/caption mismatch, distorted quotations, misattributed imagery, numerical errors, selective comparisons, and materially missing context. Show the exact passage, source version, contrary/supporting evidence, reasoning, and review status.
- **NEWS-04:** Claim assessments must support supported, contradicted, unsupported, materially misleading, disputed, and unresolved states with stated criteria. Distinguish quoting a claim from endorsing it, opinion from factual premises, and publication-time evidence from later knowledge. Lack of corroboration or majority agreement alone must not determine truth.
- **NEWS-05:** Corrections, retractions, and case outcomes must flag dependent reports. Publisher-level summaries must show sample, method, time range, unresolved assessments, and correction history. News volume must not be equated with incident frequency, crime rate, or community criminality.

### 4.5 Reasoning and additional intelligence methods

- **REASON-01:** Deduction must expose premises, assumptions, rules, and derivation steps. AI-proposed reasoning, human-reviewed reasoning, and Lean-checked results must remain distinct. A valid derivation is conditional on its premises.
- **REASON-02:** Extrapolation must state observed domain, projection horizon, model, uncertainty method, and sensitivity. Correlation must retain matching tolerances, coverage, baseline rates where relevant, source dependencies, and multiple-comparison considerations. Neither method may silently assert causation.
- **REASON-03:** Abstraction must normalize reports into editable event structures, concepts, taxonomies, and graphs while preserving original wording, unknown values, conflicts, omissions, and version history. Entity matches and relationship types must be explicit and reversible.
- **REASON-04:** Composed analyses must preserve dependency links and source/inference types. Users must inspect and revise individual steps, compare competing hypotheses, check assumptions, record contradictions, and identify evidence that would discriminate between explanations.
- **INTEL-01:** Provide collection planning, research leads, source-lineage tracing, and corroboration analysis. Detect copied accounts and circular references; support open questions, potential sources, assignments, progress, and reassessment when evidence changes.
- **INTEL-02:** Support public links among organizations, contracts, grants, facilities, publications, and events. Track scientific/technical developments through papers, patents, grants, and reports, distinguishing proposals and patents from demonstrated capability.
- **INTEL-03:** Support geospatial change detection and maritime/aviation context using available imagery, tracks, notices, ports, and launch information. Changes must retain capture geometry, seasonal effects, baseline choices, and coverage limits. User-defined indicators and alerts must explain why they triggered.
- **INTEL-04:** Support multilingual research, original/translated passages, transliterations, and alternate names. A public-records workbench must support likely custodians, draft requests, configurable deadlines, responses, and release comparisons. Submission is separate from drafting and requires an explicit user action.

## 5. Observation and media capabilities

### 5.1 Internet cameras and anomaly detection

- **CAM-01:** Begin with existing camera websites/directories, published viewing links, supported embeds/feeds/APIs, and ordinarily accessible business, street, traffic, weather, and observatory cameras. Intentional publication by the owner is not a blanket requirement. Exclude cameras inside homes and views into home interiors. OIA must not probe hidden endpoints, guess credentials, exploit devices, or bypass access controls; a restricted source becomes unavailable rather than triggering attempts to regain access.
- **CAM-02:** Record listing page, provider, attribution, access method, automated-use/retention/redistribution conditions, camera position and orientation if known, and stream/still/archive capability. Assess actual views for private activity and support exclusion/masking. Distinguish upstream imagery, still fallback, Street View, and synthetic substitutes; only suitable observations can serve as event evidence.
- **CAM-03:** Support configurable regions, baselines, sensitivity, persistence, and notifications for sky transients, unusual apparent motion, road obstructions/stopped vehicles/wrong-way travel, and environmental changes. Appropriate methods may include background modeling, differencing, optical flow, detection, tracking, and temporal models. Camera faults, frozen frames, exposure shifts, and repositioning must have separate health states.
- **CAM-04:** Each candidate event must retain original frames/clips, trigger explanation, quality, timestamps, model/configuration, and available pre/post context. Use bounded buffers where retention is permitted. Deduplicate notifications and expose coverage loss. An anomaly is a reason to review, not an identification of a UAP, crime, or emergency.
- **CAM-05:** Review must support artifacts, likely ordinary explanations, insufficient evidence, and retained investigations with history. Cross-camera association must expose timing/geometry and shared-source dependencies. Multi-view triangulation requires valid calibration, independent views, and justified correspondence. Detector scores must not be presented as calibrated class probabilities without evaluation.
- **CAM-06:** Provide a community-mapped surveillance-infrastructure and automated license-plate-reader (ALPR) directory layer using OpenStreetMap (`man_made=surveillance`, `surveillance:type=ALPR`) via bounded Overpass API queries and DeFlock vector-tile extracts. Preserve manufacturer classification (`Flock Safety`, other named vendors, or unspecified), field survey dates separately from OSM edit timestamps, and recorded source terms. Mapped nodes represent community-contributed equipment assertions only (not verified hardware deployments, live camera feeds, or license-plate read logs), directional wedges are illustrative unless camera orientation is calibrated, and absence on the map must never be presented as absence of physical cameras.

### 5.2 Forensic media and recreation

- **MEDIA-01:** Provide original/processed comparison, zoom, frame navigation, metadata/container/codec inspection, histograms, channel and frequency views, contrast/exposure adjustments, compression/noise analysis, resampling cues, and candidate copy-move/compositing analysis. Automated authenticity cues must show limitations rather than provide unsupported binary verdicts.
- **MEDIA-02:** Support frame extraction, timing inspection, stabilization, optical flow, tracking, calibration-based lens correction, perspective analysis, photogrammetry, and geolocation/time assessment using terrain, landmarks, shadows, and celestial context. Account for blur, defocus, glare, rolling shutter, camera motion, parallax, and unknown distance.
- **MEDIA-03:** Offer interpolation, denoising, deblurring, multi-frame reconstruction, and learned super-resolution where implemented. Preserve contributing frames, alignment, settings, and models. Predictive detail must remain labeled and must not become observation evidence or establish physical measurements without a validated method and error bounds.
- **MEDIA-04:** Export editable, portable image-generation prompts for hyperrealistic illustrative recreations, with selected source references and permitted assets. Separate reported details, assumptions, unknowns, and conflicting variants. Include scene/time/viewpoint/lighting and apparent scale where supported. Imported recreations must retain prompt/settings and visible recreation labels in OIA exports; prompt wording alone does not ensure fidelity or external-generator labeling.

### 5.3 Radio and spectrum

- **RADIO-01:** Provide a location/frequency/service directory for public streams, operator-published remote receivers, and explicitly connected compatible local radio/SDR hardware. Candidate categories include broadcast, weather, amateur, aviation, and maritime audio where available. A frequency listing must not imply reception or archive availability.
- **RADIO-02:** Provide persistent playback, favorites, source/status/latency labels, and supported tuning, modulation, bandwidth, squelch, and signal measurements. Show RF spectrum/waterfall only from appropriate receiver data; label audio spectrograms as audio. Distinguish transmitter, receiver, and reported event locations.
- **RADIO-03:** Support permitted audio clips, timeline bookmarks, transcription, translation, and keyword candidates. Preserve originals, source/frequency/clock context, uncertain passages, and corrections. Initial scope is receive-only, respects receiver restrictions, and does not bypass encrypted or access-controlled communications. A recorded statement remains an attributed claim.

### 5.4 Personal hardware and UAV workspace

- **DEVICE-01:** Register personal drones, cameras, SDRs, and sensors through capability-aware adapters. Candidate paths include manufacturer SDKs, supported network video, local capture, and MAVLink on compatible systems. Video, telemetry, media import, and controls are distinct capabilities. Local connection must not automatically publish footage/location.
- **DEVICE-02:** Provide a configurable UAV HUD with supported heading, attitude, position, altitude/reference, speed, gimbal orientation, battery, link/navigation health, recording, timestamps, and freshness. Link flight path and calibrated camera footprint to the map. Missing values stay unavailable; stale and derived values must be identified. A viewing marker does not itself measure range.
- **DEVICE-03:** Support live and recorded flights, event bookmarks, frame-copy inspection while reception continues, motion compensation, and forensic handoff. Keep original footage/telemetry separate from rendered HUD recordings. Link higher-quality onboard recordings later without replacing the version used by prior analyses.
- **DEVICE-04:** Synchronize video and telemetry with documented clock offsets; expose drops/disconnections. Moving-camera analyses must account for vibration, gimbal motion, parallax, and exposure changes. Analysis must not block telemetry handling. Initial integration leaves flight control to the existing controller; later control support requires separate design and verification.

### 5.5 Personal observatories and field research

- **FIELD-01:** Combine compatible personal sky cameras, weather sensors, SDRs, and other devices into an observation station with health, coverage, clock synchronization, retention, and searchable history. Event-triggered capture must report the actual coverage of each device.
- **FIELD-02:** Provide witness forms for time, location, duration, direction, apparent size/motion, sound, and conditions, including sketches against a reconstructed sky. Preserve the initial account before showing candidate explanations and retain subsequent edits.
- **FIELD-03:** Plan observations using weather/visibility forecasts, light pollution, terrain, celestial context, and available viewpoints. Calibrate camera orientation, field of view, distortion, and timing against suitable references, retaining errors, version, and validity conditions. Multi-observer reconstruction must show uncertainty regions and non-unique solutions.
- **FIELD-04:** Support offline preparation of permitted maps, documents, predictions, forms, and profiles; local capture; and later explicit conflict resolution. Mark cached/expired data and preserve device time uncertainty. A validation lab must provide labeled real fixtures and clearly separate simulations with controlled noise, blur, geometry, and clock drift.

## 6. Prediction and market research

### 6.1 Market analysis and event impact

- **MARKET-01:** Support selected equities, indices, funds, bonds/yields, commodities, currencies, and digital assets according to provider capability. Record stable instrument identity, venue, currency, time, frequency, delays, trading calendar, corporate actions, and price adjustment basis. Company filings, earnings, economic releases, news, and event context must be linkable.
- **MARKET-02:** Provide linked price/return/volume/volatility charts, event overlays, watchlists, alerts, event studies, historical comparisons, and cited briefs. Exposure links among companies, facilities, supply chains, regions, and commodities must cite dated evidence. Benchmarks, windows, return definitions, and sample choices must be inspectable.
- **MARKET-03:** Forecast direction, ranges, volatility, or explicitly defined outcomes where supported using news, weather, declassifications, disclosures, and other public data. Distinguish current-condition forecasts from conditional scenarios. Keep event probability separate from response conditional on that event, and expose impact mechanisms and assumptions.
- **MARKET-04:** Show forecast origin/horizon, information cutoff, model, baseline, uncertainty, and sensitivity. Distinguish an event's historical date from public release and market awareness. Consider novelty and prior expectations; real-world effects need not imply incremental price effects. Unsupported precision must yield insufficient-evidence or qualitative outputs.
- **MARKET-05:** Preserve forecast vintages, revisions, failures, and observed outcomes. Use chronological/out-of-sample evaluation, task-appropriate baselines, calibration and interval coverage, and point-in-time data. Detect leakage from revised series, later news, instrument selection, and language-model knowledge. Strategy simulations, if added, must state fees/spreads/slippage and never present simulation as observed returns. Order execution is not included.

### 6.2 Historical precedents and general forecasting

- **FORECAST-01:** Build sourced event-response histories across governments, institutions, industries, and communities. Record initiating conditions, actors, actions, announced intentions, outcomes, elapsed time, and evidence. Distinguish temporal sequence from established causation and historical facts from what was knowable at the forecast origin.
- **FORECAST-02:** Retrieve analogues using explicit event scale, geography, actors, institutions, tensions, economic conditions, and information quality. Explain similarities/differences and allow comparison-set edits. Include counterexamples, divergent responses, documented non-response, and missing/censored coverage. Related episodes must not automatically count as independent precedents.
- **FORECAST-03:** Produce conditional response sequences, time ranges, alternative branches, defined outcome predictions where justified, and indicators to monitor. Historical frequencies must not become calibrated probabilities without validation. Multi-step probabilities must account for dependencies; sparse or poorly transferable history must be identified.
- **FORECAST-04:** Freeze origin, cutoff, sources, comparison set, method, assumptions, outcome definition, and resolution rules before evaluation. Preserve original and revised forecasts. Use prospective tracking and chronological backtests with base-rate baselines, calibration/outcome metrics, timing evaluation, and explicit unresolved outcomes. New evidence must explain revisions without rewriting previous predictions.

## 7. Collaboration, reports, and privacy tools

### 7.1 Cases and reports

- **COLLAB-01:** Shared cases must support owner/editor/reviewer/viewer responsibilities, comments, assignments, competing interpretations, review history, and access controls for unpublished material and sensitive locations. A review must record its author and scope rather than imply universal truth.
- **COLLAB-02:** Report building must create editable drafts containing question/scope, coverage, chronology, source lineage, relevant context/media, hypotheses, supporting/conflicting evidence, bounded conclusions, open questions, citations, and an evidence inventory. AI-authored content must retain its generated/review status. Selecting a statement must open its evidence in the same workspace.
- **COLLAB-03:** Saving, exporting privately, sharing with collaborators, and publishing must be distinct actions. Exports must retain labels, source versions, and citations and preview redactions/omissions. Changes to dependencies must flag affected report versions without overwriting previously issued reports.

### 7.2 Reproducible packages

- **REPRO-01:** Export an investigation bundle containing permitted originals or references, manifests, hashes, timestamps, calibration, queries, grouping decisions, annotations, analyses, models/configuration identifiers, forecasts, and reports. List omitted/restricted/missing assets and reproducibility limits.
- **REPRO-02:** Import must validate schema, versions, paths, sizes, hashes, and references before atomic case creation. A bundle must not execute code, connect devices, fetch arbitrary sources, or overwrite an existing case merely by being opened. Readers must report unsupported features explicitly.
- **REPRO-03:** Supported analyses must be replayable from fixed inputs/settings, with deterministic or tolerance-based results documented. Missing models, unavailable sources, stochastic outputs, and changed licenses must be reported rather than silently substituted.

### 7.3 Personal awareness and privacy

- **PRIVACY-01:** A local file privacy inspector must identify supported metadata such as location, author, timestamps, thumbnails, and document properties, explain findings, and create a cleaned copy. Originals must remain unchanged; removal claims must be format-specific and verified by reinspection. Content itself may still reveal information.
- **PRIVACY-02:** Personal exposure and infrastructure tools must help users examine their own public profiles, account connections, domains, DNS/certificate history, and exposed contact information. Proposed account matches must retain uncertainty. Ownership or authorized scope must be recorded for personal assessments.
- **PRIVACY-03:** Local situation monitoring and public change tracking must combine selected alerts/notices and dated page/dataset snapshots, explain changes, and preserve attribution. Notifications must expose source coverage, freshness, and the saved rule that triggered them.

## 8. Technical and release obligations

The [architecture](architecture.md) defines mandatory contracts for identity, provenance, typed evidence, uncertainty, ingestion, jobs, permissions, exports, and Lean claims. All product modules must use those contracts rather than create isolated evidence stores or incompatible timelines.

The [delivery and verification plan](delivery.md) defines the proposed first release and subsequent capability stages. No module is complete solely because a UI, adapter, model, or proof file exists: its data contract, source behavior, user workflow, failure handling, and applicable acceptance scenarios must pass.

The current repository contains documentation and assets only. This specification does not claim any production integration, trained model, measured performance, passing product tests, or completed Lean proof.
