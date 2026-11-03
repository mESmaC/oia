# Globe instrument

Implemented slice of [#23](https://github.com/mESmaC/oia/issues/23) in the desktop workbench ([desktop-workbench.md](desktop-workbench.md)): an interactive CesiumJS globe inside the Tauri WebView, drawing a bundled synthetic case in the [visual language v2](../design/visual-language-v2.md) instrument language over live basemap tiles from a registered online source. It follows the owner's direction of 2026-11-03 that online sources, satellite imagery included, are part of the product and preferred free ([ADR 0008 §10](../adr/0008-d01-workspace-renderer.md#10-amendment-registered-online-imagery-in-the-webview-2026-11-03)).

Requirements touched: `GEO-01` (areas, points, approximate locations, reported local time with uncertainty), `GEO-02` (imagery with capture statement, coverage, source status), `UX-01` to `UX-05`, and the globe and camera subsets of `A02` and `A04`. Persistent cases, clustering on the globe, footprints and viewing cones, tracks, terrain, and the `#24` to `#26` context sources are not in this slice.

## Run

```text
scripts/run.sh fetch      # once: locked crates + the pinned CesiumJS build (verified SHA-512)
cargo run -p oia-app --locked --offline
```

Then press Ctrl+G or choose Globe in the rail. `fetch` must precede the build: the vendor directory is embedded into the binary at build time. Without it the instrument shows "CesiumJS build not present" with the command to run; nothing else in the workbench depends on it.

## What it does

- **Case.** `fixtures/globe/synthetic-valley-case.json` ([fixture README](../../fixtures/globe/README.md)) is compiled in and validated at startup by `crates/oia-app/src/globe/mod.rs`. Only `origin_class: "synthetic"` is accepted by this build. An invalid fixture is reported in the instrument as a fixture error and nothing is drawn.
- **Geometry (v2 §4).** A `point` is a 7 px dot with its uncertainty radius drawn as a ring; an `approximate_centroid` is a hollow amber dot with a dashed radius ring and a `≈ place` tag; a `region` is a striped fill with a dashed outline and no centre, and its tag leads to its northernmost vertex. Each plotted record has a callout tag (order number, label) with a one-elbow leader ending in a dot on the geometry. Records without geometry go to the **Not plotted** tray with their recorded reason and no leader. Tags whose anchors coincide on screen are stacked in display order.
- **Selection.** Click a tag, a record in the inspector, a tray entry, or the drawn geometry; Escape clears. One bracket is drawn around the projected bounds of the selected geometry, and a target card lists the record's values (`±` and `≈` in amber, "not recorded" where the fixture says so). If the selected geometry is off screen the camera flies to it.
- **Imagery.** The inspector's Imagery tab lists the registered sources from `crates/oia-app/src/globe/imagery.rs`: NASA GIBS Blue Marble (composite, no date), VIIRS true colour (daily, UTC date chosen by the user, validated against the coverage periods GIBS published when its capabilities were read on 2026-11-03), and a coastline reference overlay. The attribution strip names the provider, layer, date and the tile counts this renderer observed (loaded, failed, pending). A day inside a published gap or in the future is refused with the reason; a day after the recorded coverage is allowed with a note that tiles may not exist yet.
- **Camera.** Drag to orbit, wheel to zoom; with the globe focused, arrows pan, `+`/`-` zoom, Home returns to the case anchor, `0` shows the whole globe. After each move the page reports the camera to Rust, which validates it (finite, in range) and returns the view; the view-state strip prints the altitude from that record.
- **Effects.** The shell's Effects toggle also removes the atmosphere and fog; Clean evidence (Ctrl+Shift+E, or the canvas tool) hides tags, leaders, brackets and drawn geometry. Neither changes a value. Sun, moon, sky box and globe lighting are off so nothing on the globe reads as a live time of day for a historical case.

## Boundary

Rust owns the state (ADR 0008 §4 rule 1): the parsed case, the imagery registry and selection, the camera and the selected record live in `GlobeState`, exposed through four commands (`globe_view`, `globe_select`, `globe_camera`, `globe_imagery`) that return the whole `GlobeView` in the `oia-local-tools/1` envelope. `ui/globe.js` draws that view and sends intents; its own state is the Cesium objects, projected screen positions, the overlays toggle and tile counters it observed. It computes no domain value: positions, radii, times, zones and labels are printed as sent, and the only arithmetic is projection to the screen and camera motion.

Geometry and time types are provisional pending `#12` (`oia-time-geo`), kept to a validated WGS84 point, a reported-time record with a precision kind, and a camera view. They are not a second domain model and should be replaced by the shared primitives when they merge.

| Bound | Value |
| --- | --- |
| Records per case | 512 |
| Region ring | 3 to 256 vertices |
| Text fields | 2,048 bytes |
| Camera height | 50 m to 60,000 km |
| Remote origins | one, `https://gibs.earthdata.nasa.gov`, allow-listed in the CSP |
| Tile requests | bounded by Cesium's per-server limit (6 in flight) and the visible view; no prefetch beyond it |
| Renderer messages kept | 24, shown in the inspector |

## Renderer

CesiumJS 1.147.0 (Apache-2.0, [notices §3](../../THIRD_PARTY_NOTICES.md)) is downloaded by `scripts/fetch-ui-vendor.{sh,ps1}` into the gitignored `crates/oia-app/ui/vendor/cesium/`, after the npm tarball's SHA-512 is verified. Only the minified browser build, `Assets/`, `Workers/` and `ThirdParty/` are kept with the licence files. Cesium Ion is disabled (no token, no Ion assets); terrain is the WGS84 ellipsoid; the widget stylesheet is replaced by six rules in `ui/globe.css`.

Two things were needed to run the bundle under the workbench CSP (`default-src 'self'`, no `'unsafe-inline'`, no `'unsafe-eval'`), both recorded here so that a Cesium upgrade re-examines them:

1. **Fetch-time patch.** The bundled Knockout library resolves the global object with `var t=this||(0,eval)("this")` while the bundle loads; `eval` is refused and the whole bundle fails to define `Cesium`. The fetch scripts replace that expression with `var t=globalThis`, require that it matches exactly once, and fail otherwise. OIA uses no Cesium widget, so no Knockout binding (which would also use `new Function`) is exercised.
2. **Worker path.** The combined build also ships its worker code as a string (`globalThis.CESIUM_WORKERS`) and, when it is present, starts workers from `blob:` URLs with `importScripts`, which `script-src 'self'` refuses. `ui/globe.js` deletes that global after the bundle loads, so Cesium starts the same-origin module workers from `vendor/cesium/Workers/` instead.

The CSP gained `'wasm-unsafe-eval'` for Cesium's bundled WebAssembly modules (compiled from files, not strings), `blob:` in `img-src` and `worker-src`, `'self'` in `connect-src` for the renderer's own asset fetches, and the registered imagery origin in `connect-src` and `img-src`. Inline style attributes that Cesium's unused widget code tries to set remain blocked and are listed in the inspector's renderer messages; they have no effect on the instrument.

## Honesty rules applied

- Nothing drawn from the fixture is an observation; the case chip says `synthetic` and every inspector view names the fixture path.
- A record without recorded geometry never receives a position (tray only). A region never receives a centre. An approximate centroid is labelled `≈ place` with its radius and basis.
- Imagery is display context: the capture statement says what the pixels are (a 2004 composite; a daily VIIRS mosaic for a UTC day with variable overpass time) and that none of it shows the case period. A missing tile or an unpublished day is a coverage gap, never an empty scene.
- Tile counts are what this renderer observed in this session, labelled as such. The attribution strip carries the GIBS acknowledgement, and the licence, terms URL and capabilities check date are in the inspector.
- Camera altitude is a view setting from the Rust record, not a measurement. No terrain, labels, roads or tracks are drawn, because none is registered.

## Validation (2026-11-03, Windows 11, Windows PowerShell 5.1 driving the real application)

- `cargo +1.99.0 fmt --all -- --check`, `cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings`, `cargo +1.99.0 test --workspace --locked --offline`: all passed. New targeted tests: the bundled fixture separates plotted from not-plotted records and a region keeps its ring (`GEO-01`); an invalid fixture (missing absence reason, out-of-range latitude, non-synthetic origin) and a non-finite camera are refused, not clamped (`AGENTS.md` §5); imagery dates follow the published coverage, refusing a gap and the future and noting a day beyond the recorded coverage (`GEO-02`, including a month-boundary default and a leap-day round trip).
- `scripts/fetch-ui-vendor.sh` and `.\scripts\run.ps1 fetch` (which runs `fetch-ui-vendor.ps1`) both end to end: download, SHA-512 match, patch applied exactly once, 13 MB vendored, `VERSION` written; a second run is skipped as already present.
- Real application (`target/debug/oia.exe`, launched and driven by keystrokes, clicks and window capture): Ctrl+G opens the Globe over the case anchor at 80 km with Blue Marble tiles (32 loaded, 0 failed in the first capture) and three stacked tags; Tab and Enter select Account A, then Account C, with the bracket, target card and inspector agreeing with the fixture (`6 (no centre)`, `± 150 m`, `≈ 21:40 ±15 min · America/Denver`, `— not recorded`); `0` shows the whole globe and the altitude readout updates from the Rust record (20,000 km); the Imagery tab switches to VIIRS true colour for 2026-10-01 and the attribution strip and inspector show the layer, date and tile counts; Effects on makes the panels opaque and removes the atmosphere while every value stays the same. Screenshots were inspected by the authoring session and are not committed.
- Failure states exercised on the way: the bundle absent from `vendor/` (renderer-not-present message), the unpatched bundle (EvalError reported in the instrument with the CSP directive), and the blob worker path (importScripts refusals listed in the renderer messages). Each stayed inside the instrument; the rest of the workbench kept working.
- Coastline overlay: enabled from the Imagery tab at world scale in the real application; the attribution strip listed `+ Coastlines_15m` and the lines drew over the Blue Marble tiles.
- Not run: screen readers; Linux and macOS WebViews; touch; a network-down session (the expected behaviour is tiles counted as failed and the surface left dark, which was not provoked); no browser harness or axe pass was run for this slice.

## Follow-ups

- Replace the provisional point, time and camera types with `oia-time-geo` when `#12` merges, and carry the case from the persistent store when `#14` lands; the fixture loader is the seam.
- Footprints, viewing cones and clustering on the globe (`GEO-03`, `GEO-04`) once their geometry types exist; the `oia-query` clustering ledger has no coordinates yet.
- A second registered source for terrain, and the `#24` to `#26` context layers, each added to the registry, the CSP and the notices together.
- Tag placement at the nearest visible vertex for regions, and a timeline strip for the reported windows (v2 §4.11).
