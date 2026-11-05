# Shared geometry, temporal representations, leap-second and clock-alignment primitives

Implemented slices of [#12](https://github.com/mESmaC/oia/issues/12), supporting
GEO-01/GEO-05, UAV-01/UAV-02/CAM-02 clock alignment, SPACE-01 scientific time scales,
and the boundary subset of [A04](../delivery.md#a04-geographic-reconstruction) and
[A05](../delivery.md#a05-uav-telemetry-and-footage-alignment).
The implementation lives in [`oia-time-geo`](../../crates/oia-time-geo/src/lib.rs).
It consolidates the merged geographic-context and calendar/timestamp primitives; it adds
no provider, database, renderer, external crate dependency or production Lean runtime.

## Contracts and consumers

- `GeoPointWgs84` retains finite latitude/longitude degrees in `[-90, 90]` and
  `[-180, 180]`. Coordinates are private after validation; no normalization rewrites
  supplied values. A point carries no implied altitude or accuracy.
- `GeoBoundingBoxWgs84` is a closed latitude extent and eastward longitude arc.
  West greater than east crosses the dateline. Equal endpoints mean a meridian;
  `[-180, 180]` is the full circle and `[180, -180]` is the seam only. Queries treat
  `+180` and `-180` as the same meridian and all longitudes at a pole as one point.
  Full-world containment includes wrapped regions. Edge/pole contact is closed
  overlap but does not establish positive interior area for composite planning.
- Angular tolerances expand the comparison extent explicitly; they do not change
  stored coordinates or measurement uncertainty. Negative/non-finite tolerances
  and interior-overlap thresholds return false. Zero tolerance compares stored
  endpoints directly (splitting wrapped arcs at the seam); it does not reconstruct
  endpoints from rounded offsets and spans. Shared edges compare equal, while the
  next representable longitude outside stays outside. Positive tolerance expansion
  and angular extent calculations still use rounded floating-point arithmetic.
- `Altitude`, `AltitudeBand` and `GeoPosition` preserve metres, reference category,
  negative heights and optional missing height. Cross-category and unknown-reference
  comparisons fail with a typed error. Shared comparisons also reject unbound
  `RelativeToTakeoff` values: equal category names cannot establish a shared origin.
  Other compatible geoid/terrain/pressure models remain a caller precondition;
  these categories are not versioned datum identities. There is no height conversion.
- `TimeWindow` is a nonempty half-open interval of signed Unix-epoch UTC milliseconds.
  Its comparisons do not subtract endpoints, including at `i64` limits. It neither
  converts a calendar date to midnight nor establishes clock quality.
- `CalendarDate`, `is_leap_year`, `days_in_month`, `days_from_civil` and
  `civil_from_days` provide proleptic Gregorian calendar validation and Hinnant civil-day
  conversion (`1970-01-01 == 0`), plus strict `YYYY-MM-DD` parsing (`1..=9999`).
  `has_rfc3339_utc_second_shape`, `parse_rfc3339_utc_seconds` and
  `parse_rfc3339_utc_nanos` consolidate strict RFC 3339 UTC timestamp validation across
  [`oia-documents::lineage`](../../crates/oia-documents/src/lineage.rs),
  [`oia-news-claims`](../../crates/oia-news-claims/src/regional.rs),
  [`oia-query::geo_workspace`](../../crates/oia-query/src/geo_workspace.rs) and
  [`oia-app::globe::imagery`](../../crates/oia-app/src/globe/imagery.rs).
  `parse_rfc3339_utc_seconds` rejects `:60` leap seconds because POSIX epoch seconds
  cannot represent an unambiguous leap-second instant without folding; `parse_rfc3339_utc_nanos`
  accepts `:60` only when the UTC time-of-day is `23:59:60`.
- `LocalTimeContext`, `DstAmbiguity` and `TimeRepresentation` (`ExactInstant`,
  `BoundedInterval`, `CalendarDate`, `ApproximateTime`, `UnknownTime`) retain the
  reported representation, optional UTC offset (`-840..=840` minutes), optional IANA
  timezone identifier and explicit DST fold ambiguity (`Unambiguous`, `EarlierOffset`,
  `LaterOffset`, `UnresolvedFold`). Date-only (`CalendarDate`) and `UnknownTime`
  representations never silently promote to midnight UTC (`overlaps_utc_window` returns
  `None`), and `UnknownTime` preserves a structured `oia_domain::evidence::UnknownReason`.
- `TimeScale` (`Utc`, `Tai`, `Tt`, `Tdb`) and `LeapSecondTable` (`iers_standard`,
  version `iers-bulletin-c-2026-01`, all 28 historical IERS Bulletin C steps from
  `1972-01-01T00:00:00Z` to `2017-01-01T00:00:00Z`, expiring `2026-12-28T00:00:00Z`)
  implement versioned `UTC <-> TAI <-> TT` millisecond conversions (`TT - TAI = 32,184 ms`).
  Timestamps before `1972-01-01T00:00:00Z` fail with `PreLeapSecondEpoch` and timestamps
  at or after `expires_at_utc_ms` fail with `LeapTableExpired`. Relativistic `TDB`
  ephemeris conversion remains outside this slice.
- `ClockSyncState` (`Disciplined`, `OffsetCalibrated`, `FreeRunningWithDrift`,
  `Unsynchronized`), `ClockAlignment` and `AlignedCaptureTime` align source capture
  timestamps to UTC milliseconds, widen uncertainty linearly for bounded drift
  (`|elapsed_ms| * max_drift_ppm / 1,000,000` rounded up), and refuse `Unsynchronized`
  clocks with `TimeGeoError::UnsynchronizedClock` so receipt/ingestion time is never
  silently substituted for capture time.

Geographic-context and space-imagery analysis import these shared types directly.
`oia_analysis::geo_context::provisional` remains a compatibility re-export of the
same types, not another implementation. Constructors now return `TimeGeoError`;
analysis operations map it to the existing `GeoContextError` variants. Callers
matching a direct constructor error must update its type.
The unused public `TimeGeoBoundary` bootstrap marker has been removed.

Consumer-visible changes include a full-world sensor mask covering a wrapped
envelope becoming `FullyCoveredByActiveSensors` instead of partial. A mask touching
only at the seam or pole changes from `UncoveredNoSensorData` to partial coverage
with `PartialCoverageBlindSpots`; it does not become fully covered. A `[170, 180]`
notice now contains `(0, -180)` and intersects a `[-180, -170]` envelope. Negative
tolerances previously acted as margins (and negative interior thresholds could
admit edge contact); they now return false. In-tree composite callers use positive
`1e-9` allowances, so the negative-input change affects only other callers.

`oia_devices::uav::AltitudeReference` re-exports the same enum, including the
device-specific `RelativeToTakeoff` category. Device position uncertainty, telemetry
ranges, session-local clock alignment and session-local origins remain in the UAV kernel.
This does not yet unify every provisional point/time/quantity type in the repository.

## Numerical scope

The existing Haversine distance uses the IUGG R1 spherical mean radius
6,371,008.8 metres (Moritz 2000), not WGS84 ellipsoidal geodesics. Initial bearing
is likewise spherical and undefined for coincident/antipodal points or departure
at a pole; callers must exclude those cases. `nearest_point_to` retains the
documented planar approximation, which can overestimate spherical distance near
corners at high latitude. It is not a certified nearest-point solver.

The migration starts from the already-merged
`oia-analysis/src/geo_context/provisional.rs` at `a715c5f`, including the composite
comparison helpers. The historical P0 prototype at `aba70b2` and #56 review were
inspected but not copied. No new prior-art port or formal proof is claimed.

## Verification and remaining work

The four assertion groups in
[`primitives_contract.rs`](../../crates/oia-time-geo/tests/primitives_contract.rs)
cover concrete coverage failures at the dateline/full world/poles, invalid input,
missing or incompatible height, half-open timeline endpoints, civil calendar and
RFC 3339 boundary parsing, date-only non-promotion, unresolved DST fold ambiguity,
28-step IERS leap-second conversions/expiry, and unsynchronized clock refusal plus
drift-widened alignment uncertainty. The quarter-circle distance uses an analytic
spherical reference with `1e-8 m` numerical tolerance; this is not a measured
geodetic accuracy claim. Existing analysis, document lineage, news regional-account,
query workspace, desktop globe imagery and UAV contract tests exercise the migrated
consumers. Exact executed commands and environment are recorded in the delivery PR.

#12 remains open for relativistic `TT <-> TDB` ephemeris conversions; shared units,
exact decimal and measurement uncertainty contracts; model-backed height conversions;
remaining provisional type migrations (including desktop/query locations, UAV telemetry
clock-alignment migration and `oia-media` camera positions); the frame-selection
correspondence bridge; and integrated A04/A05/A14 acceptance.
