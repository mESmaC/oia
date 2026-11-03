# Evidence-set recreation composer

Implemented application-kernel slice of [#139](https://github.com/mESmaC/oia/issues/139),
under #31 / **MEDIA-04**, **COLLAB-02/COLLAB-03**, and the prompt-export subset of
**A10**. It does not complete the issue or integrated acceptance scenario.

## Ownership and use

`oia_app::recreation_composer::compose_recreation_prompt` connects the existing
`oia_analysis::statement_comparison` and `oia_media::recreation` kernels through
the application boundary (architecture section 1, ADR 0001). It adds no dependency,
provider, model, framework, storage, or renderer. The single-witness desktop
export is unchanged. There is no port from prior work and no Lean proof claim.
The single-witness and composer paths share the existing source-qualification
formatter, including its explicit post-explanation marker. Comparison inputs do
not carry revision rationales; that absence is named instead of inventing one.

The caller supplies a `CompositionRequest`: pinned source versions, an explicit
coding version/coder, exact cited assertions, comparison parameters and copy
candidate dismissals, prompt/case/version IDs, required aspects, optional context
bindings, declared assumptions, selected assets, and a composition rationale.
The composer runs comparison itself, so a forged or stale public result struct
cannot bypass comparison validation. It then builds `RecreationPromptInput`
and calls the existing permission-aware exporter with explicit limits and policy.

The result exposes read-only `comparison()`, `input()`, `exported()`, `changes()`,
and `portable_text()`. Borrowed source/coding inputs must remain retained by the
caller. The portable text begins with the existing export verbatim, including
its mandatory recreation label and fidelity caveat. A composition appendix records
generated/unreviewed status, source classes/versions, attribution, coverage and
capture limitations, representative selection, method/parameters/coding record,
declared derivations, both group definitions/counts/caveats, copy candidates and
reviewer dismissals, and regeneration changes. Consumers must save
`portable_text()`, not just the lower-level kernel export, to keep this appendix.

## Evidence contract

- Every reported detail preserves all supporting representative source versions,
  source class, exact wording (quoted/escaped for one-line export), and UTF-8 byte
  ranges. The original wording remains directly inspectable in `comparison()`.
  Analyst codes group assertions; they are labeled coding, not verified facts.
- One coded value becomes one detail, including unique and dependent reports.
  Shared-list placement is not a corroboration claim. The detail separately names
  declared reported groups and candidate-adjusted reported groups. Quoted or copied
  accounts do not become extra independent support. Candidate dependence remains
  a candidate; dismissals and caveats are retained.
- Competing reported values become reciprocal alternatives **within that aspect**.
  Alternatives across different aspects are not a cross-product of hypothetical
  scenes and are not presented as reconstructed whole accounts. No variant wins
  by majority. All representative reports and omissions remain in comparison.
- Simulated accounts and accounts whose declared in-set lineage is only simulated
  do not contribute reported details or context. Their wording remains explicitly
  excluded in unknown notices and their origin remains in the source appendix.
  A wholly unsupported composition fails the existing empty-content rule.
- Stated unknowns retain the citing source and wording. Uncoded aspects retain
  coverage-aware omissions, not observed absence. Explicit required aspects absent
  from the coding become unknowns. Conflicted/uncoded/unsupported context remains
  unknown rather than silently choosing a value. Assumed context must reference a
  declared assumption. Apparent scale can carry only selected angular/relative
  reported appearance; it never establishes physical size or distance.
- Selected reference assets use the unchanged media policy: refuse unpermitted
  assets or omit with explicit notices. Permission declarations and supplied asset
  digests are not independently verified. This function loads no image bytes.

## Explicit regeneration

Pass an earlier result as the predecessor and use a new prompt version. The
composer does not mutate that result or its inputs. The new result records the
predecessor, caller rationale, added/removed source versions, changed accounts,
added/removed/changed details, and changed method, candidate reviews, context,
assumptions, unknowns, conflicting-variant structure, assets/policy, export limits,
or title. The export budgets and asset policy also travel in the portable text.
Detail IDs are SHA-256-derived from
the aspect and coded value, with a fixed little-endian 64-bit aspect-length prefix;
they stay stable as other aspects are added or ordered. This is identity for
change comparison, not evidence authenticity or an implementation proof.

Source bytes and source metadata cannot change under a version retained from the
immediate predecessor. Recoding those sources requires a new coding version;
changing the coder also requires a new coding version. Adding/removing sources
can reuse existing coding for unchanged versions. Prompt identity/case must match,
and prompt version IDs cannot reuse an ID from the retained predecessor chain.
The chain is capped at 64 predecessors; exceeding it refuses rather than truncates.

This is not a persistent version registry: callers must enforce version uniqueness
across branches/restarts and source versions removed before the immediate
predecessor. The common evidence envelope, authorization, timestamps, durable
retention, reassessment propagation and job publication fencing remain #10/#11/#14.
Regeneration is an explicit function call, not a desktop watcher or automatic job.

## Operational bounds

Comparison retains its existing source, text, assertion, pair-search and control
bounds. Composition additionally refuses more than 128 compared aspects, 128
required aspects, 32 assumptions or 32 assets. Individual option text is nonempty
and at most 4,096 UTF-8 bytes. A 262,144-byte aggregate budget conservatively
charges option text, cited wording, escaped-text expansion and repeated provenance
before building detail strings. The caller's `RecreationLimits` still govern
kernel validation/export; the final portable text is capped at 1,048,576 bytes.
No output is silently shortened. Typed errors distinguish comparison, media,
identity/coding, option, resource and stop failures.

Cancellation/deadline/supersession checks run on entry, throughout comparison,
between aspects, before media export, and immediately before returning the final
result. Bounded media rendering and appendix construction are synchronous between
checks. A stop returns no composition. A host still must fence stale completions
before saving/showing results. No filesystem, network, generation or hardware
mutation occurs here.

## Local validation

The read-only synthetic example exercises mixed source classes, partial capture,
declared/candidate copied wording, conflicting colour, unknown scale, a simulated
account, permitted/omitted reference declarations, and a new prompt version after
adding an account:

```text
cargo run -p oia-app --example compose-recreation --locked
```

The fixture stands in for reported police/news/testimony inputs; it is not real
testimony, real photography, or a measured detector evaluation. Two focused
contract tests in `crates/oia-app/tests/recreation_composer_contract.rs` target
the critical evidence-preservation, non-promotion, permission and immutable-version
contracts, rather than adding a speculative test matrix. Executed commands and
outcomes are recorded in the PR.

## Remaining #139 scope

- A desktop/corpus assertion-coding and comparison/composer view beside the prompt,
  with the existing no-clobber save path and single-witness workflow retained.
- Automatic regeneration triggers with explicit user-visible new-version records;
  durable evidence-set versions and dependency correction/reassessment handling.
- Persisted generated/reviewed status and cross-session identity/permission checks.
- Automatic assertion extraction, if separately evaluated as inference.
- Negative-keyword integration with #140 and explicitly linked cases under #141.
