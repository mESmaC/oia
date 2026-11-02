# OIA workspace: UI and UX design draft

Version: 0.2 draft. Date: 2026-11-02. Status: design proposal for the investigation workspace ([#16](https://github.com/mESmaC/oia/issues/16)). Production workspace behavior is not implemented or approved by this draft. The companion prototype implements only the fixture interactions listed in §15. Where a section proposes a value, it is a starting point for the implementing session to confirm, not a shipped default.

This draft covers requirements CASE-01 to CASE-05 and UX-01 to UX-05 from the [product specification](../spec.md), and the presentation obligations in [architecture](../architecture.md) §1.1, §7, and §8. It is written to be independent of decision D01 (native renderer versus rendering adapter). Everything below is expressed in layout, state, token, and interaction terms that either technology can realize. The D01 ADR belongs to [#9](https://github.com/mESmaC/oia/issues/9); this document does not settle it.

Companion artifacts:

- [Shell prototype](mockups/workspace-shell.html): a self-contained, interactive HTML page with schematic case geometry, a source reader, and fixture search. It preserves the black canvas and glass instruments while adapting to narrow screens. All case content is synthetic; nothing is live or persisted. Open the file directly; no server or dependencies are required.

## 1. Design principles

These follow from the shared research rules in the specification §2 and the evidence contract in `AGENTS.md` §2. Each one changes what the interface is allowed to show.

1. **The evidence is the canvas.** Glass panels frame the thing under examination; they never sit on top of the part being read. When a panel must overlap the canvas, it is dismissible and the canvas keeps its position.
2. **Every readout is either data or a labeled estimate.** A coordinate, time, count, or status that appears on screen comes from a domain record, a declared estimate with its method, or a fixture that says so. No decorative telemetry, placeholder activity, or animated numbers without a source.
3. **State is never color alone.** Every semantic color pairs with a glyph and a short text label, and generated or simulated material also differs in border treatment. This is required by UX-03 and confirmed necessary by the palette check in §8.1.
4. **Context travels with the user.** Case, area, interval, investigation clock, and selection persist across instruments. A view that departs from the shared context shows a visible local-filter chip. Every drill-down has a route back.
5. **Work is visible, cancellable, and honest about completeness.** Long-running jobs show real progress and partial extent. A result produced for a superseded query is never applied to the current view.
6. **Saving is private. Publishing is a separate verb.** Save, export, share, and publish are distinct actions with distinct wording and placement.
7. **Polish does not imply certainty.** An unsupported claim, a partial extraction, or an approximate time looks finished and professional while remaining visibly qualified.
8. **Effects are decoration, not capability.** Blur, depth, motion, and sound can be reduced or removed without changing a single data value or analysis result.

## 2. Information architecture

A workspace holds cases and a library. A case opens into a set of instruments that share one context (§4). Instruments map to the specification modules as follows. "Phase" is the proposed delivery phase from [delivery](../delivery.md) §1, not a commitment.

| Instrument | Specification modules | Central canvas | Inspector shows | Timeline contributes | Phase |
| --- | --- | --- | --- | --- | --- |
| Case | CASE-01..05, INTEL-01 | Case home: question, scope, evidence inventory, hypotheses, leads | Case record, visibility, collaborators | Case interval | P0/P1 |
| Library | DOC-01..05 | Search results, collections, document reader | Source record, extraction coverage, claims, notes | Publication, revision, retrieval times | P1 |
| News | NEWS-01..05 | Event groups, article lineage graph, claim review | Article version, syndication links, assessment | Event and publication times | P1 |
| Report | COLLAB-02, COLLAB-03 | Structured draft editor with statement-to-evidence links | Evidence for the selected statement, citation status | Report revisions | P1 |
| Export | REPRO-01..03, COLLAB-03 | Export preview with omissions and redactions | Manifest, bundled versus referenced assets | None | P1 |
| Globe | GEO-01..05, INTEL-03 | Map or globe with areas, points, footprints, viewing cones | Selected event, coverage per source, calibration limits | Observation and capture times | P2 |
| Sky | SPACE-04, SPACE-05, GEO-05 | Observer sky and solar system views on the investigation clock | Ephemeris version, frame, geometric versus apparent | Investigation clock scrubbing | P2 |
| Space archive | SPACE-01..03 | Catalog mosaic, mission feed player | Instrument, processing level, color mapping, feed mode | Observation intervals | P2 |
| Witness | FIELD-02, GEO-01 | Account form with sky sketch | Initial account (frozen) and later edits | Reported time with uncertainty | P2 |
| Cameras | CAM-01..05 | Directory, feed player, candidate review queue | Source conditions, health, trigger explanation | Clip times, coverage loss | P3 |
| Media | MEDIA-01..04 | Forensic workbench: original versus derivative, frame navigator | Container metadata, processing history, derivative class | Frame timing | P3 |
| Radio | RADIO-01..03 | Directory, player, receiver spectrum when available | Source, latency, receiver versus transmitter location | Bookmarks | P3 |
| Devices | DEVICE-01..04, FIELD-01 | HUD over video, flight path on map | Capability set, clock offset, health | Telemetry samples | P3 |
| Reasoning | REASON-01..04 | Hypothesis comparison, derivation steps, graphs | Premises, assumptions, step provenance | Analysis runs | P4 |
| Markets | MARKET-01..05 | Linked charts and event studies | Instrument identity, adjustment basis, provider delay | Event overlays | P5 |
| Forecasts | FORECAST-01..04, MARKET-03..05 | Scenario builder, analogue comparison set | Frozen origin, cutoff, outcome definition | Forecast vintages and resolutions | P5 |
| Privacy | PRIVACY-01..03 | File inspector, exposure audit, change monitor | Findings with format-specific removal claims | Snapshot dates | P6 |
| Jobs | CASE-04, architecture §6.1 | Job list with attempts and outputs | Attempt details, input versions, error | Run intervals | P0/P1 |
| Settings | UX-04, UX-05 | Appearance, effects, accessibility, sound, providers, devices | Explanation of each setting's effect | None | P1 |

Instruments that are not available in a build are shown in the navigation rail as unavailable with a plain reason ("Not in this build" or "No provider configured"). They are not hidden, and they are not shown as if they worked.

## 3. Shell layout

The P1 release must be usable without a globe, so the shell is built around a generic evidence canvas rather than a map.

```text
┌──────────────────────────────────────────────────────────────────────────────────────┐
│ A  Command field  [scope: this case ▾]  search, id:, near:, t:, in:        ⌘K / Ctrl+K │
│ G  Context strip   CASE  ›  AREA (optional)  ›  INTERVAL  ›  SELECTION    [route back] │
├────┬─────────────────────────────────────────────────────────────┬─────────────────────┤
│ B  │ C  Evidence canvas                                          │ D  Inspector        │
│ nav│    (document reader, search results, globe, player, editor) │    Source           │
│ rail                                                             │    Claims           │
│    │    local-filter chip appears here when the view departs     │    Notes            │
│    │    from the shared context                                  │    Lineage          │
│    │                                                             │                     │
├────┴─────────────────────────────────────────────────────────────┴─────────────────────┤
│ E  Shared timeline    investigation clock · mode badge · marks for distinct time kinds  │
│ F  Job tray (collapsed strip; expands upward)   2 running · 1 failed · 1 superseded     │
└──────────────────────────────────────────────────────────────────────────────────────┘
```

Region behavior:

- **A. Command field.** Always present, single line, keyboard reachable from anywhere. Accepts free text, exact identifiers, locations, and dates (§5.2). Shows the active search scope as a chip. Never auto-executes a mutation.
- **G. Context strip.** The shared context as a breadcrumb: case, optional area, interval, current selection. Each segment is a control that opens the matching editor. The route-back control restores the previous canvas, timeline, and inspector state as one step.
- **B. Navigation rail.** Compact icon-and-label list of instruments. Collapsed shows icons with visible text labels on focus or hover; expanded shows labels permanently. Unavailable instruments stay listed with a reason.
- **C. Evidence canvas.** The one region that changes per instrument. Owns its own local filters and shows a local-filter chip when they differ from the shared context (CASE-02). Corner brackets mark the region that currently holds the selection. A view-state readout strip along its bottom edge lists only facts about the current view (instrument, page, anchor, selection size, pinned version, clock, overlay state); it never shows ambient figures.
- **D. Inspector.** Contextual to the current selection. Tabs are fixed per object type so the same tab is in the same place across instruments: Source, Claims, Notes, Lineage, Runs. Source fields are read-only and visually separated from user notes (CASE-03).
- **E. Shared timeline.** One investigation clock. Marks distinguish event, capture, publication, revision, retrieval, and analysis times with different glyphs (§7.6). Scrubbing moves the investigation clock for instruments that follow it. The mode badge states live, historical, replay, or simulated.
- **F. Job tray.** Collapsed count strip by default; expands to the job list. Each entry shows state, real progress, partial extent, and cancel or retry (§5.3).

Layout variants:

- **Narrow widths (under about 1100 px):** the inspector becomes a right drawer, the rail collapses to icons, and the timeline collapses to a one-line clock with an expand control.
- **Focused reading:** hides rail and timeline, widens the canvas, keeps the command field and inspector reachable by keyboard.
- **Clean evidence:** hides overlays, annotations, selection brackets, and grid ticks on the canvas while keeping the underlying evidence and the inspector. The clean state is visible in the context strip so the user knows overlays are off.

## 4. Shared context and navigation

The shared context is one record, held by the application layer and restored on restart (A01):

- Selected case and its visibility.
- Optional area: point, polygon, or approximate region with uncertainty.
- Interval: start and end with their time reference and approximation.
- Investigation clock: a single instant inside the interval, plus mode (live, historical, replay, simulated).
- Selection: the current object ID and version.
- Route stack: the ordered list of prior (instrument, canvas state, selection) entries.
- Per-instrument local filter deltas.

Rules:

1. Switching instruments keeps the context. The new instrument renders against it and shows a local-filter chip only for deltas it owns.
2. Selecting an event or object from any instrument updates selection, moves the timeline focus to the object's relevant times, and loads the inspector in one coordinated transition. The route stack gains one entry. This is UX-02's coordinated selection.
3. Route back restores exactly the previous entry, including scroll position and local filters. Browser-style forward is not required.
4. Opening a source passage from a report statement, a claim, or a search result is a selection, so it is always one route-back away from where the user was (COLLAB-02).
5. Local filters never silently become shared context. Promoting a local filter is an explicit action in the chip.
6. The context strip and the timeline always agree. If a view cannot honor the investigation clock (a document has no clock), it says "not time-indexed" rather than showing a stale value.

## 5. Core interaction patterns

### 5.1 Create a case (CASE-01)

A single form, reachable from the command field and the Case instrument:

- Question (required), title (defaults from the question, editable).
- Area (optional): type a place name, paste coordinates, or draw later. Stores the approximate region and its uncertainty; an unknown area stays unknown.
- Interval (optional): start, end, time reference. Supports "approximately" and open-ended bounds. Reported local time and timezone are preserved as entered.
- Topics (tags), visibility (private by default).
- Primary action is **Save**. The form states "Saving keeps this case private" next to the button. Publishing does not exist on this form.

### 5.2 Command field and search (CASE-05)

- Free text searches titles, extracted text, and notes within the scope chip (this case, library, everything accessible).
- Prefixes narrow the query: `id:` exact identifier or hash prefix, `near:` place or coordinates, `t:` date or interval, `in:` collection or source. Prefixes are discoverable from a help row that appears when the field is focused.
- Semantic retrieval is a toggle on the results view, off by default, and labeled with the model identifier when on. Results from exact matching and semantic retrieval are grouped separately.
- The results header always shows: scope, filters, result count, pagination, and a coverage line ("Searched 1,204 indexed passages from 38 documents. 2 documents not indexed: extraction pending."). An empty result is distinguished from a failed search and from an unindexed scope.
- Saved searches keep their definition and revisions. Editing one creates a new revision and shows the previous one.
- Enter runs a query. The command field never issues a command that mutates state without a confirmation step inside the canvas.

### 5.3 Long-running work (CASE-04, architecture §6.1)

Each job entry in the tray shows:

- Name, target (the case or object), and the job state from the architecture: queued, running, succeeded, partially succeeded, failed, cancelled, superseded.
- Real progress where the worker reports it, as a fraction with units ("7 of 9 pages"). Indeterminate work shows an indeterminate bar labeled "progress not reported", never a fake percentage.
- Cancel for queued and running jobs; retry for failed and cancelled; "open result" for succeeded and partially succeeded.
- Partially succeeded jobs state their extent ("OCR complete for 7 pages; pages 4 and 8 unreadable") and the result view carries the same coverage statement.

Supersession: when the user changes the query or selection while a job is running, the job is marked superseded. If it later completes, its result is retained and reachable from the tray but is not applied to the current canvas. A small notice reads "A result for a previous query finished. Open it or dismiss." The current view never changes without a user action.

Restart: the tray is rebuilt from durable job records. A job that was running at shutdown shows "interrupted" with retry; it is never shown as succeeded.

### 5.4 Saving and annotating (CASE-03)

- Source fields in the inspector are read-only, set in monospace where they are identifiers or times, and grouped under the heading "Source record".
- User notes, tags, hypotheses, and leads live under "Your notes" with an edit affordance. They are separate records linked to the source version, so the source view is byte-for-byte unchanged.
- A saved query, reasoning result, hypothesis, or lead appears in the case evidence inventory with its origin class glyph (§7.1).

### 5.5 Save, export, share, publish (COLLAB-03)

Four distinct verbs with distinct placement:

- **Save** is the ordinary primary action everywhere and is private.
- **Export** opens the Export instrument, which previews redactions and omissions before writing a file.
- **Share** opens the collaborator list for a case and names the roles granted.
- **Publish** is only present where publishing is implemented, sits in a separate menu, and requires an explicit confirmation naming what becomes visible to whom.

### 5.6 Statement to evidence (COLLAB-02)

In the report editor, each statement carries a citation marker. Activating the marker selects the cited source version, opens it on the canvas at the anchor, and loads the inspector. The report draft remains one route-back step away. A statement with no citation shows an "uncited" marker that cannot be dismissed without either adding a citation or marking the statement as an explicit assumption.

## 6. State vocabulary and cues

Every state below has three cues: a glyph, a short label, and where noted a treatment such as a dashed border. Color is the fourth cue and never the only one. Glyphs are proposals and should become a consistent icon set; the text forms here are the fallback for plain exports.

### 6.1 Origin class (architecture §2.3)

| Class | Glyph | Label | Treatment |
| --- | --- | --- | --- |
| Acquired source material | ▣ | Source | Solid border |
| User testimony | ◉ | Account | Solid border |
| Normalized observation | ● | Observation | Solid border |
| Extracted claim | ❝ | Claim | Solid border |
| Analytical inference | ◇ | Inference | Dotted border |
| Forecast | ◈ | Forecast | Dotted border |
| Conventional derivative | ▤ | Derived | Solid border, "from" link visible |
| Predictive derivative | ▨ | Predicted detail | Hatched edge, persistent label |
| Simulation | ◌ | Simulated | Dashed border, persistent label |
| Illustrative recreation | ✎ | Recreation | Dashed border, persistent label |

Predicted, simulated, and recreated material keep their label visible even in clean-evidence view, since the view is meant to remove overlays, not provenance.

### 6.2 Claim assessment (architecture §2.4)

| Assessment | Glyph | Label | Color role |
| --- | --- | --- | --- |
| Supported | ✓ | Supported | Confirm (green) |
| Contradicted | ✕ | Contradicted | Alert (red) |
| Unsupported by available evidence | ○ | Unsupported | Neutral |
| Materially misleading | ⚠ | Misleading | Caution (amber) |
| Disputed | ⇄ | Disputed | Caution (amber) |
| Unresolved | … | Unresolved | Neutral |

### 6.3 Review state

| Review | Glyph | Label |
| --- | --- | --- |
| Unreviewed | ◻ | Unreviewed |
| In review | ◩ | In review |
| Reviewed | ◼ | Reviewed by (name), (scope) |
| Challenged | ◭ | Challenged |
| Superseded | ⤳ | Superseded by (version) |

Assessment and review are always shown side by side so that "reviewed" is never read as "supported".

### 6.4 Availability

Available, reference only, missing, restricted, removed, expired, corrupt. Each shows its label and a one-line reason where known. Restricted and removed are never rendered as a retraction or as a prompt to find another way in.

### 6.5 Job state, result outcome, and source health

Job states follow §5.3. Result outcomes from the architecture result envelope (success, partial, empty, unavailable, unauthorized, rate limited, malformed, cancelled) appear in the results header. Source health (unconfigured, connecting, available, degraded, stale, unavailable, disabled) appears in the Library source list and in the inspector Source tab, with the time of the last successful acquisition.

### 6.6 Time kinds and clock mode

| Time kind | Timeline glyph | Label |
| --- | --- | --- |
| Event (reported or observed) | ◆ | Event |
| Capture | ▮ | Captured |
| Publication | ▲ | Published |
| Revision | △ | Revised |
| Retrieval | ▽ | Retrieved |
| Analysis | ◇ | Analyzed |

Approximate times draw as a range bar with the uncertainty stated in text ("21:40 ±15 min, America/Denver, reported local"). The timeline mode badge is one of **LIVE**, **HISTORICAL**, **REPLAY**, **SIMULATED**, shown as text in a bordered chip. Live mode additionally shows the time of the last received sample, so a frozen feed is visibly frozen.

### 6.7 Estimates

A value that is derived rather than observed carries the prefix "≈" and the label "est." with the method available on hover or focus ("est. from barometric altitude, geoid EGM2008, not pressure corrected"). Missing values show "—" and the word "unavailable". A default is never shown in place of a missing value.

## 7. Visual system

The direction is a futuristic military terminal with refined glass surfaces and clear information: dark canvas, smoked glass, hairline borders, restrained depth, a readable sans-serif for prose and navigation, and a monospace for instrumentation. The identity is OIA's own; references are atmospheric, not stylistic templates.

### 7.1 Color tokens

Proposed values for the default dark theme. A light theme is not in P1 scope; reports and plain exports use the print rules in §11.

| Token | Value | Use |
| --- | --- | --- |
| `canvas` | `#000000` | Workspace background. True black so OLED displays switch pixels off and glass reads as glass. |
| `canvas-raised` | `#07090c` | Opaque surface where a panel cannot be translucent |
| `glass-1` | `rgba(6, 9, 12, 0.66)` | Floating instruments (rail, inspector, timeline, readout) |
| `glass-2` | `rgba(8, 12, 16, 0.80)` | Command dock, menus, expanded job tray |
| `glass-3` | `rgba(10, 14, 19, 0.94)` | Modal and reading surfaces over imagery |
| `line` | `rgba(255, 255, 255, 0.08)` | Hairline borders |
| `line-strong` | `rgba(255, 255, 255, 0.18)` | Focused or selected borders |
| `text` | `#e6edf3` | Primary text |
| `text-2` | `rgba(230, 237, 243, 0.55)` | Secondary text and labels |
| `text-3` | `rgba(230, 237, 243, 0.32)` | Disabled, hints |
| `accent-cyan` | `#45d1ff` | Selection, focus ring, current context, interactive emphasis |
| `accent-green` | `#5fe08a` | Confirmed, available, succeeded, supported |
| `accent-amber` | `#f59e2e` | Caution: stale, partial, degraded, needs reassessment, disputed, estimate |
| `accent-red` | `#ff6b6b` | Alert: failed, contradicted, unavailable after being configured, actual alerts only |
| `accent-violet` | `#b59cff` | Generated, predictive, simulated, recreated material |

Semantic roles are fixed. Cyan is the only interactive color; green, amber, and red are never used for emphasis or decoration; violet is reserved for the three generated classes.

**Palette check (measured, 2026-11-01).** The five accents were run through the dataviz palette validator (`scripts/validate_palette.js`, Node 24.18.0) in dark mode against `#0b0f14`. The canvas was later moved to `#000000`; contrast against pure black is higher than against the tested surface, so the contrast result holds, and hue separation does not depend on the surface:

- Contrast against the canvas: all five at or above 3:1 (pass). Body text tokens are separately targeted at 4.5:1.
- Colorblind separation: the weakest adjacent pair is green and amber at ΔE 8.6 under deutan simulation, at the recommended floor. Red and amber under tritan simulation separate at only ΔE 3.0.
- Normal-vision separation: weakest pair ΔE 22.2 (pass).
- The validator's lightness band fails because these are UI accents chosen for text contrast rather than chart series. The validator states its scope is categorical chart palettes; chart series inside instruments should use a separate, validated categorical set rather than these accents.

Conclusion: green, amber, and red cannot be distinguished by color alone for a meaningful share of users. The glyph and label cues in §6 are therefore required, not optional. Chart series in Markets and Forecasts need their own validated palette; the status accents are not to be reused as series colors.

### 7.2 Materials and depth

- The canvas is full-bleed and black. Instruments float over it as separate glass objects with space between them; they do not tile the screen into a grid of bordered boxes. The evidence (map, sky, document, media) is the largest thing on screen.
- Four elevation levels: canvas (0), floating instrument (1), dock and menus (2), modal or reading surface (3), mapped to the canvas and the three glass tokens.
- Glass uses background blur between 14 px and 20 px with the tokens above, a 1 px hairline, a faint top highlight, and a deep soft shadow so panels read as objects over the black canvas. Blur is a display effect and is the first thing removed by reduced transparency and by lower effects settings.
- Reduced transparency replaces each glass token with an opaque equivalent (`#0a0d11`, `#0c1014`, `#0e1318`) and removes blur. Layout, borders, and text are unchanged.
- Cyan may carry a soft outer glow on the single active element (selected marker, active rail item, primary action). Glow is never applied to status colors and never to more than one element per instrument.
- Reading surfaces (document text, report editor) use `glass-3` or its opaque equivalent so prose remains legible over bright imagery (A02). When the canvas behind a reading surface is imagery, a dark scrim at 40 percent sits between them.
- Borders are 1 px hairlines. Selection and focus use `line-strong` plus the cyan focus ring, never a glow alone.
- Shadows are soft and low (0 8 px 24 px at 35 percent black). No colored glows except the focus ring.

### 7.3 Typography

- Sans-serif for prose, navigation, and controls. Monospace for identifiers, hashes, coordinates, times, counts, and instrument readouts. Specific typefaces are an open choice (§13) and must have a license compatible with distribution; the mockup uses system stacks.
- Scale (px): 11 label caps, 12 instrument, 13 secondary, 14 body, 16 reading body, 20 section, 28 title. Reading body may be raised by the user; minimum body text is 13 px.
- Compact uppercase labels with 0.08 em tracking for region titles and status chips. Never for prose, citations, or anything longer than three words.
- Line length in reading surfaces is capped near 72 characters.

### 7.4 Spacing, radius, instrumentation

- 4 px grid. Panel padding 16 px, dense lists 8 px, chips 4 px by 8 px.
- Radii: 6 px controls, 10 px panels, 14 px floating surfaces.
- Instrumentation elements (grid ticks, coordinate readouts, selection brackets, orientation markers) are drawn only from real view state: the actual viewport bounds, the actual selected object's geometry, the actual investigation clock. If a value is unknown, the readout shows "—". There is no ambient instrumentation.

### 7.5 Motion and sound

- Durations: 120 ms for state changes, 200 ms for panel expansion, 320 ms for coordinated selection transitions. Easing is a standard ease-out.
- Coordinated selection moves canvas, timeline, and inspector together so the user can see the relationship. Under reduced motion this becomes an immediate switch with the same end state.
- Progress indicators move only when progress is reported. Live mode shows a subtle "receiving" pulse only while samples arrive.
- Sound is off by default. The only optional sounds are completion of a job the user started and an actual alert rule firing. No ambient audio.

### 7.6 Visual first, words second

The first draft of the shell mockup was reviewed as too word-dense. The rule that follows applies to every instrument:

- Each instrument leads with a visualization of the selected evidence: geometry, bearings, coverage, time, lineage, or media. Prose is confined to the inspector and the reader, and the inspector shows short key-value rows and small charts before any paragraph.
- Essential labels use at least 11 px text at the contrast target in §8.2; controls and evidence details use the larger scale in §7.3. Compact monospace and restrained accents retain the instrument aesthetic without making text faint or unreadable. Scaled geometry labels have readable HTML equivalents.
- Absence has a non-color cue and a text explanation. An unknown position is listed outside the geometry, never placed at an invented coordinate; an unknown bearing or time has an explicit unknown state. A missing value must not acquire an apparent location or interval from its presentation.
- Counts belong in badges and charts, not in sentences.
- A view that cannot yet show a visualization (an empty case) shows the empty canvas with its grid and a single short prompt, not a block of explanatory text.

### 7.7 Reference: God's Eye View shell patterns

Inspected on 2026-11-01 at `0d1a6816` in the local checkout listed in [prior-art](../prior-art.md). These are the UI patterns taken as reference; the code was not copied.

| Pattern | Where seen | Taken into OIA | Not taken |
| --- | --- | --- | --- |
| Full-bleed globe with floating glass panels | `index.html`, `src/ui/styles/foundation.css` (`--glass-bg`, `--panel-radius: 16px`) | Full-bleed canvas, floating instruments, 14 px radius | Cesium as a decided renderer (D01 is open) |
| Tiny uppercase mono labels at half opacity | `foundation.css` `--dock-label-*`, `status-badge.css` (8 to 9 px, 1.3 px tracking) | Compact mono labels and glyph-plus-label badges, enlarged for essential text | Tiny low-contrast controls and color-only badges |
| Bottom command dock with collapsible trays | `src/ui/templates/command-dock.html` | A single pill-shaped command dock at the bottom center | Multiple trays in P1 |
| Cyan accent with soft glow, text secondary at 50 percent | `foundation.css` `--accent`, `--accent-glow`, `--text-secondary` | One glowing active element per instrument; secondary text as alpha over the canvas | Glow on status colors |
| HUD frame lines kept decorative and separate from telemetry | `src/ui/templates/cockpit.html` comment "Decorative only; telemetry and interactive panels remain separate" | Edge frame lines and corner brackets as decoration; readouts only from view state | Simulated pitch and roll scales outside the Devices instrument |
| Alternate theme with clipped polygon panel frames and a technical grid | `src/ui/styles/cyber.css` `--cyber-panel-frame`, `--cyber-grid` | Faint technical grid masked toward the canvas edges | Red accent, 1 px radius theme |

The reference project's "Apple meets Blade Runner" direction and OIA's "military terminal with refined glass" direction overlap on black canvas, glass, mono instrumentation, and restraint. OIA differs in keeping every readout bound to a record or a labeled estimate, in giving status glyph and label cues, and in drawing absence explicitly.

### 7.8 Identity

The wordmark is "OIA" in the sans-serif with "Open Intelligence Agency" in label caps beneath it on the case home. The existing repository icon (`assets/icon.svg`) is a placeholder blue tile and does not match this palette; replacing it is an open item (§13), not part of this draft.

## 8. Accessibility and settings (UX-04, UX-05)

### 8.1 Keyboard model

Proposed bindings. Conflicts with operating-system and renderer defaults must be checked during implementation; the implementing session may remap, but every action must stay reachable.

| Action | Binding |
| --- | --- |
| Focus command field | Ctrl/Cmd+K, or `/` when not in a text field |
| Switch instrument | Ctrl/Cmd+1 to 9 in rail order; Ctrl/Cmd+Shift+] and [ to cycle |
| Move focus between regions | F6 forward, Shift+F6 back (rail, context, canvas, inspector, timeline, tray) |
| Route back | Alt+Left |
| Inspector tabs | Ctrl/Cmd+Alt+1 to 5 |
| Timeline scrub | Left and Right by one step, Shift for ten, Home and End for interval bounds |
| Toggle timeline mode menu | Ctrl/Cmd+Shift+T |
| Cancel focused job | Delete or Backspace on the job entry, with confirmation |
| Toggle focused reading | Ctrl/Cmd+Shift+F |
| Toggle clean evidence | Ctrl/Cmd+Shift+E |
| Open help for the focused control | F1 or `?` |

Within regions, lists and tabs use arrow keys with a roving focus; Tab leaves the region. A "skip to canvas" link is the first focusable element.

### 8.2 Focus and contrast

- Focus ring: 2 px cyan outline with a 2 px canvas-colored inner gap so it is visible on glass and on imagery.
- Body text targets 4.5:1 against its surface, labels and instrument text 4.5:1, borders and icons 3:1. Text is never drawn directly over imagery without a reading surface or scrim.
- Status chips carry glyph and label; icon-only controls carry a visible label on focus or hover and an accessible name always.

### 8.3 Settings

| Setting | Effect | Does not change |
| --- | --- | --- |
| Reduced motion | Replaces transitions with immediate state changes; disables pulses | Data, selection behavior |
| Reduced transparency | Opaque surfaces, no blur | Layout, text, borders |
| Effects quality: low, standard, high | Blur radius, shadow, marker density on globe, imagery prefetch | Numerical results, counts, analysis |
| Clean evidence | Hides overlays, annotations, brackets, ticks on the canvas | Provenance labels on generated material, inspector |
| Focused reading | Hides rail and timeline, widens reading surface | Shared context, route stack |
| Sound | Off by default; per-event toggles | Visual notifications |
| Reading text size | Scales reading surfaces only | Instrument text |

Each setting shows a one-line explanation of what it affects and a statement that it does not change analysis results (UX-05). The Settings instrument also shows which settings are currently active, and the context strip shows a short indicator for clean evidence and focused reading so the user knows why overlays or regions are missing.

## 9. View drafts

The first five views are the P1 release. Later views are sketched at the level needed to confirm the shell accommodates them.

### 9.1 Case home

Canvas: the question as the title, scope summary (area, interval, topics, visibility), then four columns: Evidence inventory (grouped by origin class, each with count and glyph), Hypotheses (with supporting and contradicting counts linked to claims), Open questions and leads, Recent activity (job completions and corrections, each dated). A "needs reassessment" section lists dependent results flagged by source corrections, with the triggering correction linked. Inspector: the case record. Timeline: the case interval with evidence marks.

### 9.2 Library and search results

Canvas: result list with scope header and coverage line (§5.2). Each row: title, source and publication time, origin glyph, availability, extraction status, and a one-line matched passage with the anchor. Collections sit as a left sub-rail inside the canvas. Source list view shows source health and last acquisition time per source. Inspector: the selected document's source record.

### 9.3 Document reader

Canvas: the extracted text with page or passage anchors in the left margin as monospace identifiers. An original-view toggle shows the retained bytes rendered as a page image where the format allows, side by side or alternated. A coverage bar above the text states pages readable, unreadable, missing, and pending ("7 of 9 pages extracted; pages 4 and 8 unreadable; OCR run 2024-… model …"). Unreadable regions are shown as labeled gaps in sequence, not skipped. Highlighting a passage offers "Cite in report", "Record claim", "Add note". Inspector tabs: Source (envelope, hash, retrieval, reuse conditions), Claims (claims anchored in this document with assessment and review), Notes, Lineage (releases, revisions, redacted copies, republications as a small graph).

### 9.4 Claim inspector

A card per claim: exact asserted proposition in quotation, claimant, source version and anchor, temporal scope, assessment and review side by side (§6.2, §6.3), supporting and contradicting evidence lists, assessment criteria used, and the review history. Actions: assess (opens criteria form), request review, open discriminating evidence. The card never shows a probability.

### 9.5 Report editor

Canvas: a structured draft with fixed sections from COLLAB-02 (question and scope, coverage, chronology, source lineage, context and media, hypotheses, supporting and conflicting evidence, bounded conclusions, open questions, citations, evidence inventory). Each statement has a citation marker, an origin marker (human, generated, reviewed), and an uncited warning where applicable. Generated text is set with a thin violet left rule and the label "Generated, unreviewed" until a human reviews it. Inspector: evidence for the selected statement. Actions: Save (private), Export (preview first), Share, Publish (separate menu, if available). Revisions list shows prior issued versions and flags affected by dependency changes.

### 9.6 Export preview

Canvas: the rendered export as it will be written, with an omissions panel listing redactions, restricted assets, reference-only material, and external dependencies, each with its reason. The manifest summary states bundled versus referenced assets. The write action is enabled only after the preview has loaded. Formats offered are those actually implemented; proposed formats appear as "planned" and disabled.

### 9.7 Job tray

As §5.3. The expanded tray groups running, waiting, needs attention (failed, interrupted, partially succeeded), and finished. Each entry links to the job record with attempt history and input versions.

### 9.8 Later instruments

- **Globe.** Areas, points with uncertainty radii, footprints and viewing cones only where geometry is known. Missing tracks render nothing and the coverage panel says why. Display interpolation between samples is drawn in a distinct dotted stroke and excluded from readouts. Marker density is an effects setting; counts come from the query result, not from what is drawn.
- **Sky and solar system.** One investigation clock. Frame selector and true-scale versus illustrative toggle are always visible; the illustrative state is labeled on the canvas. Geometric and apparent positions are separate readouts with the ephemeris version shown in the inspector.
- **Space archive.** Mosaic cards with processing level and color-mapping labels. Feed player mode badge distinguishes live camera, delayed, replay, commentary, simulation.
- **Witness.** The initial account is frozen on save and shown above later edits. Candidate explanations are hidden until the account is saved.
- **Cameras.** Directory with source conditions per camera, health states separate from detections, review queue where each candidate shows frames, trigger explanation, and the three review outcomes. No candidate is ever labeled as an identification.
- **Media workbench.** Original on the left, derivative on the right, derivative class and processing history under the derivative. Predicted detail carries the hatched edge and label at all zoom levels. Automated cues list limitations before scores.
- **Radio.** Directory entries state stream, remote receiver, or local hardware. Spectrum and waterfall appear only for receiver data; audio spectrograms are labeled as audio. Transmitter, receiver, and event locations use three different glyphs on the globe.
- **Devices and HUD.** HUD fields are individually present or marked unavailable; stale values dim and show their age; derived values carry the estimate prefix. The HUD is a separate rendering layer from the video and is never baked into retained footage.
- **Reasoning.** Derivation steps as a vertical list with premises and assumptions at the top; AI-proposed, human-reviewed, and Lean-checked steps have distinct glyphs. Hypothesis comparison is a table with evidence per cell and an explicit "evidence that would discriminate" row.
- **Markets and forecasts.** Charts follow the dataviz method with a validated categorical palette separate from the status accents, one axis per chart, and event overlays drawn as labeled markers. Forecast cards show origin, cutoff, horizon, outcome definition, and the frozen comparison set; revisions appear beside, never over, the original.
- **Privacy.** Findings per field with format-specific removal claims and a reinspection result after cleaning. Account matches show their uncertainty.

## 10. Interface honesty rules

These apply to every view and are testable in review:

1. No animated or decorative values. A number on screen is traceable to a record, an estimate with method, or a labeled fixture.
2. No "connected", "secure", "verified", or "live" labels without the corresponding domain state. Health and mode badges come from source health and clock mode.
3. Unavailable instruments and providers are listed with a reason, not hidden and not faked.
4. Empty is not failed is not unavailable. The three states have three different messages.
5. A smooth player shows dropped frames and gaps in its timeline; it does not interpolate them away in analysis views.
6. Generated, predictive, simulated, and recreated material keeps its label in every view, including clean evidence, exports, and print.
7. Progress bars move only on reported progress.
8. A completed job for a superseded query never changes the current view by itself.

## 11. Print and plain export

- Reports render in a light print stylesheet with black text, source and version in the footer of every page, and citation anchors as visible references. Status glyphs and labels survive without color; the text forms in §6 are the fallback.
- Generated material is marked with a text label in print. Uncertain conclusions keep their qualifiers.
- Plain exports (Markdown, JSON) use the state labels from §6 as literal strings, not display-only glyphs, and never rely on color.

## 12. Open questions and dependencies

| Item | Owner or decision | Effect on this draft |
| --- | --- | --- |
| D01 UI/runtime technology | #9 ADR | Determines how glass, blur, and focus rings are implemented and which keyboard conflicts exist. Layout and states are unchanged. |
| D05 export formats | P1 exporter | Determines which formats the Export instrument offers as implemented versus planned. |
| Typeface selection and licensing | Implementing session for #16 | Mockup uses system stacks. Chosen faces must meet the contrast and size rules. |
| Icon set for glyphs | Implementing session for #16 | The text glyphs in §6 are placeholders; the icon set must preserve one glyph per state and a text fallback. |
| Repository icon | Separate small task | Current `assets/icon.svg` does not match the palette. |
| Chart categorical palette | Markets and Forecasts packages | Must be validated separately; status accents are not series colors. |
| Light theme | Not in P1 | Print stylesheet covers reports; a full light workspace theme is deferred. |
| Measured performance | P0/P1 benchmarks | Targets in delivery §4 are not measured here. |

## 13. Requirement traceability

| Requirement | Sections |
| --- | --- |
| CASE-01 | §5.1 |
| CASE-02 | §3, §4 |
| CASE-03 | §5.4, §9.3 |
| CASE-04 | §5.3, §9.7, §10 |
| CASE-05 | §5.2, §9.2 |
| UX-01 | §7 |
| UX-02 | §3, §4, §5.6 |
| UX-03 | §6, §7.4, §10 |
| UX-04 | §8 |
| UX-05 | §7.2, §8.3, §11 |
| COLLAB-02, COLLAB-03 | §5.5, §5.6, §9.5, §9.6 |
| A01 | §4, §5.3 |
| A02 | §7.2, §8, §6.6 |

## 14. Status of claims in this document

- **Proposed:** every layout, token, binding, and view in §2 to §11.
- **Measured:** the palette validator results in §7.1, on the stated tool, date, and inputs. They measure color separation, not OIA.
- **Implemented in the prototype only:** the fixture interactions in §15. No production application, domain-service integration, persistence, acquisition, or formal verification is supplied by this design slice. A01 and A02 remain unfulfilled integration scenarios.

## 15. Interactive prototype scope and walkthrough

This follow-up to the merged design draft is a dependency-independent slice of #16. It does not choose the D01 renderer, establish a configuration schema, or copy the unmerged #9/#14 implementation. JavaScript is confined to temporary presentation state over three synthetic accounts. The transcript was written for the prototype; `FIXTURE-DOC-0007`, its version, and its paragraph anchors are illustrative identifiers, not acquired evidence.

Implemented here:

- Select A, B, or C to coordinate the inspector, geometry selection, coverage row, source anchor, and timeline explanation. B has no plotted position. B and C have no event-time marker; selecting them retains the case clock and explicitly says their event times are unknown.
- Open **Read source passage** or **Library** to inspect the corresponding versioned fixture paragraph. **Back** or Alt+Left restores the prior view, selection, inspector tab, submitted search, display modes, scroll position, and available focus target. Route history is bounded to 40 transitions and ends on reload. When a search-result control was recreated, focus returns to the search field.
- Search literal text or an account identifier in all three synthetic passages. Results retain anchors and fixture labels. The coverage statement names the complete searched scope; no matches is an empty result, not a failed acquisition. No saved queries, semantic retrieval, provider search, pagination, or acquisition is simulated.
- Use Tab/Shift+Tab, the skip link, Enter/Space, Ctrl/Cmd+K (or `/` outside an input), and arrow/Home/End keys on inspector tabs. Other proposed bindings in §8.1 are not implemented in this prototype.
- Toggle **Clean evidence** to remove rings and selection brackets while preserving the schematic and fixture labels. **Focus view** removes the rail and timeline; its exit control and search remain reachable. Opaque panels and reduced motion change presentation only and honor the corresponding system preferences. No audio is emitted.
- Read disabled instruments' inline reasons. Jobs shows **no connected worker**, not fabricated progress. Save, publish, assessment, and export actions are absent because they have no functioning service in this prototype.

Responsive behavior: wide screens retain separate glass instruments around the evidence canvas. At 1100 px and below, the inspector follows the evidence in normal document flow. At 640 px and below, the layout becomes a single column with labeled navigation and account controls. The source reader and search use the same layout; no fixed-width 1440 px stage or overlapping drawer is required. The print stylesheet preserves fixture and source labels, including uncertainty.

Local verification on Windows, 2026-11-02:

- Playwright 1.56.1 with headless Microsoft Edge 156.0.4314.58, opening the local HTML file directly: checked 1440, 1024, 768, 390, and 320 px viewports for horizontal overflow and overlapping top-level regions; inspected desktop and phone screenshots.
- Exercised unknown-time selection, anchored source reading, Back and focus restoration, inspector keyboard tabs, search shortcut, matching/empty/all-account queries, literal markup input, clean/focus views, and reduced effects. No JavaScript errors or remote HTTP requests were observed.
- axe-core 4.10.3 reported no WCAG 2 A/AA or 2.1 AA violations in the initial geometry view at 1440 and 390 px. This automated result does not establish full accessibility conformance or replace assistive-technology review.
- Reviewed relative links, requirement references, and the diff locally. No Rust or Lean code changed; no product-runtime, provider, persistence, or full A01/A02 acceptance claims follow from these prototype checks.

Reproduce the core walkthrough by opening the linked HTML file, selecting B, opening its source, returning with Back, searching for `ridge`, opening C's result, and returning with Alt+Left. Try an unmatched query and then an empty query. Repeat at a narrow width using only the keyboard and the presentation toggles. All displayed evidence remains explicitly synthetic throughout.
