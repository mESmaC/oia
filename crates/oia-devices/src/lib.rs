//! `oia-devices` — Receive-only radio, UAV telemetry, and personal sensor boundary stub (`#9`).
//!
//! Full implementation follows in P3 (`#32`–`#34`).

/// Crate boundary marker for `oia-devices` (`docs/architecture.md` §1.2, `#9`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DevicesBoundary;
