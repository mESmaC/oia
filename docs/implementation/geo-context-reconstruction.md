# Geographic context reconstruction kernel

This implements the dependency-independent context normalization and reconstruction
slice of [#24](https://github.com/mESmaC/oia/issues/24), supporting
[`GEO-02`, `GEO-05`, and `INTEL-03`](../spec.md#42-geographic-reconstruction-and-context),
and the deterministic track/weather/notice/coverage subset of [A04](../delivery.md#a04-geographic-reconstruction).
The implementation is in [`oia_analysis::geo_context`](../../crates/oia-analysis/src/geo_context.rs) (with submodules `provisional`, `weather`, `tracks`, `notices`, and `coverage`),
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
  - Forecast issuance must not occur after the valid window end (`issuance <= valid_window.end`), allowing in-progress amended forecasts, while retrieval must not precede issuance.
- Validates atmospheric parameters and verifies that cloud layer base/top altitudes share compatible vertical reference datums.
- **Contract:** A missing weather observation or reporting station never defaults to clear skies, calm winds, or standard atmosphere.

#### 2. Observation vs. interpolation separation (`GEO-05`, `A04`, `AGENTS.md` §2, B3′, B6′, N2, S1, NB3)
- Direct sensor pings are tagged as `ObservationSource::DirectObservation` and record observed `transmit_instant_utc_ms`, `receiver_instant_utc_ms`, and `retrieval_instant_utc_ms` (validated with $tx \le rx \le ret$, $tx \le ret$). They never set `target_interpolation_instant_utc_ms`.
- **Direct Observation Timestamp Requirement (N2):** Direct observations must provide a physical event time (`transmit_instant_utc_ms` or `receiver_instant_utc_ms`). Retrieval time records when data was fetched by a client, not when the physical event occurred.
- `effective_instant_utc_ms` returns `Option<i64>` (interpolation target time, receiver time, or transmit time; never retrieval time, with no fallback to 0). Points with unresolvable event times fail closed toward inconclusive traffic assessments.
- **Aircraft Altitude Datum Validation (S1):** `AircraftTrackPoint::validate` enforces strict vertical datum boundaries:
  - `barometric_altitude` must use `AltitudeReference::BarometricUnadjusted` (standard unadjusted pressure altitude). Barometric readings labeled MSL or AGL are rejected.
  - `geometric_altitude` must use `AltitudeReference::Wgs84Ellipsoid` (GNSS height) or `AltitudeReference::OrthometricMsl`. Geometric readings labeled Barometric or Unknown are rejected.
- **Kinematic Speed Bounds at Construction (NB3):** Successive points within `AircraftTrack` and `MaritimeVesselTrack` are validated at constructor time (`new`) and at `validate()` against kinematic speed limits (`max_aircraft_speed_mps`, `max_vessel_speed_mps`), rejecting non-physical position jumps (`GeoContextError::ImpliedSpeedExceedsLimit`).
- Evaluated flight positions between recorded observations are tagged as `ObservationSource::InterpolatedSegment`:
  - `method_id`: `"bounded-great-circle-spherical-v1"`
  - `elapsed_gap_ms`: elapsed interval between bounding observations
  - `distance_gap_m`: shortest spherical great-circle arc length on the unit sphere
  - `uncertainty_radius_m`: positional uncertainty bounding radius: $\text{base\_uncertainty} + \text{growth\_rate} \times \Delta t \times 4 f (1 - f)$
  - `base_uncertainty_m`: configurable sensor uncertainty (default 15.0 m nominal radius / 1-sigma, half of the RTCA DO-260B / FAA AC 20-165B Table 2-2 $\text{NAC}_p \ge 9$ 95% horizontal containment bound of $< 30\,\text{m}$; note 14 CFR §91.227(c)(1)(i) operational minimum for ADS-B Out in US rule airspace is $\text{NAC}_p \ge 8$ / $< 92.6\,\text{m}$)
  - `growth_rate_mps`: configurable kinetic drift rate (default 2.0 m/s operational heuristic representing transport aircraft straight-line inertial wind-drift uncertainty bound during brief dropouts; vessels share this default unless overridden)
  - `max_aircraft_speed_mps`: speed jump limit (default 1000.0 m/s ~ Mach 3; rejects impossible kinematic jumps)
  - `max_vessel_speed_mps`: vessel speed jump limit (default 50.0 m/s ~ 97 knots)
- **Spherical SLERP (B3′):** Kinematic position interpolation uses exact spherical linear interpolation (SLERP) on unit-sphere 3D Cartesian vectors:
  $$\mathbf{v}(f) = \frac{\sin((1-f)\theta)}{\sin\theta} \mathbf{v}_1 + \frac{\sin(f\theta)}{\sin\theta} \mathbf{v}_2$$
  This follows the true great-circle arc across all latitudes and across the 180° antimeridian, eliminating planar cylindrical distortion.
- **Time Separation (B6′):** Interpolated points set `target_interpolation_instant_utc_ms = Some(target_utc_ms)` and leave `transmit`, `receiver`, and `retrieval` as `None` (preventing fabricated sensor reception timestamps).
- **Traffic Assessment Separation (B2):** Interpolated states are derived models. Only direct observations (`ObservationSource::DirectObservation`) inside the envelope volume increment `observed_aircraft_count` or `observed_vessel_count`.

### 3. Absent track is NOT an empty scene (`GEO-05`, `A04`, B1′, N1, H1, NB2)
- Evaluating an envelope where zero aircraft or vessel tracks are detected does **not** assert an empty sky or clear sea.
- The kernel cross-references the envelope against registered `SensorCoverageMask` entries via `evaluate_sensor_coverage`:
  - **Spatial Overlap Filtering:** Masks that do not overlap the envelope spatial bounding box are skipped and do not cause spurious blind spot or feed-down reasons for remote sensors.
  - **4D Containment:** Operational window must contain envelope time window (`contains_window`), coverage bounding box must contain envelope spatial bounds (`contains_box`), and sensor altitude band must encompass envelope altitude band (`contains_band`).
  - **Unbounded vs. Bounded Altitudes:** If the query envelope has no altitude band (`altitude_band: None`), the query encompasses an unbounded vertical column. A bounded sensor mask (e.g. 0–20 km) cannot cover an unbounded column, correctly flagging `IncompleteAltitudeCoverage`.
  - **Feed Reporting Through Window End:** Sensor feed last update (`last_feed_update_utc_ms`) must be $\ge \text{envelope.time\_window.end\_utc\_ms()}$. If a feed stopped reporting before the window ended, data is missing (`SensorFeedDownOrStale`).
  - **Log Completeness for Archived Windows (H1):** For an archived window that has ended prior to `evaluation_instant` ($\text{window.end} \le \text{evaluation\_instant}$), log completeness through the window's end is sufficient; how old the archive is at evaluation time is irrelevant (evaluating an archived event 1 day or 1 year later yields consistent results).
  - **Age Check for Live/Ongoing Evaluations (H1):** The age check against `limits.max_sensor_staleness_ms` (default 5 minutes) applies strictly when the query window reaches into or beyond the evaluation instant.
  - **Refusal of Future Feeds (H1):** A sensor mask reporting `last_feed_update_utc_ms > evaluation_instant_utc_ms` represents invalid future telemetry and is rejected (`InvalidTemporalOrdering`).
  - **Unresolved Altitude Observations (B1′):** If a direct observation falls inside the 2D bounding box and time window but lacks altitude or carries an incompatible vertical datum, the kernel cannot verify absence in that altitude band and emits `InconclusiveUnobserved { reason: IncompleteAltitudeCoverage }`.
  - **Maritime Surface Vessels & Datums (N1, NB2):** Surface vessels operate at sea level ($0.0\,\text{m}$ MSL / physical surface). When the query envelope altitude band uses a surface-compatible datum (`OrthometricMsl` or `AboveGroundLevel`), sea-level vessels match the envelope and increment `observed_vessel_count`. When the query envelope uses an incompatible vertical datum (such as `Wgs84Ellipsoid` or `BarometricUnadjusted`), the sea surface height cannot be assumed to be 0.0 m without local geoid or pressure profiles; the vessel's vertical overlap is treated as unresolvable, preventing an unfounded absence claim and yielding `InconclusiveUnobserved { reason: IncompleteAltitudeCoverage }`.
  - **Sea-Surface Assumption Disclosure (NB2):** In `ContextReconstructionResult`, matching vessel tracks set `vessel_altitude_basis: Some(VesselAltitudeBasis::AssumedSeaSurface)`. Note that astronomical tides and dynamic sea-surface height / ocean topography variations are ignored in this nominal 0.0 m surface assignment.
  - **Datum Tolerance:** Comparing an incompatible sensor mask datum does not abort the query with an error; it treats that mask as non-covering.
  - **Single-Mask Coverage Note:** Currently, coverage is evaluated on a per-mask basis; each covering sensor must encompass the envelope independently. Multi-mask geometric unions are deferred.

### 4. Dateline crossing support (`A04`, B5′, NB5)
- Bounding boxes crossing the 180° antimeridian have `west_lon_deg > east_lon_deg` (e.g. 175.0°E through ±180° to -175.0°W).
- Point containment (`contains_point`), box containment (`contains_box`), box overlap (`overlaps_box`), and spherical distances correctly account for longitude wrap-around.
- **Notice Boundary Distance & Planar Approximation (B5′, NB5):** Circle boundary intersection with bounding boxes calculates shortest modular angular distance to the box's longitude interval across the antimeridian. Clamping latitude and selecting the closest boundary longitude provides a fast, coordinate-aligned planar equirectangular approximation of the nearest point. At higher latitudes, this planar clamp overestimates geodesic distance (e.g., by ~1.6 km at 60° latitude, +25 km at (60°, -15°)), which can yield false negatives (`NoOverlap`) for circular notices near bounding box corners. Exact spherical point-to-polygon geodesic computation is deferred.

### 5. Airspace & navigation notice containment (`INTEL-03`, N3, N3′)
- `NavigationNotice` models NOTAMs, hazardous rocket launch corridors, and maritime navigation warnings with 3D spatial volumes (circle or bounding box, lower/upper altitude limits) and effective intervals `[start, end)`.\n- Validates notice structure: non-negative finite radius, valid coordinates, valid time intervals, and retrieval timestamp $\ge$ issuance timestamp.
- **Mixed Vertical Datums (N3):** When notice limits mix vertical datums (e.g., SFC in AGL to FL180 in barometric unadjusted pressure altitude) or differ from the query envelope's vertical datum, vertical overlap is classified as `NoticeEnvelopeOverlap::VerticalOverlapUnknown`. Such notices are preserved in `result.notices_with_vertical_overlap_unknown` and counted in `result.notice_summary.vertical_overlap_unknown_count` rather than dropped silently.
- **Unknown Datum Fails Closed (N3′):** In `intersects_position`, altitude comparisons rely on `compare_in_same_reference`. An `Unknown` altitude datum compared against any position (even another `Unknown`) fails closed, yielding `NoticePositionIntersection::VerticalOverlapUnknown` rather than evaluating as intersecting or non-intersecting.

### 6. Vertical datum integrity (`docs/architecture.md` §3.2, B4)
- Explicit vertical datums via `AltitudeReference`:
  - `Wgs84Ellipsoid` (GNSS ellipsoidal height)
  - `OrthometricMsl` (geoid-referenced mean sea level)
  - `AboveGroundLevel` (AGL)
  - `BarometricUnadjusted` (standard barometric flight levels)
  - `Unknown`
- **Contract:** Cross-datum comparisons fail closed with `GeoContextError::IncompatibleAltitudeReferences`.

### 7. Validation, Numeric Integrity & Encapsulation (B8′, B9, N2, S2, NB1)
- `AircraftTrack` and `MaritimeVesselTrack` fields are encapsulated with public accessors and strict constructor validation.
- All points within a track must be strictly monotonic in timestamp; duplicate timestamps within a track are rejected (`DuplicateTimestamp`). Monotonic errors emit `InvalidTemporalOrdering`.
- Transponder squawk codes are validated as 4 ASCII octal digits $[0-7]^4$.
- Maritime IMO numbers are validated using the official 7-digit checksum ($7 d_1 + 6 d_2 + 5 d_3 + 4 d_4 + 3 d_5 + 2 d_6 \pmod{10} = d_7$).
- Midpoint time calculation uses `i128` arithmetic to prevent integer overflow on extreme time intervals.
- `ContextLimits` enforces finite non-negative parameters (`base_observation_uncertainty_m >= 0`, `interpolation_growth_rate_mps >= 0`, `max_interpolation_gap_ms > 0`, `max_sensor_staleness_ms > 0`, `max_aircraft_speed_mps > 0`, `max_vessel_speed_mps > 0`). Invalid zero/negative limits emit typed `InvalidLimitValue`. Out-of-range headings and speeds emit typed `InvalidLimitValue`.
- Resource limits `limits.max_tracks` enforces separate upper bounds on `aircraft_tracks` and `vessel_tracks` independently.
- `StopReason` is the shared definition re-exported from `oia_analysis::multilingual::StopReason`, preserving `Cancelled`, `DeadlineExceeded`, and `Superseded` across crate boundaries.
- `EARTH_RADIUS_METERS` is defined as `6_371_008.8` metres based on the IUGG R1 spherical mean radius (Moritz 2000).
- `UnobservedReason` implements `fmt::Display` across all variants.
- **Synthetic Fixtures (B9):** All test fixtures and examples use strictly synthetic, unallocated identifiers:
  - ICAO addresses in the unallocated reserved block `000001..=003FFF` (per ICAO Annex 10)
  - Maritime MMSIs using ITU prefix `111` (designated under ITU-R Recommendation M.585 Annex 2 Table 2 for Search and Rescue aircraft, ensuring no collision with merchant ship stations)
  - Simulated vessel fixtures omit commercial IMO numbers; rejection tests verify check-digit enforcement using deliberately failing IMO `1000000` ($1 \times 7 \pmod{10} = 7 \ne 0$)
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
