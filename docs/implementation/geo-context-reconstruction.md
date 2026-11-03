# Geographic context reconstruction kernel

This implements the dependency-independent context normalization and reconstruction
slice of [#24](https://github.com/mESmaC/oia/issues/24), supporting
[`GEO-02`, `GEO-05`, and `INTEL-03`](../spec.md#42-geographic-reconstruction-and-context),
and the deterministic track/weather/notice/coverage subset of [A04](../delivery.md#a04-geographic-reconstruction).
The implementation is in [`oia_analysis::geo_context`](../../crates/oia-analysis/src/geo_context.rs),
with a read-only local example in [`reconstruct-geo-context.rs`](../../crates/oia-analysis/examples/reconstruct-geo-context.rs)
and targeted contract tests in [`geo_context_contract.rs`](../../crates/oia-analysis/tests/geo_context_contract.rs).

## Implemented workflow and evidence contracts

Reconstructs atmospheric conditions, aircraft flight tracks, maritime vessel tracks, and navigation/airspace notices across a user-defined `SpatioTemporalEnvelope`.

### 1. Weather observation vs. forecast model separation (`GEO-02`, `GEO-05`, `AGENTS.md` §2)
- Distinguishes `WeatherReportKind::DirectObservation` (e.g. METAR, surface station, radiosonde sounding) from `WeatherReportKind::ForecastModel` (e.g. TAF, NWP model grid).
- Preserves distinct observation instant, forecast issuance instant, model version ID, and forecast valid window `[valid_from, valid_until)`.
- Sensed and modelled parameters: temperature, dew point, relative humidity, pressure / altimeter setting, wind vector (speed, direction, gusts), visibility with restriction code, cloud layers, and upper-air sounding levels at discrete altitude steps.
- **Contract:** A missing weather observation or reporting station never defaults to clear skies, calm winds, or standard atmosphere.

### 2. Observation vs. interpolation separation (`GEO-05`, `A04`, `AGENTS.md` §2)
- Direct sensor pings are tagged as `ObservationSource::DirectObservation`.
- Evaluated flight positions between recorded observations are tagged as `ObservationSource::InterpolatedSegment { elapsed_gap_ms, distance_gap_m, uncertainty_radius_m }`.
- Track interpolation calculates fraction $f \in [0.0, 1.0]$, unwraps longitude across the 180° antimeridian, interpolates altitudes and velocity, and interpolates heading using shortest-arc modular arithmetic.
- Positional uncertainty grows with elapsed time gap: $\text{base\_error} + \text{growth\_rate} \times \Delta t \times 4 f (1 - f)$, peaking at the midpoint.
- **Contract:** Interpolations across gaps exceeding `limits.max_interpolation_gap_ms` are rejected (`GeoContextError::InterpolationGapExceeded`), preventing the fabrication of flight paths across signal outages.
- Interpolated states are never presented as direct observations.

### 3. Absent track is NOT an empty scene (`GEO-05`, `A04`)
- Evaluating an envelope where zero aircraft or vessel tracks are detected does **not** assert an empty sky or clear sea.
- The kernel cross-references the envelope against registered `SensorCoverageMask` entries:
  - If no sensors cover the envelope: produces `SceneTrafficAssessment::InconclusiveUnobserved { reason: NoSensorsCoveringRegion }`.
  - If a sensor covering the area is offline, stale, or incomplete: produces `SceneTrafficAssessment::InconclusiveUnobserved { reason: PartialCoverageBlindSpots }`.
  - Only when all operational sensor coverage masks verified zero detections across the entire 3D spatio-temporal volume does it produce `SceneTrafficAssessment::ObservedAbsenceWithinCoverage`.

### 4. Dateline crossing support (`A04`)
- Bounding boxes crossing the 180° antimeridian have `min_lon_deg > max_lon_deg` (e.g. 175.0°E through ±180° to -175.0°W).
- Point containment and great-circle spherical distances correctly account for longitude wrap-around, avoiding inverted or out-of-bounds errors on Pacific / dateline-crossing events.

### 5. Airspace & navigation notice containment (`INTEL-03`)
- `NavigationNotice` models NOTAMs, hazardous rocket launch corridors, and maritime navigation warnings with 3D spatial volumes (circle or bounding box, lower/upper altitude limits) and effective intervals `[start, end)`.
- Checks intersection against target positions and timestamps to correlate anomalous events with known airspace restrictions or launch hazards.

### 6. Altitude reference preservation (`docs/architecture.md` §3.2)
- Altitudes carry an explicit `AltitudeReference`:
  - `MeanSeaLevel` (MSL)
  - `AboveGroundLevel` (AGL)
  - `Wgs84Ellipsoid` (GNSS geometric)
  - `PressureAltitudeStandardAtmosphere` (barometric flight levels)
  - `UnknownReference`
- Barometric altitude is never conflated with geometric height.

## Limits and cancellation (`AGENTS.md` §5)

- Hard bounds enforced via `ContextLimits`:
  - `max_points_per_track`: 10,000 points
  - `max_tracks`: 1,000 tracks
  - `max_notices`: 1,000 notices
  - `max_weather_records`: 1,000 records
  - `max_interpolation_gap_ms`: 300,000 ms (5 minutes)
- Non-finite coordinates, altitudes, or velocities yield typed `GeoContextError::NonFiniteValue`.
- Cooperative cancellation callback (`StopReason::Cancelled`, `StopReason::DeadlineExceeded`) checked before expensive loops and before publishing outputs.

## Run locally

```text
cargo run -p oia-analysis --example reconstruct-geo-context --locked --offline
cargo test -p oia-analysis --test geo_context_contract --locked --offline
```

## Remaining unclaimed scope on #24

- Live/external network ingestion adapters for NWS, OpenSky / ADS-B Exchange, and AIS providers (#13).
- SQLite relational and R-tree spatial indexing (#11, #14).
- 3D interactive globe and timeline visualization (#23).
- Full end-to-end multi-provider acceptance scenario A04 integration.
