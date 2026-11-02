# Witness capture kernel

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
not generated, evaluated or displayed by this API.

This is transient state, not a persisted evidence record. A host must authorize
case access, add the shared envelope and capture timestamps, durably retain the
original before displaying explanations, and persist transitions atomically.
There is no serialization, durable save, sky sketch, regional attachment, event
grouping, source validation, UI or service integration. Those depend on the open
foundation and presentation packages. No Lean property or completed A04/A13
scenario is claimed. No new dependency or architecture decision is introduced.

Run the synthetic local workflow:

```text
cargo run -p oia-domain --example capture-witness --locked --offline
```

The two targeted `witness_contract` tests protect the critical original-preservation
and rejected-edit contracts, including workflow exposure ordering, missing fields,
stale/reused identifiers, malformed and oversized text, and the history budget.
They do not validate a durable application workflow or empirical testimony.
