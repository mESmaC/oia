# Space Imagery Catalog, Scientific Provenance, and Mission Feeds Kernel

Status: implemented in `oia-analysis::space_imagery` (`crates/oia-analysis/src/space_imagery.rs`).
This delivers the standalone domain and analysis catalog slice of [#26](https://github.com/mESmaC/oia/issues/26),
supporting [SPACE-01, SPACE-02, and SPACE-03](../spec.md#43-space-imagery-and-solar-system), the space observation
and preview provenance subset of [A05](../delivery.md#a05-space-observations-and-planetary-context), mosaic coverage
accounting ([GEO-04](../spec.md#42-geographic-context-and-terrain)), and geometry-aware baseline change detection
([INTEL-03](../spec.md#45-reasoning-and-additional-intelligence-methods)).

This package is a local, provider-independent Rust kernel with zero external network or database dependencies.
Live provider ingestion, SQLite storage, and UI visualization remain open in [#13](https://github.com/mESmaC/oia/issues/13),
[#11](https://github.com/mESmaC/oia/issues/11), and [#16](https://github.com/mESmaC/oia/issues/16). Decision D06
remains open with its owner.

## Boundary and Evidence Semantics

The `oia_analysis::space_imagery` module enforces four analytical and evidence integrity contracts:

1. **Scientific vs. Preview Provenance (`SPACE-02`, `A05`):** Raw scientific products (`ScientificProductRecord`,
   e.g. FITS image arrays, FITS binary tables, calibrated radiance GeoTIFFs) are preserved alongside derived previews
   (`ImagePreviewRecord`). Calibration processing levels (`Level0RawTelemetry` through `Level4ModelAssimilated`), exposure
   durations (in seconds), stacking history (input frame count, integration method, registration residual uncertainty),
   and source versions are explicitly tracked.
2. **Outreach and Simulation Labeling Invariant (`SPACE-02`, `AGENTS.md` §2):** Color mappings (`ColorMapping`) strictly
   differentiate natural RGB observations (`TrueColor`), explicit band assignments (`FalseColor`), single-band greyscale
   (`SingleBandGreyscale`), outreach aesthetic presentations (`OutreachRendering`), and computer-generated models
   (`IllustrativeSimulation`). An outreach rendering or simulation is identified by `is_outreach_or_simulated()` and
   can never masquerade as a natural observation or physical measurement. Queries with `exclude_outreach_renderings = true`
   strictly exclude entries containing only outreach or simulated previews.
3. **Public Mission Stream Classification (`SPACE-03`, `A05`):** Space station and spacecraft mission feeds
   (`SpaceMissionFeed`) are classified into distinct, immutable streams:
   - `LiveCamera`: Unaltered real-time camera imagery direct from orbit or spacecraft.
   - `DelayedTransmission`: Transmission delayed by buffer transit or speed-of-light propagation (recording delay seconds).
   - `ReplayArchive`: Playback of prior recorded pass or historical mission activity.
   - `CommentaryBroadcast`: Studio commentary, narrated coverage, or graphics overlays.
   - `SimulationOrRecreation`: Computer-generated trajectory visualization or CAD animation.
   Bookmarks (`MissionFeedBookmark`) retain the stream classification token, capture timestamp, timing uncertainty,
   and source limitations (e.g. loss-of-signal handover, nightside eclipse pass).
4. **Mosaic Coverage Accounting and Change Detection (`GEO-04`, `INTEL-03`):** Terrestrial mosaic queries evaluate
   all contributing observation footprints (`GeoBoundingBoxWgs84`). If coverage across the envelope boundaries is incomplete,
   the report returns `MosaicCoverageAssessment::PartialCoverageWithGaps` with the exact count of uncovered boundary regions.
   Seamless continuous reconstruction is never assumed or claimed. Pairwise baseline change detection (`compare_baselines`)
   evaluates illumination and viewing geometry differences (sun elevation delta, off-nadir angle delta, seasonal day gap,
   resolution ratio) and flags false change triggers (`LikelyIlluminationArtifact`, `SeasonalVegetationShift`).
   Change candidates strictly enforce `is_calibrated_probability = false` in the absence of a pinned held-out evaluation.

## Multi-Criteria Catalog Filtering (`SPACE-01`, `GEO-02`)

The catalog supports query filtering across:
- **Mission / Observatory:** Case-insensitive string matching (e.g. Landsat-9, Sentinel-2, JWST, SDO, ISS).
- **Instrument:** Case-insensitive string matching (e.g. OLI-2, MSI, NIRCam, AIA, EHDC).
- **Spectral Regime:** Broad electromagnetic regime (`Radio`, `Microwave`, `FarInfrared`, `MidInfrared`, `NearInfrared`,
  `OpticalVisible`, `Ultraviolet`, `ExtremeUltraviolet`, `XRay`, `GammaRay`, `Multispectral`, `Hyperspectral`).
- **Target Keyword:** Substring search across terrestrial region labels, celestial object names, solar features, or planetary bodies.
- **Terrestrial Bounding Envelope:** Intersects box checking using `GeoBoundingBoxWgs84::overlaps_box`.
- **Temporal Interval:** Bounded UTC millisecond overlap using `TimeWindow::overlaps`.
- **Maximum Ground Sample Distance:** Filter on spatial resolution in metres for Earth observation.
- **Exclusion of Outreach Renderings:** Filter on `has_outreach_or_simulated_preview()`.

Candidate providers (`SpaceDataProvider`, e.g. NASA CMR, NASA GIBS, STScI MAST, SDO Science Center) explicitly record
`is_candidate_catalog_entry = true` to preserve the contractual distinction between catalog entries and verified direct integrations.

## Local Verification

Executed locally on Windows x86_64 with Rust 1.99.0:

```text
cargo +1.99.0 fmt --all -- --check
cargo +1.99.0 clippy -p oia-analysis --all-targets --locked -- -D warnings
cargo +1.99.0 test -p oia-analysis --test space_imagery_contract --locked
cargo +1.99.0 run -p oia-analysis --example inspect-space-observations --locked
```

All commands exited with code 0. Three targeted contract tests in `space_imagery_contract.rs` verify:
1. `scientific_provenance_and_outreach_rendering_integrity`: FITS metadata preservation, Level-1B calibration, stacking
   residuals, outreach preview labeling, and strict outreach exclusion in queries.
2. `mosaic_coverage_and_absence_without_seamless_illusion`: Evaluation of empty, partial, and complete multi-footprint
   mosaics without assuming seamless coverage.
3. `mission_stream_classification_and_geometry_change_detection`: Live vs. simulation stream discrimination, bookmark
   limitations, sun elevation difference detection, and uncalibrated change candidate score invariants.

## Limitations and Remaining Scope

- **Live Provider Adapters:** Direct network connections to NASA CMR/GIBS, ESA Copernicus SciHub, and STScI MAST APIs
  remain open in [#13](https://github.com/mESmaC/oia/issues/13).
- **Persistent Storage:** SQLite table definitions and blob storage migration remain open in [#11](https://github.com/mESmaC/oia/issues/11).
- **UI and Interactive Visualization:** Interactive 3D Cesium globe and sky canvas rendering remain open in [#16](https://github.com/mESmaC/oia/issues/16)
  and [#23](https://github.com/mESmaC/oia/issues/23).
- **End-to-End Acceptance:** Full multi-provider live acceptance under scenario A05 across real decoded FITS imagery
  remains open for future milestone delivery.
