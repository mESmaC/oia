# Bounded exact-text passage search

Status: implemented in the workspace's `oia-documents` validation
package; not integrated into application services. This is the exact-text
matching slice of [#18](https://github.com/mESmaC/oia/issues/18), supporting
[CASE-05](../spec.md#31-investigation-workspace) and
[DOC-04/DOC-05](../spec.md#41-public-document-library). Verification covers
only the anchor/coverage/passive-text subset of
[A03](../delivery.md#a03-cited-document-research).

For an explicit list of documents with shared byte/hit limits and per-input
coverage, see [bounded multi-document search](document-corpus-search.md).

## Run locally

From the repository root with Rust 1.99.0 installed:

```text
cargo +1.99.0 run --manifest-path crates/oia-documents/Cargo.toml --example find-text --locked --offline -- fixtures/documents/a03/originals/report-v1.txt "two lights"
```

Arguments: `[--token | --] PATH QUERY [MAX_MATCHES] [MAX_INPUT_BYTES] [MAX_PASSAGES]`.
Use `--` before a path named `--token` or `--`. Substring search remains the default.
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

### Exact whitespace-token mode

`search::find_token` and the example's `--token` flag match one complete token
bounded by Unicode whitespace (`char::is_whitespace`) or passage edges. For example:

```text
cargo +1.99.0 run --manifest-path crates/oia-documents/Cargo.toml --example find-text --locked --offline -- --token fixtures/documents/a03/originals/report-v1.txt "lights."
```

This returns the original `lights.` bytes at `121..128`. Punctuation is literal:
`lights` does not match `lights.`. Tabs, CR/LF and non-breaking spaces delimit
tokens; BOMs, zero-width characters and combining marks remain token content.
This is an explicit deterministic matching option, not linguistic word
segmentation. Case folding, normalization and stemming are unsupported.
Queries containing any whitespace are rejected before file access (LF retains
the existing `MultilineQuery` error; other whitespace uses `WhitespaceInToken`).

`SearchResult::mode()` records `ExactSubstring` or `WhitespaceToken`, and the CLI
prints the selected matching rule. Only accepted whole-token hits consume the
match budget. Rejected substring candidates still check cancellation before
the next scan. Both modes share the coverage and control rules below; complete
coverage means evaluation under the selected rule, not observed absence.

### Shared limits and anchors

`search::find_text(extraction, query, limits, check_control)` consumes a borrowed
[UTF-8 extraction](document-text.md). The extraction already bounds input bytes
and passage count; `SearchLimits` separately bounds query bytes and returned
match count. Match metadata uses fallible reservation. No new dependency,
workspace crate, source registry, job system, or runtime subprocess is added.

- `find_text` uses exact, case-sensitive, non-overlapping UTF-8 substring matching
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

## Resumable in-memory pages

`SearchSession::new(extraction, query, limits, mode)` pins immutable borrowed
inputs for its lifetime. `next_page(check_control)` returns only the next
page's hits, ordered by original byte offset, with **cumulative** coverage from
session creation. `max_matches` bounds each page. The private cursor cannot be
supplied by a caller or reused with different text, query, or matching rules.
A successful page advances the cursor; allocation or control failures leave it
unchanged, so a retry cannot skip unpublished hits. Previously returned pages
remain available if a later page fails. Callers must bound total pages and
provide cancellation/deadline checks.

A full page stops immediately after its last match. An additional empty page
may be needed to establish that the remaining text has no matches. `None`
means all **extracted** text has been searched; it does not erase an unextracted
range or establish complete acquisition. The first call returns a coverage
page even for empty input. This is an in-process primitive, not the versioned
service cursor from the corpus search contract. No cursor serialization,
source identity, authorization, or cross-document ranking is implemented.

The read-only example holds one immutable file buffer for the whole session:

```text
cargo run -p oia-documents --example find-text-pages --locked --offline -- fixtures/documents/a03/originals/report-v1.txt fictional 1 10
```

Arguments: `[--token | --] PATH QUERY [PAGE_SIZE] [MAX_PAGES] [MAX_INPUT_BYTES]
[MAX_PASSAGES]`. Defaults are 100 hits/page, 10 pages, 1,048,576 input bytes,
10,000 passages, and a 4,096-byte query limit. All limits are positive. Input
and query validation, escaping, and matching rules follow `find-text` above.
The command prints each page's hits and cumulative gaps, plus a final session
status. Exit 0 means complete buffer search; 2 means the page budget or
extraction budget leaves a gap; 1 means an error. Earlier successful pages may
already have been printed when a later call fails. One five-second cooperative
processing deadline spans extraction and every page; blocking I/O is not
interruptible. No timing guarantee or scientific proof is claimed.

## Integration and remaining scope

This kernel implements only a matching primitive from the
[search and cited-QA contract](../contracts/document-search-and-cited-qa.md).
The package is now a workspace member using the root lockfile. Final module
placement under the proposed `oia-query` boundary requires coordination with
the P0 owner after shared contracts merge. Do not copy unmerged P0 code or
invent identity envelopes.

Application integration must authorize the selected scope before supplying
text, bind matches to retained artifact/source/extraction versions, preserve
origin/review/freshness state and missing pages, and check current job ownership
again before publication. A byte range alone is not a publishable citation.
Searching duplicate text does not combine source identities or count independent
corroboration. This kernel cannot enforce access isolation or atomic publication.

Still open on #18: exact-identifier/filter search, linguistic tokenization, corpus ranking and
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

Token-mode verification on 2026-11-02, Windows x86-64/MSVC, Rust 1.99.0,
base `24bbb4f`: package fmt, all-target Clippy with warnings denied, all eight
tests, and example builds passed with the commands above. The two additional
tests protect original token anchors, literal punctuation/Unicode boundaries,
accepted-hit budgets, both coverage gaps and cancellation at every callback.
Six direct CLI checks passed: A03 token anchor, punctuation-sensitive zero hits,
match budget, extraction budget, invalid token rejected before file access,
and default substring mode with `--`. The original fixture SHA-256 was unchanged.
Both A03 integrity and QA fixture validators (`verify_a03_qa.py --self-test`)
passed. These fixture checks do not establish integrated QA or model behavior.

Pagination verification on 2026-11-02, Windows x86-64/MSVC, Rust 1.99.0,
base `86dabac`: workspace `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets --locked -- -D warnings`, and
`cargo test --workspace --locked` passed. Example builds with
`cargo build -p oia-documents --examples --locked --offline` and both A03
validators above passed. Two focused assertions groups cover paged/one-shot
hit equivalence, original Unicode anchors, extraction gaps, empty terminal
pages, and retry after every second-page control boundary. Six direct CLI
checks cover complete pagination, page budget, token anchor, extraction gap,
zero page budget, and invalid token rejection before file access. The original
fixture hash was unchanged. Relative documentation targets/fences and staged
whitespace checks passed. No hosted CI, full A03 acceptance, cross-platform,
model, performance, or Lean verification was run for this slice.
