# Local records-request drafts

This implements only the dependency-independent drafting slice of [#36](https://github.com/mESmaC/oia/issues/36), [INTEL-04](../spec.md) and the draft-without-submission subset of [A08](../delivery.md). It is a transient Rust kernel, not the complete public-records workbench or a persisted domain schema.

[`RecordsRequestDraft`](../../crates/oia-analysis/src/records_request.rs) validates caller-supplied draft wording, a proposed custodian, the author's assessment and rationale, and exact source-version identifiers. Fields are private after construction and inspectable through `input()`. Revisions require construction of another draft; the caller supplies its version ID. The kernel does not enforce global version uniqueness or resolve source access, existence, or relevance.

The plain-text `preview()` is a display derivative; `input()` retains the exact original wording and custodian rationale. Every custodian, rationale and request line starts with `> `, including empty and trailing lines, so field text cannot create an unquoted preview heading or reference. Source-reference order stays intact. Backslashes render as `\\`, tabs as `\t`, and the supported display-control ranges render as uppercase `\u{XXXX}` escapes: U+061C, U+200B–U+200F, U+2028–U+202E, U+2060–U+2064, U+2066–U+2069 and U+FEFF. Literal backslashes are escaped too, distinguishing source text that already contains an escape from a rendered control character. Other text, including ordinary non-ASCII wording, is unchanged. This is a bounded plain-text representation, not a comprehensive Unicode confusable detector.

Every preview is labeled **DRAFT — NOT SUBMITTED**, qualifies candidate custodians as unconfirmed, and identifies missing research references. `AuthorConfirmed` records the author's assessment, not independent verification. Text is research data and is never interpreted as tool instructions. Callers embedding the preview in HTML, Markdown or another format must escape it for that format rather than interpreting its field lines as markup.

Input limits are 32,768 UTF-8 bytes of request wording, 2,048 bytes each of custodian and rationale, and 64 unique source-version IDs. Empty fields and control characters are rejected; multiline wording/rationale allow LF and tab. CR is rejected without rewriting the source wording. Callers accepting CRLF documents must explicitly prepare a separate draft from their retained originals. Validation errors identify the field without echoing potentially sensitive content. Preview allocation is fallible and reserved within `MAX_PREVIEW_BYTES` (156,928 UTF-8 bytes) before formatting. Each field needs at most four output bytes per input byte, plus its line prefix and terminator; the remaining budget covers labels, separators and bounded IDs. No truncation or source-text normalization occurs.

The kernel performs no network, file, email, device or submission operations. It computes no statutory deadlines and makes no jurisdiction-specific legal claims. Workspace authorization, common envelopes, immutable persistence, response tracking, configurable deadlines, release comparisons and an explicitly authorized submission adapter remain future application/domain integration. No unmerged prerequisite implementation is imported, and no Lean claim is made.

Run the synthetic local example:

```text
cargo run -p oia-analysis --example draft-records-request --locked --offline
```

Local validation on 2026-11-02, Windows x86_64 MSVC, rustc 1.99.0 (`b940084d7`), based on `c7f4d9d`:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`
- `cargo test --workspace --locked --offline`
- The example above: inspected the synthetic preview, uncertainty label and explicit missing-reference state.

The original contract test covers exact UTF-8 input/reference preservation and rejects oversized multibyte text, absent rationale, terminal control characters, duplicate references and excess references. The preview regression test covers forged headings/references in multiline fields, trailing blank lines, visible bidi/invisible-control escapes, distinction from literal escapes, and maximum-size escape expansion without truncation. These assertions address evidence preservation, misleading preview structure and bounded untrusted input. They do not establish full A08 acceptance or application integration.

## Preview follow-up verification and handoff

The PR #81 preview follow-ups were implemented locally on 2026-11-02 by the Codex records-preview-local session, based on `e1d171ce0da9791771dc5b86bfd370135869e907`. On Windows x86_64 MSVC with pinned Rust 1.99.0, the following passed for the modified working tree:

```text
cargo +1.99.0 test -p oia-analysis --test records_request_contract --locked --offline
cargo +1.99.0 fmt --all -- --check
cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings
cargo +1.99.0 test --workspace --locked --offline
cargo +1.99.0 run -p oia-analysis --example draft-records-request --locked --offline
git diff --check
```

Both records-request contract tests passed. The synthetic preview was inspected; relative documentation links and code fences were checked. No hosted CI, live submission, UI acceptance or Lean verification was run or claimed.

The [verified claim](https://github.com/mESmaC/oia/issues/36#issuecomment-6591057957) covers only the three preview-related files. The user subsequently authorized a branch, commit, push, PR against `main`, and monitoring for review feedback. Delivery uses `codex/records-request-preview` in `C:/Users/engin/.t3/worktrees/oia/codex-records-request-preview`, initially based on `origin/main` at `2e26bad750fc35e60d9a90591548aa20fd47a9e5` and rebased onto current `main` before the PR. All checks above were repeated after rebasing before delivery. Only this session's verified edits were transferred; the shared checkout was restored to its original clean state without switching or updating its branch. Issue #36 remains open and its Project item In Progress pending review and remaining scope. This session owns PR follow-up fixes; the user owns the merge decision. The task worktree is retained while the PR is open and must be cleaned up after a confirmed merge. Other #36 implementation scope remains unclaimed by this session.
