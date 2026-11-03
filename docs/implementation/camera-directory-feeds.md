# Camera directory registration, viewing-cone geometry, frame freshness, and bounded clip-retention kernel

Implemented slice of [#28](https://github.com/mESmaC/oia/issues/28), supporting
`CAM-01`, `CAM-02`, `CAM-04`, `docs/architecture.md` §§3.2, 4.3, 4.4, 8, and the
deterministic source-admission and buffer-expiry subset of `A09` in
[the product specification](../spec.md) and [delivery plan](../delivery.md).
The Rust entry points live in [`crates/oia-media/src/camera_directory.rs`](../../crates/oia-media/src/camera_directory.rs):

- `oia_media::camera_directory::CameraDirectoryLimits`
- `oia_media::camera_directory::CameraDirectoryRegistry`
- `oia_media::camera_directory::RegisteredCameraEntry`
- `oia_media::camera_directory::ViewingConeGeometry`
- `oia_media::camera_directory::ClassifiedCameraFrame`
- `oia_media::camera_directory::RetainedCameraClipManifest`

## Contract

1. **One domain model across `oia_media::camera_directory` and `oia_media::camera_anomaly` (`AGENTS.md` §4):**
   - `camera_directory` directly imports and re-exports [`CameraAccessStatus`], [`CameraCalibrationState`], [`CameraHealthState`], [`CameraSourceProfile`], [`CameraViewKind`], [`ContextWindowStatus`], [`FeedObservationKind`], [`FrameArtifactOrigin`], [`MAX_CLOCK_UNCERTAINTY_MS`], [`RESTRICTED_SOURCE_POLICY_NOTICE`], [`RecordedHealthEvent`], [`RetainedFrameRef`], and [`SourceUsePermission`] from [`crate::camera_anomaly`](../../crates/oia-media/src/camera_anomaly.rs).
   - `FeedObservationKind` in `camera_anomaly` is extended with `PeriodicUpstreamStill`, `StaleStillHold`, and `HistoricalArchiveFrame` alongside `UpstreamCameraFeed`, `StillFallback`, `StreetViewFallback`, and `SyntheticPlaceholder`. `FeedObservationKind::eligible_as_event_evidence()` admits only `UpstreamCameraFeed`, fresh `PeriodicUpstreamStill`, and verified `HistoricalArchiveFrame`; `FeedObservationKind::eligible_for_live_anomaly_monitoring()` admits only `UpstreamCameraFeed` and `PeriodicUpstreamStill`.
   - Vertical-datum / altitude primitives belong to `oia-time-geo` (`#12`) and are not duplicated in `oia-media`. `CameraHorizontalPosition` records horizontal WGS84 coordinates with explicit uncertainty (`KnownWgs84`), labeled approximate gazetteer/city centroids (`ApproximateGazetteerCentroid`), or `Unknown { reason }` (never `(0.0, 0.0)`).
2. **Camera directory registration, ordinary access, and privacy exclusions (`CAM-01`, `CAM-02`, `docs/architecture.md` §4.4):**
   - `CameraDirectoryRegistry::register_camera` catalogs cameras reachable through ordinary access (`OrdinaryAccessMethod { ExistingDirectoryListing, PublishedViewingLink, SupportedEmbed, SupportedFeedOrApi }`) and attaches `DIRECTORY_LISTING_PROVENANCE_NOTICE` stating that a directory listing does not by itself establish live stream availability or historical archive access.
   - `IntentionalPublicationStatus { EstablishedByOwner, NotEstablishedOrdinaryAccess }` is recorded as provenance metadata only and never refuses or degrades an otherwise ordinarily accessible camera (`CAM-01`, `AGENTS.md` §6).
   - Home interiors (`CameraViewKind::HomeInterior`, `CameraViewKind::ViewIntoHomeInterior`) are unconditionally refused (`CameraDirectoryError::HomeInteriorExcluded`).
   - `CameraViewKind::ResidentialExteriorStreetView` requires `PrivacyViewAssessment::RequiresMaskZones` with non-empty, unique `PrivacyMaskZone`s (`CameraDirectoryError::ResidentialExteriorRequiresMaskZones`). Unmasked zones (`mask_applied == false`) or `PrivacyViewAssessment::ExcludedPrivateActivity` can be cataloged to record the assessment, but block frame acquisition (`CameraDirectoryError::UnmaskedPrivacyZone`, `CameraDirectoryError::PrivateActivityViewExcluded`).
   - Catalog-only listings (`StreamCapability::None`, `StillRefreshCapability::None`, `ArchiveCapability::NoArchiveEstablished`, or `automated_processing` / `clip_retention` set to `Unknown` or `Prohibited`) are accepted without fabricating stream or archive capabilities.
3. **Restricted-source cutoff and explicit re-registration (`CAM-01`, `A09`, `docs/architecture.md` §4.3):**
   - Source lifecycle health (`SourceLifecycleHealth { Unconfigured, Connecting, Available, Degraded, Stale, UnavailableRestricted, DisabledByPolicy }`) is tracked via `record_health_transition`.
   - Recording `SourceLifecycleHealth::UnavailableRestricted` or `Some(CameraHealthState::SourceRestricted)` transitions `access_status` to `CameraAccessStatus::RestrictedOrUnavailable` and halts frame ingestion (`CameraDirectoryError::SourceRestricted` with `RESTRICTED_SOURCE_POLICY_NOTICE`).
   - Once restricted, all subsequent frame submissions and health transition retries are refused (`CameraDirectoryError::RestrictedSourceRequiresExplicitReregistration`). Only an explicit `reregister_ordinarily_accessible_version` call with a new, unseen `VersionId` and updated listing reference can re-admit the camera while preserving `prior_source_versions` and transition history.
4. **Viewing-cone geometry and movement invalidation (`CAM-02`, `CAM-05`, `docs/architecture.md` §§3.2, 8):**
   - `compute_viewing_cone(camera_id, range_m, arc_vertex_count)` validates `range_m` (`(0.0, MAX_VIEWING_CONE_RANGE_M]` where `MAX_VIEWING_CONE_RANGE_M = 100_000.0` m) and `arc_vertex_count` (`MIN_VIEWING_CONE_ARC_VERTICES..=MAX_VIEWING_CONE_ARC_VERTICES`, `2..=128`) before allocating.
   - Projects a spherical horizontal viewing cone on `EARTH_MEAN_RADIUS_M` (`6_371_008.8` m) only when the camera has `CameraHorizontalPosition::KnownWgs84`.
   - Distinguishes `ViewingConeClass::CalibratedFieldOfView` (`is_calibrated() == true`, pinned `calibration_version` and `max_angular_error_mdeg`) from `ViewingConeClass::IllustrativeReportedBearing` (`is_calibrated() == false`, with `bearing_uncertainty_deg`), and refuses unknown or movement-invalidated orientations (`CameraDirectoryError::OrientationUnavailableForViewingCone`).
   - Recording `Some(CameraHealthState::CameraRepositioned)` automatically transitions the camera's orientation to `CameraViewingOrientation::InvalidatedByMovement`, preserving any prior `calibration_version` and mapping to `CameraCalibrationState::InvalidatedByMovement` in `to_anomaly_source_profile()`.
5. **Frame classification, periodic-still freshness, and archive window verification (`CAM-02`, `AGENTS.md` §2):**
   - `ingest_frame` validates 64-character lowercase hex SHA-256 digests, `byte_len >= 1`, `FrameArtifactOrigin::OriginalRetainedFrame` (rejecting `PredictiveOrGeneratedDerivative`), `clock_uncertainty_ms <= MAX_CLOCK_UNCERTAINTY_MS` (`86_400_000` ms = 24 h), and strictly increasing `(ingestion_time_ms, sequence_number)` plus non-regressing live event timestamps.
   - `PeriodicUpstreamStill` frames are evaluated against the camera's declared `stale_after_ms` threshold: `FrameFreshnessEvaluation` records `stale_after_ms`, `source_capture_time_ms`, `ingestion_time_ms`, `observed_capture_age_ms`, and `unchanged_hold_duration_ms`. If capture age or unchanged digest/timestamp hold duration exceeds `stale_after_ms`, the frame is downgraded to `FeedObservationKind::StaleStillHold` (`eligible_as_event_evidence() == false`), omitted from the retention ring buffer, and camera health transitions to `SourceLifecycleHealth::Stale`.
   - `HistoricalArchiveFrame` submissions require `ArchiveCapability::VerifiedArchiveWindow`, a matching `archive_version`, and `source_capture_time_ms` within `[earliest_capture_ms, latest_capture_ms]`. Verified archive frames are eligible as retrospective observation evidence (`to_retained_frame_ref()`) without entering the live ring buffer.
   - Non-observation substitutes (`StillFallback`, `StreetViewFallback`, `SyntheticPlaceholder`, `StaleStillHold`) are classified as `FrameFreshnessEvaluation::NonEvidentiarySubstitute`, never enter the ring buffer, and return `Err(CameraDirectoryError::NonEvidentiaryFrameCannotBecomeEvidence)` from `to_retained_frame_ref()`.
6. **Bounded retention ring buffer, deterministic FIFO eviction, and clip manifest handoff (`CAM-04`, `A09`, `docs/architecture.md` §8):**
   - Only live evidentiary frames (`UpstreamCameraFeed` and fresh `PeriodicUpstreamStill`) on cameras with `clip_retention == SourceUsePermission::Permitted` enter the per-camera ring buffer.
   - Eviction order is strictly deterministic: FIFO by `(ingestion_time_ms, sequence_number, frame_version)`. Expired frames (`age_ms > max_retention_window_ms`) and over-capacity frames (`max_buffered_frames_per_camera`, `max_buffered_bytes_per_camera`) are evicted from the front of the queue and tallied in `RingBufferEvictionSummary`.
   - Sequence gaps (`sequence_number > prev_sequence + 1`) are recorded in bounded `FrameDiscontinuitySpan`s.
   - `extract_retained_clip_manifest` packages a `RetainedCameraClipManifest` with `primary_frame: RetainedFrameRef`, `event_frames`, `context_window: ContextWindowStatus` (`Complete`, `PartialBufferExpired`, or `OmittedByRetentionPolicy`), `dropped_frame_count_in_span`, `discontinuity_spans_in_window`, `source_session_id`, `redistribution_permission`, and `CANDIDATE_EVENT_CAVEAT`, bridging directly into `oia_media::camera_anomaly::SceneTriggerInput`.

## Limitations and remaining `#28` scope

- This module is an in-memory directory, geometry, classification, and ring-buffer kernel; it does not perform live network HTTP/RTSP/HLS/WebRTC fetching (`#13`), decode video containers or image codecs, persist camera catalogs or retained clip blobs to `oia-store` (`#11`), schedule background worker jobs (`#14`), or render map/globe viewing cones and camera drawers in the UI (`#15`, `#16`).
- Vertical-datum / altitude primitives belong to `oia-time-geo` (`#12`).
- No Lean theorem or formal Rust-to-Lean correspondence claim is made for this slice.

## Local verification

Executed on Windows x86_64 MSVC with pinned `rustc 1.99.0` on 2026-11-03:

- `cargo +1.99.0 fmt --all -- --check`
- `cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings`
- `cargo +1.99.0 test -p oia-media --locked --offline`
- `cargo +1.99.0 test --workspace --locked --offline`
- `cargo +1.99.0 run -p oia-media --example inspect-camera-directory --locked --offline`
- `powershell -ExecutionPolicy Bypass -File scripts\run.ps1 proofs`

Five targeted contract tests in [`crates/oia-media/tests/camera_directory_contract.rs`](../../crates/oia-media/tests/camera_directory_contract.rs) verify:
1. Directory registration with `IntentionalPublicationStatus::NotEstablishedOrdinaryAccess` as non-blocking metadata (`CAM-01`), catalog-only camera listings (`CAM-02`), unconditional home-interior refusal (`HomeInterior`, `ViewIntoHomeInterior`), required privacy mask zones on `ResidentialExteriorStreetView`, and acquisition refusal on unmasked zones or `ExcludedPrivateActivity`.
2. Calibrated vs. illustrative viewing-cone geometry on the WGS84 sphere, automatic orientation invalidation upon `CameraRepositioned`, and pre-allocation bounds checks on cone range and vertex count.
3. Periodic-still freshness evaluation against `stale_after_ms` (`FreshPeriodicStill` vs. downgrade to `StaleStillHold`), non-evidentiary substitute isolation (`StillFallback`, `StreetViewFallback`, `SyntheticPlaceholder`), verified historical archive window checks, and generated-frame / clock-uncertainty refusals.
4. Permanent restricted-source cutoff (`SourceRestricted`) blocking both frame ingestion and automatic retry transitions until explicit `reregister_ordinarily_accessible_version` with a new `VersionId`.
5. Deterministic FIFO `(ingestion_time_ms, sequence_number, frame_version)` ring-buffer eviction by capacity and retention window, sequence discontinuity accounting, `RetainedCameraClipManifest` extraction (`PartialBufferExpired`), and direct handoff into `CameraAnomalyMonitor`.
