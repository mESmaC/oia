# Implementation: ALPR and Flock Camera Map Kernel (`oia-alpr-map`)

This document records the delivered implementation for the **community-mapped ALPR / Flock camera catalog & map draft slice** of GitHub Issue [#28](https://github.com/mESmaC/oia/issues/28) (`[P3] Integrate eligible camera directories, feeds, and retained clips`), implementing requirement [`CAM-06`](../spec.md#51-internet-cameras-and-anomaly-detection) and the contract in [ALPR Camera Map Contract](../contracts/alpr-camera-map.md).

## 1. Delivered Scope

| Artifact | Purpose | Status |
| --- | --- | --- |
| [`docs/spec.md`](../spec.md#51-internet-cameras-and-anomaly-detection) & [`docs/prior-art.md`](../prior-art.md) | Adds requirement `CAM-06` (community-mapped surveillance-infrastructure and ALPR directory layer, kept distinct from `CAM-01`/`CAM-02` viewable camera feeds) and records the `src/layers/alpr/` selective reuse entry from God's Eye View (`0d1a68160e532ba65dc34b48ea205de6de4028fc`) | Implemented |
| [`docs/contracts/alpr-camera-map.md`](../contracts/alpr-camera-map.md) | Canonical OIA contract for `CAM-06` community-mapped ALPR and Flock camera directories, provenance boundaries, upstream source terms (Overpass API, DeFlock vector tiles, ODbL 1.0 share-alike / Collective vs. Derivative Database boundary checked `2026-11-02`), and viewport/coverage rules | Implemented |
| [`crates/oia-alpr-map/`](../../crates/oia-alpr-map/Cargo.toml) | Standalone zero-dependency Rust 2024 package implementing tag normalization, manufacturer classification (`FlockSafety`, `OtherNamed`, `Unspecified`), illustrative directional bearing-wedge geometry, viewport bounds, precision caching, coverage status, and synthetic-marker preservation | Implemented and verified |
| [`crates/oia-alpr-map/examples/inspect-alpr-map.rs`](../../crates/oia-alpr-map/examples/inspect-alpr-map.rs) | Offline CLI inspection tool that evaluates the fictional New Meridian corridor fixture and prints coverage status, manufacturer breakdown, marker details, and synthetic provenance tags | Implemented and verified |
| [`fixtures/cameras/alpr/`](../../fixtures/cameras/alpr/README.md) | Deterministic synthetic New Meridian corridor fixture (`new-meridian-corridor-fixture.json`) with fictional geography/operators, reserved synthetic OSM node IDs (`>= 900_000_000_000`), and per-record `"synthetic": true` + `note="SYNTHETIC FIXTURE - NOT OBSERVED"` + `oia:synthetic="true"` markers | Implemented and verified |
| [`docs/design/mockups/flock-camera-map.html`](../design/mockups/flock-camera-map.html) | Self-contained offline interactive Flock / ALPR camera map prototype in OIA's military-glass visual system (`UX-01` through `UX-05`, `CAM-06`) | Implemented |

## 2. Rust Kernel Architecture (`crates/oia-alpr-map`)

The standalone package (`crates/oia-alpr-map`) is isolated with its own `[workspace]` table so concurrent sessions can build and test without contending on a root `Cargo.toml`. It enforces strict Rust 2024 safety and error handling with zero third-party dependencies (`--locked --offline`).

### Core Types and Functions (`src/lib.rs`)

- [`AlprQueryPolicy`](../../crates/oia-alpr-map/src/lib.rs) and [`GeoBBox`](../../crates/oia-alpr-map/src/lib.rs): Enforces city-scale viewport bounds (`DEFAULT_MAX_VIEWPORT_DEGREES = 3.0`, outward `0.05°` grid snap, `DEFAULT_DETAIL_MAX_TILES = 16` across `z9..=z12`, `DEFAULT_QUERY_LIMIT = 1,500`, `DEFAULT_SOURCE_RECORD_LIMIT = 6,000`, `DEFAULT_PRECISION_CACHE_LIMIT = 20,000`) and exponential backoff (`alpr_retry_delay_ms`, `30,000 ms` doubling to `240,000 ms`).
- [`AlprManufacturerClass`](../../crates/oia-alpr-map/src/lib.rs) and [`classify_manufacturer`](../../crates/oia-alpr-map/src/lib.rs): Normalizes `manufacturer`, `brand`, and `operator` tags into `FlockSafety`, `OtherNamed(String)`, or `Unspecified` without inventing a vendor when a node is untagged.
- [`normalize_direction_deg`](../../crates/oia-alpr-map/src/lib.rs) and [`direction_wedge_polygon`](../../crates/oia-alpr-map/src/lib.rs): Parses numeric bearings in `[0.0, 360.0)` (`360.0` wraps to `0.0`), rejects blank strings and cardinal words (`"N"`, `"NE"`) as `None` so missing data never becomes `0°` north, and computes a 3-point spherical bearing wedge (`90 m`, `±20°`) tagged with `GeometryClass::IllustrativeBearingWedge`.
- [`normalize_alpr_node`](../../crates/oia-alpr-map/src/lib.rs) and [`AlprCameraRecord`](../../crates/oia-alpr-map/src/lib.rs): Validates positive node IDs and WGS84 coordinates, matches case-insensitive semicolon-delimited `surveillance:type` values (`is_alpr_surveillance_type`), keeps `last_verified` (`check_date` / `survey:date`) strictly separate from `osm_timestamp`, and preserves `note` and `synthetic` markers (`raw.synthetic`, `oia:synthetic=true`, or `osm_id >= SYNTHETIC_OSM_ID_FLOOR` where `SYNTHETIC_OSM_ID_FLOOR = 900_000_000_000`).
- [`AlprPrecisionCache`](../../crates/oia-alpr-map/src/lib.rs): Retains finer-zoom coordinates (`z12`) when coarser overview tiles (`z9`, `z10`) are loaded later for the same camera ID, while still refreshing non-coordinate tags.
- [`evaluate_viewport_query`](../../crates/oia-alpr-map/src/lib.rs): Evaluates bounded viewport queries with cooperative cancellation (`StopReason`), local manufacturer/bearing filtering (`AlprLocalFilter`), unfiltered `ManufacturerBreakdown` accounting, and explicit `AlprCoverageStatus` (`Complete`, `Saturated`, `Empty`, `NoCoverage`, `ZoomRequired`).

### Upstream Source Terms Summary (`CAM-06`, checked `2026-11-02`)

As recorded in [contract §2.2 and §2.3](../contracts/alpr-camera-map.md#22-upstream-source-terms-and-reuse-conditions-agentsmd-6-cam-06-checked-2026-11-02):

- **Overpass API (`https://overpass-api.de/api/interpreter`):** Shared community endpoint governed by the Overpass API usage policy (`https://dev.overpass-api.de/overpass-doc/en/preface/commons.html`). OIA complies via `3.0°` max viewport span, `0.05°` snap reuse, `[out:json][timeout:20]`, `out body 1500;`, and `30 s..240 s` retry backoff (`alpr_retry_delay_ms`).
- **DeFlock vector tiles (`https://tiles.dontgetflocked.com`, `https://deflock.me`):** Third-party hourly US/Canada OSM-derived ALPR extract. Requires ODbL 1.0 attribution (`© OpenStreetMap contributors`) and DeFlock source credit; because no standalone commercial bulk-scraping SLA is published on the tile host (checked `2026-11-02`), automated bulk tile scraping is prohibited and queries remain bounded to interactive `z9..=z12` viewports (`<= 16` tiles).
- **ODbL 1.0 share-alike and Collective vs. Derivative Database boundary:** OIA's `MIT OR Apache-2.0` code license (`D14`) does not cover OSM/DeFlock data. Retained or exported ALPR node collections in OIA cases/bundles (`REPRO-01`) form an ODbL 1.0 Derivative Database inside a Collective Database boundary, requiring `© OpenStreetMap contributors (ODbL 1.0)` attribution and machine-readable ODbL 1.0 share-alike availability when redistributed.

## 3. CLI Inspection Usage

Run the standalone CLI example against the fictional New Meridian corridor fixture:

```powershell
cargo +1.99.0 run --manifest-path crates/oia-alpr-map/Cargo.toml --example inspect-alpr-map --locked --offline -- all
cargo +1.99.0 run --manifest-path crates/oia-alpr-map/Cargo.toml --example inspect-alpr-map --locked --offline -- flock
```

Sample output (`all` filter mode):

```text
origin=synthetic-fixture corridor="New Meridian Synthetic Corridor" requirement=CAM-06 source=OpenStreetMap-ODbL-1.0
viewport=37.6050,-121.9150..37.6350,-121.8750 snapped=37.60,-121.95..37.65,-121.85 detail_zoom=Some(12)
coverage=Complete active_filter="All mapped ALPR" local_filter_omitted=0
viewport_breakdown total_alpr=8 flock_safety=5 other_named=2 unspecified=1 with_bearing=6 field_verified=5
camera id=alpr:900000000101 badge=ALPR-0101 synthetic=true class="Flock Safety" operator=Some("New Meridian Municipal Transit Authority (Synthetic)") ref=Some("SYNTH-NM-001") pos=(37.6172,-121.8931) wedge=illustrative-wedge(180deg, 90m, +/-20deg) check_date=Some("2026-09-14") osm_timestamp=Some("2026-10-18T14:22:10Z") note=Some("SYNTHETIC FIXTURE - NOT OBSERVED")
```

## 4. Interactive Map Prototype (`docs/design/mockups/flock-camera-map.html`)

The interactive HTML prototype ([`flock-camera-map.html`](../design/mockups/flock-camera-map.html)) implements the OIA military-glass layout (`UX-01` through `UX-05`, `CAM-06`) using the fictional `New Meridian Synthetic Corridor` fixture and runs 100% offline:

- **Header & Fixture Banner:** Clearly labels the view as a synthetic fixture (`New Meridian Synthetic Corridor`, `37.605°..37.635° N, 121.915°..121.875° W`, reserved synthetic OSM node IDs `900000000101`..`900000000108`, `CAM-06`) with opaque-panel and reduced-motion toggles.
- **Toolbar & Local Filter Bar (`CASE-02`):** Filter pills (`All mapped ALPR (8)`, `Flock Safety only (5)`, `Other / unspecified (3)`, `Bearing recorded (6)`), simulated coverage-mode buttons (`City z12 (Complete)`, `Cap=2 (Saturated)`, `Outside US/CA`, `Wide >3°`), and a dashed local-filter chip reporting omitted counts.
- **Center Map Stage & HUD:** Displays `DETAIL ZOOM z12 (≤16 tiles) · OUTCOME: COMPLETE · 8 of 8 mapped in extract` so coverage is explicitly scoped to the mapped extract rather than claiming exhaustive physical road coverage. Renders fictional corridor roads (`Meridian Axis Blvd`, `Harbor Gate Causeway`, `North Spire Transit Way`, `Kestrel Commons Pkwy`, `Solstice Ridge Way`), non-color-only vendor glyphs, and dashed 90 m / ±20° illustrative bearing wedges (hidden in `Clean evidence` mode).
- **Right Entity Inspector (`Overview` and `Source & ODbL` tabs):** Displays `id`, `synthetic: true` origin badge, fictional operator, coordinates, illustrative bearing status, separate `Field verified` vs. `OSM edit time`, `note="SYNTHETIC FIXTURE - NOT OBSERVED"`, and `CAM-06` source/ODbL 1.0 conditions (noting how `CAM-06` stays distinct from `CAM-01`/`CAM-02` viewable feeds).
- **Shared Timeline & Search Dock:** Plots `check_date`/`survey:date` (`▲`) separately from `osm_timestamp` (`▽`) across July–November 2026 and supports live keyword filtering (`/` or `Ctrl+K`).

## 5. Verification Summary

Executed on Windows 11 (`x86_64-pc-windows-msvc`) with `rustc 1.99.0` on `2026-11-02`:

| Check | Command | Result |
| --- | --- | --- |
| Formatting | `cargo +1.99.0 fmt --manifest-path crates/oia-alpr-map/Cargo.toml --all -- --check` | Pass |
| Clippy lint gate | `cargo +1.99.0 clippy --manifest-path crates/oia-alpr-map/Cargo.toml --workspace --all-targets --locked --offline -- -D warnings` | Pass (0 warnings) |
| Contract tests | `cargo +1.99.0 test --manifest-path crates/oia-alpr-map/Cargo.toml --workspace --locked --offline` | Pass (4 tests) |
| CLI example (`all` & `flock`) | `cargo +1.99.0 run --manifest-path crates/oia-alpr-map/Cargo.toml --example inspect-alpr-map --locked --offline -- all` | Pass |

## 6. Remaining Scope on Issue #28

This slice delivers requirement `CAM-06`, the community-mapped ALPR / Flock camera directory contract, standalone Rust normalization/viewport kernel, fictional New Meridian corridor fixture, and interactive map prototype. Remaining work under [#28](https://github.com/mESmaC/oia/issues/28) for subsequent slices includes:

- Public traffic / municipal DOT live camera feed adapters (`CAM-01`, `CAM-02`, `CAM-03`) and health/freshness monitoring.
- Retained clip / snapshot archive replay controls synchronized with the global timeline (`CAM-04`, `TIME-01`, `TIME-02`).
- Native desktop shell integration once the root workspace manifest is established.
