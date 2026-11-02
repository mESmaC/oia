# Mapped ALPR and Flock camera directory contract

**Status: proposed implementation contract and selective port specification; integrated application ingestion and live globe rendering remain open.**
This is the mapped camera-directory and map-view slice of [issue #28](https://github.com/mESmaC/oia/issues/28), with geometry and workspace coordination aligned to [#23](https://github.com/mESmaC/oia/issues/23) and [#16](https://github.com/mESmaC/oia/issues/16), based on specification v0.1 and repository revision `e72064d` (2026-11-01, America/New_York). Decision `D07` (cameras and retention) remains open.

Controlling requirements:
- [CAM-01 and CAM-02](../spec.md#51-internet-cameras-and-anomaly-detection): camera directories, attribution, access/reuse conditions, position/orientation, and distinction between catalog metadata and live/recorded observations.
- [GEO-01, GEO-02, and GEO-04](../spec.md#42-geographic-events-and-uap-investigations): bounded area queries, capture/verification times, coverage limits, and viewing cones without implying continuous coverage or calibrated optical footprints.
- [CASE-02 and UX-01..UX-05](../spec.md#3-core-experience-and-visual-design): shared investigation context, visible local-filter chips, non-color state cues, keyboard access, and clean-evidence / reduced-effect views.
- Applicable acceptance scenarios: catalog/access/fallback portions of [A09](../delivery.md#a09-camera-source-and-detector-integrity), geographic coverage bounds in [A04](../delivery.md#a04-geographic-reconstruction), and workspace usability in [A02](../delivery.md#a02-cinematic-but-usable-workspace).

Companion artifacts in this slice:
- [Standalone Rust kernel and CLI example](../../crates/oia-alpr-map/Cargo.toml) (`oia-alpr-map`), documented in [implementation notes](../implementation/alpr-camera-map.md).
- [Interactive Flock / ALPR camera map prototype](../design/mockups/flock-camera-map.html): self-contained offline HTML mockup in the OIA military-glass visual system.
- [Corridor fixture corpus](../../fixtures/cameras/alpr/austin-corridor-fixture.json): deterministic test and demonstration fixture.

## 1. Analytical boundary: what a Flock / ALPR map shows and does not show

Public interest in locating Automated License Plate Reader (ALPR) deployments—frequently referred to colloquially as "Flock camera maps" after Flock Safety, alongside other ALPR vendors such as Motorola Solutions / Vigilant, Genetec AutoVu, and Leonardo / ELSAG—relies primarily on community-mapped OpenStreetMap (`man_made=surveillance`, `surveillance:type=ALPR`) directories and regional extracts (such as DeFlock / `tiles.dontgetflocked.com`).

Under `AGENTS.md` §2 and §6, OIA must enforce strict epistemological boundaries on these records:

1. **Mapped directory presence, not live feeds or plate reads:** An ALPR catalog record represents a community-mapped physical camera installation and its reported OpenStreetMap tags. It does **not** provide access to camera video, still frames, license-plate read logs, retention databases, or live operational telemetry.
2. **No home-interior views or restricted endpoint probing (`CAM-01`):** ALPR directory mapping ingests only public map nodes and published municipal/community catalog extracts. OIA must never probe camera IP addresses, guess credentials, or attempt to connect to restricted ALPR vendor endpoints.
3. **Manufacturer attribution vs. generic ALPR classification:** Users frequently search for "Flock cameras" when investigating corridor coverage, while OpenStreetMap tags `surveillance:type=ALPR` across multiple manufacturers and often leaves `manufacturer` untagged. OIA must classify each mapped node's manufacturer state explicitly:
   - `FlockSafety`: `manufacturer`, `brand`, or `operator` explicitly identifies Flock Safety (e.g., `Flock Safety`, `Flock`).
   - `OtherNamed(String)`: another explicit manufacturer is recorded (e.g., `Motorola Solutions`, `Genetec`, `Leonardo`).
   - `Unspecified`: the node is tagged as an ALPR camera (`surveillance:type` includes `ALPR`), but no manufacturer tag is present.
   Filtering a map view to `Flock Safety only` must display a visible **local-filter chip** (`CASE-02`) and report how many additional ALPR nodes in the same viewport are `OtherNamed` or `Unspecified`, so an untagged ALPR node is never silently mistaken for absence of cameras or promoted into a confirmed Flock Safety device.
4. **Community mapping is not exhaustive coverage (`GEO-02`, `GEO-05`):** An empty viewport in a covered region means **zero mapped ALPR nodes in the queried extract**, not proof that no physical cameras exist on that road. Outside the extract's geographic bounds, the state is `NoCoverage`, not `Empty`.

## 2. Selective reuse from God's Eye View

Following [prior-art.md](../prior-art.md) and `AGENTS.md` §7, this contract selectively adapts the ALPR catalog normalization, viewport bounding, tile zoom selection, coordinate precision cache, and bearing-wedge geometry from God's Eye View at commit `0d1a68160e532ba65dc34b48ea205de6de4028fc`:

| Reference path in God's Eye View | Reused contract / lesson | OIA tightening / deliberate difference |
| --- | --- | --- |
| `src/layers/alpr/records.js` | `isAlprSurveillanceType` (case-insensitive semicolon multi-value `surveillance:type`, e.g. `camera;ALPR`), `normalizeAlprNode`, `normalizeDirection` (blank/whitespace and compass words return `null`, never `0°`), `snapAlprBox` (`0.05°` outward grid), `boxContains`, `buildOverpassQuery`, `validateAlprSnapshot`, and `alprRetryDelayMs` (`30 s` doubling to `240 s`). | Expressed as typed Rust domain primitives with explicit error variants (`AlprMapError`), explicit manufacturer classification (`FlockSafety` / `OtherNamed` / `Unspecified`), and separate `last_verified` vs. `osm_timestamp` fields. |
| `src/layers/alpr/policy.js` | City-scale viewport cap (`MAX_VIEWPORT_DEGREES = 3.0°`), query snap (`0.05°`), tile zoom range (`z9..=z12`, max `16` tiles), record limits (`QUERY_LIMIT = 1500`, `SOURCE_RECORD_LIMIT = 6000`, `PRECISION_CACHE_LIMIT = 20000`), and illustrative cone parameters (`90 m` radius, `±20°` half-angle, Earth mean radius `6,371,008.8 m`). | Kept as configurable, validated `AlprQueryPolicy` constants with zero-limit rejection and explicit saturation accounting (`total_in_viewport`, `retained_count`, `omitted_by_cap`). |
| `src/layers/alpr/source.js` & `tileRecords.js` | `alprDetailZoom` (finest zoom in `9..=12` covering the viewport within `16` XYZ tiles), `createAlprPrecisionCache` (coarser zoom tiles never overwrite finer coordinates already observed for the same camera ID), center-distance truncation when a dense metro exceeds the cap, and explicit `noCoverage` / `zoomIn` states. | Separation of `AlprCoverageStatus` (`Complete`, `Saturated`, `Empty`, `NoCoverage`, `ZoomRequired`) from source freshness (`Fresh`, `CachedReuse`, `Stale`) and local manufacturer filter counts. |
| `src/layers/alpr/model.js` & `visuals.js` | `destinationPointDeg` (spherical great-circle destination for bearing lines), `directionWedgePositions` (apex + left/right corners at `bearing ± 20°`), `alprDisplayId` (`ALPR-XXXX` short display label while retaining full `alpr:<osm_id>` selection identity), and HTTPS-only escaped attribution. | Bearing wedges carry an explicit `GeometryClass::IllustrativeBearingWedge` tag so neither the UI nor downstream analysis treats the 90 m / ±20° graphic as a calibrated camera footprint. |

### Attribution and licensing notice

- **Code attribution:** Selective algorithmic and contract port from God's Eye View (`src/layers/alpr/*`), Copyright (c) Bilawal Sidhu, licensed under the MIT License.
- **Data license boundary (`CAM-02`):** OpenStreetMap camera records are licensed under the **Open Database License (ODbL 1.0)** (`© OpenStreetMap contributors`, `https://www.openstreetmap.org/copyright`), which is distinct from OIA's source-code license. Any view, report, or investigation bundle that displays or exports OSM-derived ALPR records must preserve the OpenStreetMap ODbL attribution and record `automated_use`, `retention`, and `redistribution` conditions on the `Source` record.

## 3. Normalized record contract (`AlprCameraRecord`)

Every normalized ALPR directory node must preserve:

| Field | Type / domain | Contract |
| --- | --- | --- |
| `id` | `alpr:<osm_id>` (`u64 > 0`) | Canonical selection and deduplication identity within an OSM-backed catalog. Display labels (`ALPR-0421`) never replace `id`. |
| `osm_id` | `u64` (`> 0`) | Positive OpenStreetMap node identifier. Non-node elements (`way`, `relation`) or non-positive IDs are rejected by node normalization. |
| `latitude_deg`, `longitude_deg` | `f64` (`[-90, 90]`, `[-180, 180]`, finite) | WGS84 horizontal position in degrees. Height is not provided by OSM ALPR nodes and must remain `None` (unknown) in domain records; terrain clamping is a presentation-only transform. |
| `coordinate_zoom` | `Option<u8>` | Finest tile zoom (`9..=12`) or `None` for unquantized Overpass/raw coordinates. Used by the precision cache so coarse tile reads (`z9` ≈ 19 m quantization) never degrade a previously observed `z12` (≈ 2 m) or exact coordinate. |
| `manufacturer` | `Option<String>` | Trimmed `manufacturer` or `brand` tag when non-empty; `None` when absent or whitespace-only. |
| `manufacturer_class` | `AlprManufacturerClass` | `FlockSafety`, `OtherNamed`, or `Unspecified`, derived deterministically from `manufacturer` and `operator` tags without mutating the raw strings. |
| `operator` | `Option<String>` | Trimmed `operator` tag (e.g., law enforcement agency, municipality, HOA); `None` when absent. |
| `camera_type` | `Option<String>` | Trimmed `camera:type` tag (e.g., `fixed`, `panning`); `None` when absent. |
| `zone` | `Option<String>` | Trimmed `surveillance:zone` tag (e.g., `traffic`, ` street`, `parking`); `None` when absent. |
| `direction_deg` | `Option<f64>` | Compass bearing in `[0.0, 360.0)` degrees clockwise from true north. `360.0` normalizes to `0.0`. Blank/whitespace strings, out-of-range numbers (`< 0` or `> 360`), and cardinal words (`"N"`, `"NE"`) yield `None`. |
| `reference` | `Option<String>` | Trimmed `ref` tag; `None` when absent. |
| `last_verified` | `Option<String>` | Trimmed `check_date` or `survey:date` tag recording field verification. Must remain `None` when neither tag is present. |
| `osm_timestamp` | `Option<String>` | Upstream OSM element edit/extract timestamp. Never copied into `last_verified`. |
| `source_tag` | `Option<String>` | Trimmed `source` tag on the OSM node when supplied. |

## 4. Viewport bounding, tile zoom selection, and coverage semantics

1. **Bounded city-scale viewport:** Queries require a finite bounding box with `-90 <= south < north <= 90`, `-180 <= west < east <= 180`, and span `<= MAX_VIEWPORT_DEGREES + 2 * QUERY_SNAP_DEGREES` (`3.10°`). Wider viewports return `AlprCoverageStatus::ZoomRequired` before issuing any network request.
2. **Outward grid snapping and reuse:** Valid viewports snap outward to `QUERY_SNAP_DEGREES` (`0.05°`) so minor pans reuse the accepted snapshot without redundant upstream requests, provided the cached snapshot is fresh and its tile zoom is at most one level coarser than the new view's `alprDetailZoom`.
3. **Detail zoom selection (`z12` down to `z9`):** For tiled extracts, select the finest zoom in `12..=9` whose Web Mercator XYZ tile count for the viewport is `<= DETAIL_MAX_TILES` (`16`). If even `z9` requires more than `16` tiles, return `ZoomRequired`.
4. **Geographic extract coverage:** When querying a regional extract (e.g., US/Canada bounds `lat 17..=84`, `lon -180..=-50` with per-country TileJSON bounds), a viewport disjoint from the extract bounds returns `AlprCoverageStatus::NoCoverage` with zero network tile reads.
5. **Deterministic saturation capping:** If a viewport contains more than `max_records` (`1,500`) cameras, records are ordered deterministically by equirectangular squared distance to the viewport center `(lat - center_lat)^2 + ((lon - center_lon) * cos(center_lat))^2` (with ascending `osm_id` tie-breaking), truncated to `max_records`, and marked `AlprCoverageStatus::Saturated { total_found, retained }`. A saturated result never caches its outer tile bounds as complete coverage.

## 5. Workspace map presentation and interaction (`UX-01`–`UX-05`)

The companion interactive map draft ([flock-camera-map.html](../design/mockups/flock-camera-map.html)) applies the [workspace UI/UX design](../design/workspace-ui-ux.md) to the Cameras / Globe instrument:

- **Distinguishable manufacturer & bearing markers:** Every camera marker pairs color with shape/glyph and text labels so state is never color alone (`UX-03`):
  - `Flock Safety` (cyan `#45d1ff` solid diamond + `FLOCK` badge)
  - `Other named ALPR` (amber `#ffc06b` square + vendor badge)
  - `Unspecified ALPR` (muted slate `#a2b0bc` hollow ring + `UNSPECIFIED` badge)
- **Illustrative bearing wedges:** Drawn only for cameras with a numeric `direction_deg`, using a dashed boundary and explicit legend/inspector disclosure (`Illustrative 90 m / ±20° bearing wedge — not measured FOV or capture range`). Clean-evidence mode (`data-clean="true"`) hides illustrative wedges, grid rings, and selection brackets while keeping camera positions and provenance badges visible.
- **Visible local-filter chips (`CASE-02`):** Filtering by manufacturer (`All mapped ALPR`, `Flock Safety only`, `Other / unspecified`) or bearing availability displays an active local-filter chip in the canvas header alongside the unfiltered viewport totals.
- **Inspector provenance tabs:** Selecting any camera coordinates the map selection brackets, shared timeline marks (`survey:date` vs. `osmTimestamp`), and inspector (`Overview` and `Source & ODbL` tabs), while pushing onto the bounded route-back stack (`Alt+Left` or `← Back`).
