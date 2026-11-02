# Original and translated passage comparison

[`oia_analysis::multilingual`](../../crates/oia-analysis/src/multilingual.rs)
implements the dependency-independent passage-review slice of
[#36](https://github.com/mESmaC/oia/issues/36),
[INTEL-04](../spec.md#45-reasoning-and-additional-intelligence-methods) and
[A08](../delivery.md#a08-additional-intelligence-methods). It compares supplied
translations against an exact UTF-8 extraction. It does not generate translations
or establish their accuracy.

## Implemented behavior

`compare(&ComparisonInput, check_control)` returns an immutable borrowed
`Comparison`. It performs no allocation, I/O, provider invocation or publication.
The caller retains the input; `input()` exposes the same unchanged text, references,
metadata, review notes and alternatives. `alternatives()` yields each translation
in supplied order with structural byte coverage. No alternative is selected as
correct or counted as independent corroboration.

The source reference contains a source ID, source version and extraction version.
Every alternative must name that same reference and have a distinct derivative
version that does not reuse either input version. The selection and each segment
use half-open byte offsets in the **full extraction**, including when the selection
starts in the middle of a document. Each supplied original quote must match that
range byte for byte. Newlines, leading/trailing whitespace, composed/decomposed
Unicode and control characters are preserved; nothing is silently normalized.

Each alternative must partition the entire selection in order. Every nonempty
segment either supplies translated text or an explicit omission reason. Gaps,
overlaps, reversed/out-of-bounds ranges, split UTF-8 characters, mismatched quotes,
wrong versions and missing suffixes are rejected. Coverage counts selected source
bytes, source bytes mapped to translations and explicitly omitted bytes. These are
structural counts, **not translation quality scores**. A whole-paragraph translation
can omit meaning even when its byte mapping covers the selection. The original
extraction can also be partial; required extraction notes remain visible alongside
the selection and full supplied-text size.

Languages are caller-declared free-text labels, with `None` for an unknown source
language; there is no language detector or BCP 47 validator. Methods retain either
a human actor ID or provider/model/run version references. These are declarations,
not independently verified attribution. Optional attributed review notes preserve
disagreement and their review versions. No supplied review means unreviewed;
supplying a note does not confer system approval or promote translated/model text
into an observation. Free-text limitations are required for each alternative.

This module uses only merged domain identifiers. Its inputs are transient
comparison arguments, not an additional common envelope or persisted evidence
model. It follows the provider separation and structural-versus-semantic validation
boundary in [ADR 0007](../adr/0007-d04-ai-execution-and-grounding.md) without choosing
or running a model. The authorized application service must resolve immutable
source bytes, enforce access/use conditions, authenticate reviews and attach the
shared evidence origins/envelopes before persistence. A caller can lie about the
relationship between its bytes and IDs; this kernel cannot resolve an external
store or detect that lie. A correction requires new pinned input and comparison;
automatic dependency invalidation awaits application integration.

## Bounds and failure behavior

- Supplied extraction: 1,048,576 UTF-8 bytes; selected passage: 65,536 bytes.
- At most eight alternatives, each with at most 256 segments; both lists nonempty.
- Each translated segment: 65,536 bytes; each language label, limitation, review
  note and omission reason: 2,048 bytes, with non-whitespace content required.
- All quoted text, translated text and free-text metadata together: 524,288 bytes.
  Source text has its separate bound. Opaque identifiers retain the domain's
  128-byte cap. Oversized input is rejected rather than truncated.
- Cancellation/deadline/supersession checks occur on entry, before each alternative
  and segment, and immediately before returning. An interrupted comparison returns
  a typed error and no result. Individual bounded string checks are not preempted.

The caller owns input loading/allocation and must bound it before calling. The
kernel checks lengths/counts before scans and quote comparisons, uses checked
aggregate accounting and safe UTF-8 slicing, and does not allocate result buffers.
Errors identify fields and zero-based alternative/segment indexes without echoing
the submitted content. The service must also recheck job ownership at publication.

## Run locally

The example runs offline and prints two **synthetic** alternatives, one with an
explicit omission. Its model attribution is simulated; no model was executed.

```text
cargo +1.99.0 run -p oia-analysis --example inspect-translation --locked --offline
```

To inspect your own UTF-8 source and a supplied human translation, pass explicit
version references, target language and the caller-declared translator ID:

```text
cargo +1.99.0 run -p oia-analysis --example inspect-translation --locked --offline -- --files original.txt translated.txt source-a source-v1 text-v1 translation-v1 en translator-a
```

Both files are read-only and individually bounded to 65,536 bytes before UTF-8
decoding. This mode maps the whole source to one translation segment, reports
unknown source language and unverified original-document coverage, and supplies no
review. Use the library API to describe partial/multiple-segment alternatives and
model attribution. Exit 0 means structural inspection succeeded (including explicit
omissions); exit 1 means an error. Output states actual coverage and never uses the
exit code as a translation-completeness assertion.

The CLI prints text in quoted ASCII-escaped form to expose controls, bidirectional
formatting, line breaks and field boundaries. Exact multilingual UTF-8 remains in
`input()`; the display escaping never changes citation offsets or source bytes.
The output is an inspection aid, not a report/export format or a graphical reader.
Consumers displaying raw fields must provide appropriate escaping/isolation.

## Verification scope

The compact [contract checks](../../crates/oia-analysis/tests/multilingual_contract.rs)
address evidence preservation and bounded untrusted input: nonzero source offsets,
multibyte/decomposed text, CRLF and controls, conflicting alternatives and review
notes, honest omission counts, exact version/quote binding, malformed ranges and
partitions, byte/count/aggregate limits, and entry/intermediate/final stop signals.
They do not evaluate linguistic accuracy, identity authenticity or a model.

Executed locally on 2026-11-02, Windows x86_64 MSVC, rustc 1.99.0
(`b940084d7`), against base `e1d171c` plus this slice:

- `cargo +1.99.0 fmt --all -- --check` — passed.
- `cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings` — passed.
- `cargo +1.99.0 test --workspace --locked --offline` — passed, including both new contract checks.
- The synthetic example above — inspected both alternatives, versions, review
  notes and the explicit 29-byte omission in the second alternative.
- Direct file-mode execution using temporary UTF-8/CRLF/control-bearing inputs —
  unchanged SHA-256 hashes, quoted control escaping and unreviewed state confirmed.
  Malformed UTF-8, oversized/empty inputs, reused derivative version, missing file
  and invalid arguments returned exit 1 without a comparison result.
- New-file whitespace, documentation links/anchors/fences and Git diff checks — passed.

The workspace formatting, Clippy and test checks also passed after rebasing this
slice onto `origin/main` at `2e26bad` (test rerun used `--quiet`).

This is local verification on Windows, not cross-platform acceptance. File-mode
reads are synchronous and byte-bounded; no preemptive filesystem-I/O deadline is
provided by the inspection example.

Application authorization/persistence, automatic correction propagation, actual
translation providers, transliteration/alternate-name workflows and graphical
review remain open on #36 and its prerequisites. Full INTEL-04/A08 acceptance and
Lean proofs are not claimed. No unmerged prerequisites were copied.
