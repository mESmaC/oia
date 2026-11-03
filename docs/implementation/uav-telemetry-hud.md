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
2. **Clock-offset synchronization, time-scoped history, and degraded fallback (`DEVICE-04`, `docs/architecture.md` §3.1, `A12`):**
   - `ClockAlignmentRecord` pins an immutable `alignment_version`, `device_to_utc_offset_ms` (`aligned_utc_ms ≈ device_clock_ms + device_to_utc_offset_ms + drift_adjustment_ms`), `linear_drift_ppm`, non-zero `uncertainty_ms` (`UavError::ZeroClockUncertainty` if `0`), reference clock and method labels, and a half-open validity interval `[valid_from_device_ms, valid_until_device_ms)`.
   - Offset and drift bounds use `.unsigned_abs()` so signed minimum values (`i64::MIN`, `i32::MIN`) are rejected cleanly in both debug and `--release` builds.
   - `UavFlightSession` retains a bounded, time-scoped history of clock alignment records (`max_calibration_updates`) selected by explicit validity windows, independent of attachment order. Covering windows use the latest `valid_from_device_ms`; equal starts are rejected as ambiguous. Capture-pinned alignments are preserved and flagged for reassessment when later records change their interpretation.
   - `UavFlightSession::align_device_timestamp` uses checked integer arithmetic and returns `TimeAlignment::Synchronized` with linear drift adjustment and `total_uncertainty_ms == uncertainty_ms` when `device_clock_ms` lies inside the active validity interval. If no alignment record was active at `device_clock_ms`, if `device_clock_ms` lies outside `[valid_from_device_ms, valid_until_device_ms)`, or if timestamp arithmetic would overflow `i64` (`UnsynchronizedReason::AlignmentArithmeticOverflow`), alignment degrades explicitly to `TimeAlignment::UnsynchronizedDegraded`.
3. **Freshness-aware UAV HUD, physical units, altitude datums, and footprint limitations (`DEVICE-02`, `docs/architecture.md` §3.2, `A12`):**
   - Every telemetry field uses explicit physical units (`degrees`, `m`, `m/s`, `V`, `dBm`, `%`) and `Option<...>` so unreported values remain `HudFieldState::Unavailable` rather than defaulting to `0.0` or Null Island (`0.0, 0.0`). Reporting a horizontal position while `GnssFixKind::NoFix` is active is rejected with `UavError::PositionReportedWithoutGnssFix`.
   - `AltitudeSample` carries an explicit `AltitudeReference` (`Wgs84Ellipsoid`, `OrthometricMsl`, `AboveGroundLevel`, `BarometricUnadjusted`, `RelativeToTakeoff`, or `Unknown`), and `AltitudeSample::try_in_reference` refuses cross-datum extraction and always refuses `AltitudeReference::Unknown` (`UavError::IncompatibleAltitudeReference`).
   - At any `inspect_device_ms`, `project_hud_snapshot` marks present fields—including `motion_flags: HudFieldState<MotionDynamicsFlags>`—as `HudFieldState::Stale` when sample age exceeds `stale_telemetry_after_ms` (`StaleReason::AgeExceededThreshold`) or when the latest applicable `TelemetryDisconnectionSpan` occurred at or after the sample (`StaleReason::TelemetryLinkDisconnected`).
   - `LookDirectionMarker`, `CalibratedFootprint`, and `UavHudSnapshot` are sealed against external literal construction outside `oia-devices` so callers must derive them through `UavFlightSession`.
   - `LookDirectionMarker` distinguishes `GimbalMode::IndependentWorldLocked` (stabilized against the local horizontal plane and `TrueNorth`, so airframe pitch and `MagneticNorth` airframe heading do not perturb the camera ray) from `GimbalMode::FollowHeading` (the full intrinsic ZYX airframe rotation composed with the body-relative gimbal rotation), folds past-nadir angles (`> 90°`) into `[0°, 90°]` with a `180°` azimuth flip, and always sets `establishes_slant_range: false`.
   - `FootprintStatus` distinguishes:
     - `TelemetryCapabilityNotDeclared` when the device did not declare `UavCapability::TelemetryStream`;
     - `UncalibratedViewingRayOnly` when no camera calibration was active at `inspect_device_ms`;
     - `CalibrationInvalidated` when the calibration active at `inspect_device_ms` was invalidated by camera/mount movement;
     - `InsufficientFreshTelemetry` when any required field (`gimbal`, `heading`/`attitude` for `FollowHeading`, `position`, `position.horizontal_uncertainty_m`, `altitude`, `altitude.vertical_uncertainty_m`, `navigation_health`) is missing/stale or `navigation_health` reports `NoFix`/`DeadReckoningEstimate`;
     - `IncompatibleAltitudeReference` when altitude is not `AboveGroundLevel`;
     - `IncompatibleHeadingReference` when `GimbalMode::FollowHeading` is combined with `HeadingReference::MagneticNorth` (preventing magnetic azimuth from being projected onto true-north/east ground offsets);
     - `NonZeroRollUnsupported` when composed image roll is non-zero modulo 180 degrees. Both image-right and image-down axes are rotated, so non-zero airframe pitch combined with gimbal yaw is checked even when both input rolls are zero. The modulo convention preserves the identical rectangular footprint of an upside-down image;
     - `NonPositiveHeightAboveGround` when `AboveGroundLevel <= 0.0 m`;
     - `UnresolvableGeometry` when the optical-axis depression angle, the far frustum ray (`depression_deg - vertical_fov_deg / 2`), or the rear horizon clearance of a nadir-straddling frustum (`180° - (depression_deg + vertical_fov_deg / 2)`) is shallower than `MIN_FOOTPRINT_DEPRESSION_DEG` (`5.0°`); and
     - `CalibratedDerivedFootprint(CalibratedFootprint)` with `azimuth_reference: HeadingReference::TrueNorth`, `is_derived_estimate: true`, signed cotangent (`cos / sin`) ground-plane intersections supporting nadir-straddling frustums, and linearized horizontal center uncertainty (`pos_unc_m + alt_unc_m * |cot(dep)| + (slant_range / sin(dep)) * angular_uncertainty_rad`).
4. **Original streams, rendered HUD derivatives, non-blocking frame-copy handoff, and onboard-recording linkage (`DEVICE-03`, `DEVICE-04`, `A12`):**
   - `OriginalFlightStream` rejects `FlightStreamOrigin::RenderedHudOverlayDerivative` (`UavError::HudDerivativeCannotBeOriginalStream`). Rendered HUD overlays are registered separately via `register_hud_derivative`, whose `can_replace_original_evidence()` is always `false`.
   - `record_bookmark` binds an immutable `FlightBookmark` to the case, aligned timestamp, and `pinned_stream_version`.
   - `export_forensic_frame_handoff` returns a self-contained `ForensicFrameHandoff` value copying the `session_id`, `source_device_id`, `source_stream_version`, `source_stream_sha256_hex`, `source_stream_byte_len`, unaltered `VideoFrameRecord`, time-scoped `camera_calibration_state_at_frame`, `frame_drops_at_or_before_frame`, synchronized HUD/telemetry snapshot (including `HudFieldState<MotionDynamicsFlags>`), and `MOVING_CAMERA_FORENSIC_CAVEAT`. Subsequent telemetry, frame ingestion, clock re-alignments, and calibration updates continue on `UavFlightSession` without mutating or retroactively altering historical handoffs.
   - `link_onboard_recording` attaches higher-quality onboard media (`OnboardRecordingLink`) to the session and existing bookmarks with a documented `stream_to_onboard_offset_ms` (`onboard_clock_ms ≈ live_stream_device_clock_ms + stream_to_onboard_offset_ms`), rejecting any reused `VersionId` (`UavError::DuplicateOrOverwrittenVersion`) and leaving `original_stream().stream_version()` and prior bookmark/handoff bindings untouched (`overwrote_prior_evidence() == false`).
5. **Resource bounds and text hygiene (`AGENTS.md` §5):**
   - All collections (including `max_calibration_updates` for clock records, camera calibrations, and dated invalidations together), cumulative text/identifier bytes, and rendered output lengths are bounded by `UavSessionLimits`, and state mutations occur only after input validation and budget checks succeed.
   - Single-line text fields reject ASCII/Unicode control characters, Unicode line/paragraph separators, bidi overrides/isolates, and invisible formatting/tag characters while allowing `U+200C` (ZWNJ) and `U+200D` (ZWJ) for Persian, Indic, and emoji text.

## Limitations and remaining `#33` scope

- This module is an in-memory validation, alignment, HUD-projection, and handoff kernel; it checks 64-character lowercase hexadecimal SHA-256 digest strings on stream, frame, derivative, and onboard-recording metadata, but does not open network/serial links, parse MAVLink or vendor SDK packets, decode video containers, or persist records to `oia-store`.
- `D08` (selection of concrete drone video/telemetry protocols, SDRs, and stream integrations) remains open.
- Shared coordinate/time conversion consolidation (`#12`), durable job orchestration (`#14`), interactive workspace HUD and map footprint rendering (`#16`), forensic workbench UI integration (`#30`), and live hardware validation for full end-to-end `A12` acceptance remain open on `#33`.
- No Lean theorem or formal Rust-to-Lean correspondence claim is made for this slice.

## Dated metadata and reassessment

- `CameraCalibration::effective_from_device_ms` and `invalidate_camera_calibration(effective_from_device_ms, reason)` explicitly date state changes. Camera history is append-only and exposed in effective-time order. A state applies until the next dated event; equal-time changes are rejected so an invalidation cannot silently cover zero time. Invalidating requires a calibration valid at that instant.
- Clock windows are half-open. The covering record with the latest start wins, regardless of attachment order; an expired window cannot shadow another covering record. Equal-start windows are rejected rather than using arrival order as a hidden tie-breaker. This API does not replace records with the same effective time.
- Samples, frames, and bookmarks retain the metadata known at ingestion/issuance. Late alignment or calibration changes set sticky `needs_reassessment` flags when the pinned interpretation differs. Even the first alignment/calibration attached after frame import cannot rewrite its pinned state.
- A new `ForensicFrameHandoff` includes pinned camera/alignment values, the current dated interpretations, and reassessment status. The HUD text marks a pinned projection needing reassessment before its geometry is shown. Current `project_hud_snapshot` queries use current dated records, including historical invalidation refusals. Previously exported copies stay immutable; consumers inspect the session's frame record or export again to discover later corrections. Durable dependency notifications remain outside this kernel.
- A viewing marker carries `is_derived_estimate` and `azimuth_undefined_at_nadir` (also true at zenith). At a vertical optical axis, its numeric azimuth is an image-axis convention, not a measured direction. Footprint axes use the horizontal image-right direction at nadir.
- Regression coverage includes reversed/disjoint clock imports, metadata attached after frames, a shock at device time 3000 between frames at 1000 and 6000, preserved issued handoffs, ambiguous dates, induced image roll, and vertical-ray labeling. These checks address concrete review findings C1-C3 and DEVICE-02..04 evidence contracts.

## Local verification

Executed on 2026-11-02 (America/New_York), Windows x86_64 MSVC / pinned Rust 1.99.0, at `70a00e312e001bfd97eec2a0609d21630117cc65` after integrating `main` at `92c0511`. This record is a subsequent documentation-only commit.

- `cargo +1.99.0 fmt --all -- --check`: exit 0.
- `cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings`: exit 0, including the default desktop feature.
- `cargo +1.99.0 test --workspace --locked --offline`: exit 0; 134 passed, none failed or ignored, including 5 UAV contract tests and the existing Lean cutoff correspondence fixture.
- `cargo +1.99.0 test -p oia-devices --test uav_contract --release --locked --offline`: exit 0; 5 passed.
- `cargo +1.99.0 run -p oia-devices --example replay-uav-flight --locked --offline`: exit 0; simulated handoff, stale gimbal diagnostic, and non-destructive onboard link inspected.
- `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run.ps1 proofs`: exit 0; Lean build, 263-declaration axiom audit, and cutoff fixture byte check passed. These existing models do not prove this UAV algorithm.
- Branch diff, relative documentation links, and code fences checked.

No hosted CI, Linux/macOS execution, live hardware, end-to-end A12 acceptance, or UAV Rust-to-Lean correspondence claim is made.
