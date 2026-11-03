# Witness capture kernel and session-only desktop workflow

This implements a dependency-independent slice of [#27](https://github.com/mESmaC/oia/issues/27),
[FIELD-02](../spec.md#55-personal-observatories-and-field-research) and the initial-account
portion of [A13](../delivery.md#a13-fieldwork-and-multi-observer-research).
The Rust API is in `oia_domain::witness`.

`WitnessCapture::new` validates and retains the initial form before this object can
record explanation exposure. `revise` appends an immutable version with its actor
and required rationale; it never replaces the initial form. Callers supply an
expected current version for revisions and exposure transitions. Stale edits and
reused version identifiers fail without changing history. Concurrent branches and
offline conflict resolution are outside this kernel.

Form fields preserve exact UTF-8 wording, including CRLF. Narrative is required;
optional time, location, duration, direction, apparent size, motion, sound and
conditions distinguish unreported (`None`) from an explicit report. Blank optional
values are rejected. These are reported descriptions, not normalized coordinates,
validated measurements or system inferences. No value is filled from a default.
The fixed reported/simulated origin applies to all versions.

Every text field and rationale is limited to 8,192 UTF-8 bytes; captures retain at
most 64 versions including the initial account (less than 5 MiB of form/rationale
text). Other controls than CR, LF and tab are rejected. Exceeding a budget returns
a typed error without dropping old versions. Storage reservation occurs before a
revision is appended. Caller-owned input allocations occur before validation.

`expose_explanations` records the version current at the first in-workflow
exposure, idempotently. Later revisions carry `after_explanations = true` and the
first exposure marker cannot be cleared. This records workflow ordering only:
`false` does not establish independent testimony or absence of exposure elsewhere.
External exposure must be disclosed in the account. Explanations themselves are
not generated, evaluated or displayed by this domain API.

## Session-only desktop presentation

`oia-app` now connects this kernel to the Tauri workbench through four commands:
`witness_view`, `witness_capture`, `witness_revise`, and
`witness_expose_explanations`. Rust owns one `WitnessState`, generates the
session-local capture/version/actor identifiers, checks expected versions, and
maps kernel failures to stable error codes. A rejected revision does not consume
a version identifier. The file-operation worker and globe state remain
independent.

The Witness instrument provides every form field and an explicit reported or
simulated origin. After capture it shows the initial account read-only, an
append-only revision editor with required rationale, and retained revision
history. Completely empty optional controls become `None`; nonempty values are
sent without trimming or measurement conversion. Source wording is rendered as
text, not HTML.

Candidate explanations are hidden until the initial capture succeeds. Explicit
exposure records the transition before the page reveals a static, unassessed
checklist. The checklist is not generated, ranked, evaluated, or presented as an
evidence-backed finding. Exposure markers cover this workflow only.

After capture, a Recreation panel in the same instrument can build an
illustrative recreation prompt from any retained version through the
`oia-media` recreation kernel (`MEDIA-04`); the mapping, limits and the
`.txt` export are described in the
[desktop workbench note](desktop-workbench.md#available-workflows). Building a
prompt reads the held account and changes nothing in it.

The interface uses **Capture**, not **Save**, and repeatedly labels the account
memory-only and discarded on exit. It is a workflow demonstration, not durable
evidence capture: the domain host requirement to retain the original durably
before showing explanations is not satisfied by this transient presentation.
Process restart intentionally loses the account.

## Remaining boundary

This is transient state, not a persisted evidence record. A production evidence
host must authorize case access, add the shared envelope and capture timestamps,
durably retain the original before displaying explanations, and persist
transitions atomically. There is no serialization, durable save, sky sketch,
regional attachment, event grouping, source validation, durable service
integration, or actual explanation analysis. Those depend on open foundation and
integration packages. No Lean property or completed A04/A13 scenario is claimed.
No new dependency or architecture decision is introduced.

Run the synthetic local workflow:

```text
cargo run -p oia-domain --example capture-witness --locked --offline
```

The two targeted `witness_contract` tests protect the critical original-preservation
and rejected-edit contracts, including workflow exposure ordering, missing fields,
stale/reused identifiers, malformed and oversized text, and the history budget.
One `oia-app` adapter test also covers generated version wiring, rejected-edit
atomicity, DTO origin/exposure state, and initial-account preservation. These
checks do not validate a durable application workflow or empirical testimony.

## Desktop slice validation (2026-11-03, Windows x86_64, Rust 1.99.0)

- `cargo +1.99.0 fmt --all -- --check`, `cargo +1.99.0 clippy --workspace
  --all-targets --locked --offline -- -D warnings`, and `cargo +1.99.0 test
  --workspace --locked --offline`: passed for the complete workspace.
- `cargo +1.99.0 test -p oia-app --lib --locked --offline`: six tests passed,
  including the witness adapter and existing desktop/globe gates.
- `node --check crates/oia-app/ui/witness.js` and `node --check
  crates/oia-app/ui/app.js`: passed.
- A temporary local Microsoft Edge headless harness loaded the actual UI files
  with witness-only Tauri IPC fixtures. Capture, exact optional fields, simulated
  origin, explanation gating, immutable initial text, rationale revision,
  history, and version projection passed. Fixture responses are simulated IPC,
  not observations or a live provider.
- `cargo +1.99.0 build -p oia-app --locked --offline` passed. A temporary Windows
  UI Automation driver then exercised the real `oia.exe` and Tauri IPC: reported
  capture, unreported optional fields, exposure gating, revision, initial
  preservation, and `witness-v2` history passed. The test process was closed
  afterward. This was one Windows walkthrough, not a cross-platform or
  screen-reader conformance claim.

The temporary harness files are local verification artifacts under `target/tmp`
and are not part of the implementation or a hosted CI configuration.
