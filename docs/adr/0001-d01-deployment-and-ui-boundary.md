# ADR 0001: Deployment Model, Application Shell, and UI Boundary (D01)

- **Date:** 2026-11-01
- **Status:** Accepted for P0 foundation; graphical renderer selection deferred to P1/P2 (#16, #23)
- **Decision ID:** D01
- **Issues:** #9, #14, #16
- **Requirements:** `CASE-01`–`CASE-05`, `UX-01`–`UX-05`

## Context

`D01` requires establishing a local-first Rust core and resolving the boundary between application/domain logic and the user interface without coupling renderer or provider types into `oia-domain`.

## Decision

1. **Accepted for P0 (Local-First Rust Runtime & Strict Layering):**
   - OIA runs local-first as a single-process Rust workspace (`edition = "2024"`, pinned to `rustc 1.99.0` via `rust-toolchain.toml`) with zero mandatory cloud accounts for local case creation, document import, evidence inspection, search, or report/bundle export.
   - `oia-domain` and `oia-time-geo` contain pure domain contracts and calculations with `#![forbid(unsafe_code)]` and zero dependencies on UI toolkits, HTTP clients, database engines, or model SDKs.
   - `oia-app` defines the shared application service boundary separating read-only queries from state-changing commands, alongside a renderer-independent presentation state model (`WorkspaceShellState`, `VisualSystemConfig`, `AccessibilityConfig`, `ViewKind`, `NavigationHistory`, `SemanticAccent`).
   - `UX-01` through `UX-05` visual and accessibility invariants (dark military-glass palette tokens, keyboard focus order, reduced motion/transparency, clean-evidence and focused-reading modes, muted routine audio by default, non-color mode badges for `Live`, `Historical`, `Simulated`, and `Generated`, and independence of scientific/numerical calculations from visual quality settings) are enforced as typed state transitions in `oia-app`.
   - Validated initial development and runtime target: `x86_64-pc-windows-msvc` (with portable pure-Rust standard library code compatible with `x86_64-unknown-linux-gnu`).

2. **Open Proposal for P1/P2 (#16, #23):**
   - The interactive graphical windowing and 3D globe adapter (native Rust WGPU/egui/Iced shell versus a Tauri/WebView adapter hosting CesiumJS for `GEO-01`/`SPACE-04`) remains an open proposal to be resolved in #16 and #23 against the `oia-app` service and state contracts established here.

## Alternatives Considered

- **Embed UI/renderer types directly in domain crates:** Rejected because rendering defaults (such as interpolated positions, display altitudes, or degraded visual quality) could leak into persisted evidence records, violating `AGENTS.md` Sections 2 and 4.
- **Select and bundle a heavy WebView + CesiumJS or WGPU stack in P0 before domain contracts exist:** Rejected for P0 because P0's exit criteria focus on the evidence foundation, transactional storage, bounded ingestion, durable jobs, and Lean correspondence without premature renderer lock-in.

## Validation

- Verified by compiling the 12-crate workspace (`cargo clippy --workspace --all-targets --locked -- -D warnings`) and exercising `oia-app` workspace shell navigation, accessibility toggles, quality-setting isolation, and localhost transport protection in `crates/oia-app`.
