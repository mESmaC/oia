# Implementation: ALPR and Flock Camera Map Kernel (`oia-alpr-map`)

This document records the delivered implementation for the **community-mapped ALPR / Flock camera catalog & map draft slice** of GitHub Issue [#28](https://github.com/mESmaC/oia/issues/28) (`[P3] Integrate eligible camera directories, feeds, and retained clips`), implementing the contract in [ALPR Camera Map Contract](../contracts/alpr-camera-map.md).

## 1. Delivered Scope

| Artifact | Purpose | Status |
| --- | --- | --- |
| [`docs/contracts/alpr-camera-map.md`](../contracts/alpr-camera-map.md) | Canonical OIA contract for community-mapped ALPR and Flock camera directories, provenance boundaries, viewport and coverage rules, and selective reuse from God's Eye View (`0d1a68160e532ba65dc34b48ea205de6de4028fc`) | Implemented |
| [`crates/oia-alpr-map/`](../../crates/oia-alpr-map/Cargo.toml) | Standalone zero-dependency Rust 2024 package implementing tag normalization, vendor classification, directional cone geometry, viewport bounds, coverage status, and case-pin exports | Implemented and verified |
| [`crates/oia-alpr-map/examples/inspect-alpr-map.rs`](../../crates/oia-alpr-map/examples/inspect-alpr-map.rs) | Offline CLI inspection tool that evaluates the Austin corridor fixture and prints coverage status, vendor distribution, marker details, and case-pin payloads | Implemented and verified |
| [`fixtures/cameras/alpr/`](../../fixtures/cameras/alpr/README.md) | Deterministic synthetic Austin corridor fixture (`austin-corridor-fixture.json`) exercising Flock Safety Falcon, Motorola Vigilant, Genetec AutoVu, Generic ALPR, tile-mirror fallback, and rejected non-ALPR inputs | Implemented and verified |
| [`docs/design/mockups/flock-camera-map.html`](../design/mockups/flock-camera-map.html) | Self-contained offline interactive Flock / ALPR camera map prototype in OIA's military-glass visual system (`UX-01` through `UX-05`) | Implemented |

## 2. Rust Kernel Architecture (`crates/oia-alpr-map`)

The standalone package (`crates/oia-alpr-map`) is isolated with its own `[workspace]` table so concurrent sessions can build and test without contending on a root `Cargo.toml`. It enforces the repository's strict Rust 2024 lint baseline (`unsafe_code = "forbid"`, `unwrap_used = "deny"`, `expect_used = "deny"`, `indexing_slicing = "deny"`).

### Core Types and Functions (`src/lib.rs`)

- [`AlprSourceCatalog`](../../crates/oia-alpr-map/src/lib.rs): Distinguishes `OsmOverpassLive` (canonical global OSM Overpass nodes) from `DeflockTileMirror` (hourly US/Canada vector-tile mirror fallback).
- [`AlprVendorFamily`](../../crates/oia-alpr-map/src/lib.rs) and [`classify_vendor_family`](../../crates/oia-alpr-map/src/lib.rs): Normalizes `brand`, `manufacturer`, `operator`, and `model` tags into `FlockSafety`, `MotorolaVigilant`, `GenetecAutoVu`, `LeonardoElsag`, or `GenericAlpr`, with explicit `is_flock()` helper and visual tokens (`#FF6B35` high-visibility amber-orange for Flock Safety, `#F59E0B` warm gold for other ALPR vendors).
- [`parse_direction_cardinal`](../../crates/oia-alpr-map/src/lib.rs) and [`compute_sight_cone`](../../crates/oia-alpr-map/src/lib.rs): Parses numeric bearings (`0..360`), range bearings (`45-90`), multi-valued bearings (`180;0`), and 16-wind cardinal tokens (`N`, `NNE`, `NE`, ..., `NNW`) into a 55-degree, 70-meter horizontal sight-cone triangle in local tangent-plane meters.
- [`normalize_osm_node`](../../crates/oia-alpr-map/src/lib.rs) and [`normalize_tile_feature`](../../crates/oia-alpr-map/src/lib.rs): Validates coordinates, enforces `man_made=surveillance` + `surveillance:type=ALPR` for OSM nodes, strips sensitive or unverified claims, and assigns explicit `VerificationState::CommunityMapped` and `AccessClass::PublicCommunityCatalog` metadata.
- [`evaluate_viewport`](../../crates/oia-alpr-map/src/lib.rs): Enforces minimum camera zoom (`11.0`) and maximum viewport span (`1.4 deg`), deduplicates markers by `record_id`, filters by `AlprFilterMode` (`AllMappedAlpr` or `FlockSafetyOnly`), and computes `AlprCoverageState` (`Ready`, `ZoomInRequired`, `ViewportTooWide`, `FallbackTileMirror`, `OutsideMirrorBounds`, or `NoMappedCamerasInView`).
- [`AlprCameraRecord::to_case_pin`](../../crates/oia-alpr-map/src/lib.rs): Exports an immutable `AlprCasePin` with `osm_reference`, `license` (`ODbL-1.0`), `verification_state`, and `evidence_note` for `CASE-02` saved views and dossiers.

## 3. CLI Inspection Usage

Run the standalone CLI example against the Austin corridor fixture:

```powershell
cargo +1.99.0 run --manifest-path crates/oia-alpr-map/Cargo.toml --example inspect-alpr-map --locked --offline -- all
cargo +1.99.0 run --manifest-path crates/oia-alpr-map/Cargo.toml --example inspect-alpr-map --locked --offline -- flock
```

Sample output (`flock` filter mode):

```text
OIA ALPR / Flock Camera Map Inspection
======================================
Corridor: Austin Metro I-35 & Congress Ave (Synthetic Fixture)
Filter Mode: flock-only
Coverage State: ready
Total Visible: 4 (Flock Safety: 4, Other ALPR: 0, Sight Cones: 4)

ID               VENDOR                    OPERATOR                       COORDS                  BEARING  SOURCE
------------------------------------------------------------------------------------------------------------------------
osm-node-910001  Flock Safety              Austin Police Department       30.26720, -97.74310     180 deg  OpenStreetMap Overpass (Live ALPR Query)
osm-node-910002  Flock Safety              Sunset Valley Police Dept      30.25140, -97.73880      15 deg  OpenStreetMap Overpass (Live ALPR Query)
osm-node-910003  Flock Safety              Barton Creek HOA               30.26980, -97.76890      90 deg  OpenStreetMap Overpass (Live ALPR Query)
osm-node-910006  Flock Safety              Austin District 9 Neighborhood 30.25890, -97.74950     135 deg  OpenStreetMap Overpass (Live ALPR Query)
```

## 4. Interactive Map Prototype (`docs/design/mockups/flock-camera-map.html`)

The interactive HTML prototype ([`flock-camera-map.html`](../design/mockups/flock-camera-map.html)) implements the five-region OIA military-glass layout (`UX-01` through `UX-05`) and runs 100% offline:

- **Top Command Bar:** Corridor jump presets (`Austin I-35 Corridor`, `Downtown Congress`, `Wide Regional Span (Guardrail)`, and `Reykjavik (Outside Tile Bounds)`), live search by street/operator/model/OSM ID, and a coverage-status pill.
- **Left Stack Dock:** Filter pills (`All Mapped ALPR` vs. `Flock Safety Only`), source catalog switcher (`OSM Overpass Live` vs. `DeFlock Tile Mirror Fallback`), sight-cone toggle, zoom slider (`9` to `15`), vendor telemetry counters, and an interactive camera directory list.
- **Center 2D Map Stage:** Pan/zoom vector road and waterway canvas with pulsing Flock Safety (`#FF6B35`) and other ALPR (`#F59E0B`) camera markers, 55-degree directional sight-cone overlays, hover tooltips, and zoom/span guardrail banners.
- **Right Entity Inspector & Case Pin Drawer:** Displays full OSM tag provenance (`brand`, `operator`, `model`, `direction`, `surveillance:zone`), explicit provenance boundary callout (`Verification: Community-Mapped (ODbL 1.0)` / `No plate reads or private feeds`), and a one-click **Pin Camera to Case** action that builds a live JSON dossier preview.
- **Bottom Timeline & Freshness Bar:** Shows catalog snapshot timestamp, ODbL 1.0 attribution, and source latency status.

## 5. Verification Summary

Executed on Windows x86_64 with Rust `1.99.0`:

| Check | Command | Result |
| --- | --- | --- |
| Formatting | `cargo +1.99.0 fmt --manifest-path crates/oia-alpr-map/Cargo.toml --all -- --check` | Pass |
| Clippy lint gate | `cargo +1.99.0 clippy --manifest-path crates/oia-alpr-map/Cargo.toml --workspace --all-targets --locked --offline -- -D warnings` | Pass (0 warnings) |
| Unit + contract tests | `cargo +1.99.0 test --manifest-path crates/oia-alpr-map/Cargo.toml --workspace --locked --offline` | Pass (4 integration tests + unit tests) |
| CLI example (`all` & `flock`) | `cargo +1.99.0 run --manifest-path crates/oia-alpr-map/Cargo.toml --example inspect-alpr-map --locked --offline -- all` | Pass |

## 6. Remaining Scope on Issue #28

This slice delivers the community-mapped ALPR / Flock camera directory contract, standalone Rust normalization/viewport kernel, corridor fixture, and interactive map prototype. Remaining work under [#28](https://github.com/mESmaC/oia/issues/28) for subsequent slices includes:

- Public traffic / municipal DOT live camera feed adapters (`CAM-01`, `CAM-03`) and health/freshness monitoring.
- Retained clip / snapshot archive replay controls synchronized with the global timeline (`CAM-04`, `TIME-01`, `TIME-02`).
- Native desktop shell integration once the root workspace manifest is established.
