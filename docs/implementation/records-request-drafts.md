# Local records-request drafts

This implements only the dependency-independent drafting slice of [#36](https://github.com/mESmaC/oia/issues/36), [INTEL-04](../spec.md) and the draft-without-submission subset of [A08](../delivery.md). It is a transient Rust kernel, not the complete public-records workbench or a persisted domain schema.

[`RecordsRequestDraft`](../../crates/oia-analysis/src/records_request.rs) validates caller-supplied draft wording, a proposed custodian, the author's assessment and rationale, and exact source-version identifiers. Fields are private after construction and inspectable through `input()`. Revisions require construction of another draft; the caller supplies its version ID. The kernel does not enforce global version uniqueness or resolve source access, existence, or relevance.

The plain-text `preview()` retains exact wording, source-reference order and custodian rationale. It labels every preview **DRAFT — NOT SUBMITTED**, qualifies candidate custodians as unconfirmed, and identifies missing research references. `AuthorConfirmed` records the author's assessment, not independent verification. Text is research data and is never interpreted as tool instructions. Callers embedding the preview in HTML or another format must escape it for that format.

Input limits are 32,768 UTF-8 bytes of request wording, 2,048 bytes each of custodian and rationale, and 64 unique source-version IDs. Empty fields and control characters are rejected; multiline wording/rationale allow LF and tab. CR is rejected without rewriting the source wording. Callers accepting CRLF documents must explicitly prepare a separate draft from their retained originals. Validation errors identify the field without echoing potentially sensitive content. Preview allocation is fallible and reserved within a fixed upper bound before formatting.

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

The single added contract test covers exact UTF-8 wording/reference preservation and rejects oversized multibyte text, absent rationale, terminal control characters, duplicate references and excess references. These assertions address evidence preservation and bounded untrusted input. They do not establish full A08 acceptance or application integration.
