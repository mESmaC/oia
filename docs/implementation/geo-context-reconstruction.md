# Geographic context reconstruction kernel

This implements the dependency-independent context normalization and reconstruction
slice of [#24](https://github.com/mESmaC/oia/issues/24), supporting
[`GEO-02`, `GEO-05`, and `INTEL-03`](../spec.md#42-geographic-reconstruction-and-context),
and the deterministic track/weather/notice/coverage subset of [A04](../delivery.md#a04-geographic-reconstruction).
The implementation is in [`oia_analysis::geo_context`](../../crates/oia-analysis/src/geo_context.rs),
with a read-only local example in [`reconstruct-geo-context.rs`](../../crates/oia-analysis/examples/reconstruct-geo-context.rs)
and targeted contract tests in [`geo_context_contract.rs`](../../crates/oia-analysis/tests/geo_context_contract.rs).

> [!NOTE]
> **Provisional Geometry & Datum Primitives (Pending #12 `oia-time-geo`):**
> Geometry primitives (`GeoPointWgs84`, `GeoBoundingBoxWgs84`, `Altitude`, `AltitudeReference`, `TimeWindow`)
> defined in `oia-analysis::geo_context` are provisional and scoped strictly to analysis reconstruction
> pending completion of the dedicated core crate in issue [#12](https://github.com/mESmaC/oia/issues/12).
> Once `#12` merges, these primitives will migrate to `oia-time-geo`.

## Implemented workflow and evidence contracts

Reconstructs atmospheric conditions, aircraft flight tracks, maritime vessel tracks, and navigation/airspace notices across a user-defined `SpatioTemporalEnvelope`.

### 1. Weather observation vs. forecast model separation (`GEO-02`, `GEO-05`, `AGENTS.md` §2)
- Distinguishes `WeatherReportKind::DirectObservation` (e.g. METAR, surface station, radiosonde sounding) from `WeatherReportKind::ForecastModel` (e.g. TAF, NWP model grid).
- Preserves distinct multi-instant timestamps: observation instant, forecast issuance instant, retrieval instant, model version ID, and forecast valid window `[valid_from, valid_until)`.
- Validates that direct observation timestamps and forecast issuance timestamps fall within the declared valid window (`InvalidWeatherTimes`).
- Sensed and modelled parameters: temperature, dew point, relative humidity, pressure / altimeter setting, wind vector (speed, direction, gusts), visibility with restriction code, cloud layers, and upper-air sounding levels at discrete altitude steps.
- **Contract:** A missing weather observation or reporting station never defaults to clear skies, calm winds, or standard atmosphere.

### 2. Observation vs. interpolation separation (`GEO-05`, `A04`, `AGENTS.md` §2)
- Direct sensor pings are tagged as `ObservationSource::DirectObservation`.
- Evaluated flight positions between recorded observations are tagged as `ObservationSource::InterpolatedSegment`:
  - `method_id`: `"bounded-great-circle-spherical-v1"`
  - `elapsed_gap_ms`: elapsed interval between endpoints
  - `distance_gap_m`: shortest spherical great-circle arc length
  - `uncertainty_radius_m`: positional uncertainty bounding radius
  - `base_uncertainty_m`: configurable sensor uncertainty (default 15.0 m)
  - `growth_rate_mps`: configurable kinetic growth rate (default 2.0 m/s)
- Positional uncertainty grows with elapsed time gap: $\text{base\_error} + \text{growth\_rate} \times \Delta t \times 4 f (1 - f)$, peaking at the midpoint.
- **Contract (B2):** Interpolated flight states are derived kinematic models, never direct observations. Only points with `ObservationSource::DirectObservation` falling within the spatio-temporal envelope increment `observed_aircraft_count` or `observed_vessel_count`.
- **Contract (B3):** Interpolations across gaps exceeding `limits.max_interpolation_gap_ms` are rejected (`GeoContextError::InterpolationGapExceeded`). Singularities in polar domains ($|\text{latitude}| > 80.0^\circ$) are rejected (`LatitudeOutOfDomainForLinearInterpolation`). If an attribute (altitude, speed, squawk) is missing at either endpoint, the interpolated state leaves it as `None` rather than holding or guessing values.

### 3. Absent track is NOT an empty scene (`GEO-05`, `A04`, B1)
- Evaluating an envelope where zero aircraft or vessel tracks are detected does **not** assert an empty sky or clear sea.
- The kernel cross-references the envelope against registered `SensorCoverageMask` entries via `evaluate_sensor_coverage`:
  - **4D Containment:** Operational window must contain envelope time window (`contains_window`), coverage bounding box must contain envelope spatial bounds (`contains_box`), and sensor altitude band must contain envelope altitude band (`contains_band`).
  - **Feed Staleness:** Sensor feed update age (`evaluation_instant.abs_diff(last_feed_update_utc_ms)`) must not exceed `limits.max_sensor_staleness_ms` (default 300,000 ms).
  - If no sensors cover the envelope: produces `SceneTrafficAssessment::InconclusiveUnobserved { reason: NoSensorsCoveringRegion }`.
  - If a sensor covering the area is offline or stale: produces `SceneTrafficAssessment::InconclusiveUnobserved { reason: SensorFeedDownOrStale }`.
  - If coverage is partial horizontally or vertically: produces `InconclusiveUnobserved` with `PartialCoverageBlindSpots` or `IncompleteAltitudeCoverage`.
  - If query interval is outside sensor logging: produces `InconclusiveUnobserved { reason: QueryIntervalOutsideSensorLogWindow }`.
  - Only when verified active covering sensors encompass the entire 4D query envelope with zero detections does it produce `SceneTrafficAssessment::ObservedAbsenceWithinCoverage`.

### 4. Dateline crossing support (`A04`)
- Bounding boxes crossing the 180° antimeridian have `west_lon_deg > east_lon_deg` (e.g. 175.0°E through ±180° to -175.0°W).
- Point containment (`contains_point`), box-to-box containment (`contains_box`), box-to-box overlap (`overlaps_box`), and great-circle spherical distances correctly account for longitude wrap-around.

### 5. Airspace & navigation notice containment (`INTEL-03`, B5)
- `NavigationNotice` models NOTAMs, hazardous rocket launch corridors, and maritime navigation warnings with 3D spatial volumes (circle or bounding box, lower/upper altitude limits) and effective intervals `[start, end)`.
- Overlap evaluation in `overlaps_envelope` calculates geodesic circle-to-box shortest spherical distance and altitude band intersection.
- Position intersection in `intersects_position` validates spatial containment and vertical limits. Comparisons across mismatched vertical datums fail closed.

### 6. Vertical datum integrity (`docs/architecture.md` §3.2, B4)
- Explicit vertical datums via `AltitudeReference`:
  - `Wgs84Ellipsoid` (GNSS ellipsoidal height)
  - `OrthometricMsl` (geoid-referenced mean sea level)
  - `AboveGroundLevel` (AGL)
  - `BarometricUnadjusted` (standard barometric flight levels)
  - `Unknown`
- **Contract:** Cross-datum comparisons without conversion models fail closed with `GeoContextError::IncompatibleAltitudeReferences`.

### 7. Validation & Numeric Integrity (B8, B9)
- Transponder squawk codes are validated as exactly 4 octal digits ($[0-7]^4$, value $0 \le s \le 07777_8$).
- Maritime IMO numbers are validated using the official 7-digit weighted check digit checksum ($7 d_1 + 6 d_2 + 5 d_3 + 4 d_4 + 3 d_5 + 2 d_6 \pmod{10} = d_7$).
- Conflicting duplicate position reports at identical timestamps are rejected.
- All timestamp differences use overflow-safe `abs_diff`.
- All tests and examples use synthetic, fictional identifiers (`A00001`, `SIM101`, `station:synth-pacific-01`).

## Limits and cancellation (`AGENTS.md` §5, B7)
- Hard bounds enforced via `ContextLimits`:
  - `max_points_per_track`: 10,000 points
  - `max_tracks`: 1,000 tracks
  - `max_notices`: 1,000 notices
  - `max_weather_records`: 1,000 records
  - `max_interpolation_gap_ms`: 300,000 ms (5 minutes)
  - `max_sensor_staleness_ms`: 300,000 ms (5 minutes)
- Reuses crate-level `crate::multilingual::StopReason` (`Cancelled`, `DeadlineExceeded`, `Superseded`).
- Non-finite coordinates, altitudes, or velocities yield typed `GeoContextError::NonFiniteValue`.

## Run locally

```text
cargo run -p oia-analysis --example reconstruct-geo-context --locked
cargo test -p oia-analysis --test geo_context_contract --locked
```

## Remaining unclaimed scope on #24

- Live/external network ingestion adapters for NWS, OpenSky / ADS-B Exchange, and AIS providers (#13).
- SQLite relational and R-tree spatial indexing (#11, #14).
- 3D interactive globe and timeline visualization (#23).
- Full end-to-end multi-provider acceptance scenario A04 integration.
