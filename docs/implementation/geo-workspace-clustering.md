# Geographic workspace, coverage envelopes, and reversible event clustering (`#23`)

## 1. Scope and status (`AGENTS.md` §3, §4)

- **Implemented (`crates/oia-query/src/geo_workspace.rs`):**
  - Dependency-independent Rust query, clustering, mosaic-footprint, and cross-view context kernel in `oia-query` (`edition = "2024"`, `rust-version = "1.99.0"`, `#![forbid(unsafe_code)]`, `unwrap_used = "deny"`, `expect_used = "deny"`, `panic = "deny"`).
  - Reuses `oia_domain::{ActorId, OpaqueId, SchemaVersion, VersionId}` and `oia_domain::witness::AccountOrigin`.
  - Covers `GEO-01`, `GEO-02`, `GEO-03`, `GEO-04`, `CASE-02`, `UX-02`, and the deterministic geographic-query, reversible-clustering, mosaic-footprint, and cross-view navigation subset of `A04`.
- **Tested:**
  - `crates/oia-query/tests/geo_workspace_contract.rs` (3 focused contract tests covering dateline-crossing envelopes, calendar-date-only historical accounts, gazetteer centroid uncertainty, missing-feed coverage-gap protection, reversible merge/split/revert clustering with distinct article/account/independent-source/event counts, discrete viewing-cone mosaic alignment degradation, and cross-view navigation with local filter divergence disclosures).
  - `crates/oia-query/examples/inspect-a04-geo-workspace.rs` (read-only CLI inspection over a synthetic `A04` dateline-corridor fixture).
- **Remaining open on `#23` (proposed/target):**
  - Interactive 3D globe and timeline UI canvas (`#16`), SQLite spatial index persistence (`#11`, `#14`), and live external weather/aircraft/maritime/satellite provider adapters (`#24`–`#26`).

## 2. Requirements and analytical contracts

### 2.1 Area/time selection, approximate geometry, and historical time qualifications (`GEO-01`, `A04`)

- `GeoPointWgs84` validates finite WGS84 coordinates (`lat_deg` in `[-90, 90]`, `lon_deg` in `[-180, 180]`) and computes spherical haversine distance (`EARTH_MEAN_RADIUS_M = 6,371,008.8` m) and initial forward azimuth `[0, 360)`.
- `GeoBoundingBoxWgs84` supports antimeridian-crossing regions (`west_lon_deg > east_lon_deg`, e.g., `175.0°E` to `-175.0°W`) for both point containment and box intersection.
- `GeoLocation::ApproximateGazetteerCentroid` requires a strictly positive `radius_uncertainty_m` so a gazetteer centroid is never mistaken for an exact surveyed point (`docs/architecture.md` §3.2).
- `ReportedEventTime` preserves verbatim reported local time text, timezone name, optional UTC offset (`None` when historical offset/DST is ambiguous), `ReportedTimePrecision::CalendarDateOnly` (which never promotes a date-only historical record to a midnight UTC timestamp, per `docs/architecture.md` §3.1), and `EarliestKnownSourceQualification` when an account is attested only via a later archival/secondary source.

### 2.2 Multi-modality coverage envelopes and missing-feed discipline (`GEO-02`, `GEO-05`, `A04`)

- `GeoCoverageEnvelope::evaluate_scene_conclusion` inspects `ModalityCoverageRecord` entries across requested modalities (`SatelliteImagery`, `PublicCameraRecords`, `WeatherContext`, `AircraftVesselActivity`, `AstronomicalContext`, `WitnessReports`, `Documents`).
- Any modality with `MissingHistory`, `Unavailable`, `StaleSnapshot`, `PartialCoverage`, incomplete spatial/temporal overlap, or `TrackDataBasis` interpolation/dropouts produces `SceneCoverageConclusion::CoverageGapCannotInferEmptyScene`. A missing camera feed or absent track never becomes a clear-sky or empty-scene conclusion.

### 2.3 Editable, reversible event clustering and distinct counts (`GEO-03`, `A04`)

- `EventClusterLedger` groups `GeoEvidenceItem` records into `ProposedEventCluster` snapshots with immutable `VersionId`s.
- Each cluster and the ledger as a whole report `DistinctEvidenceCounts`:
  - `article_count`: published articles (`GeoEvidenceKind::PublishedArticle`),
  - `account_count`: reported witness testimonies (`AccountOrigin::Reported`),
  - `independent_source_count`: equivalence classes of non-simulated items after collapsing `Republishes`/`Quotes`/`Copies` lineage chains, shared `source_id`s, and identical `content_hash` values (unanchored citation cycles contribute `0` and are surfaced as `ClusterConflict::UnanchoredLineageCycle`),
  - `proposed_event_count`: active proposed clusters,
  - `simulated_item_count`: `AccountOrigin::Simulated` items, kept strictly out of observed `account_count` and `independent_source_count`.
- `merge_clusters`, `split_cluster`, and `revert_decision` record `ClusterDecisionRecord` audit entries and allow splitting, merging, and reverting decisions under new immutable `VersionId`s without erasing prior cluster history.

### 2.4 Discrete footprint and viewing-cone mosaic (`GEO-04`, `A04`, `A13`)

- `FootprintMosaic` enforces `MosaicPresentationMode::DiscreteFootprintAndConeMosaicOnly` so continuous seamless scene reconstruction is never implied.
- `evaluate_cross_view_alignment` checks pairwise `MosaicTile` records and degrades alignment (`CrossViewAlignmentStatus::DegradedAlignment`) whenever camera calibration was invalidated by movement (`TileCalibrationState::InvalidatedByCameraMovement`), uses uncalibrated nominal FOV, or lost clock synchronization (`ClockAlignmentStatus::LostSyncReceiptTimeOnly`, refusing to treat frame receipt time as capture time).

### 2.5 Shared workspace context and route-back navigation (`CASE-02`, `UX-02`)

- `SharedWorkspaceContext` coordinates `WorkspaceSurface` (`Globe`, `Sky`, `Documents`, `Media`, `Graph`, `MarketCharts`, `Report`), shared area/time, selected event, and dossier evidence while maintaining bounded `route_back` and `route_forward` stacks.
- `surfaces_with_divergent_local_filters` exposes every surface whose `LocalViewFilter` overrides the shared investigation area, time window, modality set, or simulation visibility.
