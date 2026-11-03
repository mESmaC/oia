# Space Imagery Catalog, Scientific Provenance, and Mission Feeds Kernel

Status: implemented in `oia-analysis::space_imagery` (`crates/oia-analysis/src/space_imagery.rs`).
This delivers the standalone domain and analysis catalog slice of [#26](https://github.com/mESmaC/oia/issues/26),
supporting [SPACE-01, SPACE-02, and SPACE-03](../spec.md#43-space-imagery-and-solar-system), the space observation
and preview provenance subset of [A05](../delivery.md#a05-space-observations-and-planetary-context), mosaic coverage
accounting ([GEO-04](../spec.md#42-geographic-events-and-uap-investigations)), and geometry-aware baseline comparison
([INTEL-03](../spec.md#45-reasoning-and-additional-intelligence-methods)).

This package is a local, provider-independent Rust kernel with zero external network or database dependencies.
Live provider ingestion, SQLite storage, raster pixel decoding, and UI visualization remain open in [#13](https://github.com/mESmaC/oia/issues/13),
[#11](https://github.com/mESmaC/oia/issues/11), and [#16](https://github.com/mESmaC/oia/issues/16).

## Boundary and Evidence Semantics

The `oia_analysis::space_imagery` module enforces four analytical and evidence integrity contracts:

1. **Scientific vs. Preview Provenance (`SPACE-02`, `A05`):** Raw scientific observation metadata records
   (`ScientificProductRecord`, e.g. FITS primary arrays, FITS binary tables, calibrated radiance GeoTIFFs) are preserved
   alongside derived previews (`ImagePreviewRecord`). Calibration processing levels (`Level0RawTelemetry` through
   `Level4ModelAssimilated`), exposure durations (in seconds), stacking history (input frame count, integration method,
   registration residual uncertainty), and source versions are explicitly tracked and bounded.
2. **Outreach and Simulation Labeling Invariant (`SPACE-02`, `AGENTS.md` §2):** Color mappings (`ColorMapping`) strictly
   differentiate natural RGB observations (`TrueColor`), explicit band assignments (`FalseColor`), single-band greyscale
   (`SingleBandGreyscale`), outreach aesthetic presentations (`OutreachRendering`), and computer-generated models
   (`IllustrativeSimulation`). An outreach rendering or simulation is identified by `is_outreach_or_simulated()` and
   can never masquerade as a natural observation or physical measurement. Queries with `exclude_outreach_renderings = true`
   strictly exclude entries containing outreach or simulated previews.
3. **Public Mission Stream Classification (`SPACE-03`, `A05`):** Space station and spacecraft mission feeds
   (`SpaceMissionFeed`) are classified into distinct, immutable streams:
   - `LiveCamera`: Unaltered real-time camera imagery direct from orbit or spacecraft.
   - `DelayedTransmission`: Transmission delayed by buffer transit or speed-of-light propagation (recording delay seconds and reason).
   - `ReplayArchive`: Playback of prior recorded pass or historical mission activity.
   - `CommentaryBroadcast`: Studio commentary, narrated coverage, or graphics overlays.
   - `SimulationOrRecreation`: Computer-generated trajectory visualization or CAD animation.
   Bookmarks (`MissionFeedBookmark`) are created through owning feeds (`feed.create_bookmark(...)`), pinning `source_feed_id`,
   snapshotting `StreamClassification`, and computing `estimated_event_utc_ms` from display time, stream propagation delay,
   and declared clock offsets. Feeds enforce host matching and reject bookmarks created for other feeds.
4. **Exact Mosaic Coverage Accounting (`GEO-04`):** Terrestrial mosaic queries evaluate all contributing observation
   footprints (`GeoBoundingBoxWgs84`). Rather than checking isolated sample points or corners, `evaluate_mosaic_coverage`
   computes clipped envelope rectangle-union coverage via latitude slice decomposition across non-wrapped sub-intervals.
   Antimeridian dateline crossings on both query envelopes and observation footprints are decomposed into non-wrapped
   pieces (`[west, 180.0]` and `[-180.0, east]`), preventing clamp panics and omission of wrapped coverage.
   The resulting coverage assessment reports `uncovered_angular_area_fraction` as a planar angular-coordinate area fraction
   ($(\Delta\text{lat}^\circ \times \Delta\text{lon}^\circ) / (\Delta\text{lat}_\text{env}^\circ \times \Delta\text{lon}_\text{env}^\circ)$)
   with latitude slice cut tolerance $1\times 10^{-7\circ}$ and longitude merge tolerance $1\times 10^{-9\circ}$ (rather than
   geodetic ellipsoidal ground area). Seamless continuous coverage is never assumed or claimed when coverage is partial.
5. **Geometry-Aware Baseline Comparison Without Fabricated Detections (`INTEL-03`):** Pairwise baseline comparison
   (`compare_baselines`) evaluates viewing and illumination geometry differences (sun elevation delta, off-nadir angle delta,
   seasonal day gap, resolution ratio) and flags confounding limitations (`SignificantSunElevationDifference`,
   `OffNadirPerspectiveParallax`, `SeasonalFoliageOrSnowDiscrepancy`, `ResolutionDisparity`, `IncompatibleOrUnknownResolution`).
   Without raster pixel detectors or pinned empirical evidence, status is reported as `RasterDetectorNotPerformed` and
   the candidate collection remains strictly empty. No synthetic detections, placeholder candidates, or uncalibrated anomaly
   scores are fabricated.

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

Nine contract tests in `space_imagery_contract.rs` verify:
1. `scientific_provenance_and_outreach_rendering_integrity`: FITS metadata preservation, Level-1B calibration, stacking
   residuals, outreach preview labeling, and strict outreach exclusion in queries.
2. `mosaic_coverage_and_absence_without_seamless_illusion`: Evaluation of empty, partial, four-corner disconnected, and complete
   multi-footprint mosaics, verifying that disconnected corner tiles never report complete coverage.
3. `resolution_accounting_preserves_unavailable_data`: Preserves `None` spatial resolution without inventing nominal values.
4. `nested_metadata_revalidation_and_collection_limits_at_admission`: Revalidates directly constructed nested structs (wavelengths,
   stack limits, preview limits, feed bookmark bounds) and rejects invalid/oversized data at catalog admission boundaries.
5. `mission_stream_classification_and_bookmark_timing_integrity`: Live vs. simulation stream discrimination, bookmark feed
   pinning, and event timestamp calculation from stream delays.
6. `geometry_aware_baseline_comparison_never_fabricates_detections`: Baseline comparison returns `RasterDetectorNotPerformed`
   with zero candidates, preventing fabricated detections while evaluating geometry limitations.
7. `dateline_crossing_mosaic_union_and_wrapped_footprint`: Correctly handles antimeridian crossings in query envelopes
   and observation footprints without clamp panics or dropped coverage.
8. `bookmark_timestamp_checked_arithmetic_overflow`: Enforces checked integer arithmetic on bookmark event timestamps and
   propagation delay conversions, rejecting overflow with typed errors.
9. `baseline_comparison_revalidates_geometry_inputs`: Revalidates input capture geometries at the analysis boundary,
   rejecting non-finite or invalid parameters.

## Limitations and Remaining Scope

- **Raster Pixel Processing:** This kernel evaluates metadata and geometry compatibility. Actual pixel decoding, raster
  radiometric subtraction, and pinned detector evidence ingestion remain downstream tasks.
- **Live Provider Adapters:** Direct network connections to NASA CMR/GIBS, ESA Copernicus SciHub, and STScI MAST APIs
  remain open in [#13](https://github.com/mESmaC/oia/issues/13).
- **Persistent Storage:** SQLite table definitions and blob storage migration remain open in [#11](https://github.com/mESmaC/oia/issues/11).
- **UI and Interactive Visualization:** Interactive 3D Cesium globe and sky canvas rendering remain open in [#16](https://github.com/mESmaC/oia/issues/16)
  and [#23](https://github.com/mESmaC/oia/issues/23).
