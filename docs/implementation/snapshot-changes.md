# Dated UTF-8 snapshot comparison

This implements the dependency-independent comparison slice of
[#48](https://github.com/mESmaC/oia/issues/48), `PRIVACY-03`, and the dated
public-page revision subset of [A18](../delivery.md#a18-privacy-and-change-monitoring).
The implementation is in
[`oia_analysis::snapshot_change`](../../crates/oia-analysis/src/snapshot_change.rs),
with a [read-only local example](../../crates/oia-analysis/examples/compare-snapshots.rs).
It evaluates a rule over two caller-supplied text captures. It does not acquire
sources, persist rules, send notifications, or establish full A18 acceptance.

## Contract

`Snapshot::new` validates a borrowed UTF-8 representation, its immutable version,
workspace/source/resource IDs, source attribution, and retrieval instant. Those
IDs use the merged `oia-domain` types. The input text, attribution, whitespace,
line endings, Unicode, and partial-capture explanation remain unchanged.
Complete empty text, partial text, and unavailable content are separate states.
Unavailable reasons distinguish missing, restricted, rate-limited, failed,
not-retained, and malformed acquisition results.

The caller must establish identity, authorization, origin/processing lineage,
retention rights and capture completeness before constructing an input. A source
ID alone is insufficient to identify a page, so both the saved rule and captures
also carry a resource ID. All three scope IDs must agree. Matching IDs do not
grant authorization. Distinct captures require distinct version IDs, even when
the bytes are identical. These transient values cannot enforce version uniqueness
across separate calls; that remains the persistence layer's responsibility.

`ChangeRule` pins a rule version, scope, and positive maximum retrieval age. Its
criterion is **any exact text change**. The caller supplies the saved rule's
version; constructing the rule does not save it. `evaluate` returns immutable
references to both complete input envelopes and the rule, an evaluation instant,
current retrieval age, and a comparison:

- `Changed`: exact before/after UTF-8 byte ranges for a replacement span.
- `Unchanged`: both complete captured representations are byte-identical.
- `Inconclusive`: either capture is partial or unavailable. Even equal partial
  text cannot establish an unchanged resource; missing text is never a deletion.

Method `exact-utf8-prefix-suffix-v1` removes the longest common Unicode-character
prefix, then the longest non-overlapping common suffix. The remainder is one
replacement span, which may include unchanged text between several edits. Empty
ranges represent insertions/deletions. This deterministic linear comparison
preserves valid UTF-8 boundaries; it is not a line diff, minimal edit script,
semantic interpretation, or correction/retraction detector. HTML is compared
literally if supplied; no rendering, markup removal, OCR, or normalization occurs.
Changes in extraction methods can also change text and need separate review.

The rule decision is `ChangeDetected` only for a changed pair of complete captures
whose current retrieval age is within the inclusive threshold. Complete identical
captures yield `NoTextChange` under that same freshness condition. Otherwise,
`IncompleteCaptures` takes precedence over `StaleCurrentCapture`. Independent
freshness and both coverage states remain inspectable, including when several
limitations apply. Stale complete captures still expose their exact comparison.
These decisions are analytical results, not delivered notifications.

### Line/segment-level change explanation and notification provenance (`PRIVACY-03`, `A18`)

`ChangeEvaluation::explain_changes` decomposes an evaluated snapshot pair into a
structured `ChangeExplanation` under explicit `ChangeExplanationLimits`
(`1..=16,384` lines per capture and `1..=1,024` emitted hunks; defaults `4,096`
lines and `256` hunks):

- `ChangeExplanation::Changed` (`EXPLANATION_METHOD_ID = "exact-utf8-line-segments-v2"`):
  splits both complete captures with `split_inclusive('\n')` and computes a
  deterministic 64-bit FNV-1a hash per line so exact line endings (`\r\n` versus
  `\n`) and trailing newline differences are preserved without normalization. Using an
  iterative sub-window work stack:
  1. Each sub-window first trims identical leading and trailing lines in $O(n + m)$ time.
  2. When a residual sub-window has $n \cdot m \le \text{MAX\_LCS\_CELLS}$ (`262,144`),
     it runs exact line-level dynamic-programming LCS in $O(n \cdot m)$ time, allocating
     an $(n + 1)(m + 1)$ `u32` DP table (at most ~1 MiB).
  3. When a residual sub-window exceeds `MAX_LCS_CELLS` and cumulative alignment work is
     strictly below `MAX_ALIGNMENT_STEPS = 1_048_576`, it first extracts the longest
     increasing subsequence (LIS) of lines that appear **exactly once** in both
     sub-windows (Patience unique-line anchors in $O((n + m) \log(n + m))$ time via
     iterative bottom-up merge sort on `(hash, len, text)` indices and binary-searched
     LIS tails, charging 1 step per comparison and checking `check_control` every 1,024
     steps) and recursively splits the sub-window around those anchors, trimming any
     adjacent non-unique common lines.
  4. If an oversized sub-window has no unique common lines (for example, repeated
     boilerplate lines between distant edits), it runs bounded forward Myers alignment up
     to `MAX_MYERS_EDIT_DISTANCE = 256` edits ($O((n + m) D)$ time, allocating a
     $(D_{\max} + 1)(2 D_{\max} + 3)$ `usize` trace buffer of ~1 MiB at $D_{\max} = 256$)
     using checked index arithmetic and enforcing `MAX_ALIGNMENT_STEPS` inside the
     diagonal and snake loops.
  5. Only when an individual unanchored residual sub-window exceeds `MAX_LCS_CELLS` and
     cannot be aligned via unique anchors or bounded Myers does *that* sub-window
     coalesce into a single bounded `Modified` hunk while other sub-windows retain exact
     hunks.
  - **Per-hunk alignment provenance (`HunkAlignment`) and `LineChangeCounts`:**
    Every `ChangeHunk` carries an explicit `HunkAlignment` marker (`hunk.alignment()`):
    - `HunkAlignment::ExactMinimal`: globally minimal alignment on the full non-identical
      window (via prefix/suffix trimming, direct `MAX_LCS_CELLS` DP, or direct
      `MAX_MYERS_EDIT_DISTANCE` Myers without unique-anchor partitioning). When all hunks
      are `ExactMinimal`, `line_counts` is
      `LineChangeCounts::Exact { unchanged_lines, inserted_lines, removed_lines }`.
    - `HunkAlignment::UniqueAnchorPartitioned`: exact alignment within a sub-window
      produced by Patience unique-line anchor partitioning. Because unique anchors pin
      human-meaningful landmarks across large files, duplicate lines moved across a
      pinned unique anchor are emitted as removed and re-inserted rather than globally
      minimal (`AGENTS.md` §3). When unique-anchor partitioning is used and no sub-window
      coalesces, `line_counts` is
      `LineChangeCounts::AnchorPartitioned { matched_unchanged_lines, inserted_lines, removed_lines }`.
    - `HunkAlignment::Coalesced(CoalescedReason)`: quality-reducing coalesced replacement
      hunk for an unanchored sub-window that exceeded `MAX_LCS_CELLS`, recording whether
      it coalesced due to `CoalescedReason::NoUniqueAnchorsOverEditBudget` or
      `CoalescedReason::AlignmentStepBudgetExceeded`. When any hunk is `Coalesced`,
      `line_counts` is
      `LineChangeCounts::CoalescedWindowUpperBound { trimmed_unchanged_lines, coalesced_before_window_lines, coalesced_after_window_lines }`
      so coalesced window sizes (upper bounds on removed and inserted lines) are never
      presented as measured line-edit counts (`AGENTS.md` §2).
  Emitted `ChangeHunk` records (`Inserted`, `Removed`, `Modified`) are ordered and
  non-overlapping, with exact half-open UTF-8 byte ranges (`before_bytes`, `after_bytes`)
  and 1-indexed inclusive line ranges (`before_lines`, `after_lines`). Applying the
  emitted hunks in reverse byte order reconstructs the exact current UTF-8 text from
  the baseline text (and vice versa).
- `ChangeExplanation::Unchanged`: counts lines in a single linear pass with zero
  heap allocation when both complete captures are byte-identical (without failing
  `LineLimitExceeded` on large unchanged captures).
- `ChangeExplanation::Inconclusive`: retains `CaptureCoverageSummary` (`Complete`,
  `Partial { captured_byte_len, reason }`, or `Unavailable(UnavailableReason)`) for
  both captures so partial or missing captures never masquerade as deletions or
  unchanged segments.
- `ChangeExplanation::Unavailable { before_line_count, after_line_count, reason }`:
  used by `ChangeEvaluation::explain_notification` when `explain_changes` exceeds
  `limits.max_lines_per_capture()` (`ExplanationUnavailableReason::LineLimitExceeded`)
  or `limits.max_hunks()` (`ExplanationUnavailableReason::HunkLimitExceeded`). Rather
  than failing the notification outright, `explain_notification` returns
  `Ok(ExplainedNotification)` with the rule decision, capture coverage, retrieval
  freshness, and top-level `TextComparison` replacement span preserved alongside the
  explicit unavailable-explanation reason.

`ChangeEvaluation::explain_notification` projects an `ExplainedNotification`
envelope binding the saved `ChangeRule` version and criterion
(`RULE_CRITERION_ID = "any-exact-utf8-text-change-v1"`), `SnapshotScope`
(`workspace`/`source`/`resource`), `max_retrieval_age`, baseline and current capture
versions, source attributions, retrieval instants/ages, `RetrievalFreshness`, both
`CaptureCoverageSummary` states, `RuleDecision`, the top-level replacement span, and
the `ChangeExplanation` so notifications expose source coverage, freshness, and the
saved rule that triggered them (`PRIVACY-03`, `A18`).

## Time, coverage and evidence limits

Retrieval instants and the evaluation instant use `std::time::SystemTime`. Retrieval
order must be nondecreasing and evaluation cannot precede the current retrieval.
The caller selects the baseline; it may be old. Both retrieval ages and freshness
classifications remain inspectable under the same rule threshold. Only the current
capture's age controls notification eligibility, so an explicitly historical
baseline does not suppress a current change. Equal retrieval instants are accepted with
distinct versions and explicit caller-selected baseline/current roles.

This slice supports caller-established exact retrieval instants. It does not
convert calendar dates, uncertain device clocks, source publication times,
provider revisions, leap seconds, or original timezones. Such source metadata
must remain attached to the pinned source version and use the eventual shared
time contract from #12. Unknown retrieval time cannot be replaced with now or a
file modification time to satisfy the API.

Retrieval freshness is **not content freshness**: a recent acquisition can contain
old published content or a cached response. Complete means the supplied captured
representation was complete, not that an entire website, dataset, geographic area,
time interval, or real-world situation was covered. No change or absent reports
cannot establish safety. No hash/authenticity claim is made; this method compares
the actual supplied text bytes.

## Bounds and cancellation

Each capture is limited to 1,048,576 UTF-8 bytes. Attribution and partial-capture
explanations are each limited to 4,096 bytes and must contain non-whitespace text.
Oversized inputs produce typed errors without truncation. There is no quadratic
diff matrix, output-text copy, or allocation in `evaluate`. `explain_changes`
enforces `max_lines_per_capture` (`<= 16,384`), `max_hunks` (`<= 1,024`),
`MAX_LCS_CELLS` (`262,144`), `MAX_MYERS_EDIT_DISTANCE` (`256`), and a strict ceiling
of `MAX_ALIGNMENT_STEPS` (`1,048,576`) across cumulative sub-quadratic partitioning
and Myers steps, using fallible `try_reserve` allocations and returning typed
`ChangeError` variants (`LineLimitExceeded`, `HunkLimitExceeded`,
`InvalidExplanationLimits`, `AllocationFailed`) rather than silently truncating
hunks. Across an entire `explain_changes` invocation, total sub-quadratic partitioning
and Myers work is strictly capped at `MAX_ALIGNMENT_STEPS` (`1,048,576` steps), and
because each unique-anchor split consumes at least one line from both captures and
disjoint sub-windows satisfy $\sum n_i \le N \le 16,384$ and $\sum m_i \le M \le 16,384$
with $n_i m_i \le \text{MAX\_LCS\_CELLS}$ (`262,144`) per DP leaf, total DP cells
across all leaves are bounded by $\sum n_i m_i \le \max(N, M) \cdot \sqrt{\text{MAX\_LCS\_CELLS}} \approx 8.39 \times 10^6$
cells, while peak transient heap memory is at most ~2 MiB (`LineSlice` tables for
both captures, the work stack, and at most one active ~1 MiB DP or Myers table at a
time). `explain_notification` converts `LineLimitExceeded` and `HunkLimitExceeded`
into `ChangeExplanation::Unavailable` while still propagating `Stopped(StopReason)`
and `AllocationFailed`. `StopReason` is re-exported from
`oia_analysis::multilingual::StopReason` (`Cancelled`, `DeadlineExceeded`,
`Superseded`). Source text is data, including embedded instructions and display
controls.

The cancellation/deadline callback runs before work, at intervals of at most 1,024
scanned characters/lines/cells/alignment steps, and before returning a result. A
stop yields no comparison, explanation, or notification. The application service must
still fence cancelled or superseded job completions when saving/publishing a result;
this pure function does not implement a job queue.

## Local use

```text
cargo run -p oia-analysis --example compare-snapshots --locked --offline -- BEFORE.txt AFTER.txt SOURCE_REFERENCE BEFORE_RETRIEVED_SECONDS AFTER_RETRIEVED_SECONDS [MAX_AGE_SECONDS]
```

Times are caller-supplied, nonnegative Unix seconds representing the captures'
retrieval instants; evaluation uses the current system clock. The default maximum
retrieval age is 86,400 seconds. The example treats each file as a complete capture
of the same caller-declared resource. Use the library's explicit partial/unavailable
states when that assertion is not justified. The example assigns visibly transient
local scope/version labels; it neither imports evidence nor persists those IDs.

Both paths must be regular local files containing UTF-8. Reads stop at the hard
byte limit plus one byte to detect oversized inputs. The example never writes
either file. A five-second deadline bounds comparison after file reads; local
filesystem open/read latency itself has no wall-clock deadline.

Output includes source attribution, method, rule/version, retrieval/evaluation
times, coverage, freshness, decision, byte ranges, and per-hunk line/byte
explanations (or `explanation_status=Unavailable` when line/hunk limits are exceeded).
Text previews are ASCII escaped and capped at 240 characters with an
explicit truncation flag; comparison still uses the entire bounded input. This
display limit never changes evidence or calculations. Exit `0` means a fresh
complete evaluation (changed or unchanged), `2` means a stale/incomplete decision,
and `1` means invalid input or failure.

## Integration and remaining scope

This is a transient analytical kernel in `oia-analysis`. It introduces no source
registry, network transport, storage format, scheduler, service authorization,
provider or UI choice. Integration must use the [common domain envelope and
result contracts](../architecture.md), preserve input origins and processing
history, pin the method/rule/capture versions, and apply workspace access checks.
No unmerged prerequisite code is copied.

Change ranges are ordinary `std::ops::Range<usize>` values, not a second citation
model. Report adapters should use the existing
[`oia_export::CitationAnchor::Utf8ByteRange`](../../crates/oia-export/src/lib.rs)
with `start = range.start` and `end = range.end`, targeting the corresponding
snapshot version's retained UTF-8 inventory entry. These offsets must not be
applied to an original HTML/PDF when the compared text is a different extraction,
or to a redacted copy without mapping. `is_unadjusted_original_offset` must describe
that target relationship honestly. Empty insertion/deletion ranges are valid
change positions but are rejected as quotations by the exporter; retain those
positions in analysis metadata and cite the nonempty side. Do not fabricate a
nonempty quote for an empty capture. This slice adds no dependency from analysis
back to the export adapter and does not yet emit report citations.

Issue #48 remains open for source registry/ingestion (#13), regional news (#20),
weather/context (#24), durable services (#14), workspace UI (#16), saved-rule and
history storage (#10/#11), notification delivery, and integrated A18 verification.
Existing D02 storage and other owners' decisions are unchanged. No Lean proof or
Rust-to-Lean correspondence claim applies to this new kernel.

## Verification

The [two focused contract tests](../../crates/oia-analysis/tests/snapshot_change_contract.rs)
use synthetic captures. They address original/version preservation, UTF-8 byte
anchors, line-ending-only changes, multi-hunk line/segment reconstruction,
distant-edit exactness beyond `MAX_LCS_CELLS` (via Patience unique-line anchors and
bounded Myers), selective sub-window coalescing (`CoalescedWindowUpperBound`),
notification rule/coverage/freshness provenance, `ChangeExplanation::Unavailable`
when notification explanation limits are exceeded, empty versus missing coverage,
stale decisions, scope/time rejection, byte/line/hunk bounds, and cancellation
without publishing a result. They do not establish provider access, operational
notification quality, persistence, or source truth.

On 2026-11-04, local Windows x86_64 MSVC verification with pinned Rust 1.99.0
passed `cargo +1.99.0 fmt --all -- --check`,
`cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings`,
`cargo +1.99.0 test --workspace --locked --offline`, and
`powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run.ps1 proofs`. The
two focused tests and the `compare-snapshots` CLI example also passed
independently. Other operating systems and live providers were not exercised.
