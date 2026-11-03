# Bounded multi-document text search

Implemented slice of [#18](https://github.com/mESmaC/oia/issues/18):
CASE-05/DOC-04 matching across an explicit selected input list, with DOC-05
passive source handling. This composes the merged [passage matcher](document-search.md)
in `oia-documents`; it is not the application search service or full A03 acceptance.
Final integration belongs to `oia-query` and the shared P0 contracts.

## Library contract

[`corpus::search_corpus`](../../crates/oia-documents/src/corpus.rs) takes an ordered
list of borrowed extractions or explicit unavailable entries. Unavailable reasons
are `OcrRequired`, `Unreadable`, `Missing`, and `ReferenceOnly`, supplied by the
caller. The caller must first authorize and select the input list. This kernel
does not implement workspace authorization, source/version identity, domain
filters, or a source registry.

Each result identifies its zero-based input slot and preserves original byte
anchors and line numbers from the passage matcher. Results follow input order,
then byte order; there is no relevance ranking. Identical bytes in two slots
produce separate results. This neither merges their identities nor establishes
independent corroboration. Input slots and local filenames alone are not valid
source-version citations; the integrating service must retain the mapping to
authorized immutable versions and artifacts.

Exact-substring and Unicode-whitespace-token modes preserve their existing
case-sensitive, non-overlapping, within-passage semantics. No match crosses a
document boundary. There is no normalization, OCR, semantic retrieval or model
execution. Source text is never executed, fetched or interpreted as instructions.

The request has positive limits on document count, total known original input
bytes, query bytes and total hits. Document count includes unavailable entries.
The byte sum includes the unextracted suffix of each supplied buffer; duplicates
count separately. Sizes of unavailable documents remain unknown. Count/byte
overflow rejects the entire request before matching, rather than silently
choosing a subset. The global hit limit is shared across documents.

Every supplied input has a result entry even when the hit limit is reached:

- `Searched` preserves both the search coverage and original extraction coverage.
  Reaching a hit limit inside a file can leave an unsearched extracted suffix.
- `MatchLimitReached` means the input was not searched at all. Its extraction
  coverage still identifies any independently unextracted suffix.
- `Unavailable` preserves the caller's reason; it is never an empty result.

Empty buffers can be completely searched even after the hit cap. An empty input
list is complete only for that empty selection. Overall completeness requires
every input to have been fully extracted and searched; zero hits do not establish
absence outside that scope. No cursor or resumption is provided by this slice.
The existing single-document `SearchSession` remains available separately.

Allocation failures and cancellation/deadline checks return typed errors and no
partial result. Checks run at entry, per input validation/search, through the
existing passage matcher, and at the final return boundary. Control is cooperative:
a single substring scan is not preemptible. Admission bounds input/work size,
not elapsed time. The application must still fence late job publication.

## Read-only local command

From the repository root, with pinned Rust 1.99.0:

```text
cargo run -p oia-documents --example find-corpus --locked --offline -- "two lights" 100 1048576 10000 fixtures/documents/a03/originals/report-v1.txt fixtures/documents/a03/originals/report-v2.txt
cargo run -p oia-documents --example find-corpus --locked --offline -- --token observer 1 1048576 10000 fixtures/documents/a03/originals/report-v1.txt fixtures/documents/a03/originals/report-v2.txt
```

Syntax: `find-corpus [--token | --] QUERY MAX_MATCHES MAX_TOTAL_BYTES MAX_PASSAGES_PER_FILE PATH...`.
Use `--` before a literal query named `--token` or `--`. At most 100 explicit
regular local files are accepted, with a 4096-byte query cap. Paths after the
numeric limits are literal; there is no recursive traversal or network fetch.
The total byte budget is shared during reading, not applied independently to
each file. At most one extra byte is read to detect overflow. Buffers grow in
bounded chunks rather than reserving the full corpus budget for every file.

Output names each input slot/path, matching mode, limits, hit count, original
byte ranges and separate coverage gaps. Source text, queries and paths use debug
escaping for terminal controls. Exit codes: `0` complete, `2` partial, `1` error.
All files must open, read and decode successfully; an I/O/malformed UTF-8/size
error rejects the CLI request without emitting search results. The library's
explicit unavailable inputs support callers that already have such coverage
records. The CLI does not invent them from a failed local read.

The five-second deadline is checked around file reads and during extraction and
matching; a blocking filesystem call cannot be preempted. No input is modified,
imported, persisted or published. Files are searched as bytes read during this
invocation, not as immutable snapshots of a changing filesystem.

## Verification and remaining scope

Two focused contract tests cover the critical risks introduced by composition:
the synthetic A03 original retains separate slots and exact anchors when reused,
missing/OCR/unextracted/unsearched coverage stays visible, total admission limits
apply across files, and cancellation/deadline failure at every callback boundary
returns no corpus result. The Unicode token check also exercises both input slots.
These are synthetic contract checks, not real source or model-quality evaluation.

Executed locally on 2026-11-03 UTC (2026-11-02 America/New_York), Windows
x86-64/MSVC, Rust/Cargo 1.99.0, task branch `codex/corpus-search` based on
`67ca449` (the delivered revision is recorded in the PR):

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo test --workspace --locked --offline
cargo build -p oia-documents --example find-corpus --locked --offline
python -I scripts/fixtures/verify_a03.py
python -I scripts/fixtures/verify_a03_qa.py --self-test
```

All passed. Twelve direct executable checks passed: complete A03
original/correction search, global token hit limit, extraction gaps, shared byte
refusal, scoped zero hits, duplicate inputs, path-count bound, malformed UTF-8,
document boundaries, escaped terminal controls, missing file, and query validation
before file access. A03 original/correction SHA-256 hashes were unchanged.

Application authorization, immutable version binding, metadata filters, identifier
search, ranking, saved searches, corpus pagination, model integration and complete
A03 acceptance remain open on #18. No Lean correspondence or performance claim
is made. No new dependency or hosted CI job is introduced.
