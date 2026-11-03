//! `oia-devices` — Capability-aware device sessions, receive-only UAV telemetry/HUD, and sensor
//! boundaries (`#9`, `#33`).
//!
//! [`uav`] implements the dependency-independent personal drone registration, capability-aware
//! HUD projection, clock-offset alignment, frame-copy forensic handoff, and non-destructive
//! onboard-footage linking kernel for `#33` (`DEVICE-01`–`DEVICE-04`, deterministic subset of
//! `A12`). [`radio`] implements receive-only capability and session state contracts for `#32`;
//! [`station`] implements the personal observatory capture kernel for `#34`. Live radio
//! adapters and physical station hardware integration remain open.

#![forbid(unsafe_code)]
#![deny(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

pub mod radio;
pub mod station;
pub mod station_trigger;
pub mod uav;

/// Crate boundary marker for `oia-devices` (`docs/architecture.md` §1.2, `#9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DevicesBoundary;
