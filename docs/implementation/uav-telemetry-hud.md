# Personal UAV telemetry, HUD projection, and forensic handoff kernel

Implemented slice of [#33](https://github.com/mESmaC/oia/issues/33), supporting
`DEVICE-01`, `DEVICE-02`, `DEVICE-03`, `DEVICE-04`, and the deterministic
domain/replay subset of `A12` in [the product specification](../spec.md),
[architecture and data contracts](../architecture.md), and
[delivery and verification](../delivery.md). The Rust entry points live in
[`crates/oia-devices/src/uav.rs`](../../crates/oia-devices/src/uav.rs):

- `oia_devices::uav::RegisteredUavDevice`
- `oia_devices::uav::UavFlightSession`
- `oia_devices::uav::UavHudSnapshot`
- `oia_devices::uav::ForensicFrameHandoff`
- `oia_devices::uav::OnboardRecordingLink`

## Contract

1. **Capability-scoped registration and passive receive-only boundary (`DEVICE-01`, `DEVICE-04`, `A12`):**
   - `RegisteredUavDevice::register` validates a `UavAdapterRegistrationInput` whose `declared_capabilities` separately enumerate `VideoStream`, `TelemetryStream`, `RecordedMediaImport`, and `FlightControl`.
   - Video reception does not imply telemetry or media import; telemetry reception does not imply video (`AGENTS.md` §4).
   - Initial UAV integration leaves flight control to the existing controller: registering `UavCapability::FlightControl` fails with `UavError::FlightControlUnsupported`, and calling `attempt_flight_control_command` always fails with `UavError::FlightControlCommandForbidden`.
   - Local connection never auto-publishes footage or location: setting `auto_publish_footage_or_location = true` is rejected with `UavError::AutoPublicationForbidden`.
2. **Clock-offset synchronization and degraded fallback (`DEVICE-04`, `docs/architecture.md` §3.1, `A12`):**
   - `ClockAlignmentRecord` pins an immutable `alignment_version`, `device_to_utc_offset_ms`, `linear_drift_ppm`, `uncertainty_ms`, reference clock and method labels, and a half-open validity interval `[valid_from_device_ms, valid_until_device_ms)`.
   - `UavFlightSession::align_device_timestamp` returns `TimeAlignment::Synchronized` with linear drift adjustment and propagated uncertainty when `device_clock_ms` lies inside the validity interval.
   - When no alignment record is configured or `device_clock_ms` lies outside `[valid_from_device_ms, valid_until_device_ms)`, alignment degrades explicitly to `TimeAlignment::UnsynchronizedDegraded` (preserving raw `device_clock_ms` and `receipt_utc_ms` separately without pretending receipt time is capture time).
3. **Freshness-aware UAV HUD, physical units, altitude datums, and footprint limitations (`DEVICE-02`, `docs/architecture.md` §3.2, `A12`):**
   - Every telemetry field uses explicit physical units (`degrees`, `m`, `m/s`, `V`, `dBm`, `%`) and `Option<...>` so unreported values remain `HudFieldState::Unavailable` rather than defaulting to `0.0` or Null Island (`0.0, 0.0`). Reporting a horizontal position while `GnssFixKind::NoFix` is active is rejected with `UavError::PositionReportedWithoutGnssFix`.
   - `AltitudeSample` carries an explicit `AltitudeReference` (`Wgs84Ellipsoid`, `OrthometricMsl`, `AboveGroundLevel`, `BarometricUnadjusted`, `RelativeToTakeoff`), and `AltitudeSample::try_in_reference` refuses cross-datum extraction (`UavError::IncompatibleAltitudeReference`).
   - At any `inspect_device_ms`, `project_hud_snapshot` marks present fields `HudFieldState::Stale` when sample age exceeds `stale_telemetry_after_ms` (`StaleReason::AgeExceededThreshold`) or when a `TelemetryDisconnectionSpan` occurred at or after the sample (`StaleReason::TelemetryLinkDisconnected`).
   - `LookDirectionMarker` combines heading (`TrueNorth` or `MagneticNorth`), airframe pitch, and `GimbalOrientation` (`FollowHeading` vs. `IndependentWorldLocked`) and always sets `establishes_slant_range: false`.
   - `FootprintStatus` distinguishes:
     - `UncalibratedViewingRayOnly` when no camera calibration is attached;
     - `CalibrationInvalidated` after `invalidate_camera_calibration` flags camera/mount movement;
     - `InsufficientFreshTelemetry` when any required field (`heading`, `attitude`, `position`, `position.horizontal_uncertainty_m`, `altitude`, `altitude.vertical_uncertainty_m`, `gimbal`) is missing or stale;
     - `IncompatibleAltitudeReference` when altitude is not `AboveGroundLevel`;
     - `NonPositiveHeightAboveGround` when `AboveGroundLevel <= 0.0 m`;
     - `UnresolvableGeometry` when either the optical-axis depression angle or the upper frustum ray (`depression_deg - vertical_fov_deg / 2`) is shallower than `MIN_FOOTPRINT_DEPRESSION_DEG` (`5.0°`); and
     - `CalibratedDerivedFootprint(CalibratedFootprint)` with `is_derived_estimate: true`, explicit flat-ground terrain assumption, and propagated horizontal uncertainty.
4. **Original streams, rendered HUD derivatives, non-blocking frame-copy handoff, and onboard-recording linkage (`DEVICE-03`, `DEVICE-04`, `A12`):**
   - `OriginalFlightStream` rejects `FlightStreamOrigin::RenderedHudOverlayDerivative` (`UavError::HudDerivativeCannotBeOriginalStream`). Rendered HUD overlays are registered separately via `register_hud_derivative`, whose `can_replace_original_evidence()` is always `false`.
   - `record_bookmark` binds an immutable `FlightBookmark` to the case, aligned timestamp, and `pinned_stream_version`.
   - `export_forensic_frame_handoff` returns a self-contained `ForensicFrameHandoff` value copying the unaltered `VideoFrameRecord`, synchronized HUD/telemetry snapshot, `MotionDynamicsFlags` (vibration, gimbal slew, rolling-shutter risk, exposure shift), and `MOVING_CAMERA_FORENSIC_CAVEAT`. Subsequent telemetry and frame ingestion continues on `UavFlightSession` without blocking or mutating exported handoffs.
   - `link_onboard_recording` attaches higher-quality onboard media (`OnboardRecordingLink`) to the session and existing bookmarks with a documented `stream_to_onboard_offset_ms`, rejecting any reused `VersionId` (`UavError::DuplicateOrOverwrittenVersion`) and leaving `original_stream().stream_version()` and prior bookmark/handoff bindings untouched (`overwrote_prior_evidence() == false`).
5. **Resource bounds and text hygiene (`AGENTS.md` §5):**
   - All collections, cumulative text bytes, and rendered output lengths are bounded by `UavSessionLimits`.
   - Single-line text fields reject ASCII/Unicode control characters, Unicode line/paragraph separators, bidi overrides/isolates, and invisible formatting/tag characters while allowing `U+200C` (ZWNJ) and `U+200D` (ZWJ) for Persian, Indic, and emoji text.

## Limitations and remaining `#33` scope

- This module is an in-memory validation, alignment, HUD-projection, and handoff kernel; it checks 64-character lowercase hexadecimal SHA-256 digest strings on stream, frame, derivative, and onboard-recording metadata, but does not open network/serial links, parse MAVLink or vendor SDK packets, decode video containers, or persist records to `oia-store`.
- `D08` (selection of concrete drone video/telemetry protocols, SDRs, and stream integrations) remains open.
- Shared coordinate/time conversion consolidation (`#12`), durable job orchestration (`#14`), interactive workspace HUD and map footprint rendering (`#16`), forensic workbench UI integration (`#30`), and live hardware validation for full end-to-end `A12` acceptance remain open on `#33`.
- No Lean theorem or formal Rust-to-Lean correspondence claim is made for this slice.

## Local verification

Executed on Windows x86_64 MSVC with pinned `rustc 1.99.0` on 2026-11-02:

- `cargo +1.99.0 fmt --all -- --check`
- `cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings`
- `cargo +1.99.0 test --workspace --locked --offline`
- `cargo +1.99.0 run -p oia-devices --example replay-uav-flight --locked --offline`

Two targeted contract tests in [`crates/oia-devices/tests/uav_contract.rs`](../../crates/oia-devices/tests/uav_contract.rs) verify:
1. Clock drift adjustment, validity-window degradation, omitted vs. stale vs. fresh HUD fields, independent gimbal motion, uncalibrated look markers (`establishes_slant_range == false`) vs. calibrated AGL footprints, grazing-angle and barometric-datum footprint refusals, calibration invalidation, frame-copy forensic handoffs during ongoing reception, separate HUD derivatives, and non-destructive onboard-footage linking.
2. Rejection of `FlightControl` capability and commands, automatic local publication, cross-capability telemetry/video calls, HUD derivatives submitted as original streams, cross-datum altitude conversions, `NoFix` positions, bidi/control text injection (while preserving `U+200C`/`U+200D`), reused version IDs, and output-budget overflows.
