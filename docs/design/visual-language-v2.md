# OIA visual language v2: the instrument shell

Version: 0.1 draft. Date: 2026-11-03. Status: design proposal for [#16](https://github.com/mESmaC/oia/issues/16). Nothing here is implemented in the product; the companion mockup is a static study. On 2026-11-02 the project owner supplied a set of reference images and stated that this direction may take precedence over the earlier design work in [`workspace-ui-ux.md`](workspace-ui-ux.md) v0.4, which they were not satisfied with. The owner's clarification, relayed on [#16 (comment 6592351842)](https://github.com/mESmaC/oia/issues/16#issuecomment-6592351842): "Some of the previous designs looked like document management systems rather than military intelligence software. So the spec still captures this technically, but im counting on fable to actually deliver on it." Precedence was offered so that the design would deliver; it is not an advance approval. v2 replaces §7 and the v0.4 mockups only if the owner judges that it does.

This document replaces the **visual system** (§7) and the **mockup direction** (§15) of the v0.4 draft. It keeps the v0.4 information architecture (§2), shell regions (§3), shared context (§4), interaction patterns (§5), state vocabulary (§6), accessibility rules (§8), honesty rules (§10), and print rules (§11) by reference, with the departures listed in §7 below. Requirement IDs are from the [product specification](../spec.md) §3.2 (`UX-01` to `UX-05`) and §3.1 (`CASE-02` to `CASE-04`).

Companion: [`mockups/workspace-v2.html`](mockups/workspace-v2.html), a self-contained static page on the same synthetic 1994 valley case used by the earlier prototypes. All content is fixture content; nothing is live, saved, or acquired. Open the file directly; it loads no fonts, scripts, or network resources.

## 1. What was wrong with the v0.4 direction

The earlier mockups satisfy the honesty rules but do not look or feel like an instrument. Reviewed against the owner's references on 2026-11-02:

- **Tiled boxes.** Every region is a bordered card of equal weight. The evidence is one card among many instead of the thing the instruments float over.
- **Prose-led.** Panels open with sentences. Labels, values, and counts are embedded in paragraphs, so a user reads rather than scans.
- **No binding between label and geometry.** Account A's record sits in a list; its point sits in a drawing. Nothing on screen connects the two.
- **Web-dashboard feel.** Large sans-serif headings, a bright cyan on near-black, rounded 10 px panels, and generous padding read as a SaaS admin page, not a terminal.
- **Selection is colour.** The selected item is a blue card border. Nothing frames the object that is selected.

The v2 direction fixes each of these without loosening a single honesty rule. Where a reference pattern would require fake data, it is rejected in §3.

## 2. References reviewed

Owner-supplied images, reviewed 2026-11-02. These are design references only. No image, icon, font, or code from them is used in OIA; the mockup draws every element from scratch.

| Ref | What it is | Attribution as visible | Taken as |
| --- | --- | --- | --- |
| R1 | Submarine operations concept "OP: ICE VEIL": contour map with unit markers, mission card, telemetry strip, wireframe vessel with numbered callouts, sonar dial, mission timeline, "AI assistant" panel. Four stills. | Dribbble user `sogasoux`, named in the image | Primary reference for the instrument language (§4) |
| R2 | Fixed-wing drone platform dashboard: photograph with panels over it, wireframe highlights on the airframe with leader lines into metric cards, sparkline and bar charts, large percentage readout. One still. | Not identified in the image | Reference for evidence-first layout and leader lines |
| R3 | "SurveilTrack" unit map: monochrome 3D city, bracket-framed cards, monospace throughout, tick-bar meters, status pills, segmented scope control, tab strip, card carousel. One still. | Not identified in the image | Reference for monochrome restraint, brackets, meters |
| R4 | Stock motion-graphics HUD "Global sat pos": globe with orbits, world clocks, dense ambient data columns, status text cycling "searching, tracing, analyze". 17 s clip. | Pond5 watermark | Mostly a counter-example (§3) |

## 3. Adopt, adapt, reject

Each row names the honesty rule that decides it. Rules are the v0.4 §10 list and `AGENTS.md` §2 and §9.

| Pattern | Seen in | Decision | Rule and consequence |
| --- | --- | --- | --- |
| Readout strip of `LABEL │ value` cells separated by hairlines | R1 top edge | **Adopt** as the view-state strip (v0.4 §3 C), moved to the top edge | Every cell is a fact about the current view: case, clock, mode, selection, pinned version. No ambient figures. Unknown shows `—`. |
| Corner-bracket frames | R1 unit markers, R3 cards | **Adapt**: brackets are the selection cue, drawn around the selected object's actual geometry and around the selected-object card | Brackets never decorate an unselected panel. One bracketed object per instrument. |
| Numbered callout tags with leader lines | R1 vessel callouts, R2 airframe | **Adapt**: tags bind an evidence record to its drawn geometry; the leader ends on the object's edge | A leader is drawn only to a recorded position or region. An unknown position goes to the **Not plotted** tray with a tag and no leader. Numbers are display order, not identifiers; the identifier stays on the tag. |
| Dotted or hatched area fill | R1 selected territory | **Adopt** for reported regions that are not positions | A region is an area claim; it must not acquire a centre point. The fill carries the "region" word in its tag. |
| Selected-object "target card" over the canvas | R1 mission and sector cards, R3 unit card | **Adopt** with key-value rows in monospace | Rows come from the source record; estimates carry `≈` or `±`, and missing fields say "not recorded". |
| Tick-bar meters `▮▮▮▯▯` | R3 power meter, R2 battery | **Adapt** for bounded, countable quantities only: coverage shown of total, extraction coverage, reported job progress | Never for confidence, likelihood, or anything without a denominator. The numerator and denominator are printed beside the bar. |
| Large percentage or number readouts | R2 battery level | **Reject** except as counts in badges (v0.4 §7.6) | A big number invites reading it as a measurement. Counts of records are the only large figures. |
| Timeline ruler with minor and major ticks | R1 bottom scrubber, R4 frame edges | **Adopt** as the shared timeline (v0.4 §3 E) | Marks are placed only at recorded times with their time-kind glyph. Records with no event time are listed beside the ruler as not placed. |
| Rounded-square icon rail | R1 left rail, R3 left rail | **Adopt** with an 11 px label under every icon | Unavailable instruments stay listed, dimmed, with a reason on hover and focus. |
| Wireframe object with highlighted sections | R1 vessel, R2 airframe | **Defer** to the Devices and Media instruments | Highlighted sections must map to recorded capability or forensic regions, never to a decorative model. Not part of this slice. |
| Contour or terrain backdrop | R1 map | **Adapt**: the study draws a schematic ridge and road labelled as such | In the product, contours come from an elevation source with its version in the inspector, or they are not drawn. A faint technical grid is the default backdrop. |
| Segmented scope control (City / District / Street) | R3 | **Adopt** as the search-scope chip and coverage-mode control already used in the Flock camera mockup | The selected scope is printed in the coverage statement of every result. |
| Status pills with a dot | R3 unit list, R2 "nominal" | **Adapt** to the glyph-plus-label chips in v0.4 §6 | Colour never carries state alone (palette finding in v0.4 §7.1). |
| World clocks (several cities) | R4 header | **Adapt** into the time-kind display: the same instant in UTC, reported local, and the user's zone | Only when the instant is recorded. A reported local time with no zone stays a reported local time. |
| Horizontal card carousel | R3 unit list | **Adopt** for evidence inventories in narrow layouts | Shown and total counts stay visible; a hidden card is still counted. |
| Photograph as canvas with panels over it | R2 | **Adopt** for media and camera instruments, with the 40 percent scrim rule from v0.4 §7.2 | The image is evidence; panels must not hide it, and the scrim is a display effect removed by reduced effects. |
| "AI assistant" panel with brief, insights, `Conf 0.74` | R1 right panel | **Reject** | Violates v0.4 §10 rules 1 and 2 and the generated-material rule. OIA's equivalent is the Reasoning instrument: premises, steps, and provenance, in violet, labelled generated. No confidence chips without a stated evaluation. |
| Sonar dial, radar sweep, live sparkline | R1, R2 | **Reject** as decoration; **permit** only as a chart of recorded samples with its source | v0.4 §10 rule 1. A sweep that does not represent a sensor is simulated activity. |
| Dense ambient data columns, 6 px text, cycling status words | R4 | **Reject** | v0.4 §7.4 and §7.6. Essential text is 11 px or larger; status comes from job and source state only. |
| Glow on status colours | R1 green panel | **Reject** | v0.4 §7.2: glow only on the single active element. |

## 4. The instrument language

These are the components every instrument is built from. Each one names what binds it to data. The companion mockup shows each at least once.

1. **Readout strip.** A row of cells, each `LABEL` in 11 px tracked monospace caps and a value in 12 px monospace, separated by 1 px hairlines inside one glass bar. It states the shared context (`CASE`, `CLOCK`, `MODE`, `SELECTION`). Estimates render in amber with `±` or `≈`. Bound to the shell state in `oia-app`; it never computes.
2. **Callout tag.** A two-cell chip: an order number in the accent, a divider, then the record label in caps. Tags sit on the canvas in the same coordinate space as the geometry so the leader and tag never drift apart. A selected tag takes the accent border.
3. **Leader.** A 1 px line from the tag edge, elbowed once, ending in a 2 px dot on the edge of the referenced geometry. Drawn only to a recorded point or region.
4. **Bracket frame.** Four accent corners, 12 units long, around the bounding box of the selected object's actual geometry, and around the selected-object card. One per instrument.
5. **Target card.** A glass card with brackets, a caps label (`01 · SELECTED`), the record name in the sans-serif, and right-aligned monospace key-value rows. It floats over the canvas near, but never on top of, the selected geometry.
6. **Region fill.** A dotted pattern with a dashed outline for an area claim. Its tag says `region`.
7. **Not plotted tray.** A glass strip at the canvas's lower edge listing records that have no drawable geometry, with their tags and no leaders. The tray exists so that absence is visible without inventing a point (v0.4 §7.6).
8. **View-state strip.** A low-contrast monospace row at the canvas's lower right: case id, anchor, pinned version, overlay state, scale. `—` for unknown. This replaces the ambient instrumentation of the references.
9. **Meter.** Label, `n / d`, and a bar of `d` equal segments with `n` lit in a neutral fill (`text-2`), never the accent, so that a filled count cannot read as a selection. Only for bounded counts and reported progress; a partial or stopped quantity may use amber.
10. **Chip.** Glyph plus label in 11 px monospace with a hairline border. Dashed border and violet text for fixture, generated, simulated, and recreated material. Chips are the only carriers of status colour, and never colour alone.
11. **Ruler timeline.** Hour ticks (major) and ten-minute ticks (minor) on a hairline baseline; reported windows as translucent bands with their glyph and label inside; the investigation clock as an accent line with its time above; a legend that names each time kind and says "none recorded" where that is the case.
12. **Rail.** 72 px, icon above an 11 px monospace label, the active instrument on an accent-tinted background with a 2 px inset accent edge. Disabled instruments dimmed with a trailing dot and a reason in the tooltip and accessible name.
13. **Command field.** A glass bar with a scope chip in the accent, a placeholder listing the operators, and the shortcut as a `kbd`. It never auto-executes a mutation (v0.4 §5.2).

## 5. Visual system v2

### 5.1 Colour

| Token | Value | Use |
| --- | --- | --- |
| `canvas` | `#000000` | Workspace background |
| `glass` | `rgba(9, 11, 13, 0.74)` | Floating instruments |
| `glass-2` | `rgba(11, 13, 16, 0.88)` | Tags, dock, menus |
| `solid` | `#0b0d10` | Opaque equivalent and tag boxes |
| `line` | `rgba(255, 255, 255, 0.10)` | Hairlines |
| `line-2` | `rgba(255, 255, 255, 0.22)` | Tag borders, hover |
| `text` | `#e9edf0` | Primary |
| `text-2` | `rgba(233, 237, 240, 0.64)` | Labels, secondary |
| `text-3` | `rgba(233, 237, 240, 0.52)` | Hints, disabled, view-state, legends |
| `accent` | `#8ff0a4` | The single interactive colour: selection, focus, active, brackets, leaders' tag numbers |
| `amber` | `#f3b64a` | Caution and estimate: `±`, `≈`, stale, partial, disputed |
| `red` | `#ff7b7b` | Alert: failed, contradicted |
| `violet` | `#b9a6ff` | Fixture, generated, predictive, simulated, recreated |

**The accent change is the one decision in this document that needs the owner.** v0.4 used cyan for interaction and green for "supported, succeeded". The references the owner chose are green-led, and the mockup follows them: a pale phosphor green is the only interactive colour. The cost is that "supported" and "succeeded" can no longer be green without reading as "selected". Two ways to resolve it:

- **A (used in the mockup):** keep green as the interactive accent. Supported and succeeded states use the neutral chip with their glyph (`✓`) and label, and no colour. Status colour is then only amber, red, and violet. This is consistent with the palette finding in v0.4 §7.1 that status cannot rely on colour anyway.
- **B:** keep v0.4's cyan accent and green status, and apply only the structural changes in §4. Cyan and green are one token apart in the mockup.

Recommendation: A. It is simpler, it matches the owner's references, and it removes a colour that the earlier palette check showed to be weakly separable from amber under deutan simulation.

Contrast, computed 2026-11-03 with the WCAG 2 relative-luminance formula against `#000000`, alpha composited over black: `text` 17.8:1, `accent` 15.2:1, `amber` 11.6:1, `violet` 10.0:1, `red` 8.4:1, `text-2` 7.2:1, `text-3` 4.9:1. The first draft of this study set `text-3` at 40 percent alpha, which computes to 3.3:1 and fails the 4.5:1 body-text target; it was raised to 52 percent before the mockup was verified. Over a glass surface the effective background is slightly lighter than black, so these are upper bounds. On the opaque `solid` surface (`#0b0d10`) the same formula gives `accent` 14.1:1, `red` 7.8:1, `text-2` 7.1:1, and `text-3` 5.0:1, all above the 4.5:1 target. All essential text is 11 px or larger (§8).

### 5.2 Materials

- The canvas is true black with a 48 px technical grid at 4.5 percent white and a radial vignette so the edges fall away. The grid is a display effect and is removed by reduced effects.
- Instruments are separate glass objects with 12 px gaps, 8 px radius, a 1 px hairline, a faint 1 px top highlight, and a soft 28 px shadow at 45 percent. Blur is 16 px and is the first thing removed by reduced transparency, which replaces glass with `solid`.
- Controls have a 4 px radius. Tags have a 3 px radius. Nothing is rounder than 8 px.
- Glow is not used in the mockup. If it returns, it is confined to the single bracketed object (v0.4 §7.2).

### 5.3 Typography

- **Monospace carries the instrument.** Labels, tags, readouts, values, times, identifiers, chips, rail labels, tabs, and the timeline are monospace. Caps labels use 0.08 em tracking at 11 px; values are 12 to 13 px without tracking.
- **Sans-serif carries names and prose.** Record names (`Account A`), quoted claims, notes, reader text, and the report are sans-serif at 13 to 16 px.
- Proposed faces: **IBM Plex Mono** and **Inter**, both under the SIL Open Font License 1.1, which permits bundling with an `MIT OR Apache-2.0` application. The mockup uses system stacks (`Cascadia Mono`, `Segoe UI`) so that it loads nothing; the product should bundle the chosen faces so that rendering is identical across the WebView2, WKWebView, and WebKitGTK targets of the Tauri decision.
- Minimum essential text is 11 px. The view-state strip is 11 px at `text-3` because it is secondary; every record value is 12 px or larger at `text` or `text-2`.

### 5.4 Spacing and density

4 px grid. Instrument padding 12 to 14 px, down from 16. Key-value row gap 6 px. Tag padding 6 px by 8 px. Density is higher than v0.4 because the monospace labels do the work that sentences used to do.

### 5.5 Motion

Unchanged from v0.4 §7.5: 120 ms state changes, 200 ms panel expansion, 320 ms coordinated selection, immediate under reduced motion. The mockup is static and animates nothing.

## 6. Layout v2

The regions are those of v0.4 §3 with three changes: the readout strip moves to the top edge; the selected-object card, tags, not-plotted tray, and view-state strip float over the canvas; and the job tray is a one-line strip under the timeline.

```text
┌──────┬────────────────────────┬──────────────────────────────────────────────────┐
│ OIA  │ [scope] command field  │ CASE │ CLOCK │ MODE │ SELECTION          readout │
├──┬───┴────────────────────────┴───────────────────────────────┬──────────────────┤
│  │ crumb  CASE › VALLEY › GEOMETRY  ◇ synthetic   back·clean·focus │ INSPECT        │
│r │                                                             │ Account A      │
│a │   ┌01│ACCOUNT A┐╌╌╌┐        ┌03│ACCOUNT C · REGION┐          │ chips          │
│i │   ┌──────────┐    └─● ⌐ ¬    ░░░░░░░░ (dotted region)        │ SOURCE│CLAIMS… │
│l │   │ 01·SELECTED  ⌐  ◎  ¬  (brackets around real geometry)   │ key  value     │
│  │   │ Account A    ∟     ┘                                    │ key  value     │
│  │   │ key   value  │                                           │ meters n/d     │
│  │   └──────────────┘                                          │ claims         │
│  │ NOT PLOTTED  02│ACCOUNT B · NOT RECORDED   CASE ANCHOR PINNED │ [Read passage] │
├──┼─────────────────────────────────────────────────────────────┤                │
│  │ INVESTIGATION CLOCK │ ruler ticks · bands · clock · legend   │                │
│  │ 21:40 ±15 min       │ 20:00      21:00      22:00      23:00 │                │
├──┼─────────────────────────────────────────────────────────────┤                │
│  │ JOBS  no connected worker · 0 running · 0 failed             │                │
└──┴─────────────────────────────────────────────────────────────┴──────────────────┘
```

Narrow widths (under 1100 px): the readout strip wraps under the command field, the inspector follows the canvas in document flow, and the timeline legend becomes a row under the ruler. The mockup implements this fallback; the full narrow and phone layouts of v0.4 §3 are not redrawn here.

## 7. Relationship to the v0.4 draft

| v0.4 section | Status under v2 |
| --- | --- |
| §1 principles, §2 information architecture, §4 shared context, §5 interaction patterns | Retained unchanged |
| §3 shell layout | Retained; region placement amended by §6 above |
| §6 state vocabulary (origin, assessment, review, availability, time kinds) | Retained; chips in §4.10 are the rendering |
| §7 visual system | **Superseded** by §5 |
| §7.6 visual first, words second | Retained and strengthened: §4 makes the binding between label and geometry explicit |
| §7.7 God's Eye View reference table | Retained as history; §2 and §3 above are the current reference set |
| §8 accessibility and settings, §10 honesty rules, §11 print | Retained unchanged |
| §9 view drafts | Retained as scope; each view is to be redrawn in the v2 language when its instrument is built |
| §12 open questions | Typeface and icon rows answered provisionally in §5.3 and §9 below |
| §15 prototype walkthroughs and the three earlier mockups | Retained as **interaction** references (route stack, filters, claims, notes, report). Their **appearance** is superseded; they are not to be extended. |

For the running workbench in `oia-app` (the `codex/running-app` slice of #16): take the tokens in §5.1, the materials in §5.2, and the components in §4 as the presentation layer's style source. The workbench must consume `accent` as a single token and never hard-code the colour, so the owner's choice between A and B in §5.1 is a one-line change there as well. The presentation layer still draws state and sends intents only (ADR 0008 §4 rule 1).

## 8. The mockup and what was verified

[`mockups/workspace-v2.html`](mockups/workspace-v2.html) shows the case geometry instrument for the synthetic valley case: Account A selected, with brackets around its observer point, a leader from its tag, and a target card; Account C as a dotted region with its own tag; Account B in the not-plotted tray; the readout strip, rail, inspector with source rows, three coverage meters, and two claims with assessment and review side by side; the ruler timeline with A's reported window, the clock at 21:40, and a legend stating that no publication or retrieval time is recorded; and a job tray with no connected worker.

The page is static. There is no selection, routing, filtering, or search; those behaviours are specified in v0.4 §5 and demonstrated in the earlier prototypes.

Verified locally on Windows 11, 2026-11-03, with Playwright 1.58.0 (`playwright-core`) driving the Chromium headless shell build 1248 against the file opened directly, at 1440×900, 1280×800, and 1024×768:

- No horizontal overflow at any width.
- No rendered text element under 11 px (computed font size, visible elements with text nodes).
- No network requests other than the file itself; no page errors or console errors.
- axe-core 4.10.3, rules `wcag2a`, `wcag2aa`, `wcag21aa`: no violations at any width. This is an automated check and does not establish accessibility conformance.
- Screenshots at each width were inspected by the authoring session.

Not verified: screen readers, keyboard walkthrough beyond focus-visible styling, print output, WebKit rendering of the backdrop blur, and the narrow layouts below 1024 px.

## 9. Open items

| Item | Owner | Note |
| --- | --- | --- |
| Accent: green (A) or cyan (B) in §5.1 | Project owner | The mockup uses A. Switching to B is one token. |
| Typefaces | Implementing session for the workbench | §5.3 proposes IBM Plex Mono and Inter; bundle them, do not load from a network. |
| Icon set | Implementing session | The mockup's rail icons are inline strokes drawn for the study; a consistent set is needed with a text fallback per v0.4 §6. |
| Contour backdrop | Globe instrument (#23) | Only from an elevation source with a version; otherwise the grid. |
| Chart palette for Markets and Forecasts | Those packages | Still separate from the status accents (v0.4 §7.1). |
| Repository icon | Separate small task | Unchanged from v0.4 §12. |

## 10. Requirement traceability

| Requirement | Sections |
| --- | --- |
| UX-01 | §5 |
| UX-02 | §4, §6 |
| UX-03 | §3, §4.1 to §4.9, §4.11 |
| UX-04 | §4.12, §5.2, §8 |
| UX-05 | §5.2, §5.5, §8 |
| CASE-02 | §4.1, §4.13, §6 |
| CASE-03 | §4.5, §4.10 |
| CASE-04 | §4.9, §6 (job tray) |
| COLLAB-02 | §4.10 (claims chips), §8 |

## 11. Status of claims in this document

Everything here is **proposed**. The verification in §8 is **tested** for the mockup page only. The contrast ratios in §5.1 are **measured** by formula, not by a display. No product behaviour, renderer behaviour, or accessibility conformance is claimed.
