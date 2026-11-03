# Geographic context reconstruction kernel

This implements the dependency-independent context normalization and reconstruction
slice of [#24](https://github.com/mESmaC/oia/issues/24), supporting
[`GEO-02`, `GEO-05`, and `INTEL-03`](../spec.md#42-geographic-reconstruction-and-context),
and the deterministic track/weather/notice/coverage subset of [A04](../delivery.md#a04-geographic-reconstruction).
The implementation is in [`oia_analysis::geo_context`](../../crates/oia-analysis/src/geo_context.rs),
with a read-only local example in [`reconstruct-geo-context.rs`](../../crates/oia-analysis/examples/reconstruct-geo-context.rs)
and targeted contract tests in [`geo_context_contract.rs`](../../crates/oia-analysis/tests/geo_context_contract.rs).

> [!NOTE]
> **Provisional Geometry & Datum Primitives (`provisional` module, pending #12):**
> Geometry and time primitives (`GeoPointWgs84`, `GeoBoundingBoxWgs84`, `Altitude`, `AltitudeReference`,
> `AltitudeBand`, `TimeWindow`) are **provisional domain models** grouped in `oia_analysis::geo_context::provisional`
> pending the delivery of `#12` (`crates/oia-time-geo`). Upon completion of `#12`, these types will migrate
> directly to the shared `oia-time-geo` contracts without changing the analytical semantics or evidence
> preservation rules of this kernel. PR #102 (`oia-query`) has narrowed its scope to clustering and deferred
> its geometry primitives to `#12` / `#24`.

## Implemented workflow and evidence contracts

Reconstructs atmospheric conditions, aircraft flight tracks, maritime vessel tracks, and navigation/airspace notices across a user-defined `SpatioTemporalEnvelope`.

### 1. Weather observation vs. forecast model separation (`GEO-02`, `GEO-05`, `AGENTS.md` §2)
- Distinguishes `WeatherReportKind::DirectObservation` (e.g. METAR, surface station, radiosonde sounding) from `WeatherReportKind::ForecastModel` (e.g. TAF, NWP model grid).
- Preserves distinct multi-instant timestamps: observation instant, forecast issuance instant, retrieval instant, model version ID, and forecast valid window `[valid_from, valid_until)`.
- Validates temporal ordering:
  - Observation instant must fall strictly within the declared valid window (`start <= obs <= end`) and retrieval must not precede observation (`retrieval >= obs`).
  - Forecast issuance must not occur after valid window start (`issuance <= valid_window.start`) and retrieval must not precede issuance.
- Validates atmospheric parameters and verifies that cloud layer base/top altitudes share compatible vertical reference datums.
- **Contract:** A missing weather observation or reporting station never defaults to clear skies, calm winds, or standard atmosphere.

### 2. Observation vs. interpolation separation (`GEO-05`, `A04`, `AGENTS.md` §2, B3′, B6′)
- Direct sensor pings are tagged as `ObservationSource::DirectObservation` and record observed `transmit_instant_utc_ms`, `receiver_instant_utc_ms`, and `retrieval_instant_utc_ms` (validated with $tx \le rx \le ret$). They never set `target_interpolation_instant_utc_ms`.
- Evaluated flight positions between recorded observations are tagged as `ObservationSource::InterpolatedSegment`:
  - `method_id`: `"bounded-great-circle-spherical-v1"`
  - `elapsed_gap_ms`: elapsed interval between bounding observations
  - `distance_gap_m`: shortest spherical great-circle arc length on the unit sphere
  - `uncertainty_radius_m`: positional uncertainty bounding radius: $\text{base\_uncertainty} + \text{growth\_rate} \times \Delta t \times 4 f (1 - f)$
  - `base_uncertainty_m`: configurable sensor uncertainty (default 15.0 m, based on RTCA DO-260B / FAA TSO-C166b standard GPS/ADS-B $\text{NAC}_p \ge 9$ containment bounds)
  - `growth_rate_mps`: configurable kinetic drift rate (default 2.0 m/s, conservative transport aircraft cross-track wind and drift dead-reckoning bound)
- **Spherical SLERP (B3′):** Kinematic position interpolation uses exact spherical linear interpolation (SLERP) on unit-sphere 3D Cartesian vectors:
  $$\mathbf{v}(f) = \frac{\sin((1-f)\theta)}{\sin\theta} \mathbf{v}_1 + \frac{\sin(f\theta)}{\sin\theta} \mathbf{v}_2$$
  This follows the true great-circle arc across all latitudes and across the 180° antimeridian, eliminating planar cylindrical distortion.
- **Time Separation (B6′):** Interpolated points set `target_interpolation_instant_utc_ms = Some(target_utc_ms)` and leave `transmit`, `receiver`, and `retrieval` as `None` (preventing fabricated sensor reception timestamps).
- **Traffic Assessment Separation (B2):** Interpolated states are derived models. Only direct observations (`ObservationSource::DirectObservation`) inside the envelope volume increment `observed_aircraft_count` or `observed_vessel_count`.

### 3. Absent track is NOT an empty scene (`GEO-05`, `A04`, B1′)
- Evaluating an envelope where zero aircraft or vessel tracks are detected does **not** assert an empty sky or clear sea.
- The kernel cross-references the envelope against registered `SensorCoverageMask` entries via `evaluate_sensor_coverage`:
  - **4D Containment:** Operational window must contain envelope time window (`contains_window`), coverage bounding box must contain envelope spatial bounds (`contains_box`), and sensor altitude band must encompass envelope altitude band (`contains_band`).
  - **Unbounded vs. Bounded Altitudes:** If the query envelope has no altitude band (`altitude_band: None`), the query encompasses an unbounded vertical column. A bounded sensor mask (e.g. 0–20 km) cannot cover an unbounded column, correctly flagging `IncompleteAltitudeCoverage`.
  - **Feed Reporting Through Window End:** Sensor feed last update (`last_feed_update_utc_ms`) must be $\ge \text{envelope.time\_window.end\_utc\_ms()}$. If a feed stopped reporting before the window ended, data is missing (`SensorFeedDownOrStale`).
  - **Feed Staleness:** For real-time evaluations, feed age relative to `evaluation_instant` must not exceed `limits.max_sensor_staleness_ms` (default 300,000 ms = 5 minutes).
  - **Unresolved Altitude Observations (B1′):** If a direct observation falls inside the 2D bounding box and time window but lacks altitude or carries an incompatible vertical datum, the kernel cannot verify absence in that altitude band and emits `InconclusiveUnobserved { reason: IncompleteAltitudeCoverage }`.
  - **Maritime Surface Vessels (B1′):** Surface vessels operate at sea level ($0.0\,\text{m}$ MSL). When the query envelope altitude band encompasses sea level (or is unbounded), direct vessel observations match the envelope and increment `observed_vessel_count`.
  - **Datum Tolerance (B1′):** Comparing an incompatible sensor mask datum or notice datum does not abort the query with an error; it treats that item as not covering or not intersecting.
  - **Single-Mask Coverage Note:** Currently, coverage is evaluated on a per-mask basis; each covering sensor must encompass the envelope independently. Multi-mask geometric unions are deferred.

### 4. Dateline crossing support (`A04`, B5′)
- Bounding boxes crossing the 180° antimeridian have `west_lon_deg > east_lon_deg` (e.g. 175.0°E through ±180° to -175.0°W).
- Point containment (`contains_point`), box containment (`contains_box`), box overlap (`overlaps_box`), and spherical distances correctly account for longitude wrap-around.
- **Notice Boundary Distance (B5′):** Circle boundary intersection with bounding boxes calculates shortest modular angular distance to the box's longitude interval across the antimeridian, ensuring notices near the 180° meridian (e.g. center at -179.9° near envelope 170°..179.5°) correctly intersect.

### 5. Airspace & navigation notice containment (`INTEL-03`)
- `NavigationNotice` models NOTAMs, hazardous rocket launch corridors, and maritime navigation warnings with 3D spatial volumes (circle or bounding box, lower/upper altitude limits) and effective intervals `[start, end)`.
- Geodesic circle-to-box shortest spherical distance and altitude band intersection check envelope overlap in `overlaps_envelope`.

### 6. Vertical datum integrity (`docs/architecture.md` §3.2, B4)
- Explicit vertical datums via `AltitudeReference`:
  - `Wgs84Ellipsoid` (GNSS ellipsoidal height)
  - `OrthometricMsl` (geoid-referenced mean sea level)
  - `AboveGroundLevel` (AGL)
  - `BarometricUnadjusted` (standard barometric flight levels)
  - `Unknown`
- **Contract:** Cross-datum comparisons fail closed with `GeoContextError::IncompatibleAltitudeReferences`.

### 7. Validation, Numeric Integrity & Encapsulation (B8′, B9)
- `AircraftTrack` and `MaritimeVesselTrack` fields are encapsulated with public accessors and strict constructor validation.
- All points within a track must be strictly monotonic in timestamp; duplicate timestamps within a track are rejected (`DuplicateTimestamp`).
- Transponder squawk codes are validated as 4 ASCII octal digits $[0-7]^4$.
- Maritime IMO numbers are validated using the official 7-digit checksum ($7 d_1 + 6 d_2 + 5 d_3 + 4 d_4 + 3 d_5 + 2 d_6 \pmod{10} = d_7$).
- Midpoint time calculation uses `i128` arithmetic to prevent integer overflow on extreme time intervals.
- `ContextLimits` enforces non-negative finite parameters (`base_observation_uncertainty_m >= 0`, `interpolation_growth_rate_mps >= 0`, `max_interpolation_gap_ms > 0`, `max_sensor_staleness_ms > 0`).
- **Synthetic Fixtures (B9):** All test fixtures and examples use strictly synthetic, unallocated identifiers:
  - ICAO addresses in the unallocated reserved block `000001..=003FFF` (per ICAO Annex 10)
  - Maritime MMSIs using unallocated ITU Maritime Identification Digit (MID) `111` (range 000–199 has no assigned nation)
  - Synthetic test IMO `9000003` with valid check digit
  - Synthetic callsigns (`SIM101`, `SIMV01`) and abstract coordinates (`(10.0, 20.0)`, `(10.5, 20.5)`)

## Run locally

```text
cargo run -p oia-analysis --example reconstruct-geo-context --locked
cargo test -p oia-analysis --test geo_context_contract --locked
```

## Remaining unclaimed scope on #24

- Live/external network ingestion adapters for NWS, OpenSky / ADS-B Exchange, and AIS providers (#13).
- SQLite relational and R-tree spatial indexing (#11, #14).
- 3D interactive globe and timeline visualization (#23).
- Multi-mask coverage unions (merging multiple adjacent radar sectors).
- Full end-to-end multi-provider acceptance scenario A04 integration.
