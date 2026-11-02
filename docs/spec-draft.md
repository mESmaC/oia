# OIA: working spec draft

Status: archived exploration, superseded by the [version 0.1 product specification](spec.md), [architecture](architecture.md), and [delivery plan](delivery.md). Retained as design history; provisional wording below is not the current implementation contract.

This document collected ideas for Open Intelligence Agency as they developed. The specification set above consolidates the scope and records remaining decisions explicitly.

## Project direction

OIA is a suite of public intelligence tools and a research project exploring capabilities that are already available to the general public. It should help people understand their surroundings, investigate information, and improve their individual privacy.

UAP research and disclosure are a particular area of interest. Tools developed for that research could also support aviation awareness, weather research, document research, and investigation of other unusual events.

The implementation direction is Rust, with Lean used to specify and prove selected algorithmic properties. Lean need not ship in the Rust binaries. Proofs about a Lean model require an explicit connection to the Rust implementation before supporting claims about that implementation.

## Visual identity and atmosphere

The visual direction is a futuristic military intelligence terminal: the cinematic operational atmosphere of Call of Duty, combined with modern glass surfaces and Apple-like refinement. These are aesthetic references for an original OIA identity.

The intended first impression is: "I feel like I shouldn't have access to this." The experience should feel powerful, precise, and unusually capable, while making the actual research tools approachable.

### Visual language

- **Foundation:** Deep charcoal, near-black, and midnight tones, with a globe, imagery, or investigation canvas providing the main visual focus.
- **Materials:** Smoked translucent glass, controlled background blur, fine borders, soft reflections, and layered depth. Reading surfaces need enough opacity to remain clear over complex imagery.
- **Accents:** Explore restrained phosphor green, cool cyan, and warm amber. Assign colors consistent meanings; reserve urgent treatment for actual alerts. The final palette remains open.
- **Typography:** A crisp, readable sans serif for navigation and prose, paired with a restrained monospace for coordinates, timestamps, identifiers, and instrument readouts. Compact uppercase labels can provide military character without dominating long-form research.
- **Instrumentation:** Precise map grids, coordinate ticks, selection brackets, orientation markers, orbital paths, and meaningful telemetry. Technical detail should describe the current view, source, or analysis.
- **Composition:** Carefully aligned panels, generous spacing around primary actions, and dense information revealed when useful. Glass panels should frame the evidence rather than obscure the part being examined.

### Workspace character

A possible default composition is an expansive central globe or evidence canvas, a compact navigation rail, a contextual inspector, and a shared timeline along the bottom. A persistent command field could accept searches, locations, dates, and investigation queries from anywhere in the workspace.

An event selection could bring its location into focus, expose its timeline interval, and open an evidence dossier in one coordinated transition. Documents, camera footage, forensic views, and reports should feel like different instruments within the same terminal. This composition is exploratory; exact panel arrangements remain open.

Use operational language where it makes the task clearer: investigations, observations, sources, signals, timelines, and evidence. Express the sense of privileged capability through the quality and connectedness of the tools. Source labels and access descriptions should accurately represent the underlying material.

### Motion and feedback

Favor smooth spatial transitions, controlled panel expansion, subtle selection pulses, and immediate response to input. Globe and orbital motion should preserve orientation, with reduced-motion alternatives. Loading indicators should reflect real collection or processing progress, and distinguish live activity from historical playback.

Sound can be an optional, restrained layer of feedback. Keep routine research quiet by default. Support a focused reading mode and a clean evidence view so the cinematic presentation also works for sustained analysis.

### Usability and polish

Maintain readable text sizes, strong contrast, visible keyboard focus, and status cues that work without color. Provide reduced-transparency and reduced-motion options. Scale glass and visual effects to device capability so navigation, video playback, and analysis remain responsive.

Reports and exported evidence should retain OIA's typographic polish while prioritizing clear citations, legible imagery, and print readability. Visual confidence must remain separate from evidentiary confidence: uncertain findings should look considered and professional while still being labeled uncertain.

## Unified research experience

The tools should feel like parts of one investigation workspace. Users should be able to move between a globe, historical timeline, documents, media analysis, reasoning, and report writing without rebuilding their search or losing context.

A possible UAP research journey:

1. Select an area, event, report, or research question.
2. Explore its history, tracing accounts back to the earliest available sources.
3. Collect and compare relevant reports, imagery, weather, flight activity, and documents.
4. Inspect media, test explanations, and record conclusions and open questions.
5. Assemble a cited research report and optionally export a prompt for an illustrative recreation.

Area boundaries, time windows, selected evidence, and investigation notes should carry across views. Opening a source or analysis result should preserve a route back to the same map and timeline state. Long-running collection and analysis should show progress, partial results, and failures without blocking unrelated research.

Start with readable summaries and reveal technical detail when requested. Keep source references, uncertainty, and the distinction between evidence and generated material visible at both levels. Users should be able to correct extracted details, reject proposed connections, and revise groupings without modifying source records.

### Historical and regional research

Support histories spanning the earliest discoverable reports through later reporting, investigations, releases, and reinterpretations. Separate the reported event date from the dates an account was written, published, released, and collected. Approximate or disputed dates and locations should remain visible.

"Earliest known source in this collection" is a supportable finding; "first occurrence ever" usually is not. Historical coverage will vary by region, language, digitization, and source availability. Contemporary records, retrospective recollections, and later retellings should be distinguishable.

Users could draw an area or select a region, choose a time range, and group reports by time, proximity, or reported characteristics. Show why a group was suggested and allow it to be split or merged. A cluster of reports does not necessarily represent one event or independent witnesses. Maps should distinguish report counts, candidate event counts, and source coverage; collection density alone does not establish a higher underlying event rate.

### Clear investigation reports

Build an editable, versioned report from the selected evidence and analyses. A candidate structure includes:

- Research question, geographic scope, time range, and collection coverage.
- Historical context and a sourced chronology.
- Individual accounts, proposed event groupings, and source lineage.
- Relevant media and environmental, aviation, or orbital context.
- Competing explanations, supporting and conflicting evidence, and analysis methods.
- Conclusions bounded by available evidence, unresolved questions, and useful next steps.
- Citations, an evidence inventory, and clearly labeled illustrative material when included.

AI can draft the narrative, with substantive claims linked to source passages or recorded analyses. Preserve disagreement and uncertainty rather than forcing a single coherent explanation. Users should be able to inspect and edit a draft before exporting or sharing it. Reports should identify the evidence and analysis versions used and flag subsequent changes that may affect their conclusions.

## Public document library

Build a searchable document database that collects and organizes publicly available material, including:

- Government publications, reports, hearing records, and public records releases.
- Declassified documents and disclosure archives.
- University publications, technical reports, and institutional repositories.
- arXiv papers and other openly available research.

UAP and disclosure research could be an initial collection, while the underlying library remains useful for other subjects.

### Collection and provenance

An initial workflow could accept a document URL or file, with source-specific collection added later. Candidate capabilities include:

- Preserve the original document where storage and reuse are permitted; otherwise retain its reference and available metadata.
- Record the source URL, publisher, title, authors when available, retrieval time, and relevant dates. Publication, declassification, release, and ingestion dates are distinct.
- Extract searchable text, using OCR for scanned pages, while retaining links to original pages and noting extraction problems.
- Detect exact duplicates and group related editions without discarding distinct releases, revisions, or differently redacted copies. Keep arXiv versions distinguishable.
- Preserve content hashes and processing history. A hash supports integrity checks after collection; it does not establish authenticity or truth.
- Track collection failures and source coverage so users can understand what the library may be missing.

Public availability does not necessarily grant permission to redistribute a document. Collection methods, storage, attribution, and reuse should follow the source's applicable access and licensing terms.

### AI-drafted summaries

Generate clearly labeled AI-drafted summaries that help users decide what to read and navigate back to the evidence. A candidate summary format:

- What the document is, who produced it, and when.
- Its subject, scope, and main claims or findings.
- Evidence and methods described by the authors, where applicable.
- Limitations, uncertainty, and questions left unresolved.
- References to the pages or passages supporting substantive summary statements.

Summaries should describe what a document says without turning its claims into established facts. An official record may contain an unverified account; a research paper may be a preprint rather than peer-reviewed work. Publication type and review status should be recorded when known.

Keep generated summaries separate from original text and human annotations. Record the source version, model identifier, generation time, and summarization configuration so drafts can be revisited. Updating a source should flag affected summaries for regeneration or review.

Unreadable scans, missing pages, truncated processing, or poor OCR should be visible. A partial-document summary should say which portions it covers. Human review status should remain separate from the fact that a draft was generated.

Document text is research material, not instructions to the summarization system. Extracted content should not be able to trigger tool actions or override the summarization task.

### Search and research

Potential ways to explore the library:

- Full-text search with filters for source, author, date, document type, and subject.
- Semantic search for related concepts, alongside exact keyword and identifier searches.
- Question answering with citations to retrieved passages and an explicit indication when the collection cannot support an answer.
- Connections between documents through citations, named organizations, locations, dates, and events, with machine-suggested connections available for review.
- Comparisons between releases, revisions, and redactions.
- Saved collections and research notes.

Example: "Find released documents discussing this incident, show the relevant passages, and distinguish contemporary records from later analysis."

Cross-document synthesis should preserve disagreements and attribution. Several documents repeating the same underlying account should not automatically count as independent corroboration. Failure to find a document is not proof that it does not exist.

## Event investigation and globe

Explore a globe-based workspace for investigating an observation by location and time window. A user could describe an event, collect relevant records, and inspect available evidence on a shared timeline.

### Candidate layers

- **Satellite imagery:** Publicly available imagery with acquisition times, resolution, and cloud coverage. Availability and licensing vary; this is not assumed to be continuous, live coverage.
- **Internet-accessible cameras:** Business, street, traffic, weather, observatory, and other cameras available through existing camera websites, directories, or ordinarily accessible feeds. Intentional publication by the camera owner is not a blanket requirement, but OIA will not attempt to gain additional access. Exclude cameras inside people's homes and views into home interiors. Record position, viewing direction, approximate field of view, archive availability, and source access conditions when known.
- **Weather:** Cloud cover, visibility, wind, lightning, radar, and relevant conditions at altitude.
- **Flight activity:** Available aircraft tracks with source, timestamps, and known coverage limitations.
- **Astronomical and orbital context:** Relevant celestial objects, predicted satellite passes, and launch information, with prediction uncertainty where available.
- **Space observations and footage:** Public satellite and space telescope imagery, spacecraft camera footage, and mission broadcasts, connected to their observation times, instruments, targets, and available viewing geometry.
- **Reports and documents:** Witness accounts, original media, official records, and links into the document library.

The initial "globe stitch" could be a navigable mosaic of imagery, camera views, and other observations. A seamless visual reconstruction would require more work on calibration, overlapping viewpoints, and clock alignment; it is a separate research possibility.

### Event queries

Example: "For this location between 21:10 and 21:25 local time, find cameras facing northeast, available aircraft tracks, predicted satellite passes, weather conditions, and related reports."

Results should show what evidence exists, its relevance, and its gaps before suggesting explanations. Possible correlations remain candidates until timing, geometry, and other evidence support them. "Unresolved" means the available evidence does not settle the question.

Useful investigation behaviors include:

- Preserve the reported local time and timezone while normalizing times for comparison.
- Represent approximate witness times, camera clock offsets, and uncertain locations explicitly.
- Distinguish no available recording, a failed retrieval, and an available recording with no relevant observation.
- Distinguish live feeds from historical archives; a feed available today may have no footage of an earlier event.
- Keep original media separate from enhanced, annotated, or machine-analyzed derivatives.
- Make tracking coverage gaps visible; an absent aircraft record does not establish an empty sky.

## Space imagery, observatories, and footage

Extend the imagery library to publicly available space observations and video. Support both Earth-facing observations and views of the sky, Sun, planets, and other astronomical targets within the same research workspace.

### Candidate collections

- Earth-observation and weather satellite imagery, including dated scenes, mosaics, and time sequences.
- Space telescope observations and released imagery, including public Hubble and James Webb collections.
- Solar and heliospheric observatory imagery for solar activity and space-weather context.
- Public imagery from planetary orbiters, landers, rovers, and other spacecraft.
- Public space-station camera feeds, mission video, launch and docking broadcasts, and archived footage.

NASA, ESA, STScI/MAST, and mission-specific archives are candidate discovery starting points. Specific integrations, public release availability, access methods, and reuse terms remain to be evaluated. The existence of a public mission archive does not imply every observation is released or available in real time.

### Discovery and provenance

Search by mission, instrument, target, observation interval, wavelength or filter, and footprint or field of view where supported. Record observation IDs, source links, exposure start/end times, publication and retrieval times, processing level, calibration information, attribution, and associated scientific publications when available.

Retain original scientific products, such as FITS files when supplied, alongside readable previews. Distinguish individual exposures from stacked images, mosaics, false-color composites, and outreach renderings. Record preview stretches, channel mappings, and other transformations so presentation choices remain traceable to the source data.

### Live and archived footage

A feed browser could show active sources, mission context, observation or broadcast time, known delay, and archive availability. Distinguish live camera imagery, delayed transmission, replay, simulated mission views, and commentary. If capture time or latency is unknown, say so rather than treating receipt time as capture time.

Allow users to bookmark a moment or attach a permitted clip to an investigation, with the original source and time reference. Record interruptions and fallback screens as availability changes. A live broadcast does not guarantee continuous camera coverage or a retrievable historical recording.

### Connected Earth and sky views

Use the Earth globe for georeferenced imagery and spacecraft ground context, and a linked sky view for celestial coordinates and telescope pointings. Preserve the selected investigation and time interval when switching views. A telescope image belongs at its celestial target rather than being forced into a terrestrial map location.

Where pointing, ephemeris, and calibration data permit, show what an instrument could see during a selected interval. Account for coordinate frames, time standards, exposure duration, field of view, and positional uncertainty. Space telescope imagery generally targets specific astronomical fields; it should not be assumed to cover a local atmospheric event.

Example queries include "Find released observations of this target during this period," "Show available space-station footage around this timestamp," and "Which public instruments had relevant coverage for this event?"

Selected images and clips should open directly in the media workbench, link to related papers and mission records, and remain attachable to the evidence timeline and cited reports. Image anomalies can be investigated with calibration and instrument context, including detector artifacts, saturation, cosmic-ray hits, and compression effects, before assigning a physical interpretation.

### Solar system motion and planetary context

Add a solar system view showing Earth's rotation, axial tilt, and orbital path around the Sun, together with the positions, rotations, and orbits of other planets. Include the Moon as an important part of Earth-based observation context, with other moons and spacecraft as possible extensions.

Use one investigation clock across the Earth globe, local sky, and solar system views. Users could pause, scrub to a historical date, return to the current time, or accelerate playback to explore daily rotation and planetary motion. Moving between scales should preserve the selected time, observer location, and research context.

Candidate interactions include:

- Follow Earth or another planet along its orbit, with selectable past and future path segments.
- Show rotation axes, illumination, the day/night boundary, and seasonal geometry.
- Switch among Sun-centered, Earth-centered, and observer-centered views with the reference frame identified.
- Inspect distances, orbital positions, lunar and planetary phases, and apparent directions from a selected observation site.
- Trace a planet's apparent motion across the local sky and connect it to a report's time and viewing direction.
- Open related mission imagery, telescope observations, and research documents from the selected body.

Offer a readable illustrative scale and a true-scale mode, clearly identifying any exaggerated body sizes or compressed distances. Visual orbit trails and position readouts should derive from the same underlying position model. Geographic investigation tools should remain accessible as users zoom back toward Earth.

Use established ephemerides and rotation models for research calculations; NASA/JPL Horizons and SPICE are candidate references or data sources to evaluate. Record model and data versions, supported date ranges, coordinate frames, time standards, and applicable accuracy limits. Earth's precise orientation may require additional Earth-orientation data depending on the intended accuracy.

Distinguish a body's geometric position at a shared instant from its apparent position as seen by an observer. Light travel time, observer location, and the chosen apparent-position corrections matter for observation matching. Atmospheric refraction and local obstructions introduce additional uncertainty near the horizon.

For UAP research, this could answer: "Where were the Moon, Venus, and Jupiter from this location at the reported time, and how would they move through this camera's view?" A positional match is evidence to investigate alongside brightness, weather, timing uncertainty, and the report itself.

## News-based regional intelligence

Use multiple public news sources to construct geographic situation reports about possible military movements, reported criminal activity, public safety incidents, and other developments. Integrate these reports with the same map, timeline, document library, and evidence notebook used for UAP research.

### Collection and event construction

Candidate inputs include local journalism, national and international reporting, wire services, specialist publications, and public agency statements or incident datasets. Ingest through permitted feeds, APIs, and publication access, retaining source links and appropriately licensed text or excerpts.

Extract candidate events with reported location, event time or range, publication and update times, organizations involved, incident type, and supporting passages. Preserve the original wording and translations where used. A place mentioned in an article is not necessarily the event location, and publication time is not necessarily event time.

Cluster related coverage into editable event records. Detect syndicated articles, copied accounts, and common underlying statements so multiple publishers repeating one source do not appear to provide independent corroboration. Keep conflicting accounts and uncertain event matches available for inspection.

### Example research uses

- **Possible military movements:** Assemble publicly reported exercises, deployments, convoy sightings, port visits, and official announcements into a regional chronology. Separate announced plans, reported observations, and analyst inferences; a report of movement does not by itself establish destination, mission, or intent.
- **Reported criminal activity:** Summarize reported incidents by area and period, connect follow-up coverage, and distinguish allegations, investigations, arrests, charges, and court findings. Preserve attribution and later corrections or case outcomes.
- **Broader situation reports:** Combine relevant news with emergency notices, transport disruption, weather, and other available context to explain what has been reported in an area and what remains uncertain.
- **UAP context:** Surface contemporary local reporting, public exercise or launch announcements, and later investigations relevant to an observation window. Proximity in time or place is a candidate connection, not a settled explanation.

### Evidence and uncertainty

Evaluate support at the claim level. Show the underlying source, whether it is firsthand or derivative, independent corroboration when present, contradictory evidence, and geographic and temporal precision. An official statement is attributable evidence of what an agency says, not automatic verification of every underlying claim.

News coverage is an uneven sample of events. Article volume is not an incident count or a crime rate, and a lack of reporting does not establish safety or absence of activity. Any statistical rate or trend needs an appropriate dataset, denominator, consistent definitions, and coverage notes. The geographic view should focus on reported incidents rather than assign criminality to people or communities from association or proximity.

Retain article versions and record corrections, retractions, and changed case status. Updates should flag affected event records and report conclusions for reassessment. A removed article or unavailable page should remain distinguishable from a retraction.

### Claim checking and misleading coverage

Help users identify reporting that appears factually false or materially misrepresents a situation. Assess specific claims and presentations against inspectable evidence, then connect findings to the relevant article, publisher, event, and report.

Candidate checks include:

- Factual claims contradicted by relevant records, original media, or other independently supported evidence.
- Headlines or captions that assert more than the article or cited source supports.
- Quotes that omit qualifications or change meaning when compared with the full passage.
- Old, unrelated, or incorrectly located imagery presented as evidence of a current event.
- Numerical errors, misleading denominators, incompatible time periods, or selective comparisons.
- Claims of certainty or causation that exceed the available evidence.
- Material context omitted in a way that changes the apparent meaning, with the missing context and its relevance explained.

Each finding should show the exact claim or presentation, article version and publication time, supporting and conflicting evidence, an explanation of the discrepancy, and the scope of the assessment. Distinguish a reporter asserting a claim from accurately reporting that someone else made it. Separate checkable factual statements from opinion, satire, and forecasts; assess factual premises within those formats where appropriate.

Possible finding states include supported, contradicted, unsupported by the available evidence, materially misleading, disputed, and unresolved. Define the evidence needed for each state. Lack of corroboration alone does not establish falsity, disagreement among outlets does not settle a claim, and majority coverage is not independent verification. An assessment of misleading context should identify the specific distortion rather than infer deceptive intent.

AI-generated flags begin as reviewable assessments with citations and explicit uncertainty. Users should be able to inspect the underlying sources, challenge the reasoning, record a correction, and see review history. Distinguish information available when an article was published from evidence that emerged later. Reassess findings when either the article or the underlying evidence changes, and show corrections prominently beside the original issue.

The interface could show a highlighted passage with an expandable evidence comparison, accessible from both the article and the regional report. Any publisher-level overview should summarize documented findings within a stated collection and time range, including reviewed sample size, selection method, unresolved cases, and corrections. Avoid turning a few flagged articles into an unsupported universal reliability score for an outlet.

### Regional report experience

A user selects an area, time window, and topics, then receives an editable AI-drafted report with:

- An overview of reported developments and source coverage.
- A map and chronology of candidate events, with uncertain locations and dates represented honestly.
- Claim-level citations, source lineage, and supporting or conflicting accounts.
- Context from other OIA layers, with the rationale for each proposed correlation.
- Outstanding questions, changes since the previous report, and a clear last-updated time.

Selecting a report statement should open its evidence in the same workspace; selecting an event on the map should reveal the related passages and timeline. Saved regional queries could support change notifications, including corrections and retractions. Report generation should retain the query, source versions, grouping decisions, and analysis methods so the result can be reviewed and reproduced as far as source access permits.

## Financial markets, news, and events

Add a financial research workspace that connects market data to news, public disclosures, economic releases, and geographic events. Users could investigate how markets behaved around an event, compare competing explanations, and follow developments relevant to selected instruments or sectors.

### Data and context

Candidate coverage includes equities, indices, funds, bonds and yields, commodities, currencies, and digital assets, depending on available providers. Combine price, volume, and other supported market observations with company filings, earnings announcements, central-bank decisions, economic statistics, and OIA's news and event records.

Record provider, venue, currency, instrument identity, timestamps, sampling interval, and whether data is real-time, delayed, or end-of-day. Account for trading calendars, timezones, corporate actions, and adjusted versus unadjusted prices. Publicly viewable market data is not necessarily licensed for automated collection or redistribution; source selection and access terms remain open.

### Connected market investigation

Use linked charts, an event timeline, geographic context, and a source inspector. Selecting a price move should reveal potentially relevant reporting and disclosures; selecting an event should expose related instruments and the evidence supporting those relationships.

Candidate capabilities include:

- Event overlays on price, return, volume, and volatility charts.
- User-defined watchlists and alerts for unusual market behavior or new relevant reporting.
- Event studies comparing behavior before and after an event against an explicit benchmark and analysis window.
- Comparisons across sectors, instruments, regions, and historically similar events.
- Links among companies, facilities, suppliers, commodities, and regions, grounded in dated public disclosures rather than assumed exposure.
- Scenario exploration with explicit assumptions and sensitivity analysis.
- Cited AI-drafted market briefs that distinguish observed changes, source claims, candidate explanations, and projections.

Example: "Show how energy markets behaved around this reported shipping disruption, what was known at each point, and which alternative explanations were also present."

### Market prediction and event-impact scenarios

Support forward-looking market predictions using news, weather observations and forecasts, newly released or declassified documents, economic data, company disclosures, and other relevant public information. Users should be able to ask how an event could affect a selected market, instrument, sector, or commodity over a stated horizon.

Distinguish an evidence-based forecast under current conditions from a conditional scenario such as "What if this storm closes these ports for three days?" A scenario can estimate consequences without asserting that its assumptions will occur. Where event probabilities are available, keep them separate from the estimated market response conditional on the event.

Candidate outputs include direction estimates, return ranges, volatility forecasts, and probabilities of explicitly defined outcomes where the model supports them. Show the forecast origin, horizon, information cutoff, baseline, model version, and uncertainty. Prediction intervals and outcome probabilities need empirical calibration; an AI-generated confidence number is not sufficient.

#### Inputs and impact pathways

Connect an event to a market through an inspectable proposed mechanism, such as disruption to production, transport, demand, financing costs, policy expectations, or a company's disclosed exposure. Cite evidence for each exposure link and distinguish measured relationships from assumptions.

- **Weather:** Relate forecast changes and uncertainty to relevant production regions, energy demand, transport routes, or facilities. Use the forecast vintage available at prediction time rather than later observed weather when evaluating historical predictions.
- **News and announcements:** Identify new information, revisions, denials, and changes from previously announced expectations. Group repeated coverage by underlying source.
- **Declassifications and document releases:** Distinguish the historical events described from the date the material became publicly available. Assess whether the release adds information relevant to present markets; age, novelty, and current exposure matter.
- **Other public data:** Incorporate economic releases, filings, supply and inventory observations, and market conditions where their timing, quality, and relevance are established.

An event can have a substantial real-world effect but little incremental market impact if it was already anticipated. Forecasts should consider the surprise relative to available expectations and current prices, with competing explanations and countervailing effects visible.

#### Prediction experience

From an event or market view, users could choose instruments and horizons, inspect a baseline forecast, and adjust explicit scenario assumptions. Show predicted ranges over time alongside actual observations as they arrive, clearly separating the forecast from subsequent results. Explain which inputs changed when a forecast is revised.

Example: "Given the current storm forecast and documented regional production exposure, what are plausible effects on this energy market over the next week, and how do those estimates change if disruption is shorter or longer?"

Save every issued forecast as a versioned record. Preserve misses and revisions as well as successful predictions, and allow users to compare models, horizons, and historical analogues. If relevant evidence is too sparse or outside the model's validated scope, report that limitation rather than manufacture a precise forecast.

#### Validation

Evaluate models with chronological holdouts and walk-forward testing, comparing them with simple no-change, historical-rate, or other task-appropriate baselines. Track forecast error, interval coverage, probability calibration, and performance across horizons and market conditions. Keep tuning separate from evaluation and account for repeated model or feature selection.

Historical tests must use information available at each forecast origin. Later news revisions, revised economic series, future corporate membership, and knowledge embedded in newer language models can contaminate a retrospective test. Where point-in-time fidelity cannot be established, label the result exploratory and use prospective evaluation to measure actual forecasting performance.

Lean could help establish selected calculation or data-cutoff properties, but cannot prove that a market forecast will be accurate. Explanations and impact pathways remain hypotheses unless supported by an appropriate causal analysis.

### Timing and analytical integrity

Preserve event time, first-publication time, revision time, and when OIA received the information. A historical analysis should be able to use only information available at its selected cutoff, including the then-current versions of economic releases and news articles. Timestamp uncertainty matters when relating fast market moves to a publication.

Temporal proximity and correlation do not establish that a news item caused a price movement. Consider overlapping announcements, broader market moves, liquidity, and source latency. Repeated coverage of one story is not independent evidence of its effect.

Record benchmark choice, return calculation, window selection, sample size, and statistical assumptions. Address look-ahead bias, survivorship bias, and multiple testing when evaluating historical patterns. Any strategy simulation should identify fees, spreads, slippage, and out-of-sample evaluation assumptions. Simulated or projected performance remains distinct from observed market results.

Market investigations should share OIA's versioned evidence, review, and report-export workflows. This feature is analytical; brokerage connections and order execution are separate, undecided capabilities.

## Radio listening and spectrum workspace

Add a radio workspace where users can discover stations and channels, listen to available audio, and connect relevant broadcasts to an investigation. Candidate categories include public broadcast radio, weather broadcasts, amateur radio, and available aviation or maritime audio, subject to the source's access conditions and applicable local rules.

### Listening sources

- **Internet streams:** Public station and channel streams with source attribution, location context, and stream status. These provide the audio selected by the publisher, not arbitrary tuning across the radio spectrum.
- **Shared receivers:** Integrations with operator-published remote radio or software-defined radio (SDR) receivers, using their supported interfaces and permitted tuning ranges.
- **Local receivers:** Optional support for a user's compatible radio or SDR hardware, with an explicit connection and device capability information.

A frequency directory is a discovery aid; it does not provide reception by itself. Listening requires an available stream or a receiver within suitable reception range. Record the frequency, units, modulation, bandwidth where relevant, service, source, and available geographic coverage. Channel assignments and listening rules vary by jurisdiction, and directory entries need source references and update dates.

The initial scope is receive-only. Source integrations should respect receiver access controls and channel restrictions; encrypted or otherwise access-controlled communications are not made available by bypassing those controls. Recording and redistribution conditions should be tracked separately from listening availability.

### Listening experience

Provide a searchable station and frequency browser connected to the globe, with favorites, recent listening, and a persistent audio player across workspace views. Distinguish station or transmitter location, receiver location, and the reported location of an event; these are not interchangeable.

For supported receivers, expose frequency entry, tuning steps, modulation, bandwidth, squelch, and available signal measurements. Show a spectrum or waterfall when real receiver data supports it. Internet audio streams can have an audio waveform or spectrogram, labeled accordingly, rather than simulated radio-frequency telemetry.

Users could select an area to find relevant stations or receivers, open a channel from an investigation, or bookmark a moment on the shared timeline. Make playback state, source, volume, and stop controls easy to find. Distinguish live reception, delayed streams, and archived recordings, showing known latency or unknown timing.

### Audio evidence and research

Where permitted, save a timestamped clip with its source, receiver context, frequency, original audio, and recording or receipt times. Source event time may differ from the time OIA received the stream. A live feed does not imply an archive exists for earlier events.

Optional transcription, translation, and keyword detection could make recordings searchable and surface candidate events. Keep generated text aligned with the audio, mark uncertain or unintelligible passages, and allow corrections. Radio noise, clipped transmissions, call signs, and overlapping speakers make these useful review aids rather than authoritative transcripts.

Relevant clips could be attached to a UAP or regional investigation alongside weather, flight records, reports, and camera observations. Preserve attribution: a statement heard over radio is evidence of that statement, with its underlying claim still subject to corroboration. Live and archived audio should use the same evidence and source-lineage model as other media.

## Local hardware and personal drones

Let users connect their own compatible hardware to OIA, including personal drones, cameras, SDR receivers, and other observation devices. A device becomes another source in the shared workspace, with live data, recordings, health, and evidence available through the same tools as remote sources.

Personal drone integration should combine a live camera view, available telemetry, a UAV-style HUD, and direct access to image and video forensics. Users could inspect a scene, bookmark an observation, and carry the associated footage and telemetry into an investigation without changing applications.

### Device connections and capabilities

Use explicit device registration and adapters that describe the capabilities actually available: video, telemetry, recorded media, camera metadata, or supported controls. Candidate integration paths include manufacturer SDKs, supported network video protocols, local capture devices, and MAVLink telemetry on compatible platforms. Specific hardware support remains to be evaluated; a video connection does not imply telemetry or flight-control access.

Support importing recorded flights and media as well as live connections. Local processing and storage should be possible where the selected analysis tools support them, and connecting a device should not automatically publish its footage or location.

An initial integration could consume video and telemetry while the existing controller handles flight. Camera, gimbal, and flight controls are separate capabilities to scope explicitly if added later. Analysis tasks should run independently of device communications so a slow model or export does not stall telemetry handling.

### UAV-style HUD

Apply the military-terminal visual direction to a configurable overlay around the live video. Candidate readouts include:

- Heading tape, attitude indicator, horizon, and camera-center reference marker.
- Position, ground speed, vertical speed, and altitude with units and height reference.
- Drone orientation and gimbal direction, kept distinct when the camera moves independently.
- Battery, link quality, navigation status, recording state, and telemetry freshness where supplied.
- A linked map with the recorded flight path, current position, home point when supplied, and an estimated camera footprint where calibration and terrain data support it.
- Source timestamps, known video latency, analysis status, and event bookmarks.

Offer compact, detailed, and clean-video layouts. Unsupported values should remain unavailable; stale values should be visibly marked. Distinguish sensor measurements from derived estimates, and actual onboard thermal or infrared imagery from display color effects. A center marker indicates viewing direction, not a measured ground location or distance by itself.

### Forensics and observation workflow

Allow users to freeze a copy of a frame for inspection while live reception continues, select a region of interest, compare frames, stabilize footage, and examine motion or image artifacts. Where supported, apply anomaly detection and scene-object tracking as review aids, with overlays that can be hidden to inspect the source.

Drone motion, gimbal movement, vibration, rolling shutter, parallax, and exposure changes must be accounted for when interpreting apparent motion. A moving platform needs different baselines and motion compensation from a fixed traffic camera. Location and distance estimates require sufficient calibration and geometry, with uncertainty retained.

Save original video and telemetry separately from HUD-rendered recordings, enhanced images, and analysis overlays. Preserve source timestamps and clock alignment so later analysis can relate a frame to the device's position and orientation. A live preview may be lower quality than onboard recording; permit the original recording to be linked later without silently replacing the evidence used by an earlier analysis.

Selecting a bookmarked event should open the relevant video interval, telemetry, map position, and analysis results together. Users could then attach the event to a UAP or regional investigation, compare it with other sources, and include cited stills or clips in a report. Connection loss, dropped frames, and missing telemetry should remain visible in both live and replay views.

## Image and video analysis

Provide a media workbench within the investigation workspace, with synchronized original and processed views, zoom, frame navigation, and access to the source history. Analysis should be repeatable through saved operations, parameters, software or model versions, and input hashes. Processing creates derivatives while preserving the original file.

### Forensic tools

Candidate capabilities include:

- Metadata, file structure, codec, resolution, timestamp, and edit-history inspection where available.
- Channel separation, histograms, exposure and contrast adjustments, edge views, and frequency analysis.
- Noise and compression inspection, resampling indicators, and candidate copy-move or compositing detection.
- Video keyframe extraction, frame timing inspection, stabilization, optical flow, tracking, and frame alignment.
- Lens distortion correction when calibration is available, perspective analysis, photogrammetry, and multi-view geometry when observations support them.
- Terrain, skyline, landmark, shadow, and celestial comparisons for location and time assessment.
- Tests for possible optical or sensor effects, including motion blur, defocus, glare, rolling shutter, and compression artifacts.

Each tool should explain what its output can establish and what assumptions it needs. Compression diagnostics such as error-level analysis and automated manipulation detectors are investigative cues, not standalone authenticity verdicts. Camera motion, missing calibration, uncertain distance, and processing artifacts can limit motion and size estimates.

### Automated internet-camera anomaly detection

Monitor eligible internet-accessible business, street, traffic, weather, observatory, and other camera feeds for events worth reviewing. Start with existing camera websites and directories that already make feeds available through ordinary viewing. Intentional publication by the camera owner is not a blanket inclusion requirement. Exclude cameras inside people's homes and views into home interiors.

The access boundary is ordinary availability: browsing existing listings, following published viewing links, and using supported embeds, feeds, or APIs under their access conditions. OIA will not probe for hidden camera endpoints, guess credentials, exploit devices, or bypass authentication or other access controls. If a source becomes restricted or unavailable, mark it accordingly rather than attempting to recover access through another route.

Record the listing page, feed provider, available attribution, access method, and known conditions for automated analysis, recording, and redistribution. A directory listing does not by itself settle every downstream use; ordinary viewing and automated retention may have different conditions. Non-home settings can also contain private activity, so assess the actual view and use appropriate exclusions or masking.

An anomaly means a departure from a defined rule or a camera's expected observations. It does not by itself identify a UAP, crime, emergency, or physical object. Findings should describe visible behavior and the reason for the flag.

Candidate detection categories include:

- **Sky events:** Transient lights, flashes, persistent moving objects, and unusual apparent motion within a selected sky region.
- **Road and scene events:** Possible stopped vehicles, wrong-way travel in a configured lane, obstructions, abrupt traffic changes, or smoke-like visual changes.
- **Environmental changes:** Rapid visibility changes, flooding cues, or other sustained departures from a scene's baseline.
- **Camera health:** Frozen or repeated frames, outages, timestamp jumps, camera repositioning, exposure changes, or severe compression problems. These should have their own status rather than silently becoming scene-event alerts.

#### Detection and evidence pipeline

1. Register the source, expected update cadence, viewing geometry when known, permitted uses, and regions to analyze or mask.
2. Ingest available frames with source timestamps, receipt times, clock uncertainty, and quality indicators. A periodically refreshed still provides different evidence from continuous video.
3. Apply suitable methods such as background modeling, frame differencing, optical flow, object detection, tracking, or temporal anomaly models. Select methods by camera capability and event type rather than assuming one universal detector.
4. Create a candidate event with the trigger, affected frames or region, duration, model and configuration versions, and available context before and after the trigger.
5. Correlate candidates with nearby cameras, weather, astronomical context, and available flight data, then place them in a review queue and optionally notify users according to their saved rules.

Baseline models should account for day/night conditions, weather, camera mode, and viewpoint changes. Show when a baseline is still learning or has become invalid. Camera motion, insects near the lens, rain, glare, aircraft lights, and compression artifacts are useful evaluation cases.

Detection should use original frames or documented preprocessing. Predictive upscaling and generated recreations must not supply invented detail as detection evidence. Apparent pixel speed should not be presented as physical speed without adequate geometry and distance information.

Where recording is permitted, a bounded rolling buffer could preserve context before and after a trigger. Retention should be configurable, with selected event evidence saved separately from expiring routine footage. If only a still or partial clip was captured, the event should state that limitation.

#### Review and investigation experience

Show candidate events on the shared map and timeline. Opening one should reveal original footage, an optional detection overlay, the trigger explanation, camera health, time uncertainty, and possible contextual matches. Deduplicate repeated alerts from the same event and allow users to adjust sensitivity, regions, persistence thresholds, and notification frequency.

Review outcomes could include camera artifact, likely ordinary explanation, insufficient evidence, or retained for investigation, with notes and revision history. A detector score is not a calibrated probability that an event is anomalous or has a particular explanation. Automated event descriptions should avoid attributing intent or criminality from an unusual pattern alone.

Cross-camera correlation should show its timing and geometric assumptions. Two feeds may rebroadcast the same underlying camera, so source lineage matters. Triangulation requires independent viewpoints, adequate calibration, timing alignment, and justified correspondence between observations.

#### Evaluation and initial scope

Start with a small set of eligible, stable cameras and a limited event category, such as sky transients or stopped vehicles. Evaluate with annotated original footage across different conditions and camera sites, recording false alerts per camera-hour, detection rates for labeled events, latency, and processing cost. Keep evaluation footage separate from baseline fitting and threshold tuning.

Missed frames, processing downtime, and inadequate coverage should remain visible: no alert does not establish that no event occurred. Review feedback can inform versioned detector improvements, with changes evaluated before being treated as better detection.

### Predictive upscaling and restoration

Support image enlargement and restoration, including conventional interpolation, denoising, deblurring, multi-frame reconstruction, and learned super-resolution. Let users compare methods and inspect sensitivity to settings or model choice.

Predictive methods can create plausible details that were not resolved in the input. Increased pixel count is not increased evidentiary resolution. Label predictive outputs accordingly, retain the original beside them, and avoid treating newly generated shapes, markings, or textures as observed features. Multi-frame methods should record the contributing frames and alignment assumptions.

Measurements should use original observations or a validated reconstruction method with stated error bounds. Attractive or stable-looking generated detail alone is not validation. Derived images should retain processing labels and provenance when attached to reports or exported.

### Recreation prompt export

Generate an editable image-generation prompt for a hyperrealistic illustrative recreation based on selected reports and available imagery, including limited or low-quality images. This is a way to communicate an account or hypothesis; the recreation is not new evidence about the event.

A prompt export could contain:

- Scene location, period, time of day, weather, lighting, and observer viewpoint when supported.
- Reported appearance, color, motion cues, and apparent angular size or uncertain scale.
- References to selected accounts and images, with source identifiers and a manifest of accompanying reference files.
- A separation between observed or reported details, researcher assumptions, and unresolved attributes.
- Instructions to avoid adding unsupported identifying details, together with alternate prompts for materially different accounts or hypotheses.
- A request for hyperrealistic rendering and a visible caption identifying the result as an illustrative recreation.

The prompt should be portable text with source notes and optional reference assets, rather than requiring one image provider. Reference assets would be included only where their use is permitted. Unknown physical dimensions should remain unspecified or explicitly hypothetical; conflicting accounts should not silently become one apparently definitive scene.

Prompt wording cannot guarantee factual fidelity or reliable labeling by an external generator. If generated images are brought back into OIA, store them as recreations, retain the prompt and available generation settings, and apply visible labeling in OIA's report and image exports. Recreations and predictive details should not be counted as corroboration in subsequent analysis.

## Reasoning and analysis tools

OIA will include deduction, extrapolation, correlation, and abstraction tools. Their interfaces and implementations remain exploratory. They could operate across the document library, event observations, and saved investigations, with outputs retained as inspectable research artifacts.

### Deduction

Derive conclusions from explicit premises and rules. Show the reasoning chain, where each premise came from, and which premises are assumptions or disputed claims. A conclusion follows conditionally on its premises; a valid deduction does not establish that those premises are true.

Example: Given bounded observation times, calibrated camera fields of view, and a candidate trajectory, determine whether that trajectory could intersect both views during the observation windows. Any exclusion applies to the stated bounds and model.

Selected formal deductions could have Lean-checked proofs. AI-proposed reasoning should remain distinguishable from mechanically checked derivations.

### Extrapolation

Project beyond observed data under an explicit model, such as estimating a possible continuation of a trajectory or extending a measured trend. Show the observed interval, projection horizon, assumptions, and sensitivity to changes in those assumptions. Where uncertainty bounds are available, explain how they were obtained.

Example: Extend an estimated trajectory under several speed and direction assumptions to identify which camera views might be worth examining next. Preserve alternative scenarios and distinguish projections from recorded observations. Apparent motion in an image alone does not establish an object's distance or physical speed.

### Correlation

Find possible relationships across time, location, document references, reported characteristics, and measured signals. Separate statistical associations from similarity matches and proposed links between records.

Example: Compare an observation window with flight tracks, weather, satellite passes, and other reports, then surface candidate matches with supporting and conflicting evidence.

Record matching criteria, time and location tolerances, data coverage, and source dependencies. Repeated accounts derived from one source should remain connected to that source. Statistical exploration should account for baseline rates, selection effects, and multiple comparisons where relevant. Correlation alone does not establish causation or that two records describe the same event.

### Abstraction

Turn detailed material into reusable concepts, structured event descriptions, timelines, taxonomies, or relationship graphs. Let users compare reports that use different vocabulary while retaining meaningful differences.

Example: Extract observation time, location, viewing direction, duration, reported motion, and source references into a common event representation. Preserve the original wording alongside normalized fields, unknown values, and conflicting accounts.

Every abstraction should link back to its source material and expose what was omitted or generalized. Machine-suggested categories and relationships should be editable and versioned.

### Connected reasoning workflows

The tools could be composed: abstract reports into comparable observations, correlate candidate records, deduce consequences under selected premises, and extrapolate where additional evidence might be found. Users should be able to inspect and revise each step independently.

Each result should retain its inputs and source versions, method or model, parameters, assumptions, supporting and conflicting evidence, and review status. Changed or withdrawn inputs should flag dependent results for reassessment. Feeding a generated result into another tool must preserve its status as an inference rather than silently promoting it to an observation.

Confidence, when reported, needs a defined meaning and supporting method. Model-generated confidence scores should not be presented as calibrated probabilities without validation.

## Historical precedents and general forecasting

Extend prediction beyond financial markets: examine what followed comparable historical events and use those precedents to explore possible responses to a current or hypothetical event. The central question is: "When events like X happened before, what responses followed, from whom, and under what conditions?"

Potential subjects include diplomatic responses, policy changes, public statements, institutional investigations, humanitarian assistance, trade and transport disruption, scientific follow-up, disclosure activity, and market reactions. A global response is a collection of potentially different responses across countries, institutions, industries, and communities, rather than a single uniform outcome.

### Event and response histories

Build sourced sequences connecting an initiating event with subsequent responses. Record event characteristics, relevant prior conditions, actors, locations, response types, elapsed time, and the supporting accounts. Distinguish announced intentions, actions actually taken, and reported outcomes.

Keep temporal sequence separate from a claim of causation. A response may have multiple causes, and a source's attribution of motive should remain attributed. Store event time and information-availability time separately so later disclosures do not silently become knowledge available to an earlier forecast.

### Finding useful precedents

Retrieve candidate analogues using explicit features such as event type, scale, geography, actors, institutional context, existing tensions, economic conditions, and information quality. Explain the match and important differences; similar wording alone does not make two events comparable.

Let users inspect and revise the comparison set and matching criteria. Include counterexamples, divergent responses, and cases where an expected response did not occur. Distinguish documented non-response during a defined observation window from missing coverage or a response that may still occur. Related events within one episode should not automatically count as independent precedents.

Historical coverage, changing institutions, technology, and reporting practices affect how well a precedent transfers to the present. Show the sample, coverage gaps, and sensitivity to alternative comparison sets. If useful precedents are too few or too different, return a qualitative assessment or insufficient-evidence result.

### Scenario and prediction outputs

For a selected event, build possible response sequences with explicit time horizons and conditions. Each branch should identify the supporting precedents, proposed mechanism, relevant differences, and observable developments that would strengthen or weaken it.

Candidate outputs include:

- A sourced timeline of comparable events and the responses that followed.
- Typical response sequences and timing ranges within the selected historical sample.
- Conditional scenarios for different actors and regions, including alternative or absent responses.
- Predictions for explicitly defined outcomes where the available evidence and model support them.
- Indicators to monitor and links to OIA's news, document, market, and geographic sources.
- Updates showing which new evidence changed a forecast and which assumptions remain unresolved.

Observed historical frequencies describe the selected sample; they are not automatically calibrated probabilities for a new event. Multi-step scenario probabilities must account for dependencies rather than treating every response as independent.

Example: "For a newly announced disclosure of this kind, find comparable releases, show subsequent hearings, research activity, and official responses, and identify which developments would support similar outcomes now."

### Forecast records and evaluation

Save the forecast origin, information cutoff, comparison set, method, assumptions, outcome definitions, and resolution dates before outcomes are known. Version revisions without replacing the original prediction. Preserve unsuccessful forecasts and unresolved outcomes alongside successful ones.

Use chronological evaluation and prospective tracking to assess predictions against appropriate base-rate baselines. Measure calibration and outcome accuracy where probabilities are issued, and evaluate predicted response times separately. Apply the market-forecasting safeguards around future-information leakage and retrospective language-model knowledge to these forecasts as well.

The workspace should make the path from historical evidence to proposed future response inspectable. Historical recurrence can inform a forecast without making the future deterministic.

## Additional intelligence research tools

These ideas extend the shared research workflow; their scope and priority remain open:

- **Competing hypothesis workbench:** Compare explanations against the same evidence, expose contradictions and assumptions, and identify observations that would distinguish them. Analytical scores are not automatically probabilities.
- **Collection planning and gap analysis:** Turn a research question into evidence needs, candidate sources, and next steps. Identify which additional observation could most usefully reduce uncertainty.
- **Link analysis and entity resolution:** Explore connections among documents, organizations, contracts, grants, facilities, and events using public records. Preserve relationship types and the uncertainty of proposed identity matches.
- **Geospatial change detection:** Compare dated imagery for visible developments, with capture geometry, resolution, seasonal effects, and coverage available for inspection.
- **Maritime and aviation analysis:** Relate available vessel and aircraft tracks, port information, navigation and airspace notices, and launch schedules to an investigation. Historical availability and coverage vary.
- **Scientific and technical intelligence:** Follow papers, patents, grants, and technical reports across subjects such as sensors, propulsion, atmospheric physics, and imaging. Distinguish proposals and patents from demonstrated capabilities.
- **Indications and warning:** Monitor user-defined indicators and explain why an alert fired, using recorded baselines and evaluation of false positives.
- **Source lineage and corroboration:** Trace claims to their earliest available sources and detect circular citations or copied accounts. Preserve the difference between repetition and independent evidence.
- **Public records requests:** Identify likely custodians, draft requests, track responses and applicable deadlines, and connect released records to the investigation. Actual submission remains a deliberate user action.
- **Multilingual research:** Search and translate across languages while retaining originals, alternate names, transliterations, and local date conventions.
- **Structured analytical review:** Check assumptions, search for contradictions, test sensitivity, and propose alternative interpretations with citations. Agreement among AI outputs is not independent corroboration.

Media verification and geolocation are covered by the image and video workbench above.

## Shared evidence notebook

A possible foundation across tools: save documents, observations, media, queries, reasoning results, and annotations into an investigation workspace. Preserve the dependency links between evidence and derived results so users can revisit how a conclusion was reached.

Each finding should retain its source and distinguish direct observation, a source's claim, an inferred connection, and an unresolved question. Timelines and exports should preserve those distinctions and link back to original evidence. Personal notes and sensitive observation locations should have deliberate sharing controls.

## Research operations and fieldwork

Extend the research workspace with tools for collecting better observations, coordinating investigators, and making results reproducible. These are accepted product directions; interfaces and delivery order remain exploratory.

### Personal observatory mode

Combine a user's compatible sky camera, weather station, SDR, and other sensors into a persistent observation station. Show coverage, device health, clock synchronization, and searchable observation history. An event could preserve relevant recordings across connected devices, subject to each device's actual coverage and configured retention.

### Witness report builder

Guide users through recording location, approximate time, duration, viewing direction, apparent size, motion, sound, and conditions. Allow sketches and a path drawn against a reconstructed sky. Preserve the initial account before showing candidate explanations, along with later edits and their reasons, to distinguish original recollection from subsequent interpretation.

### Observation planning

Plan observations using weather forecasts, light pollution, terrain obstructions, celestial visibility, and camera coverage. Identify useful equipment or additional viewpoints for a specific research question. Plans should identify which inputs are forecasts, estimates, or confirmed resources.

### Calibration workbench

Help establish camera orientation, lens distortion, field of view, and clock offsets using suitable stars, landmarks, or reference recordings. Save calibration versions, reference evidence, residual errors, and validity conditions. Moving or reconfiguring a device can invalidate an earlier calibration.

### Multi-observer event reconstruction

Synchronize independent recordings and evaluate candidate trajectories using their timing and geometry. Show uncertainty regions and alternative solutions when constraints are incomplete. Preserve the observations and assumptions supporting each reconstruction; two views do not guarantee a unique or accurate solution.

### Collaborative investigations

Support shared cases with roles, annotations, evidence requests, competing interpretations, and review history. Preserve contributions and disagreements without silently overwriting another researcher's conclusions. Sensitive locations and unpublished material should have explicit sharing controls, and publication should be separate from saving work within a case.

### Reproducible investigation packages

Export permitted source material or references together with hashes, timestamps, queries, grouping decisions, calibration, analysis parameters, and software/model versions. Include a manifest of missing or externally restricted assets. Another researcher should be able to replay supported analyses and understand where reproducibility is limited by changing sources, unavailable models, or nondeterministic processing.

### Validation and simulation lab

Evaluate algorithms against labeled observations of known aircraft, satellites, meteors, optical artifacts, and camera faults. Generate explicitly labeled simulated observations with controlled blur, noise, compression, geometry, and clock drift. Keep simulated material distinct from real evidence and assess transfer to real observations separately. Reusable fixtures can support Rust regression tests and appropriately scoped Lean specifications; simulation alone does not establish real-world performance.

### Offline field mode

Prepare an area with permitted maps, sky predictions, documents, device profiles, and observation forms before losing connectivity. Record observations locally and synchronize later while preserving capture times, clock uncertainty, and revision history. Resolve conflicting edits explicitly and label cached forecasts or expired source material.

### Research leads and unresolved questions

Maintain a living list of unknowns, evidence that could resolve them, potential sources, assigned researchers, and progress. Connect each lead to the claim or hypothesis it affects. New evidence should surface relevant open questions for reassessment and retain the history of resolved or abandoned leads.

## Other tool ideas to retain

- **Local situation monitor:** Geographic timelines of weather alerts, emergency notices, road closures, utility outages, and local government updates.
- **Personal exposure audit:** Help people examine their own public information and possible account connections, with uncertainty attached to inferred matches.
- **File privacy inspector:** Locally inspect photos and documents for identifying metadata, explain findings, and offer cleaned copies.
- **Public change tracker:** Monitor public pages and datasets, preserve dated snapshots, and explain changes. Could also power document-release monitoring.
- **Personal infrastructure observer:** Explore the public footprint of domains and services a user owns, including DNS and certificate changes.

## Existing groundwork and selective Rust migration

The existing God's Eye View project at `C:\Users\engin\Desktop\New folder\gods-eye-view` is a source of prior implementation work. OIA will select important capabilities and lessons from it and port or adapt them to Rust. The intent is selective reuse, with OIA's research workflow and evidence model guiding the result.

An initial inspection identified useful starting points in provider integrations, aircraft records, satellite pass calculations, camera source handling, weather timelines, recent imagery, structured query tools, and versioned scene/data-pack formats. Existing tests and documented failure cases are part of the reusable groundwork.

See [selective reuse notes](prior-art.md) for the inspected revision, concrete source paths, migration considerations, and attribution requirements. This is a source inventory, not runtime verification. Display fallbacks, inferred positions, and synthetic camera substitutes must remain distinguishable from observations when adapting the existing behavior.

Implementation choices remain open, including the renderer and UI technology. Prior work should inform a unified OIA experience without imposing the reference application's layout or complete feature catalog.

## Rust and Lean research opportunities

Candidate proof targets include time-window matching, geographic filtering, coordinate transformations, viewing-cone intersection, timeline ordering, structured privacy filtering, and document diff reconstruction. For reasoning tools, additional candidates include soundness of a bounded rule system and preservation of specified properties when converting observations into an abstract representation.

For each selected target, define the specification and assumptions, prove bounded properties, and document how the Rust implementation corresponds to the model. Tests and other implementation checks can support that connection. These proofs would not establish external data accuracy, AI-summary faithfulness, or the truth of an event explanation.

## Possible starting slices

These are options for discussion, not an agreed delivery order:

1. **Document research:** Import a small public collection, preserve originals and metadata, extract text, search it, and generate cited AI-drafted summaries.
2. **Event reconstruction:** Support one region with a map or globe, a time window, weather, available flight history, and a small catalog of eligible internet-accessible cameras under the camera scope above.
3. **Personal privacy:** Build a local file metadata inspector as a self-contained tool.

The document library could provide useful research capabilities before broader sensor and imagery integrations are available.

## Open questions

- Which document collections and research subjects should seed the library?
- Should the first experience be a local desktop tool, a hosted service, or a combination?
- Should AI summarization run locally, through a configurable provider, or both?
- How should summary review, correction, and citation validation work?
- What common navigation and evidence model will connect maps, timelines, media tools, and report writing?
- Which forensic methods and predictive imaging models can be meaningfully validated against known reference material?
- Which report and recreation-prompt export formats should be supported first?
- Which personal drone, camera, and SDR integrations should be supported first, and what video, telemetry, and control capabilities do their interfaces expose?
- Which markets, instruments, and data providers should seed financial research, and what historical depth and update frequency are needed?
- What collaboration, offline synchronization, and portable investigation-package formats should be supported first?
- What retention, storage, and access model is practical for documents and permitted camera archives?
- Which region and historical data sources could support a useful first event investigation?
- Which algorithm would provide the first meaningful Rust/Lean verification example?
