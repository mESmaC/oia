# ADR 0008: D01 follow-up — Workspace renderer and presentation adapter

- **Status:** Accepted for the P1 desktop renderer. The project owner selected option C (Tauri desktop app) in the running-app session on 2026-11-02 America/New_York, recorded on [#16 (comment 6591833455)](https://github.com/mESmaC/oia/issues/16#issuecomment-6591833455) and confirmed to the review session on [#16 (comment 6591848264)](https://github.com/mESmaC/oia/issues/16#issuecomment-6591848264). The WebView2 traffic disclosure condition in §4 remains applicable. The initial local workbench is documented in [desktop-workbench.md](../implementation/desktop-workbench.md), whose [departures from §4 rules 1 and 2](../implementation/desktop-workbench.md#departures-from-adr-0008-4) are interim and owned by #14; full case-shell integration still depends on #14.
- **Date:** 2026-11-02 UTC
- **Decision ID:** D01, renderer and presentation layer ([`docs/delivery.md` §6](../delivery.md#6-decision-register))
- **Requirements:** [`UX-01`](../spec.md#32-visual-identity) to [`UX-05`](../spec.md#32-visual-identity), [`CASE-02`](../spec.md#31-investigation-workspace), [`CASE-04`](../spec.md#31-investigation-workspace); acceptance A01 and A02 ([`docs/delivery.md`](../delivery.md))
- **Architecture references:** [`docs/architecture.md` §1.1](../architecture.md#11-proposed-deployment-model) (the UI/rendering ADR must resolve a Rust-native renderer versus an adapter around an established stack; Cesium reuse is an option to evaluate), [§1.2](../architecture.md#12-proposed-rust-workspace) (`oia-app` owns the UI boundary), [§7](../architecture.md#7-application-interfaces-and-events), [§8](../architecture.md#8-media-devices-and-rendering), [§9](../architecture.md#9-access-collaboration-and-local-operation)
- **Companions:** [design draft v0.2](../design/workspace-ui-ux.md) and [interactive prototype](../design/mockups/workspace-shell.html) (merged in #59 and #66); evaluation spikes in [`spikes/`](../../spikes/README.md): a shared renderer-independent core, a Tauri 2 front-end, and an egui front-end, measured the same way.

## 1. Context

The design draft fixes what the P1 workspace must draw and do, independent of technology: a true-black full-bleed canvas with floating glass instruments (backdrop blur 14 to 20 px, hairlines, soft shadows, opaque equivalents under reduced transparency); compact monospace instrumentation with essential labels at 11 px or larger (v0.2); a reading surface capped near 72 characters with user-scalable text; a responsive layout down to 320 px (v0.2); a keyboard model with visible focus and region cycling; coordinated selection with a route stack; a job tray whose progress moves only on reported progress and whose superseded results never change the current view; unknown positions listed outside the geometry rather than drawn at an invented point (v0.2); print and plain-text export that survive without colour; and settings that degrade effects without touching data.

Two facts about the surrounding work shape the choice:

1. The P2 globe (`GEO-01`, `SPACE-04`) is the heaviest rendering requirement in the specification. The architecture names reuse of Cesium from God's Eye View as an option to evaluate. Cesium is a web renderer.
2. `main` has no Rust workspace yet. PR #56 proposes `oia-app` as the owner of a renderer-independent `WorkspaceShellState`. Whatever renderer is chosen must consume that state through the application service boundary and must not become a second owner of it.

The question for P1 is therefore: a Rust-native renderer, or a rendering adapter around an established stack, with the domain and application layers in Rust either way.

## 2. Options considered

| Option | Description |
| --- | --- |
| A. Rust-native immediate mode | `egui`/`eframe` over `wgpu` (default in 0.37) or `glow`. One language, one process, consistent rendering across platforms. **Built and measured** as `spikes/workspace-shell-egui` (`wgpu` backend). |
| B. Rust-native retained | `iced` (Elm-style, `wgpu`, `cosmic-text`) or `Slint` (declarative markup, own renderer; dual-licensed). **Not built**; assessed from documentation only. |
| C. Rendering adapter: Tauri 2 | Rust process owns the window and all state; the system WebView (WebView2 on Windows, WKWebView on macOS, WebKitGTK on Linux) renders a bundled, no-remote-load presentation layer. **Built and measured** as `spikes/workspace-shell-tauri`, with plain HTML/CSS/JS and no build step. |
| D. Electron or CEF | Bundled Chromium with a Node or C++ host. **Not evaluated further**: it bundles a browser per install and puts a non-Rust process at the centre, against the local-first Rust core in ADR 0001 and `AGENTS.md` §4. |

Both built spikes share one model crate (`spikes/workspace-shell-core`: shell context, route stack, local filters, settings, job registry with supersession and interruption, bounded atomic persistence, synthetic fixture) so that the comparison measures the renderer, not two different applications. The persisted format in that crate is throwaway and is not the schema proposed for `oia-app`.

## 3. Evaluation

"Basis" states how each cell was established. "Measured" means in the spikes, on the machine and date in §5, with the method in [`spikes/README.md`](../../spikes/README.md). "Documented" means from the libraries' public documentation as read for this ADR, with no build; those cells are assessments, not results.

| Criterion (source) | A egui (built) | B iced / Slint (not built) | C Tauri 2 adapter (built) | Basis |
| --- | --- | --- | --- | --- |
| Glass: backdrop blur, hairlines, shadows, reduced-transparency equivalents (design §7.2, `UX-04`) | Translucent fill, hairline, shadow, opaque fallback achieved. No backdrop-blur primitive; blur would need a custom render pass per surface. | No backdrop blur in either, per docs. | All achieved with CSS and media queries. | A, C measured; B documented |
| Instrumentation typography: compact tracked uppercase mono, essential labels ≥ 11 px, 72 ch reading width, scalable reading text (design §7.3 v0.2) | Uppercase monospace at ≥ 11 px, tracking (`RichText::extra_letter_spacing(0.9)` ≈ 0.08 em), and scalable reading text achieved. | `Slint` `Text` supports `letter-spacing: length` per docs; `iced` text widgets not verified for tracking. | All achieved. | A, C measured; B documented |
| Absence drawn, not invented: unknown position listed outside the geometry, dashed ring for no bearing (design §7.6 v0.2) | Achieved ("Not plotted" row; dashed ring). | Achievable. | Achieved. | A, C measured |
| Accessible names, focus order, roving tabs, screen readers (`UX-04`) | AccessKit enabled by eframe's default features; **not tested**. | `iced` and `Slint`: partial, per docs. | Platform accessibility tree via WebView2; names, focus ring, F6 regions, roving tabs implemented; **no screen reader run**. | Implemented, not verified |
| Keyboard model (design §8.1) | Subset implemented. Chords must be read per event, not from end-of-frame modifier state. | Achievable. | Subset implemented, including F6 region cycling. | A, C measured |
| Responsive layout to 320 px (design §3 v0.2) | Not attempted; fixed-position panels. | Layout engines exist. | Partial: a narrow-width media query; not tested at 320 px. | Measured gaps |
| Document page images, video, PDF (P1 reader, P3 media) | Image decoding fine; video and PDF need native decoder bindings. | Same. | `img`, `video`, PDF come with the WebView. Frame-accurate forensic playback still needs Rust-side decoding under `MEDIA-01`. | Documented |
| P2 globe and Cesium reuse (architecture §1.1, #23) | A `wgpu` globe would be written, or a WebView embedded for Cesium anyway. | Same. | Cesium can run inside the same WebView; a native globe stays possible later. | Documented; not decided here |
| Print and plain export (`UX-05`, design §11) | Separate print renderer needed. | Same. | Print stylesheet over the same markup; the exporter from ADR 0006 is needed either way. | Documented |
| Domain logic stays in Rust (`AGENTS.md` §4) | By construction; UI and model in one language. | Same. | Needs a review rule: the presentation layer draws state and sends intents only. The spike holds every value and transition in Rust and returns the full shell view after each command. | Measured (structure) |
| Local security boundary (architecture §9) | No web surface, no second runtime. | Same. | No localhost listener: IPC with a capability allow-list and a `default-src 'self'` CSP. The WebView is a large, OS-updated dependency with its own behaviour (next row). | Measured (configuration) |
| Network silence of the UI process tree (privacy stance, `AGENTS.md` §5) | **0 established TCP connections** during a 4 s idle run. | Expected 0; not measured. | **2 established HTTPS connections** from the WebView2 browser process to a Microsoft address (no PTR record) on every run, not initiated by the page. Passing `--disable-background-networking`, `--disable-component-update`, `--disable-domain-reliability`, `--metrics-recording-only`, `--no-pings`, and feature disables through `additionalBrowserArgs` did **not** remove them (flags confirmed on the process command line). | Measured |
| Footprint and startup (release, warm) | 14.6 MB binary (`egui-wgpu`); first frame 383 to 465 ms; 157 to 208 MB working set, one process. | Not measured. | 8.9 MB binary; UI ready 305 to 321 ms; main process 27 to 33 MB; process tree with six WebView2 processes 361 to 382 MB. | Measured |
| Cross-platform consistency | Identical rendering everywhere. | Same. | Depends on the system WebView; WebKitGTK on Linux lags for `backdrop-filter`. | Documented |
| Reuse of delivered design work | Mockup re-implemented in Rust (about 1,900 formatted lines for the subset). | Same. | The merged mockup's tokens and layout map directly; the spike's stylesheet derives from it. | Measured |
| Toolchain | Cargo only. | Cargo only. | Cargo only in the spike; a TypeScript or WASM layer would add one later. | Measured |

## 4. Accepted decision

Adopt **option C, Tauri 2, for the P1 workspace**, under the rules below, with the **network finding in §3 as a named condition**: before P1 ships, either the WebView2 runtime's own outbound connections are shown to be controllable (environment options, WebView2 or Edge policies, or a documented allow-list), or the user-facing privacy documentation states them plainly. If neither is achievable, the owner should prefer option A, whose spike shows the shell is realisable natively (including label tracking via `RichText::extra_letter_spacing`) at the cost of backdrop blur and media/PDF/globe convenience.

1. **`oia-app` owns the shell.** Shared context, route stack, local filters, settings, job states, and the query sequence live in Rust and are exposed through typed commands and queries (architecture §7). The presentation layer never computes a domain value, never decides whether a late result applies, and never persists anything. Both spikes already work this way over one core crate.
2. **Versioned transport.** Every response carries a schema identifier, an outcome, and a coverage statement, as the Tauri spike's envelope does. Types are generated from Rust for the presentation layer once the P0 contracts merge; hand-written duplicates are not acceptable.
3. **Constrain presentation network authority.** Apply the registered-origin rules in §10 to direct imagery/data loads. Keep `default-src 'self'` as the baseline and grant only scoped presentation capabilities. General internet acquisition belongs to Rust source adapters under the application contracts; the page CSP is not a ban on APIs, feeds, scraping, or online development (§11). Source documents and web content remain untrusted data (`AGENTS.md` §5). The runtime's own traffic is handled by the condition above, not by this rule.
4. **Effects stay out of data.** Reduced motion, reduced transparency, effects quality, clean evidence, and focused reading are typed settings applied by the presentation layer; numerical and analysis results are unaffected by construction (`UX-05`).
5. **Presentation language: plain HTML, CSS, and JavaScript first**, with no build step, as in the spike. Rust compiled to WebAssembly (Leptos, Dioxus) stays open if presentation logic grows; TypeScript with generated types is the alternative. Not settled here.
6. **Globe deferred to #23**, with Cesium inside the same WebView as the option to evaluate first and a `wgpu` globe as the alternative.
7. **Exit path.** Because the presentation layer holds no state and no domain logic, a Rust-native renderer can replace it instrument by instrument against the same `oia-app` contracts. The egui spike is the existence proof. The ADR 0001 state model is the stable interface; this ADR adds nothing to it.

Why C over A, given that A was also built: A's measured visual gap is backdrop blur (plus hatched region fills; label tracking is supported via `RichText::extra_letter_spacing`), and A would need native video and PDF decoding for the P1 reader and P3 media and would still need a WebView for Cesium if #23 chooses it. C's spike reproduced the design as merged with less code and a smaller binary. A's advantages are real and measured: one process, under 210 MB of working set against 360 to 380 MB, and no runtime network traffic. The recommendation therefore comes with the condition rather than ignoring that difference.

## 5. Measurements

From the spikes' validation run, recorded in [`spikes/README.md`](../../spikes/README.md). Machine: Windows 11 Home 10.0.26200, Intel Core Ultra 7 265F, 64 GB RAM, NVIDIA GeForce RTX 5070, WebView2 runtime 156.0.4314.58. Toolchain: rustc 1.99.0; `tauri` 2.12.5, `wry` 0.57.1; `eframe`/`egui` 0.37.0 (`wgpu` default renderer via `egui-wgpu`). Release profile: `strip`, thin LTO, one codegen unit. Date: 2026-11-02 UTC. Five warm launches per binary, same script, same fixture, same scenarios.

| Measure | Tauri 2 | egui (`wgpu`) |
| --- | --- | --- |
| Release binary | 8,948,224 bytes | 14,620,160 bytes |
| Process start to UI ready / first frame | 305 to 321 ms | 383 to 465 ms |
| Process start to main window | 125 to 269 ms | 451 to 869 ms |
| Working set, main process | 26.6 to 32.7 MB | 156.7 to 208.3 MB |
| Working set, process tree | 361 to 382 MB (7 processes) | same as main (1 process) |
| Established outbound TCP, 4 s idle | 2 (WebView2 browser process) | 0 |
| Shared contract tests | 4 passed | 4 passed |

How the network check was made: after launch and a four-second settle, `Get-NetTCPConnection` was filtered to connections owned by the app's process tree (found through `Win32_Process` parent ids), excluding listening sockets; owners were resolved to process names and the WebView2 `--type` argument; remote addresses were looked up for PTR records. The page's CSP forbids remote origins and the UI has no network client, so page-initiated traffic is excluded by construction; the observed connections belong to the runtime.

These characterise two spikes on one warm machine. They are not OIA performance results and do not meet the measurement record the delivery plan asks for (benchmark commit, data hashes, cold and warm state).

## 6. Consequences

- **Accepted cost under C:** the WebView process tree costs several hundred megabytes of working set before any evidence is loaded, roughly twice the native spike. Note that the egui memory (156.7 to 208.3 MB) and startup figures were measured with `eframe` 0.37.0's default `wgpu` backend (`egui-wgpu`, which initializes a GPU device); an `egui_glow` OpenGL build was not measured. This belongs in P1 benchmarks and is the strongest technical argument for A if memory on field hardware (P4 offline work) turns out to matter.
- **Condition under C:** the runtime's outbound connections must be controlled or disclosed before P1 ships (see §4). A follow-up task should evaluate WebView2 environment options and policies and record the result; this ADR does not claim they work.
- **Accepted risk under C:** Linux rendering depends on WebKitGTK; glass may need the opaque fallback there. Supported platforms are a D14 matter and are not widened by this ADR.
- **Review rule under C:** any domain computation, state, or persistence appearing in the presentation layer is a blocking review finding.
- **Job completion attempt fencing for `oia-app`:** while the spike's `JobRegistry::finish` checks job state and `query_seq`, the production `oia-app` job contract must also fence completions by an attempt or generation token so a worker from a cancelled attempt can never complete a retried attempt (`AGENTS.md` §5).
- **Open items unchanged:** typeface and icon set (design §12), Rust→WASM versus TypeScript (§4.5), globe technology (#23), light theme (not P1), responsive behaviour below 1100 px in either spike.
- **Consistency item for #9/#16:** the `oia-app` state model in PR #56 carries token values (`#090d12` canvas, `#2ea043` green, `#58a6ff` cyan, `#d29922` amber) that differ from the merged design draft (`#000000`, `#5fe08a`, `#45d1ff`, `#f59e2e`, plus red and violet). One vocabulary must win before implementation; this ADR does not pick it.

## 7. Validation and limits

Ran locally in `spikes/` on 2026-11-02: `cargo +1.99.0 fmt --all -- --check`, `cargo +1.99.0 clippy --workspace --all-targets --locked -- -D warnings`, `cargo +1.99.0 test --workspace --locked` (4 tests on the shared core: route back restores the whole entry; late search is superseded and not applied; persisted state round-trips and corruption is moved aside and reported without losing the unreadable file; settings leave the context untouched), debug and release builds of both front-ends, and keyboard- and mouse-driven scenarios on both release builds with screenshots in each package's `screenshots/` directory.

Not done: no screen-reader session; no Linux or macOS build; `iced` and `Slint` not built; `egui_glow` backend not measured; no cold-boot timing; no test against bright imagery (A02 reading surfaces); no identification of the WebView2 endpoints beyond their addresses.

## 8. Alternatives rejected for now

- **Build the P1 shell in `egui` directly.** Rejected for now on the measured backdrop-blur gap and the documented media, PDF, and globe costs, while kept as the exit path and as the fallback if the network condition in §4 cannot be met. Reconsider if the memory cost in §6 proves unacceptable or if the toolkit gains a backdrop-blur primitive.
- **Decide the globe now.** Rejected; #23 owns it and the P1 shell does not need it.
- **Adopt a JavaScript framework and bundler.** Rejected for P1; the shell's presentation logic is small and a build step would add a toolchain the repository does not yet have.

## 9. Owner decision recorded

On 2026-11-02 America/New_York, the project owner selected “Tauri desktop app (recommended by the existing ADR)” for the running app. Option C is accepted with the rules and runtime-traffic disclosure condition in §4. The first workbench implements local file tools without claiming persistent cases or complete P1 acceptance. Earlier comparison results above describe the evaluation spikes, not the new application.

## 10. Amendment: registered online imagery in the WebView (2026-11-03)

**Owner decision.** On 2026-11-03, in the globe session (T3 thread `44c6c7f7`, recorded on [#23 (comment 6594215166)](https://github.com/mESmaC/oia/issues/23#issuecomment-6594215166)), the project owner stated that reading §4 rule 3 as a ban on online data "is actually completely in violation of what I decided for the spec", and that online sources are encouraged, satellite APIs included, free sources preferred. The specification already requires them: `GEO-02` queries satellite imagery and other sources for the selected event, `CAM-06` names Overpass and DeFlock, and [`docs/architecture.md` §4](../architecture.md#4-source-adapters-and-ingestion) asks that each source be registered with a bounded query, a budget and recorded coverage.

**What changes.** §4 rule 3 was written for the P1 file tools and read as "no network client in the WebView". For a tile-based globe (§4 rule 6) that reading is impractical: routing every tile through Rust IPC would add latency and code without adding control. Rule 3 is therefore amended:

> The page may load imagery, tiles and the data behind them **only** from origins that are (a) registered in Rust as sources with their licence, attribution, capture statement and the date their capabilities were checked, (b) allow-listed by origin in the CSP, and (c) shown in the instrument with attribution, source status and capture or retrieval time. Everything else in rule 3 stands: `default-src 'self'`, no inline script, no `unsafe-eval`, no plugin beyond core window and event permissions, and source content remains data.

The owner also confirmed the decision directly to the review session, recorded on [#16 (comment 6594753551)](https://github.com/mESmaC/oia/issues/16#issuecomment-6594753551).

The first registered origin is `https://gibs.earthdata.nasa.gov` (NASA GIBS, keyless; two NASA public-domain layers, each with its licence and attribution in the registry), used by the Globe instrument ([globe-instrument.md](../implementation/globe-instrument.md)). Adding an origin is a change to the Rust registry, the CSP and the notices together, reviewed as such; a page cannot compose a URL to an origin that is not registered. The WebView2 runtime-traffic condition in §4 is unchanged.

**Origins added 2026-11-03 (Globe detail imagery, #23).** `https://tiles.maps.eox.at` (EOX Sentinel-2 cloudless 2025, CC BY-NC-SA 4.0 for non-commercial use, [notices §5](../../THIRD_PARTY_NOTICES.md)) and `https://basemap.nationalmap.gov` (USGS The National Map orthoimagery, public domain, [notices §6](../../THIRD_PARTY_NOTICES.md)), each registered in Rust with its licence, attribution, capture statement, coverage and checked date. They are drawn only when zoomed in, and each is attributed in the frames that draw it ([globe-instrument.md](../implementation/globe-instrument.md)). A unit test in `imagery.rs` fails if the CSP's remote origins differ from the registry.

**Closer detail addition (2026-11-03, #23).** `https://imagery.nationalmap.gov` serves dynamic USGS NAIP natural-colour image exports, primarily 0.6 m aerial imagery within a continental-US rectangle. Rust registers its processing/capture limitations, public-domain terms, level-18 display cap and six-in-flight limit; CSP and notices §7 follow together. Transparent no-data exposes the lower tier. External scientific-product discovery addresses are copyable references only and add no page network permission or ingest capability.

**CSP as amended.** `default-src 'self'; connect-src 'self' ipc: http://ipc.localhost https://gibs.earthdata.nasa.gov https://tiles.maps.eox.at https://basemap.nationalmap.gov https://imagery.nationalmap.gov; img-src 'self' data: blob: https://gibs.earthdata.nasa.gov https://tiles.maps.eox.at https://basemap.nationalmap.gov https://imagery.nationalmap.gov; style-src 'self'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-src 'none'`. Beyond the registered origins, the additions are `'wasm-unsafe-eval'` (the renderer's bundled WebAssembly modules, compiled from files, not strings), `blob:` in `img-src` (tile bytes turned into object URLs by the renderer), and `'self'` in `connect-src` (the renderer's own asset fetches). `'unsafe-inline'`, `'unsafe-eval'` and `blob:` scripts or workers remain refused; the implementation note records the two bundle adjustments that made that possible.

**Addendum, 2026-11-03 (Media instrument).** `img-src` also allows the Rust-owned `oia-snapshot` URI scheme (`http://oia-snapshot.localhost` under WebView2, `oia-snapshot:` elsewhere), through which the page displays the image snapshot the user opened through the native dialog, keyed by its SHA-256 and served read-only with no caching; it is not a remote origin and reads no path ([desktop-workbench.md, Runtime privacy](../implementation/desktop-workbench.md#runtime-privacy)).

**Rule 6 outcome.** Cesium inside the same WebView was evaluated first, as rule 6 asked, and is adopted for the globe: self-hosted at a pinned version, fetched by the one-time `fetch` step, with Cesium Ion disabled and the one load-time patch recorded in the implementation note. The `wgpu` alternative remains the exit path of rule 7.

## 11. Clarification: internet acquisition is product scope (2026-11-03)

**Caltrans acquisition slice (2026-11-05, #164).** A provider-specific Rust worker retrieves explicitly requested public highway camera catalogs and current stills from `https://cwwp2.dot.ca.gov`, under bounded bytes/time/concurrency and pinned-path rules. The page receives no new remote permission: the existing `oia-snapshot` scheme also serves the one selected, visible traffic still at `traffic/<sha256>` from held memory with no-store headers. This applies §11's Rust-acquisition boundary rather than widening §10's direct-renderer allow-list. Catalog edits, retrieval and unknown capture times stay distinct; ground frames are separate from overhead mosaics. [Implementation and terms](../implementation/globe-instrument.md#highway-camera-imagery-164); shared source/jobs/storage integration remains open.

The owner's follow-up specification audit, tracked in [#110](https://github.com/mESmaC/oia/issues/110), confirms that public internet collection and construction of intelligence from it are central to OIA. This clarification replaces §4 rule 3's broad “no network client” wording. Section 10 records the current direct-renderer imagery capability; it is not an exclusive list of product acquisition methods. No runtime configuration or provider support changes in this documentation amendment.

| Audit finding | Scope and resolution |
| --- | --- |
| Former §4 rule 3 could be read as prohibiting remote sources; §10 only named imagery | Rule 3 now distinguishes presentation authority from Rust acquisition. SOURCE-01–SOURCE-04 explicitly cover APIs, feeds, public web discovery, scraping/crawling, and derived intelligence. |
| Local-first architecture and offline fixture corpora could be read as offline-only development/product policy | Local retained work remains usable without connectivity; online research/development and live collection are supported direction. ADR 0005's synthetic fixtures and proposed candidate list do not restrict the universe of future sources. |
| ADR 0005 tied all live source selection to D06 regional selection | D06 now gates regional choices only; online evaluation may precede production integration, and rights checks apply to the requested use. |
| ADR 0007 described all workspaces as local-only with no external egress | The default now explicitly scopes local execution and opt-in to model providers; configured public-source collection remains available. |
| No-network parsers, model synthesis, passive report/bundle opening, and self-contained design mockups | These constrain particular components or opening behavior. Collection services supply their inputs; the restrictions do not prohibit explicit acquisition jobs. |
| Source-use conditions and camera restrictions | Keep per-source, per-operation conditions and existing CAM-01/CAM-02 boundaries. Missing redistribution permission is not itself a ban on ordinary reading or reference-based research. Source-specific polling/crawl budgets do not become product-wide scraping bans. |
| Cinematic design versus evidence fidelity | The SCIF/satellite-console atmosphere comes from real collection, connected views, and inspectable reconstruction. Preserve accurate source labels and distinguish derived, simulated, and observed information. |

Implementation is still tracked by the existing packages listed in [the backlog mapping](../backlog.md#specification-clarification-internet-collection). [A19](../delivery.md#a19-internet-collection-and-derived-intelligence) defines planned integrated verification, including separately identified live acquisition. This amendment reports no new provider validation, measured performance, or passing acceptance scenario.
