# ADR 0001: Deployment Model, Application Shell, and UI Boundary (D01)

- **Date:** 2026-11-01
- **Status:** Proposed (local-first Rust workspace and renderer-independent shell state contract in `#9`; graphical renderer comparison in `#16` / `ADR 0008`; awaiting project-owner confirmation)
- **Decision ID:** D01
- **Issues:** #9, #14, #16
- **Requirements:** `CASE-01`–`CASE-05`, `UX-01`–`UX-05`

## Context

`D01` requires establishing a local-first Rust core and resolving the boundary between application/domain logic and the user interface without coupling renderer or provider types into `oia-domain`.

## Decision

1. **Proposed for P0 (`#9` Local-First Rust Workspace & Renderer-Independent Shell Contract):**
   - OIA runs local-first as a single-process Rust workspace (`edition = "2024"`, pinned to `rustc 1.99.0` via `rust-toolchain.toml`, licensed `MIT OR Apache-2.0`) with zero mandatory cloud accounts for local case creation, document import, evidence inspection, search, or report/bundle export.
   - `oia-domain` defines core identifiers (`OpaqueId`, `VersionId`, `ActorId`, `SchemaVersion`) with `#![forbid(unsafe_code)]` and zero dependencies on UI toolkits, HTTP clients, database engines, or model SDKs. Versioned evidence records and provenance graphs follow in `#10`.
   - `oia-app` defines the renderer-independent workspace shell configuration and navigation state contract (`WorkspaceShellState`, `VisualSystemConfig`, `ReadingMode`, `WorkspaceViewKind`, `DataModeBadge`, `SemanticAccent`) alongside the localhost session-token and origin guard (`LocalhostTransportGuard`, `docs/architecture.md` §9). Durable jobs and command/query service wiring follow in `#14`.
   - `UX-01` through `UX-05` visual and accessibility invariants (dark military-glass palette tokens, visible keyboard focus by default, reduced motion/transparency toggles, clean-evidence and focused-reading modes, muted routine audio by default, non-color mode badges for `Live`, `Historical`, `Simulated`, and `Generated`, and independence of shared investigation context from visual quality settings) are enforced as typed state transitions in `oia-app`.
   - Validated initial development and runtime target: `x86_64-pc-windows-msvc` (with portable pure-Rust standard library code compatible with `x86_64-unknown-linux-gnu`).

2. **Open Proposal for Graphical Renderer (`#16` / `ADR 0008`, `#23`):**
   - The interactive graphical windowing and 3D globe adapter (native Rust WGPU/egui/iced/Slint shell versus a Tauri 2 rendering adapter) remains an open proposal evaluated in `#16` (`ADR 0008`) and `#23` for the project owner to decide. `D01` stays **Proposed** until the project owner resolves the renderer choice.

## Alternatives Considered

- **Embed UI/renderer types directly in `oia-domain`:** Rejected because rendering defaults (such as interpolated positions, display altitudes, or degraded visual quality) could leak into domain records, violating `AGENTS.md` Sections 2 and 4.
- **Settle the graphical renderer before domain/shell contracts exist:** Rejected for `#9` because the shell state contract is renderer-neutral and the comparative renderer spikes are evaluated separately in `#16` (`ADR 0008`).

## Validation

- Verified locally on Windows 11 (`rustc 1.99.0`) across the 15-crate workspace via `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`, and `cargo test --workspace --locked --offline` (including `oia-app` workspace shell navigation, local filter divergence, visual-quality context isolation, and localhost transport authentication tests).
