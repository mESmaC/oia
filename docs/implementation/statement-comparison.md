# Statement comparison: convergence, divergence, drift and copied text

This implements the dependency-independent kernel slice of
[#138](https://github.com/mESmaC/oia/issues/138). It covers the copied-account
subset of `INTEL-01`, the wording, unknown, conflict and omission preservation of
`REASON-03`, and drift between retained revisions (`FIELD-02`). The code is
[`oia_analysis::statement_comparison`](../../crates/oia-analysis/src/statement_comparison.rs).
A [read-only local example](../../crates/oia-analysis/examples/compare-statements.rs)
compares text files. The kernel does not extract assertions from prose, persist
results, or provide the Witness/Documents instrument view. Full A08/A13
acceptance remains open.

## Inputs

A `ComparisonRequest` holds 2–32 `StatementSource` values and pins the caller's
coding scheme version (`coding_version`) and coder (`coded_by`). Each source is
one pinned version of one account and has the following fields:

- `account`, a caller `revision` ordinal and an immutable `version`. Versions of
  one account share the account ID. Version IDs and `(account, revision)` pairs
  must be unique. An account cannot change source class or origin between
  versions.
- A declared `SourceClass`: testimony, police report, news report or other.
- An `AccountOrigin` reused from `oia-domain::witness`. A simulated source
  cannot become reported testimony, and the result sets `contains_simulated`.
- `ExplanationExposure`: unknown, not exposed in the workflow, or after
  explanations. "Not exposed in the workflow" says nothing about exposure
  outside the workflow.
- Text as `Complete`, `Partial { reason }` or `Unavailable { reason }`. Complete
  empty text, partial text and unavailable text are separate states.
- Detail assertions. Each assertion has an `aspect` key, a `CodedValue`
  (`Reported(value)` or `StatedUnknown`) and a non-empty `wording` byte range
  on character boundaries of the captured text. A citation into unavailable
  text is refused. An aspect that a source does not mention has no assertion;
  it is never coded as unknown or zero.
- `derived_from`: accounts that this version declares it draws on, such as a
  news report that cites the sheriff's office. Derivations from accounts outside
  the compared set are retained in the input but cannot join groups.

Aspects and coded values are analyst coding over cited wording. They are not
observations or measurements. This kernel neither creates nor corrects that
coding; the result returns each report's exact wording slice beside its coded
value.

## Method

Method `word-shingle-jaccard+exact-word-run-v1` is printed in every result's
`MethodRecord`, with its tokenization, parameters, coding version, coder and
three caveats. Words are maximal runs of Unicode alphanumeric characters,
compared after `char::to_lowercase`. Every other character separates words.
Source bytes are never modified or normalized; byte ranges refer to the original
text.

- **Pairwise overlap matrix**: one entry for every pair of source versions. The
  measure is the Jaccard index of distinct `k`-word shingles (default
  `k = 5`, allowed 2–16), as an exact `shared/union` integer ratio. `TooShort`
  means that neither text has `k` words. `Unavailable` means that a text is
  missing. Each entry also reports the weaker coverage of the two texts.
- **Copy candidates**: maximal exact shared word runs of at least `m` words
  (default `m = 12`, allowed 6–256) between different accounts. Each candidate
  has byte ranges and slices in both sources. A candidate means possible copying
  or quotation; it is not established plagiarism and not corroboration. Shorter
  shared runs are common phrasing and are not flagged. Same-account revision
  pairs are marked `SameAccount` because their shared text is expected; they
  appear in drift instead.
- **Independence groups**: declared derivations and copy candidates merge
  accounts by union–find. `independence_group_upper_bound` is the group count.
  It is an **upper bound**, because paraphrase, a shared briefing, an interview
  or a common outside source can make accounts dependent without shared wording.
  Each group lists the reasons that joined it.
- **Aspect table**: one version represents each account. The caller selects the
  `Initial` or `Latest` revision; FIELD-02 review can compare the initial
  accounts. Aspects are sorted by key. Each aspect lists every report with its
  wording, the variants, the sources stating it unknown, and the representative
  sources that did not code it, with their text coverage. Its status is one of
  the following:
  - `Converging`: one value, reported from at least two independence groups.
  - `RepeatedWithinOneGroup`: one value, repeated only within a dependent group,
    such as a news report and its copy.
  - `Unique`: one value from one account.
  - `Conflicting`: two or more values. Variants stay separate, and none is
    preferred or merged.
  - `StatedUnknownOnly`: every report states that the detail is unknown.

  Every variant carries both its account count and its independence-group
  count, so a copied report raises the first count but not the second.
- **Drift**: for each account, consecutive revisions in ordinal order. Each
  step lists the aspects whose coded values were added, removed or changed, the
  number of unchanged aspects, the exposure of the later version, exact text
  identity, and the pair's shingle overlap.

Overlap is lexical. It is never agreement about what happened, a probability,
or proof of independence; `OVERLAP_CAVEAT` states this in the result.

## Bounds, determinism and failure behavior

Inputs over these limits produce typed `ComparisonError` values and are never
truncated:

- 2–32 sources.
- 262,144 bytes per text and 2,097,152 bytes in total.
- 256 assertions and 32 derivations per source.
- 256-byte aspect and value labels, which must contain text and no control or
  format characters (`oia_domain::text::is_format_control`).
- 4,096-byte attribution and coverage reasons.

Each cross-account pair's copy search stops after 64 candidates or 4,000,000
index probes. A pair that reaches either bound is marked `Truncated` and adds a
`CopySearchTruncated` limitation; further shared runs may exist. Partial and
unavailable texts, and texts too short for one shingle, also add limitations.
`is_partial()` is true whenever a limitation is present, so a partial result
stays usable without appearing complete.

The control callback runs before work, between pairs and drift steps, at least
every 1,024 tokenizing, shingle or probe steps, and before the result is
returned. A stop returns no result. The application service must still fence
cancelled or superseded completions before it saves or shows a result.

Ordering is deterministic. Sources keep input order, pairs are ordered by
`(a, b)`, candidates follow pair order and then position, groups follow the
first appearance of their accounts, and aspects are sorted by key. Figures are
integers or exact ratios; `Ratio::as_f64` exists only for display.

## Local use

```text
cargo run -p oia-analysis --example compare-statements --locked -- <class>:<account>[@<revision>]=<path> ...
```

The class is `testimony`, `police`, `news` or `other`. Files are labeled as
reported complete texts with unknown exposure and no coded assertions, so the
aspect table is empty. The example prints the method and caveats, the matrix,
copy candidates, groups and limitations. Reads stop one byte past the per-text
limit. Previews are escaped and capped at 240 characters for display only;
comparison uses the whole bounded text. Nothing is written.

## Validation

Validation on 2026-11-03 (Windows 11, Rust 1.99.0):

- **Tested:** `crates/oia-analysis/tests/statement_comparison_contract.rs` has
  two tests. They use simulated accounts that follow the fixture in #138: a news
  report that declares it draws on a partial police report, a witness who copies
  sixteen words of the news report, a witness whose revision after explanations
  changes colour and sound, an independent witness who states that size is
  unknown, and a police report that omits sound and declines to give the colour.
  - The first test checks the printed method, five accounts in at most three
    groups, the single copy candidate's exact ranges, and the `SameAccount`
    revision pair.
  - It checks that location converges with five accounts in three groups,
    motion is repeated within one group, identity is unique and size is only
    stated unknown.
  - It checks that colour conflicts (orange from three accounts in two groups,
    red from one), that the police unknown stays apart from the variants and
    omissions, and that the police omission of sound keeps its partial coverage.
  - It checks the drift step, the partial-text limitation and initial-version
    selection.
  - The second test checks that a citation off a character boundary, a citation
    into unavailable text and cancellation each return a typed error and no
    result.
- **Exercised:** the example over four fixture text files reported one 16-word
  copy candidate and two independence groups for three accounts.
- **Measured (single run, not a benchmark):** two adversarial repetitive texts
  of about 260 KB, compared on this development machine, were marked
  `CopySearchTruncated`. The run took 27 ms in a release build and 153 ms in a
  debug build.
- Workspace `cargo fmt --check`, `cargo clippy --workspace --all-targets
  --locked -D warnings` and `cargo test --workspace --locked` were run; the PR
  records their results.

No real witness, police or news material was used. These results do not measure
how well the method detects copying in real accounts.

## Remaining #138 scope

- A Witness or Documents instrument view of the matrix, shared and unique
  details, and drift over the held corpus (#134). It needs an `oia-app` adapter
  and a coding workflow for assertions.
- Persistence through the common evidence envelope (#10/#11), with the method,
  parameters, coding version and source versions pinned. A source correction
  must flag dependent results for reassessment (`COLLAB-03`).
- Any automatic assertion extraction. That would be a separate model-backed
  method, labeled as inference and evaluated before use.
- Composition into recreation prompts (#139), negatives (#140) and linked cases
  (#141).
