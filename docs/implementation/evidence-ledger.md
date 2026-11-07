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

- `derived_from` and `calibrated_by` are derivation edges. They follow the class and label
  rules above.
- `quotes`, `republishes` and `possible_same_event` only record lineage or grouping. They may
  form citation cycles and do not propagate reassessment.
- Cross-object `supersedes` is ordered (below) but does not propagate reassessment. A
  same-object replacement uses `revise`, not this edge.
- `supports` and `contradicts` propagate reassessment but are not derivations.

Every relation must satisfy these rules:

- **Ordering.** For `derived_from`, `calibrated_by` and `supersedes`, the upstream must have
  been issued before the dependent. Issue order is a topological order, so these edges cannot
  form a cycle, and the check is a map lookup rather than a graph walk. To add a later
  derivation, revise the dependent and declare the input in the new version's provenance.
- **No duplicates.** A second edge with the same kind, dependent and upstream is refused.
  Provenance inputs count as `derived_from` edges.
- **Time.** A relation cannot be created before either endpoint, and a revision cannot be
  created before its predecessor.
- **Illustrations are not evidence.** An illustrative recreation, or anything carrying the
  illustration label, cannot be the upstream of `supports` or `contradicts`. An analytical
  inference, forecast or simulation may be cited as support. Its generated tier stays visible
  on the upstream version and is not copied to the claim.

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
(version, replaced, replacement) triple is flagged at most once, and only newly flagged
versions are expanded. The walk therefore terminates on evidential cycles and visits each
propagating edge at most once per replacement. The replaced version is never flagged by its
own replacement, even when a `supports` cycle leads back to it. Neighbours are visited in
version-ID order, so results are deterministic. A version issued from a superseded or
flagged input inherits the corresponding flags immediately. Adding a propagating relation
copies the upstream's flags to the dependent and its downstream versions. A re-run produces
a new version linked to its inputs. Flags are not cleared in place.

Import never walks the graph except to propagate flags. Each walk expands at most as many
versions as the stored-flag limit allows and queues each propagating edge at most once. The
ordering and duplicate checks are `O(log n)` lookups.

> **Limitation.** Flags are never removed, so the stored-flag budget is consumed for the
> life of a ledger. Once it is exhausted, any revision or relation that would add a flag
> fails with `LimitExceeded`, while operations that add no flags still succeed. This lasts
> until flags can be resolved by recorded review (see the remaining boundary).

Default limits (`LedgerLimits`):

| Limit | Default |
| --- | --- |
| Versions | 65,536 |
| Relations | 262,144 |
| Stored flags | 1,048,576 |
| Propagation steps per mutation | 262,144 |
| Propagation steps across one JSON import | 1,048,576 |
| Inputs per version | 64 |
| Attributes per version | 64 |
| Access policies per version | 16 |
| Evidence references per relation | 64 |
| Retained text value | 8,192 UTF-8 bytes |

Exceeding a limit returns `LimitExceeded` without a partial write.

### Propagation work limits

The stored-flag limit bounds retained results. The two work limits additionally bound
repeated traversal, including dense graphs with many paths to the same dependent. They
apply to `issue`, `revise` and `relate` while planning, before any ledger mutation:

- One step is charged before copying each inherited flag (including a direct replacement
  notice), or before queuing each propagating edge candidate, including revision seeds.
- Duplicate, already flagged and cycle-return candidates still consume steps. They can
  require work even when they add no stored flags. Inheritance from multiple inputs also
  charges each candidate before deduplication.
- A mutation shares one budget across inheritance, seed creation and breadth-first
  expansion. Budget checks precede the corresponding copies/queue growth. Pending
  candidates and planned flags are bounded by the charged steps; new stored flags also
  obey the stored-flag limit. No flag list is cloned in full before checking the budget.
- Direct API calls get a fresh per-mutation budget. JSON replay resets that budget for
  each entry and also consumes a shared import budget that cannot reset between entries.
  The counters are transient planning state, not evidence or part of the JSON schema.

Exhaustion returns `EvidenceError::LimitExceeded` with `LimitKind::PropagationSteps` or
`LimitKind::ImportPropagationSteps` and the configured limit. Import wraps it in
`CodecError::Rejected` with the zero-based entry index and returns no partial ledger.
A refused direct mutation leaves versions, heads, successors, relations, flags and accepted
entries unchanged. A later operation can still succeed within its own fresh budget.

These are complete-or-refuse controls: success retains every required flag and the existing
deterministic breadth-first `via`/`hops`; exhaustion never silently truncates propagation.
Callers may explicitly configure `LedgerLimits` for a larger supported workload. There is
no automatic retry with raised limits. A ledger built through many individual API calls
can require a larger aggregate budget to reimport than the default allows.

The budgets count propagation candidates, not elapsed time or total process memory.
Parsing, validation and non-propagating operations retain their existing byte, record,
relation and field limits. Import still parses both a typed document and a shape tree;
the separately tracked double-parse peak-memory follow-up remains open. Cancellation,
deadlines and application-worker scheduling remain part of future service integration.

## JSON form

`to_json` and `from_json` implement the accepted JSON round-trip format of
[ADR 0006](../adr/0006-d05-report-and-bundle-formats.md). The document has
`format: "oia.evidence-ledger"`, `schema_version: 1` and the accepted operations in order.

- Output is deterministic pretty-printed JSON with a trailing newline. Attributes are sorted
  by name.
- Import refuses inputs over 64 MiB before parsing.
- Import rejects unknown fields, duplicate fields, unknown codes, malformed IDs, duplicate
  attributes and unsupported schema versions.
- Every structure must be a JSON object, and every key must be present, including nullable
  ones written as `null`. serde alone would accept positional arrays and treat a missing
  `Option` key as `null`, so the parsed tree is checked for shape before replay.
- Each entry is replayed through the same planning/validation paths as `issue`, `revise` or
  `relate`, with a shared import work budget, so a file cannot bypass the
  promotion, label, ordering, duplicate, time, staleness or limit rules.
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

The five `evidence_ledger_contract` tests cover:

- the byte-exact fixture round trip, original preservation, unknown reasons and exact flag
  placement;
- rejection of promotion, label shedding, out-of-order and duplicate edges, illustration
  cited as support, stale and backdated revisions and origin changes, all without mutation,
  plus flags inherited from a superseded input;
- strict import: oversize, unknown field, duplicate field, positional array, missing
  nullable key, unknown code, and a promotion injected into the file;
- exact work boundaries on a converging, cyclic graph, including refusal before seed
  allocation and during expansion, full-ledger equality after refusal, preserved original
  versions and deterministic breadth-first flags on success;
- budgeted inherited flags on all three mutation paths, fresh direct-operation budgets,
  and cumulative replay exhaustion on an already flagged relation that adds no flags.

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

### Migrations still owed to this vocabulary

These provisional types predate `oia_domain::evidence` and still need to be mapped onto it
or replaced:

| Existing type | Target |
| --- | --- |
| `oia_analysis::reasoning::EvidenceOriginClass`, `oia_documents::lineage::DerivationOriginClass`, `oia_export::OriginClass` | `OriginClass` |
| `oia_news_claims::ClaimAssessmentState` | `ClaimAssessment` |
| `oia_news_claims::ReviewStatus`, `oia_export::ReviewStatus` | `ReviewStatus` |
| `oia_news_claims::AvailabilityState`, `oia_documents::lineage::AvailabilityState` | `Availability` |
| `oia_documents::lineage` `InvalidationPropagationReport` | `ReassessmentReport` and flags |
| `oia_news_claims::ClaimReviewLedger` | ledger relations plus future review records |
| `oia_analysis::research_links` | typed `Relation`s |

### Lean model

The walk resembles [`DependencyInvalidation`](../../proofs/OiaProofs/DependencyInvalidation.lean)
and [`DependencyBounds`](../../proofs/OiaProofs/DependencyBounds.lean), with differences:

- the model includes seeds, while the ledger never flags the replaced version;
- the ledger follows only propagating relation kinds;
- `ReassessmentFlag::hops` corresponds roughly to the edge count of a `DependencyPath`.

**No Rust–Lean correspondence is claimed.** That bridge needs its own shared fixture and
verification record under `#10`/`#14`. The A03/A07 scenarios are not complete. `#10` remains
open.

## Validation (2026-11-04, Windows x86_64, Rust 1.99.0, base `main` @ `753fd2c`)

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- `cargo test --workspace --locked`: 208 passed, 0 failed. This includes the three
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

## Work-budget validation (2026-11-05)

Checked on Windows x86_64 (OS build 26200, Intel Core Ultra 7 265F), pinned Rust
1.99.0, branch `codex/ledger-propagation-budget-20261105` based on `main` at
`f039c66`, with the work-budget changes described above:

- `cargo test -p oia-domain --test evidence_ledger_contract --locked`: all five passed.
  The two added tests protect atomic refusal, complete deterministic flags and cumulative
  accounting; they are not a broad graph test matrix.
- `cargo fmt --all -- --check` and
  `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- `cargo test --workspace --locked`: 229 passed, zero failed or ignored.
- `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run.ps1 proofs`: passed;
  290 declarations audited and Lean/Rust cutoff fixture bytes matched. No proof changed,
  and no dependency-walk correspondence is established.
- `cargo build -p oia-domain --example evidence-ledger --locked`, followed by the
  executable's `fixture` command: matched all 10,896 canonical fixture bytes after
  normalizing checkout CRLF line endings in the reference JSON.
- `git diff --check`, relative documentation links and code fences: passed.

Two temporary **synthetic** compact JSON stress files exercised the built debug CLI's
`inspect` command with default limits. They use the committed fixture's first source-record
shape with library scope, empty attributes/access policies and synthetic IDs; no real
observations were imported. Each `supports` edge is directed from upstream to dependent.
Both failures occurred before the CLI could print an accepted ledger:

- **Aggregate replay:** 104 roots each support 100 left nodes; every left node supports
  all 100 right nodes. Issue all records and edges first, then correct each root once.
  This is 304 initial records, 20,400 edges, 104 corrections and 20,808 JSON entries
  (5,398,392 bytes). Each correction needs 10,100 steps, within the per-mutation limit.
  Entry 20,807 was refused with `ImportPropagationSteps`, limit 1,048,576. One run took
  2,180 ms wall time including CLI startup, parsing and failure cleanup.
  SHA-256: `76e8ed69e0be8f7e0b1e40cce8cb8baaec2b3b7ef7851d97a6fdb0457b7e3227`.
- **Single-mutation fan-out:** 27 roots support a hub; a separate bridge supports
  100 left nodes, each supporting all 100 right nodes. Correct the roots, then add the
  hub-to-bridge relation. This is 229 initial records, 10,128 edges, 27 corrections and
  10,384 JSON entries (2,709,313 bytes). The last relation needs 272,727 steps and was
  refused at entry 10,383 with `PropagationSteps`, limit 262,144. One run took 1,044 ms.
  SHA-256: `0589a0978b8f8a9c17b9a44824b9c4f1b24ca17bd9aabc12774814653350123a`.

These are individual local debug runs, not performance guarantees. The temporary stress
files are not shipped; the compact permanent contract tests reproduce the boundary and
rollback behavior. The unrelated missing-CesiumJS build warning remains; no UI, hardware,
hosted workflow or complete A01/A03/A07 integration was exercised. The larger #10 scope
and the optional import-memory follow-up remain open.
