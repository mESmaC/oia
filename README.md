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

The desktop [News desk](docs/implementation/news-desk.md) supports bounded local JSON dossiers, article-version inspection, exact-text filtering and shared publication inventory and declared lineage inspection. It is session-only and unreviewed; live feeds and persistent news cases remain unimplemented. A clearly labeled synthetic example loads only when requested.

The repository currently includes the **#9 Rust workspace bootstrap**, platform decision records, and standalone document/privacy kernels:

- **Rust workspace** (`Cargo.toml`, `rustc 1.99.0`, `edition = "2024"`, `#![forbid(unsafe_code)]`, dual-licensed `MIT OR Apache-2.0`):
  - Implemented packages: `oia-documents` (bounded UTF-8 extraction, exact-text passage and corpus search, and structural citation checks), `oia-png-privacy` (bounded PNG metadata inspection and cleanup), `oia-jpeg-privacy` (bounded JPEG container metadata inspection and cleanup), `oia-domain` (validated identifiers and schema version primitives), and `oia-app` (Tauri desktop workbench for real local text, corpus search and citation, image privacy and still-image forensic operations, plus a [Globe instrument](docs/implementation/globe-instrument.md) over a bundled synthetic case with live NASA GIBS imagery that sharpens with zoom from registered Sentinel-2 and USGS imagery, and a registered ALPR camera layer; persistent case services remain in `#14`/`#16`).
  - `oia-forecast` provides a standalone [binary scoring kernel](docs/implementation/forecast-scoring.md); forecast issuance, chronological validation, persistence, calibration, and application integration remain open in `#43`.
  - `oia-market` provides standalone [point-in-time market normalization](docs/implementation/market-point-in-time.md); provider, storage, application, and full A14 integration remain open in `#41`.
  - `oia-market` also provides a standalone [daily event-study kernel](docs/implementation/market-event-study.md) with explicit windows, benchmarks, and gaps. Charts, exposure graphs, briefs, and application integration remain open in `#42`.
  - `oia-news-claims` provides a standalone [`NEWS-02` regional-account kernel](docs/implementation/news-regional-accounts.md) with typed epistemic categories, attributed-only purposes, separate source counts, and ordered legal stages. News acquisition, clustering, persistence, UI, and integrated `A06`/`A19` remain open in `#20`.
  - `oia-time-geo` owns [shared WGS84 point/box, altitude-category and UTC interval primitives](docs/implementation/shared-time-geo.md), used by geographic context and imagery kernels; UAV telemetry shares its altitude-reference enum. Timezone/scientific-time/clock and height-conversion work remains in #12.
  - Workspace boundary stubs awaiting split P0–P6 implementation PRs (`#10`–`#15` and subsequent phases): `oia-store`, `oia-ingest`, `oia-query`, `oia-analysis`, `oia-models`, `oia-media`, `oia-devices`, and `oia-export`.
- **Lean model proofs** ([manifest and commands](proofs/README.md)): frame selection, dependency propagation, and forecast cutoff properties; a shared 24-case fixture tests the merged Rust forecast cutoff path. Model proofs and finite Rust correspondence evidence are distinct.
- **Architecture Decision Records (`docs/adr/`)**:
  - [ADR 0001 (D01): Deployment and UI boundary (Accepted)](docs/adr/0001-d01-deployment-and-ui-boundary.md)
  - [ADR 0002 (D02): Storage and indexing (Accepted: embedded SQLite + managed blobs)](docs/adr/0002-d02-storage-and-indexing.md)
  - [ADR 0003 (D14): Project licensing and distribution notices (Accepted: `MIT OR Apache-2.0`)](docs/adr/0003-d14-licensing-and-distribution.md)
  - [ADR 0004 (D13): Lean targets and Rust correspondence](docs/adr/0004-d13-lean-correspondence-target.md)
  - [ADR 0005 (D03): Seed corpus and news sources](docs/adr/0005-d03-seed-corpus-and-news-sources.md)
  - [ADR 0006 (D05): Report and bundle formats](docs/adr/0006-d05-report-and-bundle-formats.md)
  - [ADR 0007 (D04): AI execution and grounding](docs/adr/0007-d04-ai-execution-and-grounding.md)

The remaining P0 runtime packages (`#10`–`#14`) and Phases P1–P6 remain open work packages in the [GitHub implementation backlog](docs/backlog.md):

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
- Choose **Exact substring** or **Whole token (whitespace)** in Documents for either search scope. Both are case-sensitive; whole-token mode uses whitespace or line edges, keeps punctuation literal, and accepts a single token. Results state which mode ran.
- Open a corpus (Ctrl+Shift+O): select up to 20 UTF-8 files (8 MiB in total) and search them together under one shared 200-match limit. Results are grouped by file in selection order, each with its searched, unsearched and unextracted coverage; selecting a match opens that held file in the reader at the matching line. Add a citation from a selected match, or paste a byte range and quote to check it against the open document: the `oia-documents` citation kernel reports an exact structural match or the reason it fails. Citation records are session-only anchors into retained bytes, not source-version citations.
- Inspect a PNG/JPEG, select supported metadata categories, and save a reinspected cleaned copy without overwriting the original or any existing destination.
- Open the Globe (Ctrl+G): a CesiumJS globe over the bundled synthetic valley case, with tags and leaders to recorded positions, uncertainty rings, a dotted region, a not-plotted tray, and live basemap tiles from NASA GIBS (Blue Marble composite, or VIIRS true colour for a chosen UTC day). Zooming in draws sharper real imagery: EOX Sentinel-2 cloudless 2025 (10 m, global), then USGS orthoimagery (about 1 m, conterminous US) and dynamic USGS NAIP natural colour (primarily 0.6 m aerial photography, imagery level 18). The Imagery inspector defaults to a neutral display without atmosphere/fog and provides copyable provider addresses for original scientific products; these downloads remain external to OIA. The strip names the source drawn on top, with attribution, capture statement and tile counts. Arrow keys pan, plus and minus zoom, Home returns to the case anchor. The Layers panel lists the registered context layers; the first is the `CAM-06` mapped ALPR camera directory over a synthetic fixture, evaluated by the `oia-alpr-map` kernel, drawn as camera points with illustrative bearing wedges only where a bearing is tagged, with coverage, vendor breakdown and local filters.
- Open Media (Ctrl+M): with an image open, run the `oia-image-forensics` cue profile on the same retained bytes and read the result over the image: repeated-block candidates as linked bracketed regions, RGB histograms of the decoded samples, metadata cues, per-method coverage and the kernel's limitations. The assessment is always inconclusive and unreviewed; cues are diagnostics, not a verdict. For JPEG, save a labeled recompression residual PNG from the Media inspector; source bytes stay unchanged and JPEG coverage stays partial.
- Capture one session-only witness account with exact optional reported fields, an immutable initial version, rationale-required revisions, and explicit exposure gating for unassessed static explanation prompts. The memory-only workflow is discarded on exit; durable evidence capture, sky sketches, and regional/event linkage are not implemented.
- Build an illustrative recreation prompt from a retained version of that account with the `oia-media` recreation kernel: reported wording is pinned to the witness version verbatim, unreported fields stay explicit unknowns unless tied to an assumption you declare, and the labeled portable text can be saved as a `.txt` without overwriting anything. It is an illustration brief, never evidence; no image is generated.
- Use keyboard navigation (Ctrl+O document, Ctrl+I image, Ctrl+F search, F6 regions), the Focus and Effects toggles, and the [visual language v2](docs/design/visual-language-v2.md) shell. Coverage, unsupported profiles and failures remain visible in the readout strip, inspector, and jobs strip.

Files and the witness account are held in memory for this session. Persistent investigations, PDF/OCR, AI summaries and report integration are not yet implemented. The only remote origins the bundled UI contacts are the four registered imagery origins shown in the Globe inspector ([ADR 0008 §10](docs/adr/0008-d01-workspace-renderer.md#10-amendment-registered-online-imagery-in-the-webview-2026-11-03)); Windows WebView2 may make its own background connections. See the [workbench contract, limits and validation](docs/implementation/desktop-workbench.md) and the [Globe instrument note](docs/implementation/globe-instrument.md).

## Build and run

For an unsigned local Windows development ZIP, see the [packaging instructions](docs/implementation/windows-package.md). Public release gates, installers, signing and updates remain open under #49.

The workspace provides the `oia` desktop application in [`crates/oia-app`](crates/oia-app/src/main.rs) as well as read-only inspection and cleanup examples in [`crates/oia-documents`](crates/oia-documents/README.md), [`crates/oia-png-privacy`](crates/oia-png-privacy/README.md), and [`crates/oia-jpeg-privacy`](crates/oia-jpeg-privacy/README.md):

- `inspect-text` prints the UTF-8 passages, byte ranges, and coverage of a local file.
- `find-text` searches a local UTF-8 file for an exact query and prints each match's byte range, line, and search coverage.
- [Still-image forensic cues](crates/oia-image-forensics/README.md): `cargo run -p oia-image-forensics --locked --offline -- IMAGE` reports PNG/JPEG metadata signatures, RGB histograms, repeated-block candidates, and JPEG residuals; not an AI/authenticity verdict.

Neither `oia-documents` example imports, stores, or detects file types, parses PDFs, or runs OCR.

**Requirements:** Rust 1.99.0 via [rustup](https://rustup.rs/) (`rustup toolchain install 1.99.0`). Run `scripts/run.sh fetch` (or `.\scripts\run.ps1 fetch`) once to download the locked dependencies, including the desktop ones, and the pinned CesiumJS build for the Globe (verified by SHA-512 into the gitignored `crates/oia-app/ui/vendor/`); every other script command then runs without network access (`--locked --offline`). Build the desktop app after `fetch`, since the vendor directory is embedded at build time; without it the Globe instrument reports that the renderer is not present and everything else works. Windows requires the C++ build tools and WebView2 runtime. The desktop app has been checked on Windows only; Linux/macOS need Tauri platform prerequisites and remain unverified. Headless kernel checks can use `--no-default-features`. Without rustup, the scripts warn and use whatever `cargo` is on `PATH`; only the pinned 1.99.0 toolchain has been checked.

The run scripts wrap the root workspace Cargo commands and the [pinned Lean 4.35.0 proof gate](proofs/README.md). Install elan and `leanprover/lean4:v4.35.0` for `proofs` or `check`; full validation fails if Lean is unavailable. Production builds do not require Lean. Use `scripts/run.sh` on Linux, macOS, or Git Bash and `scripts\run.ps1` in Windows PowerShell:

| Task | POSIX shell | Windows PowerShell |
| --- | --- | --- |
| Fetch locked dependencies (online, once) | `scripts/run.sh fetch` | `.\scripts\run.ps1 fetch` |
| Build workspace | `scripts/run.sh build` | `.\scripts\run.ps1 build` |
| Launch desktop | `scripts/run.sh run` | `.\scripts\run.ps1 run` |
| Inspect `README.md` | `scripts/run.sh inspect` | `.\scripts\run.ps1 inspect` |
| Inspect a file | `scripts/run.sh run PATH [MAX_INPUT_BYTES] [MAX_PASSAGES]` | `.\scripts\run.ps1 run PATH [MAX_INPUT_BYTES] [MAX_PASSAGES]` |
| Search a file | `scripts/run.sh search PATH QUERY [MAX_MATCHES] [MAX_INPUT_BYTES] [MAX_PASSAGES]` | `.\scripts\run.ps1 search PATH QUERY [MAX_MATCHES] [MAX_INPUT_BYTES] [MAX_PASSAGES]` |
| Check Lean proofs and fixture synchronization | `scripts/run.sh proofs` | `.\scripts\run.ps1 proofs` |
| Check proofs, format, lint, and test | `scripts/run.sh check` | `.\scripts\run.ps1 check` |

For the text examples (`inspect`, legacy `run PATH`, and `search`), defaults are 1,048,576 input bytes, 10,000 passages, and (for search) 100 matches. Exit code `0` means complete (including empty input or zero matches), `2` means explicitly partial because a budget was reached, and `1` means an error. Oversized input is refused rather than truncated. A complete search with zero matches covers only the file that was read; it is not evidence that the text is absent from the source. Set `OIA_RUST_TOOLCHAIN` to use a different installed toolchain. If PowerShell blocks local scripts, run `powershell -ExecutionPolicy Bypass -File scripts\run.ps1 run`.

The equivalent direct commands are:

```text
cargo +1.99.0 run -p oia-documents --example inspect-text --locked --offline -- README.md
cargo +1.99.0 run -p oia-documents --example find-text --locked --offline -- fixtures/documents/a03/originals/report-v1.txt "two lights"
cargo +1.99.0 run -p oia-app --bin oia --locked
```

See [document text extraction](docs/implementation/document-text.md), [document text search](docs/implementation/document-search.md), [PNG privacy](docs/implementation/png-privacy.md), and [JPEG privacy](docs/implementation/jpeg-privacy.md) for the full contracts, limitations, and verification records.
