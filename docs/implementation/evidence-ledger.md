# Canonical evidence ledger kernel

This delivers a bounded domain slice of [#10](https://github.com/mESmaC/oia/issues/10):
the shared evidence vocabulary and version graph from
[architecture §2](../architecture.md#2-identity-versions-and-evidence), supporting
[CASE-03](../spec.md#31-investigation-workspace) (source records stay unchanged by later
work) and the version-history part of
[REASON-03](../spec.md#45-reasoning-and-additional-intelligence-methods). It covers the
domain subsets of [A03](../delivery.md#a03-cited-document-research) (a correction marks
dependents without rewriting them) and [A07](../delivery.md#a07-inspectable-reasoning)
(a generated inference keeps its type, and changing an input triggers downstream
reassessment). The Rust API is `oia_domain::evidence`.

## Contract

**Origin classes.** `OriginClass` has the ten classes from §2.3. Each belongs to a tier:

| Tier | Classes |
| --- | --- |
| Source | acquired source material, user testimony, normalized observation, extracted claim, conventional derivative |
| Generated | analytical inference, forecast, predictive derivative, simulation, illustrative recreation |

A generated input can only produce a generated output, so an inference fed into a later
step cannot become an observation, claim or conventional derivative. Provenance must also
match the class:

- Acquired source material requires `Acquired { source }`.
- User testimony requires `Manual`.
- Normalized observations, extracted claims and both derivative classes require `Derived`,
  with at least one exact input version.
- Inferences, forecasts, simulations and recreations may be `Manual` or `Derived`.

`InheritedLabels` records simulation and illustration lineage. A version inherits these
labels from its own class and from every derivation input. A later `derived_from` or
`calibrated_by` edge cannot add a label that the issued dependent lacks; that requires a new
version. `From<witness::AccountOrigin>` maps the transient witness origins: `Reported` maps to
`UserTestimony` and `Simulated` maps to `Simulation`.

**Common envelope.** `RecordFields` carries:

- the stable object ID and immutable version ID;
- the object type and origin class;
- the OIA creation time and creating actor;
- the case or library scope and access-policy references;
- provenance;
- review status and availability;
- typed attributes;
- claim fields, which are required for `claim` objects and rejected for every other type.

The ledger adds the schema version, predecessor, revision reason and rationale, and the
inherited labels. Creation time is OIA's own time, stored as Unix milliseconds within years
0001–9999. Source, event, capture and publication times are separate concerns of `#12`. Until
then they remain verbatim attributes.

**Unknown values.** Every attribute and claimant is a `FieldValue`: either `Known` or
`Unknown(reason)`. The reasons are:

- not recorded by source;
- not reported;
- not yet processed;
- processing failed;
- restricted;
- not applicable.

Text is kept verbatim, including CRLF and any Unicode. It is checked only for size and, for
propositions and rationales, for being non-blank. Values can be text, an `i64` integer or a
boolean. Quantities with units wait for the shared model in `#12` rather than adding a second
unit vocabulary.

**States.** `ClaimAssessment`, `ReviewStatus` and `Availability` are independent
vocabularies with the §2.4 values. "Unsupported by available evidence" is not "false". A
`removed` page is not a retraction.

**Relations.** `Relation` is immutable and has its own ID, actor, time, review status, cited
evidence versions and rationale. Each relation joins a `dependent` version to an `upstream`
version. For `supports` and `contradicts`, the dependent is the claim being assessed and the
upstream is the evidence.

- `derived_from` and `calibrated_by` are derivation edges. They must stay acyclic and follow
  the class and label rules above.
- `quotes`, `republishes`, `possible_same_event` and cross-object `supersedes` only record
  lineage or grouping. They may form citation cycles and do not propagate reassessment.
- `supports` and `contradicts` propagate reassessment but are not derivations.

## Ledger and reassessment

`EvidenceLedger` is append-only and in memory. It has three operations:

- `issue` creates an object.
- `revise` appends a version. The caller supplies the current head as `predecessor`, so a
  stale edit is rejected. Object type, origin class and scope cannot change. A revision
  reason (`revision`, `source_correction`, `retraction` or `calibration_change`) and a
  non-blank rationale are required.
- `relate` adds a relation.

Each operation validates fully and plans its effects before mutating anything. A rejected
operation leaves the ledger equal to its previous state.

A revision walks breadth-first from the replaced version along propagating edges: derivation
inputs, `derived_from`, `calibrated_by`, `supports` and `contradicts`. It attaches a
`ReassessmentFlag` to each reachable version. The flag records:

- the replaced version and its replacement;
- the revision reason;
- the immediate upstream on the path;
- the hop count along the first path found.

Flags sit beside versions, and the flagged versions are never modified. Each
(version, replaced, replacement) triple is flagged at most once. This makes the walk
terminate on evidential cycles, and the flag limit bounds its work. Neighbours are visited
in version-ID order, so results are deterministic. A version issued from a superseded or
flagged input inherits the corresponding flags immediately. Adding a propagating relation
copies the upstream's flags to the dependent and its downstream versions. A re-run produces
a new version linked to its inputs. Flags are not cleared in place.

Default limits (`LedgerLimits`):

| Limit | Default |
| --- | --- |
| Versions | 65,536 |
| Relations | 262,144 |
| Stored flags | 1,048,576 |
| Inputs per version | 64 |
| Attributes per version | 64 |
| Access policies per version | 16 |
| Evidence references per relation | 64 |
| Retained text value | 8,192 UTF-8 bytes |

Exceeding a limit returns `LimitExceeded` without a partial write.

## JSON form

`to_json` and `from_json` implement the accepted JSON round-trip format of
[ADR 0006](../adr/0006-d05-report-and-bundle-formats.md). The document has
`format: "oia.evidence-ledger"`, `schema_version: 1` and the accepted operations in order.

- Output is deterministic pretty-printed JSON with a trailing newline. Attributes are sorted
  by name.
- Import refuses inputs over 64 MiB before parsing.
- Import rejects unknown fields, duplicate fields, unknown codes, malformed IDs, duplicate
  attributes and unsupported schema versions.
- Each entry is replayed through `issue`, `revise` or `relate`, so a file cannot bypass the
  promotion, label, cycle, staleness or limit rules.
- Labels and flags are derived state. They are recomputed on import, never trusted from the
  file.
- Integers are exact `i64`. Consumers limited to IEEE doubles must not round them.

This is the domain schema that a later bundle transport (`#22`/`#39`) wraps. It is not a
bundle manifest, a signature or a storage format.

The only new dependency edges are `serde` and `serde_json` on `oia-domain`. Both pins were
already in `Cargo.lock`, so no package was added. The domain core still has no renderer,
HTTP, database, model or vendor dependency.

## Fixture and inspection

[`fixtures/domain/evidence-ledger-correction.json`](../../fixtures/domain/evidence-ledger-correction.json)
is a **synthetic** scenario, not observed data. It contains:

- a newspaper source record with an unknown byline and a CRLF excerpt;
- a witness account with unreported fields;
- a normalized observation, an extracted claim, an inference, a simulation, and a comparison
  that inherits the simulation label;
- a `supports` edge and a `possible_same_event` edge;
- a source correction of the article.

After the correction, the observation and the claim are flagged at hop 1, the inference at
hop 2 and the comparison at hop 3. The witness account, the simulation and both article
versions are not flagged.

```text
cargo run -p oia-domain --example evidence-ledger --locked -- inspect fixtures/domain/evidence-ledger-correction.json
cargo run -p oia-domain --example evidence-ledger --locked -- fixture
```

`fixture` regenerates the committed bytes from the builder in
[`examples/evidence-ledger.rs`](../../crates/oia-domain/examples/evidence-ledger.rs).

The three `evidence_ledger_contract` tests cover:

- the byte-exact fixture round trip, original preservation, unknown reasons and exact flag
  placement;
- rejection of promotion, label shedding, derivation cycles, stale revisions and origin
  changes, all without mutation, plus flags inherited from a superseded input;
- strict import: oversize, unknown field, duplicate field, unknown code, and a promotion
  injected into the file.

## Remaining boundary

This is a domain kernel. It does not provide:

- persistence, content hashes, blobs, transactions or migration (`#11`, SQLite per ADR
  0002);
- application services, authorization enforcement, access-policy evaluation or durable
  jobs (`#14`);
- UI (`#16`);
- source, event or publication time models (`#12`);
- typed payloads for every principal object in §2.2;
- review records with assessment history, or clearing a flag by recorded review;
- bundle transport.

The provisional origin types `oia_analysis::reasoning::EvidenceOriginClass`,
`oia_media::recreation::ContentOriginClass` and `oia_news_claims::OriginClass` still need to
be mapped onto `OriginClass`. The reassessment walk resembles the
[`DependencyInvalidation`](../../proofs/README.md) model, but **no Rust–Lean correspondence
is claimed**. That bridge needs its own shared fixture and verification record under
`#10`/`#14`. The A03/A07 scenarios are not complete. `#10` remains open.

## Validation (2026-11-04, Windows x86_64, Rust 1.99.0, base `main` @ `190bca6`)

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- `cargo test --workspace --locked`: 205 passed, 0 failed. This includes the three new
  `evidence_ledger_contract` tests and the existing `oia-domain` witness and collaboration
  contracts.
- `scripts/run.ps1 proofs` (Lean 4.35.0): passed. The audit covered 290 declarations and the
  cutoff fixture bytes matched. No Lean file changed.
- `cargo run -p oia-domain --example evidence-ledger -- fixture` reproduced the committed
  fixture byte-for-byte (`fc /b`). `inspect` printed the flags listed above.
- `Cargo.lock` diff: only the `serde` and `serde_json` dependency edges were added to
  `oia-domain`. No package was added or changed.

The desktop app build only warned that the gitignored CesiumJS vendor directory was absent.
That is unrelated to this change. No UI or hardware path was exercised.
