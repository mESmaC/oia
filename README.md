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

OIA is at the project-definition stage. This repository currently records the vision and one standalone Rust package (see [Build and run](#build-and-run)); the OIA application, shared Rust workspace, and Lean proofs have not yet been added.

The version 0.1 specification draft consolidates the product scope and a proposed implementation plan:

- [Product specification](docs/spec.md): workflows, visual identity, and functional requirements across the suite.
- [Architecture and data contracts](docs/architecture.md): Rust services, evidence and time models, integrations, storage, and the Lean verification boundary.
- [Delivery and verification](docs/delivery.md): proposed milestones, acceptance scenarios, release gates, and open decisions.
- [GitHub implementation backlog](docs/backlog.md): the Project, phase epics, and linked work packages covering the specification.
- [Selective reuse inventory](docs/prior-art.md): implementation groundwork in God's Eye View that can inform the Rust implementation.

The [earlier exploration draft](docs/spec-draft.md) is retained as design history. The specification describes intended behavior; implementation choices and delivery sequencing remain proposals where identified.

Coding agents should read [AGENTS.md](AGENTS.md) before making changes.

## Build and run

There is no OIA application binary yet. The only runnable code is `inspect-text`, a read-only example that prints the UTF-8 passages, byte ranges, and coverage of a local file using the standalone [`crates/oia-documents`](crates/oia-documents/README.md) extraction kernel. It does not import, store, or detect file types, and it does not parse PDFs or run OCR.

**Requirements:** Rust 1.99.0 via [rustup](https://rustup.rs/) (`rustup toolchain install 1.99.0`). No network access is needed; the package has no third-party dependencies and builds with `--locked --offline`.

The run scripts wrap the package-local Cargo commands. Use `scripts/run.sh` on Linux, macOS, or Git Bash and `scripts\run.ps1` in Windows PowerShell:

| Task | POSIX shell | Windows PowerShell |
| --- | --- | --- |
| Build | `scripts/run.sh build` | `.\scripts\run.ps1 build` |
| Launch (inspects `README.md`) | `scripts/run.sh run` | `.\scripts\run.ps1 run` |
| Launch on a file | `scripts/run.sh run PATH [MAX_INPUT_BYTES] [MAX_PASSAGES]` | `.\scripts\run.ps1 run PATH [MAX_INPUT_BYTES] [MAX_PASSAGES]` |
| Format, lint, and test | `scripts/run.sh check` | `.\scripts\run.ps1 check` |

Defaults are 1,048,576 input bytes and 10,000 passages. Exit code `0` means complete or empty, `2` means explicitly partial (passage limit reached), and `1` means an error. Oversized input is refused rather than truncated. Set `OIA_RUST_TOOLCHAIN` to use a different installed toolchain. If PowerShell blocks local scripts, run `powershell -ExecutionPolicy Bypass -File scripts\run.ps1 run`.

The equivalent direct command is:

```text
cargo +1.99.0 run --manifest-path crates/oia-documents/Cargo.toml --example inspect-text --locked --offline -- README.md
```

See [document text extraction](docs/implementation/document-text.md) for the full contract, limitations, and verification record. These scripts will change when the shared Rust workspace and application land ([#9](https://github.com/mESmaC/oia/issues/9)).
