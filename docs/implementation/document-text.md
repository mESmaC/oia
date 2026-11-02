# Bounded UTF-8 document text extraction

Status: implemented as a standalone Rust package, not integrated into an OIA application. Work package: [#17](https://github.com/mESmaC/oia/issues/17). Requirement: [DOC-02](../spec.md#41-public-document-library), passage extraction only. Acceptance reference: [A03](../delivery.md#a03-cited-document-research), the preservation/coverage subset only.

## Scope and boundaries

`crates/oia-documents/` temporarily houses a standard-library-only extraction kernel and a read-only local inspection example. The package name/location is not an accepted workspace boundary; the kernel can move into the crate/module selected by the P0 owner (the proposed architecture assigns ingestion/extraction to `oia-ingest`). No architecture-map or P0 crate changes are made here. It works on caller-supplied UTF-8 bytes. It does not acquire sources, create cases, deduplicate releases, assign identities, store evidence, infer bibliographic metadata, or call tools based on document text.

This slice does not copy or depend on the unmerged P0 implementation. Integrated ingestion and persistence still depend on #9, #11, and #13. The merged A03 corpus is used in place, not copied into this package. D03 (seed corpus/providers) and D04 (AI/OCR execution) remain unresolved; this parser selects neither. The manifest is non-publishable and makes no licensing/distribution choice for D14.

The package-local Cargo manifest and generated lockfile allow local execution while the shared workspace is unavailable. They are not an alternative architecture or a production subprocess boundary. Once P0 merges, coordinate the final module location with its owner, integrate the kernel there, discard this package-local lockfile, and regenerate the root workspace lockfile with Cargo. This lockfile is not a reviewed workspace lockfile and should not be hand-merged into one. Do not copy guessed domain types or unmerged prerequisites to make it appear integrated.

## Run locally

From the repository root, using the installed Rust 1.99.0 toolchain:

```text
cargo +1.99.0 run --manifest-path crates/oia-documents/Cargo.toml --example inspect-text --locked --offline -- README.md
```

The example accepts `PATH [MAX_INPUT_BYTES] [MAX_PASSAGES]`. Defaults are 1,048,576 input bytes and 10,000 passages; both limits must be positive. It reads at most the byte limit plus one sentinel byte, refuses oversized input without returning an extraction, and never writes the inspected file. A bounded read of an oversized file does not claim to establish the file's full length. The extraction callback imposes a five-second cooperative deadline after file reading; this is not a timeout on file opening or I/O.

The report shows status, configured limits, extracted versus input bytes, any unprocessed byte interval, and each passage's line number, byte interval, and escaped text. Quoting escapes terminal control characters; source instructions remain text. Exit codes are `0` for complete/empty, `2` for explicitly partial extraction, and `1` for argument, I/O, encoding, allocation, or stop errors. A broken output pipe is an I/O error, not successful completion.

For example, cap the number of returned passages:

```text
cargo +1.99.0 run --manifest-path crates/oia-documents/Cargo.toml --example inspect-text --locked --offline -- README.md 1048576 2
```

That command intentionally returns partial coverage and exit code `2`; it does not lower a default silently. The CLI is an inspection example, not an import command, application service, or supported distribution entry point.

## Kernel contract

The public entry point is `extract_utf8(input, limits, check_control)`:

- `ExtractionLimits::new` rejects zero byte/passage budgets before work. An input slice exceeding its explicit byte budget fails rather than being truncated.
- The entire supplied slice must be valid UTF-8, including text beyond a passage-limited result. Malformed input reports its exact valid-prefix length and invalid-sequence length (or an incomplete trailing sequence). There is no lossy decoding or invented replacement text.
- Each passage borrows one LF-delimited line, including its original terminator. Byte ranges are zero-based, half-open offsets into the exact supplied buffer; line numbers are one-based. CRLF, a UTF-8 BOM, blank lines, whitespace, form feeds, embedded controls, and a final line without LF remain byte-identical. A final LF does not fabricate an extra empty passage. A form feed does not establish a page boundary.
- `Complete` means all supplied text bytes are represented by returned passages. `Empty` means the supplied slice has zero bytes, not that a source was absent or acquisition succeeded with no records. `PassageLimitReached` means explicitly partial extraction; coverage retains the total supplied byte length, extracted prefix length, remaining half-open interval, and requested limits.
- Passage metadata allocation uses fallible reservation. There are no external dependencies, renderer/provider types, source registries, or persistence side effects in the kernel.
- The caller supplies a cooperative callback returning `Cancelled` or `DeadlineExceeded`. The kernel checks it before parsing, before each passage, and immediately before returning a complete, empty, or partial result. A reported stop returns a typed error with no usable extraction. UTF-8 validation and a single-line scan are not preemptible; callback responsiveness is constrained by the configured input byte budget, not a measured real-time guarantee.

The callback is a parser hook, not a second job/cancellation system. Application integration must use the shared job state/deadline and recheck current operation ownership transactionally when publishing. A parser's last callback cannot prevent cancellation or supersession that happens after it returns.

## Evidence integration still required

Ranges alone are not publishable citations. The shared application/domain layer must bind each extraction to the exact retained artifact and source record/version, an immutable extraction identity/version, processing method/settings, ownership/access policy, provenance, and applicable times/review state. The parser intentionally does not invent substitutes for these contracts. Identical text passed twice does not establish identical semantic sources or releases.

Reprocessing must produce a new extraction version; correction propagation and citation reassessment remain domain/application work. `Complete` applies only to the supplied UTF-8 slice, not to an archive, collection, PDF, source's availability, independent corroboration, or truth. No inferred pages, OCR confidence, language, publication date, or source authenticity are reported.

Unsupported here: PDF/container decoding, OCR, images, non-UTF-8 encodings, network acquisition, retention permissions, source-version persistence, citation invalidation, search, summaries, UI, and multi-user access isolation. No provider access, hardware compatibility, performance measurement, Lean theorem, or Rust-to-Lean proof claim is established.

## Local verification

Run package-local checks, not a nonexistent root workspace:

```text
cargo +1.99.0 fmt --manifest-path crates/oia-documents/Cargo.toml --all -- --check
cargo +1.99.0 clippy --manifest-path crates/oia-documents/Cargo.toml --workspace --all-targets --locked --offline -- -D warnings
cargo +1.99.0 test --manifest-path crates/oia-documents/Cargo.toml --workspace --locked --offline
```

The three compact contract tests address specific DOC-02/A03 risks: original-byte and exact-anchor preservation (including borrowed slices); explicit partial/empty/malformed/oversized outcomes; and stopping at entry, between passages, or immediately before complete/partial return. They do not stand in for integrated A03 acceptance, storage isolation, real OCR, or live provider checks.

### Executed evidence

Executed locally on 2026-11-01 (America/New_York), Windows x86-64/MSVC, Rust/Cargo 1.99.0, on repository base `edb4958` plus this uncommitted extraction slice: the formatting check, all-target clippy with warnings denied, and all three contract tests passed. The package lockfile was generated by Cargo offline and contains no third-party packages. The inspection example built locally.

Direct execution of the built example passed complete, partial, empty, malformed UTF-8, oversized-input, zero/invalid/overflow budget, missing-argument, missing-file, and directory-input checks. Exit codes and original-byte preservation were checked; BOM, CRLF, and terminal-control escaping were inspected programmatically. Generated verification inputs live only under the ignored package target directory. Other operating systems, file-I/O deadlines, real document/provider integration, and full A03 acceptance were not tested.
