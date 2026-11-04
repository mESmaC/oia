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

Bounding boxes and UTC windows use the [shared time/geometry primitives](shared-time-geo.md).
Seam edge and pole contact count as closed-extent overlap, but not positive interior
area in composite planning. Tolerances are explicit angular comparison allowances.

### Boundary and Evidence Semantics

The `oia_analysis::space_imagery` module enforces six analytical and evidence integrity contracts:

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
   with latitude slice cut tolerance $1\times 10^{-7^\circ}$ and longitude merge tolerance $1\times 10^{-9^\circ}$ (rather than
   geodetic ellipsoidal ground area). Seamless continuous coverage is never assumed or claimed when coverage is partial.
5. **Geometry-Aware Baseline Comparison Without Fabricated Detections (`INTEL-03`):** Pairwise baseline comparison
   (`compare_baselines`) evaluates viewing and illumination geometry differences (sun elevation delta, off-nadir angle delta,
   seasonal day gap, resolution ratio) and flags confounding limitations (`SignificantSunElevationDifference`,
   `OffNadirPerspectiveParallax`, `SeasonalFoliageOrSnowDiscrepancy`, `ResolutionDisparity`, `IncompatibleOrUnknownResolution`).
   Without raster pixel detectors or pinned empirical evidence, status is reported as `RasterDetectorNotPerformed` and
   the candidate collection remains strictly empty. No synthetic detections, placeholder candidates, or uncalibrated anomaly
   scores are fabricated.
6. **Multi-Source Globe Composite Planning & Unredacted Source Selection (`GEO-02`, `GEO-04`, `SPACE-01`, `SPACE-02`, `MEDIA-02`, `MEDIA-03`, `FIELD-03`, `A04`, `A05`):**
   `plan_globe_composite` produces a metadata composite plan across heterogeneous public tiers (`CompositeSourcePlatform`):
   - **Orbital Earth Observation (`SatelliteEarthObservation`):** Global open baselines (ESA Copernicus Sentinel-2 10 m,
     USGS/NASA Landsat-8/9 15 m pansharpened / 30 m multispectral, Planet NICFI 4.77 m tropical mosaics, Maxar/Capella/Umbra
     Open Data 0.30–0.50 m event releases).
   - **Space-Station Crew & Handheld Frames (`SpacecraftHandheldOrCrewFrame`):** NASA Gateway to Astronaut
     Photography of Earth (ISS crew handheld camera frames down to ~1.5–5 m native GSD when orthorectified against a DEM).
   - **National & Regional Aerial Orthophotos (`AerialOrthophoto`):** Public photogrammetric surveys (USGS NAIP / 3DEP
     0.07–0.60 m, Swisstopo SWISSIMAGE 0.10 m, IGN France BD ORTHO 0.15–0.20 m, BKG Germany DOP, PDOK Netherlands, LINZ NZ).
   - **Calibrated UAV / Drone Orthomosaics (`CalibratedUavOrDroneOrtho`):** OpenAerialMap community photogrammetry (0.02–0.10 m).
   - **3D-Posed Ground / Street Frames (`CalibratedGroundOrStreetFrame`):** Mapillary, Panoramax, and KartaView street-level
     or field frames with calibrated 6-DoF pose intersected with an orthometric DEM.

   **Explicit Scope Exclusions:**
   - **No Pixel Restoration or Synthesis:** No deblurring, super-resolution, or inpainting of redacted or low-resolution
     patches is ever performed.
   - **No Restricted or Credentialed Access:** No restricted, credentialed, or higher-tier access requests are modeled or issued;
     planning operates strictly over publicly available open-archive metadata.

   When selecting per-cell source assignments at the finest verifiable optical resolution, `plan_globe_composite` enforces:
   - **Localized Unredacted & Cloud-Free Source Selection (`LocalizedPatchDefect`, `PatchIntegrityState`):** Every applicable
     defect overlapping a grid cell is preserved simultaneously (`applicable_cell_states`) and all coarsening floors are applied
     independently. When a high-resolution aerial orthophoto contains a statutory blur/pixelation polygon
     (`DeliberatelyBlurredOrRedacted`), a cloud shadow (`ObscuredByCloudShadowOrSmoke`), or a sensor wedge gap
     (`SensorSwathGapOrArtifact`) over specific grid cells, only the affected cells are demoted and filled from an alternate
     unedited satellite, ISS crew, or UAV pass (`bypassed_degraded_patch_ids`, counted separately in
     `cells_filled_from_alternate_optical_source` and `cells_filled_from_radar_over_degraded_source`), while surrounding crisp
     aerial cells remain intact.
   - **Upsampling Defense (`effective_verifiable_gsd_m`):** Effective optical resolution is computed across all applicable cell
     states as $\max(\text{native\_gsd\_m}, \text{served\_grid\_spacing\_m}, \text{coarse\_effective\_gsd\_m})$ so a 10 m
     satellite scene bicubic-upsampled onto a 0.05 m tile pyramid never outranks genuine 0.15 m or 0.50 m native pixels, and
     an upsampled patch without a measured cloud fraction (`measured_cloud_or_haze_fraction: None`) is never assumed cloud-free.
   - **Strict Epistemic, Spectral & Geometric Filtering:** Spectral regime vs. radiometric class mismatches
     (`InvalidRadiometricModality`), outreach renderings (`OutreachOrSimulatedPresentation`), generative AI super-resolution
     or inpainting (`PredictiveOrInpaintedContent`), uncalibrated manual placements (`UncalibratedSpatialCorrespondence`),
     platform-incompatible correspondence methods (`IncompatiblePlatformCorrespondence`), oblique/crew/street frames lacking
     orthometric DEM terrain rectification (`MissingTerrainOrthorectificationForObliqueView`), and partial cell footprints
     (`InsufficientCellFootprintCoverage`) are rejected with bounded, provenance-complete `RejectedCandidateReceipt` records.
   - **Modality Isolation (`AssignedModality`):** Active microwave SAR (`SyntheticApertureRadar`, e.g. Sentinel-1 / Capella
     Open Data) is disabled unless `allow_sar_cloud_penetration_fallback` is enabled, only fills cells lacking clean optical
     coverage, and is strictly labeled `AssignedModality::RadarBackscatterFallback` (`CompositeCellOutcome::AssignedRadarFallback`).
   - **Multi-Temporal Seam & Single-Instant Accounting (`CompositeSeamSegment`):** Every cell boundary between distinct
     patches (including the full-globe $\pm 180^\circ$ wraparound seam) records a `CompositeSeamSegment` with checked `i64`
     capture timestamp delta (`capture_time_delta_ms`), effective GSD ratio, and modality/platform crossing flags.
     `is_single_instant_observation` is `false` whenever multiple passes are stitched, any cell remains uncovered, or the
     assigned patch is a multi-frame stack, `Level3GriddedComposite`/`Level4ModelAssimilated` product, or exceeds
     `max_single_instant_duration_ms`.

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

Seventeen contract tests in `space_imagery_contract.rs` verify:
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
10. `disconnected_intersection_components_preserved`: Verifies that intersecting footprints with antimeridian crossings preserve
    all disconnected geographic bounding box components in `footprint_intersections`.
11. `delay_float_to_i64_exclusive_bound_overflow`: Enforces strict exclusive $2^{63}$ upper boundary check on floating-point
    delay conversion before `i64` cast, preventing saturation overflow.
12. `multi_source_composite_stitching_deblurs_repairs_clouds_and_defends_against_upsampling`: Verifies localized statutory
    blur polygon and cloud alternate-source selection across aerial orthophoto, commercial open-data satellite, and UAV
    orthomosaics while clamping bicubic-upsampled grids to their true native GSD.
13. `composite_rejects_outreach_predictive_uncalibrated_and_isolates_sar_fallback`: Verifies rejection of outreach, AI-inpainted,
    uncalibrated ground, unrectified oblique, and partial-footprint candidates, plus explicit SAR backscatter modality isolation.
14. `composite_antimeridian_grid_and_boundary_revalidation`: Verifies antimeridian-straddling composite grid cell generation,
    wrapped footprint containment, boundary revalidation of bypassed invalid defects, and checked seam timestamp delta overflow.
15. `overlapping_blur_and_cloud_preserve_both_reasons_and_coarsening_floor`: Verifies that overlapping blur and cloud defects
    preserve all defect reasons and coarsening floors simultaneously, and that `UpsampledBeyondNative` with unknown cloud cover
    never enters `CleanOptical`.
16. `spectral_modality_validation_and_oblique_crew_street_orthorectification`: Verifies rejection of SAR/optical spectral mismatches,
    preservation of non-outreach preview color mapping, and enforcement of platform-compatible correspondence and orthometric DEMs
    for crew and street frames.
17. `single_instant_mosaic_history_total_order_permutations_and_receipt_budget`: Verifies `is_single_instant_observation = false`
    for multi-day/multi-frame mosaics, deterministic winner selection across all 6 permutations of sub-nanometre GSD ties, and
    bounded rejected-candidate receipts.

## Limitations and Remaining Scope

- **Raster Pixel Processing:** This kernel evaluates metadata, spatial registration, defect bounds, and per-cell stitch
  assignments. Actual raster pixel decoding, Poisson/Laplacian seam blending, and pinned detector evidence ingestion remain
  downstream tasks.
- **Live Provider Adapters:** Direct network connections to NASA CMR/GIBS, ESA Copernicus CDSE, USGS EarthExplorer,
  OpenAerialMap, and STScI MAST APIs remain open in [#13](https://github.com/mESmaC/oia/issues/13).
- **Persistent Storage:** SQLite table definitions and blob storage migration remain open in [#11](https://github.com/mESmaC/oia/issues/11).
- **UI and Interactive Visualization:** Interactive 3D globe composite seam overlays and sky canvas rendering remain open in
  [#16](https://github.com/mESmaC/oia/issues/16) and [#23](https://github.com/mESmaC/oia/issues/23).
