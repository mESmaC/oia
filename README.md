<img src="assets/icon.svg" width="64" height="64" alt="">

# OIA — Open Intelligence Agency

OIA is a research project and a suite of intelligence tools built around capabilities generally available to the public. It aims to help people understand what is happening around them, look out for themselves, and make informed decisions about their privacy.

As the open-source ecosystem grows, powerful tools and information sources become accessible to more people. OIA explores what those capabilities make possible, brings them together in approachable tools, and makes their implications easier to understand.

## Purpose

- **Public awareness:** Help people discover and understand publicly available information relevant to their surroundings.
- **Individual privacy:** Demonstrate what information and capabilities are already accessible, so people can better understand and reduce their own exposure.
- **Research and innovation:** Explore public intelligence capabilities, document their strengths and limitations, and encourage improvements through open-source collaboration.

The research is part of the product: OIA should explain where information comes from, what a tool can establish, and where uncertainty remains. Public availability alone does not establish accuracy, and conclusions should remain distinguishable from observations.

## Technical direction

OIA will be implemented in **Rust**, with **Lean** used to formally specify and prove selected algorithmic properties. The intent is to use those proofs to validate reasoning and guide improvements to the Rust implementations.

Lean is a development and research tool; it does not necessarily need to be included in the distributed Rust binaries.

A proof about a Lean model does not automatically prove that a corresponding Rust implementation is correct. For algorithms supported by proofs, the project should document:

- The specification, assumptions, and properties proved in Lean.
- How the Rust implementation corresponds to that specification.
- The tests or other checks that support that correspondence, and any remaining gaps.

Verification claims will be limited to the properties and implementation connections actually established. Algorithmic proofs do not establish the accuracy of external data.

## Current status

The repository currently includes the **#9 Rust workspace bootstrap**, platform decision records, and standalone document/privacy kernels:

- **Rust workspace** (`Cargo.toml`, `rustc 1.99.0`, `edition = "2024"`, `#![forbid(unsafe_code)]`, dual-licensed `MIT OR Apache-2.0`):
  - Implemented packages: `oia-documents` (bounded UTF-8 extraction and exact-text passage search), `oia-png-privacy` (bounded PNG metadata inspection and cleanup), `oia-jpeg-privacy` (bounded JPEG container metadata inspection and cleanup), `oia-domain` (validated identifiers and schema version primitives), and `oia-app` (Tauri desktop workbench for real local text and image privacy operations; persistent case services remain in `#14`/`#16`).
  - `oia-forecast` provides a standalone [binary scoring kernel](docs/implementation/forecast-scoring.md); forecast issuance, chronological validation, persistence, calibration, and application integration remain open in `#43`.
  - `oia-market` provides standalone [point-in-time market normalization](docs/implementation/market-point-in-time.md); provider, storage, application, and full A14 integration remain open in `#41`.
  - Workspace boundary stubs awaiting split P0–P6 implementation PRs (`#10`–`#15` and subsequent phases): `oia-time-geo`, `oia-store`, `oia-ingest`, `oia-query`, `oia-analysis`, `oia-models`, `oia-media`, `oia-devices`, and `oia-export`.
- **Architecture Decision Records (`docs/adr/`)**:
  - [ADR 0001 (D01): Deployment and UI boundary (Accepted)](docs/adr/0001-d01-deployment-and-ui-boundary.md)
  - [ADR 0002 (D02): Storage and indexing (Accepted: embedded SQLite + managed blobs)](docs/adr/0002-d02-storage-and-indexing.md)
  - [ADR 0003 (D14): Project licensing and distribution notices (Accepted: `MIT OR Apache-2.0`)](docs/adr/0003-d14-licensing-and-distribution.md)
  - [ADR 0005 (D03): Seed corpus and news sources](docs/adr/0005-d03-seed-corpus-and-news-sources.md)
  - [ADR 0006 (D05): Report and bundle formats](docs/adr/0006-d05-report-and-bundle-formats.md)
  - [ADR 0007 (D04): AI execution and grounding](docs/adr/0007-d04-ai-execution-and-grounding.md)

The Lean 4 formal verification package (`proofs/`, `#15`), remaining P0 runtime packages (`#10`–`#14`), and Phases P1–P6 remain open work packages in the [GitHub implementation backlog](docs/backlog.md):

- [Product specification](docs/spec.md): workflows, visual identity, and functional requirements across the suite.
- [Architecture and data contracts](docs/architecture.md): Rust services, evidence and time models, integrations, storage, and the Lean verification boundary.
- [Delivery and verification](docs/delivery.md): proposed milestones, acceptance scenarios, release gates, and decision register.
- [GitHub implementation backlog](docs/backlog.md): the Project, phase epics, and linked work packages covering the specification.
- [Selective reuse inventory](docs/prior-art.md): implementation groundwork in God's Eye View that can inform the Rust implementation.
- [Third-party notices](THIRD_PARTY_NOTICES.md) and code licenses ([`LICENSE-MIT`](LICENSE-MIT), [`LICENSE-APACHE`](LICENSE-APACHE)).

The [earlier exploration draft](docs/spec-draft.md) is retained as design history.

Coding agents should read [AGENTS.md](AGENTS.md) before making changes.

## Run the desktop app

From the repository root, run `cargo run -p oia-app --locked` or `.\scripts\run.ps1 run` on Windows (`scripts/run.sh run` on POSIX shells). With no arguments, `oia` opens the Tauri desktop workbench.

- Open a UTF-8 document, read passages, search exact text, and jump to the matching source line/byte range.
- Inspect a PNG/JPEG, select supported metadata categories, and save a reinspected cleaned copy without overwriting the original or any existing destination.
- Use keyboard navigation (Ctrl+O document, Ctrl+I image, Ctrl+F search, F6 regions), the Focus and Effects toggles, and the [visual language v2](docs/design/visual-language-v2.md) shell. Coverage, unsupported profiles and failures remain visible in the readout strip, inspector, and jobs strip.

Files are held in memory for this session. Persistent investigations, PDF/OCR, live sources, AI summaries and report integration are not yet implemented. Windows WebView2 may make its own background connections; the bundled UI and local tools do not fetch remote content. See the [workbench contract, limits and validation](docs/implementation/desktop-workbench.md).

## Build and run

The workspace provides the `oia` desktop application in [`crates/oia-app`](crates/oia-app/src/main.rs) as well as read-only inspection and cleanup examples in [`crates/oia-documents`](crates/oia-documents/README.md), [`crates/oia-png-privacy`](crates/oia-png-privacy/README.md), and [`crates/oia-jpeg-privacy`](crates/oia-jpeg-privacy/README.md):

- `inspect-text` prints the UTF-8 passages, byte ranges, and coverage of a local file.
- `find-text` searches a local UTF-8 file for an exact query and prints each match's byte range, line, and search coverage.
- [Still-image forensic cues](crates/oia-image-forensics/README.md): `cargo run -p oia-image-forensics --locked --offline -- IMAGE` reports PNG/JPEG metadata signatures, RGB histograms, repeated-block candidates, and JPEG residuals; not an AI/authenticity verdict.

Neither `oia-documents` example imports, stores, or detects file types, parses PDFs, or runs OCR.

**Requirements:** Rust 1.99.0 via [rustup](https://rustup.rs/) (`rustup toolchain install 1.99.0`). Run `scripts/run.sh fetch` (or `.\scripts\run.ps1 fetch`) once to download the locked dependencies, including the desktop ones; every other script command then runs without network access (`--locked --offline`). Windows requires the C++ build tools and WebView2 runtime. The desktop app has been checked on Windows only; Linux/macOS need Tauri platform prerequisites and remain unverified. Headless kernel checks can use `--no-default-features`. Without rustup, the scripts warn and use whatever `cargo` is on `PATH`; only the pinned 1.99.0 toolchain has been checked.

The run scripts wrap the root workspace Cargo commands (and `lake build` in `proofs/` once `#15` lands and `lake` is installed). Use `scripts/run.sh` on Linux, macOS, or Git Bash and `scripts\run.ps1` in Windows PowerShell:

| Task | POSIX shell | Windows PowerShell |
| --- | --- | --- |
| Fetch locked dependencies (online, once) | `scripts/run.sh fetch` | `.\scripts\run.ps1 fetch` |
| Build workspace | `scripts/run.sh build` | `.\scripts\run.ps1 build` |
| Launch desktop | `scripts/run.sh run` | `.\scripts\run.ps1 run` |
| Inspect `README.md` | `scripts/run.sh inspect` | `.\scripts\run.ps1 inspect` |
| Inspect a file | `scripts/run.sh run PATH [MAX_INPUT_BYTES] [MAX_PASSAGES]` | `.\scripts\run.ps1 run PATH [MAX_INPUT_BYTES] [MAX_PASSAGES]` |
| Search a file | `scripts/run.sh search PATH QUERY [MAX_MATCHES] [MAX_INPUT_BYTES] [MAX_PASSAGES]` | `.\scripts\run.ps1 search PATH QUERY [MAX_MATCHES] [MAX_INPUT_BYTES] [MAX_PASSAGES]` |
| Format, lint, and test | `scripts/run.sh check` | `.\scripts\run.ps1 check` |

For the text examples (`inspect`, legacy `run PATH`, and `search`), defaults are 1,048,576 input bytes, 10,000 passages, and (for search) 100 matches. Exit code `0` means complete (including empty input or zero matches), `2` means explicitly partial because a budget was reached, and `1` means an error. Oversized input is refused rather than truncated. A complete search with zero matches covers only the file that was read; it is not evidence that the text is absent from the source. Set `OIA_RUST_TOOLCHAIN` to use a different installed toolchain. If PowerShell blocks local scripts, run `powershell -ExecutionPolicy Bypass -File scripts\run.ps1 run`.

The equivalent direct commands are:

```text
cargo +1.99.0 run -p oia-documents --example inspect-text --locked --offline -- README.md
cargo +1.99.0 run -p oia-documents --example find-text --locked --offline -- fixtures/documents/a03/originals/report-v1.txt "two lights"
cargo +1.99.0 run -p oia-app --bin oia --locked
```

See [document text extraction](docs/implementation/document-text.md), [document text search](docs/implementation/document-search.md), [PNG privacy](docs/implementation/png-privacy.md), and [JPEG privacy](docs/implementation/jpeg-privacy.md) for the full contracts, limitations, and verification records.
