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

A `ComparisonRequest` holds 2–32 `StatementSource` values and up to 256
`CandidateDismissal` values. It pins the caller's coding scheme version
(`coding_version`) and coder (`coded_by`). Each source is one pinned version of
one account and has the following fields:

- `account`, a caller `revision` ordinal and an immutable `version`. Versions of
  one account share the account ID. Version IDs and `(account, revision)` pairs
  must be unique. An account cannot change source class or origin between
  versions.
- A declared `SourceClass`: testimony, police report, news report or other.
- An `AccountOrigin` reused from `oia-domain::witness`. See
  [Simulated accounts](#simulated-accounts) for how it affects counts.
- `ExplanationExposure`: unknown, not exposed in the workflow, or after
  explanations. "Not exposed in the workflow" says nothing about exposure
  outside the workflow.
- Text as `Complete`, `Partial { reason }` or `Unavailable { reason }`. Complete
  empty text, partial text and unavailable text are separate states.
- Detail assertions. Each assertion has an `aspect` key, a `CodedValue`
  (`Reported(value)` or `StatedUnknown`) and a `wording` byte range on
  character boundaries of the captured text. The cited wording must contain a
  non-whitespace character. A citation into unavailable text is refused. An
  aspect that a source does not mention has no assertion; it is never coded as
  unknown or zero.
- `derived_from`: accounts that this version declares it draws on, such as a
  news report that cites the sheriff's office. Derivations from accounts outside
  the compared set are retained in the input but cannot join groups.

Aspects and coded values are analyst coding over cited wording. They are not
observations or measurements. This kernel neither creates nor corrects that
coding. The result returns each report's exact wording slice beside its coded
value, with the source's origin and exposure.

A `CandidateDismissal` records a reviewer's decision that a copy candidate is not
evidence of dependence. For example, two witnesses may each quote the same public
police appeal. A dismissal identifies a candidate by its version pair and both
byte ranges, in either order, and carries the reviewer and a reason. A dismissed
candidate stays listed with its dismissal index but no longer merges groups. A
dismissal that matches no candidate adds an `UnmatchedDismissal` limitation
rather than being dropped silently.

## Method

Method `word-shingle-jaccard+exact-word-run-v2` is printed in every result's
`MethodRecord`, with its tokenization, parameters, coding version, coder and
caveats. Words are maximal runs of Unicode alphanumeric characters, compared
after `char::to_lowercase`. Every other character separates words. No Unicode
normalization is applied, so the composed (NFC) and decomposed (NFD) forms of
the same text do not match. Source bytes are never modified; byte ranges refer to
the original text.

Scripts written without spaces between words (Han, Kana, Thai, Lao, Khmer,
Myanmar), and any source with a word longer than 40 characters, are marked with
an `UnsegmentedText` limitation that carries the longest word length. Their
copy-search pairs report `CopySearch::UnsegmentedText` instead of `Complete`.
Those texts form very few, very long words, so the word-based measures are
unreliable for them. The result is marked partial rather than presented as a
complete miss. No character n-gram fallback is implemented.

- **Pairwise overlap matrix**: one entry for every pair of source versions. The
  measure is the Jaccard index of distinct `k`-word shingles (default `k = 5`,
  allowed 2–16), as an exact `shared/union` integer ratio. `TooShort` means that
  at least one text has fewer than `k` words, so the measure is not computed.
  `Unavailable` means that a text is missing. Each entry also reports the weaker
  coverage of the two texts.
- **Copy candidates**: maximal exact shared word runs of at least `m` words
  (default `m = 12`, allowed 6–256) between different accounts. Each candidate
  has byte ranges and slices in both sources. A candidate means possible copying
  or quotation; it is not established plagiarism and not corroboration. Shorter
  shared runs are common phrasing and are not flagged. Same-account revision
  pairs are marked `SameAccount` because their shared text is expected; they
  appear in drift instead.
- **Groupings**: two groupings are reported, and each lists the reasons that
  joined its groups.
  - `declared` merges accounts only through declared derivations. Its
    `reported_group_count` is an upper bound on independent reported sources,
    provided the declarations are correct. Paraphrase, a shared briefing, an
    interview or a common outside source can make accounts dependent without
    any declaration or shared wording.
  - `candidate_adjusted` also merges accounts through undismissed copy
    candidates, so copied wording lowers the count. A candidate can be a
    legitimate quotation of a common public text, so this count can fall
    *below* the true number of independent sources. It is **not a bound**.
    Review or dismiss candidates before relying on it.
- **Aspect table**: one version represents each account. The default is
  `Initial`, the account preserved before explanations (FIELD-02); `Latest` is
  available. Aspects are sorted by key. Each aspect lists every report with its
  wording, origin and exposure; the variants; the sources stating it unknown;
  and the representative sources that did not code it, with their text coverage.
  Each variant carries its total, reported and simulated account counts and its
  declared and candidate-adjusted group counts. Group counts include only
  accounts that count as reported support. The status is one of the following:
  - `Converging`: one value, with reported support from at least two
    candidate-adjusted groups.
  - `RepeatedWithinOneGroup`: one value from two or more reported accounts in
    one candidate-adjusted group, such as a news report and its copy. The
    variant's declared group count shows whether only a copy candidate joined
    them.
  - `Unique`: one value from one reported account.
  - `NoReportedSupport`: one value supported only by simulated accounts, or by
    reported accounts whose declared lineage is only simulated.
  - `Conflicting`: two or more values. Variants stay separate, and none is
    preferred or merged.
  - `StatedUnknownOnly`: every report states that the detail is unknown.
- **Drift**: for each account, consecutive revisions in ordinal order. Each
  step lists the aspects whose coded values were added, removed or changed, the
  number of unchanged aspects, the exposure of the later version, exact text
  identity, and the pair's shingle overlap.

Overlap is lexical. It is never agreement about what happened, a probability,
or proof of independence. The caveat constants state this in every result.

## Simulated accounts

A simulated account never counts as reported support, so it cannot raise a
reported account count, a reported group count or `Converging`. The same applies
to a reported account whose declared in-set derivations are all, directly or
transitively, anchored only in simulated accounts. `SourceSummary::counts_as_reported`,
`IndependenceGroup::includes_simulated` and the variant's `simulated_accounts`
count expose this; `contains_simulated` marks the whole result.

This matches the convention in
[`oia_query::geo_workspace`](../../crates/oia-query/src/geo_workspace.rs). That
module excludes simulated items from `independent_source_count` and flags
`LineageAnchoredOnlyToSimulatedEvidence`. Copy candidates carry no direction, so
a reported account that shares wording with a simulated account still counts
as reported. Its group is marked `includes_simulated`.

### Relation to other independence counters

Other modules also count independence, but with different units and evidence:

- `oia_query::geo_workspace` counts independent sources among items clustered
  into proposed events (GEO-03).
- `oia_documents::lineage::evaluate_claim_corroboration` counts independent
  supporting roots from explicit document lineage (#19).
- `oia_news_claims` evaluates claims and quotation fidelity within news
  coverage (#21).

This kernel's counts group accounts of one incident through declared
derivations and lexical copy candidates. They are not interchangeable with those
counters. When cases persist (#10/#11), integration must state which counter a
view shows. Declared derivations here should come from the same lineage records
that `oia-documents` uses rather than a separate registry.

## Bounds, determinism and failure behavior

Inputs over these limits produce typed `ComparisonError` values and are never
truncated:

- 2–32 sources and at most 256 dismissals.
- 262,144 bytes per text and 2,097,152 bytes in total.
- 256 assertions and 32 derivations per source.
- 256-byte aspect and value labels, which must contain text and no control or
  format characters (`oia_domain::text::is_format_control`).
- 4,096-byte attribution, coverage and dismissal reasons.

Each cross-account pair's copy search stops after 64 candidates or 4,000,000
index probes. A pair that reaches either bound is marked `Truncated` and adds a
`CopySearchTruncated` limitation; further shared runs may exist. The following
also add limitations: partial and unavailable texts, unsegmented texts, texts too
short for one shingle, and unmatched dismissals. `is_partial()` is true whenever
a limitation is present, so a partial result stays usable without appearing
complete.

`StopReason` is the shared `oia_analysis::multilingual::StopReason`. The control
callback runs:

- before work;
- between pairs and drift steps;
- at least every 1,024 steps of tokenizing, shingle construction, shingle
  comparison and copy search;
- before the result is returned.

A stop returns no result. The application service must still fence cancelled or
superseded completions before it saves or shows a result.

Ordering is deterministic. Sources keep input order, pairs are ordered by
`(a, b)`, candidates follow pair order and then position, groups follow the
first appearance of their accounts, and aspects are sorted by key. Figures are
integers or exact ratios; `Ratio::as_f64` exists only for display.

## Local use

```text
cargo run -p oia-analysis --example compare-statements --locked -- <class>:<account>[@<revision>]=<path> ...
```

The class is `testimony`, `police`, `news` or `other`. Files are labeled as
reported complete texts with unknown exposure, no coded assertions and no
dismissals, so the aspect table is empty. The example prints the following:

- the method and caveats;
- each source's word count, longest word and group indices;
- the matrix;
- the copy candidates;
- both reported group counts;
- the limitations.

Reads stop one byte past the per-text limit. Previews are escaped and capped at
240 characters for display only; comparison uses the whole bounded text. Nothing
is written.

## Validation

Validation on 2026-11-03 (Windows 11, Rust 1.99.0):

- **Tested:** `crates/oia-analysis/tests/statement_comparison_contract.rs` has
  three tests over synthetic fixture texts that stand in for reported accounts,
  following the fixture in #138: a news report that declares it draws on a
  partial police report, a witness who copies sixteen words of the news report, a
  witness whose revision after explanations changes colour and sound, an
  independent witness who states that size is unknown, and a police report that
  omits sound and declines to give the colour.
  1. The first test checks the printed method and caveats; five accounts in four
     declared and three candidate-adjusted groups; the single copy candidate's
     exact ranges; the `SameAccount` revision pair; and each aspect's status and
     counts.
     - Colour shows orange from three accounts in three declared and two
       candidate-adjusted groups, and red from one account.
     - The police unknown stays apart from the variants and from omissions, and
       the police omission of sound keeps its partial coverage.
     - It also checks the drift step, the partial limitation and initial-version
       selection. Dismissing the copy candidate keeps it listed, restores four
       candidate-adjusted groups, and makes motion converge.
  2. The second test checks that a citation off a character boundary, a
     whitespace-only citation, a citation into unavailable text and cancellation
     each return a typed error and no result.
  3. The third test makes the police report simulated. It checks that neither
     the police report nor the news report that declares it draws only on it
     counts as reported support (location: 5 accounts, 3 reported, 1
     simulated). It also checks that two identical Han texts give an
     `UnsegmentedText` limitation, `CopySearch::UnsegmentedText`, `TooShort`
     overlap and a partial result.
- **Exercised:** the example over four fixture text files reported one 16-word
  copy candidate.
- **Measured (single runs on this development machine, not benchmarks):**
  - Two repetitive texts of about 260 KB were marked `CopySearchTruncated`, in
    27 ms (release) and 153 ms (debug).
  - At the input limit, 32 sources × 64 KiB:

    | Input | Release | Debug |
    | --- | --- | --- |
    | Repetitive text (`the` repeated) | 1.4 s | 8.7 s |
    | Random words from a 1,000-word vocabulary | 2.1 s | 14.0 s |

    Peak working set for the random-word run in release, sampled every 50 ms,
    was about 50 MB. Sampling can under-read the true peak.
- Workspace `cargo fmt --check`, `cargo clippy --workspace --all-targets
  --locked -D warnings` and `cargo test --workspace --locked` were run; the PR
  records their results.

No real witness, police or news material was used. These results do not measure
how well the method detects copying in real accounts.

## Remaining #138 scope

- A Witness or Documents instrument view of the matrix, shared and unique
  details, drift and candidate review over the held corpus (#134). It needs an
  `oia-app` adapter and a coding workflow for assertions and dismissals.
- Persistence through the common evidence envelope (#10/#11), with the method,
  parameters, coding version, dismissals and source versions pinned. A source
  correction must flag dependent results for reassessment (`COLLAB-03`).
- A segmentation-aware method for scripts without word separators.
- Any automatic assertion extraction. That would be a separate model-backed
  method, labeled as inference and evaluated before use.
- Composition into recreation prompts (#139), negatives (#140) and linked cases
  (#141).
