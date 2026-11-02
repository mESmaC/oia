# Bounded exact-text passage search

Status: implemented in the temporary standalone `oia-documents` validation
package; not integrated into application services. This is the exact-text
matching slice of [#18](https://github.com/mESmaC/oia/issues/18), supporting
[CASE-05](../spec.md#31-investigation-workspace) and
[DOC-04/DOC-05](../spec.md#41-public-document-library). Verification covers
only the anchor/coverage/passive-text subset of
[A03](../delivery.md#a03-cited-document-research).

## Run locally

From the repository root with Rust 1.99.0 installed:

```text
cargo +1.99.0 run --manifest-path crates/oia-documents/Cargo.toml --example find-text --locked --offline -- fixtures/documents/a03/originals/report-v1.txt "two lights"
```

Arguments: `PATH QUERY [MAX_MATCHES] [MAX_INPUT_BYTES] [MAX_PASSAGES]`.
Defaults: 100 matches, 1,048,576 input bytes, and 10,000 passages. All budgets
must be positive; query length is limited to 4,096 UTF-8 bytes. Query validation
happens before file access. Empty queries and queries containing LF are errors.
The command reads at most the input budget plus one sentinel byte, rejects
oversized input without publishing matches, and leaves the file untouched.

The output prints escaped file/query/match text, exact original byte ranges,
one-based line numbers, budgets, and coverage. Exit codes: `0` for complete
search (including empty input or zero matches), `2` for partial coverage, and
`1` for invalid arguments, I/O, encoding, allocation, or stop failures. It is a
read-only example, not an evidence import or a supported application command.

To observe explicit match-budget truncation:

```text
cargo +1.99.0 run --manifest-path crates/oia-documents/Cargo.toml --example find-text --locked --offline -- fixtures/documents/a03/originals/report-v1.txt "fictional" 1
```

This intentionally returns exit code `2`. Hitting the budget stops scanning
immediately, so remaining text is conservatively unsearched even if it would
have contained no further matches. A match ending exactly at EOF can exhaust
the budget and still produce complete coverage.

## Kernel contract

`search::find_text(extraction, query, limits, check_control)` consumes a borrowed
[UTF-8 extraction](document-text.md). The extraction already bounds input bytes
and passage count; `SearchLimits` separately bounds query bytes and returned
match count. Match metadata uses fallible reservation. No new dependency,
workspace crate, source registry, job system, or runtime subprocess is added.

- Matching is exact, case-sensitive, non-overlapping UTF-8 substring matching
  within each LF-delimited passage, ordered by original byte offset. There is
  no case folding, Unicode normalization, stemming, regex syntax, tokenization,
  cross-line matching, or semantic retrieval. For example, `aa` in `aaaa`
  produces two matches, and composed `é` does not match `e` plus combining accent.
- Each match borrows the original text and carries a zero-based half-open byte
  range and one-based line number. BOMs, CRLF, controls, and non-ASCII prefixes
  retain their original widths. No source text is executed or interpreted as
  an action; the kernel has no filesystem, network, model, or publication path.
- `SearchCoverage::extraction()` preserves the original extraction coverage,
  including the exact unextracted range. `searched_bytes()` describes the
  contiguous prefix searched under the declared non-overlap/passage rule.
  `unsearched_extracted_range()` separately records text skipped after the
  match budget. Both gaps can exist simultaneously.
- `is_complete()` means the supplied byte buffer has been searched under those
  rules. It does not establish complete acquisition, OCR, corpus coverage,
  independent corroboration, source truth, or observed absence. Zero matches in
  a partial extraction remain partial, even if all available passages were
  searched. Zero-byte input is complete only for that empty supplied buffer.
- The callback checks control at entry, before each passage/match scan, and at
  the final complete/partial/empty return boundary. A reported stop produces
  `SearchError::Stopped` without usable results. A single substring scan is
  bounded by the extraction's input limit but is not preemptible; this is not
  a measured latency guarantee. The example shares a five-second cooperative
  processing deadline across extraction and search, starting after file I/O.
  File open/read/output are not deadline-controlled.

## Integration and remaining scope

This kernel implements only a matching primitive from the
[search and cited-QA contract](../contracts/document-search-and-cited-qa.md).
The existing package remains temporary; final module placement under the
proposed `oia-query` boundary requires coordination with the P0 owner after
shared contracts merge. Do not copy unmerged P0 code, invent identity envelopes,
or adopt this package's standalone lockfile as the workspace lockfile.

Application integration must authorize the selected scope before supplying
text, bind matches to retained artifact/source/extraction versions, preserve
origin/review/freshness state and missing pages, and check current job ownership
again before publication. A byte range alone is not a publishable citation.
Searching duplicate text does not combine source identities or count independent
corroboration. This kernel cannot enforce access isolation or atomic publication.

Still open on #18: exact-identifier/filter/token search, corpus ranking and
pagination, saved searches, semantic indexes, model adapters and evaluation,
grounded summaries/QA, correction propagation, and full A03 acceptance. There
is no resolution of D04's model selection or other open platform decisions,
no performance measurement, and no Lean proof claim.

## Local verification

```text
cargo +1.99.0 fmt --manifest-path crates/oia-documents/Cargo.toml --all -- --check
cargo +1.99.0 clippy --manifest-path crates/oia-documents/Cargo.toml --workspace --all-targets --locked --offline -- -D warnings
cargo +1.99.0 test --manifest-path crates/oia-documents/Cargo.toml --workspace --locked --offline
cargo +1.99.0 build --manifest-path crates/oia-documents/Cargo.toml --examples --locked --offline
python -I scripts/fixtures/verify_a03.py
```

Two targeted search tests protect critical anchor and coverage contracts: exact
borrowed A03 citation ranges (including untrusted text), Unicode offsets without
normalization, non-overlap ordering, both coverage gaps, empty/no-match results,
query validation, and stop rejection at every callback boundary. The four
existing extraction tests remain unchanged. Fixtures are synthetic and read
from their merged canonical paths; these tests do not establish live-model
containment or integrated application acceptance.

Executed on 2026-11-01 (America/New_York), Windows x86-64/MSVC, Rust/Cargo
1.99.0, against repository base `c8bc42f`: formatting, all-target Clippy with
warnings denied, all six package tests (four existing plus two search tests),
example builds, and the A03 integrity validator passed. Sixteen direct runs of
the built search command checked complete/no-hit/empty results, each coverage
gap and both together, input/query budgets, malformed UTF-8, invalid arguments,
missing/directory paths, and escaped controls. Original fixture and control-file
bytes remained unchanged. Documentation links/anchors/fences passed. Other
platforms, file-I/O deadlines, access isolation, and full A03 were not tested.
