# God's Eye View: selective reuse notes

Status: initial source inspection supporting OIA's [product specification](spec.md) and [architecture](architecture.md). This is not a complete audit, a runtime validation, or a commitment to feature parity.

## Reference

- Local checkout: `C:\Users\engin\Desktop\New folder\gods-eye-view`.
- Repository declared in `package.json`: https://github.com/bilawalsidhu/gods-eye-view.
- Inspected on 2026-11-01 at HEAD `0d1a68160e532ba65dc34b48ea205de6de4028fc`; no tracked working-tree changes were reported.
- Existing application: JavaScript, a Cesium globe, and Node provider services.

Paths below are relative to that checkout. Findings are based on selected documentation, source modules, and test cases. The application and its test suite were not run for this inventory; provider availability and terms were not independently revalidated.

## Reuse direction

OIA will selectively carry forward useful algorithms, source integration knowledge, data contracts, regression cases, and operational lessons into Rust. The existing application's screen layout, feature catalog, rendering stack, and compatibility requirements do not define OIA's design.

Preserve a behavior because it serves an OIA requirement and survives validation. A literal port can preserve a defect or a visualization shortcut just as faithfully as a useful algorithm. Keep evidence records independent of display interpolation, fallback values, and renderer-specific state.

## Concrete starting points

### Provider integrations and request handling

Inspect `server/providers/`, `server/providers/common/`, and `src/sources/`, alongside `DATA_SOURCES.md` and `THIRD_PARTY_NOTICES.md`.

These provide a catalog of source integrations and reusable patterns for normalization, bounded responses, cancellation, rate limits, and error handling. `server/providers/common/request.test.mjs` covers byte limits, partial aborts, late data, early closes, and competing completion paths.

Rust adapters can adopt the useful contracts while checking each provider's current authentication, coverage, historical availability, and reuse conditions. Missing, stale, failed, and genuinely empty responses should remain different outcomes.

### Aircraft and vessel records

Start with `src/layers/flights/records.js`, `records.test.mjs`, `recordPolicy.js`, and the corresponding ingestion and vessel modules.

Aircraft records already address intermittent missing metadata, retention across missing polls, ground transitions, and separate aviation and rendering heights. Port the relevant state transitions and regression scenarios, with explicit observation timestamps and freshness for retained fields.

Some defaults exist to keep a live visualization usable. An assumed altitude or held heading must not become a measured value in an OIA investigation. Barometric altitude is also not automatically orthometric height; geoid conversion alone is not a physical correction for pressure-derived altitude.

### Satellite passes and geographic calculations

`src/data/satellitePass.js` uses SGP4 propagation through `satellite.js`, coordinate transforms, solar position estimates, and a simplified Earth-shadow model. Its tests include pass boundaries, invalid search options, short visibility windows, and passes already underway.

This is a useful algorithm and test reference for a Rust implementation. Numerical search precision is distinct from physical prediction accuracy. Historical investigations need orbital elements appropriate to the event date, recorded element age, and explicit visibility assumptions. The documented current pass feature is a bounded future search, not an established historical reconstruction service.

`src/data/geoid.js`, `geoid.test.mjs`, and related terrain/render-altitude modules document height-reference and longitude-wrap issues. Carry forward explicit units and coordinate references; validate Rust numerical behavior against independent references as well as the existing implementation.

### Public cameras

Inspect `src/layers/cctv/source.js`, `sourcePolicy.js`, `server/providers/cctv/`, and the camera calibration and ground-height handling.

The source interface distinguishes catalog and health queries, frames, and media. Frame provenance includes upstream images, HLS, Street View, and synthetic substitutes. OIA should preserve these distinctions and admit only qualifying camera material as event evidence. A substitute view is not a recording of the camera's scene at the requested time.

Reuse relevant catalog, geometry, freshness, and stream-handling knowledge. OIA's camera scope starts with existing camera websites, directories, and ordinarily accessible feeds, without requiring intentional publication by the camera owner or attempting to gain additional access. Exclude cameras inside homes and views into home interiors. Follow requirements CAM-01 and CAM-02 in the [product specification](spec.md) and record source conditions for automated analysis and retention. Existing live support does not establish archive access.

### Weather history and recent imagery

Start with `src/layers/weather/source.js`, `clock.js`, `clock.test.mjs`, and `src/layers/recentImagery/`. The imagery source uses replaceable acquisition functions for NASA-compatible catalogs, snapshots, and tiles.

Weather validation and clock tests cover explicit available times, per-product frame selection, gaps, cancellation, and superseded requests. These are useful precedents for an investigation timeline where different sources have different capture times and coverage.

Retain distinctions among forecasts, observations, and display effects. `docs/KNOWN-ISSUES.md` documents geographic coverage limits, source latency, and a weather-history sharing limitation. OIA case exports should retain the selected investigation time and referenced source versions rather than silently reopen at the latest data.

### Query tools and common context

`docs/TOOLS.md`, `src/tools/queries/`, and `src/agent/worldPacket.js` provide references for structured queries and shared application context. The documented tool interface separates queries from actions and returns both a human-readable summary and structured data, with adapters for MCP and function calling.

Reuse that separation so OIA's map, report builder, and AI interface can operate on the same investigation services. Define OIA's result contracts around citations, time windows, completeness, and uncertainty. Queries over bounded loaded records should not imply exhaustive coverage of an area.

### Portable scenes, evidence assets, and lifecycle behavior

`docs/SCENE-DOCUMENT.md` and `docs/DIRECTOR-DATA-PACKS.md` describe versioned documents, validated imports, geographic placement, attribution, optional asset hashes, and bounded loading. `src/data/lifecycle.js` contains useful cancellation and superseded-operation patterns.

Adapt these ideas to investigation bundles, source manifests, and saved views. OIA needs relationships between evidence and conclusions in addition to presentation state. The documented media packs do not synchronize media time with scene time, so a common evidence playback clock remains additional work.

## Lessons to carry forward

- Separate acquisition, normalized records, analysis, and presentation so a rendering choice cannot silently alter evidence.
- Preserve source timestamps and geographic/vertical references throughout transformations.
- Make cancellation, partial results, stale data, and resource cleanup explicit.
- Keep source attribution and fallback identity visible in views and exports.
- Reuse regression scenarios and permitted fixtures, including failure cases, rather than only happy-path examples.
- Use the existing performance and known-issue notes as design inputs; performance claims need fresh measurements on OIA's implementation and target systems.

## Attribution and data reuse

The checkout's `LICENSE` grants MIT terms for source code, with copyright attributed to Bilawal Sidhu. Retain the required copyright and permission notice when incorporating covered code or substantial portions, and check module-level notices for other contributions.

The same license explicitly excludes third-party datasets and assets. Examples include separately licensed submarine cable data, OSM-derived datasets, event imagery, and 3D models. Review `DATA_SOURCES.md`, `THIRD_PARTY_NOTICES.md`, and asset-specific notices before carrying material over. Reusing an integration does not grant permission to redistribute its upstream data.

## Candidate port workflow

1. Choose one capability that supports an OIA investigation workflow.
2. Record its source revision, relevant files, attribution, behavior, assumptions, and known limitations.
3. Define a Rust contract with explicit units, time semantics, provenance, and error states.
4. Adapt permitted regression fixtures; compare old and new behavior and use independent references for mathematical or scientific claims.
5. Add focused Lean specifications and proofs where they establish useful properties, documenting the connection to Rust and any floating-point assumptions.
6. Integrate the capability into OIA's shared workspace and validate the end-to-end investigation behavior.

Potential early candidates are time selection and freshness handling, geographic and height-reference primitives, satellite pass calculations, and one source adapter feeding the evidence timeline. Selection and order remain open.

Document research, source lineage, historical report correlation, image forensics, and recreation prompt export remain OIA workstreams; this inspection does not establish that the reference project implements them.
