# Local file privacy and cleaned-copy contract

**Status: proposed implementation contract; no inspector is implemented here.**
This is the dependency-independent design slice of [issue #46](https://github.com/mESmaC/oia/issues/46),
based on specification v0.1 and repository revision `8307ac1` (2026-11-01,
America/New_York). It selects no parser library and establishes no passing
product acceptance or formal proof claim.

The controlling requirement is PRIVACY-01 in the
[product specification](../spec.md#73-personal-awareness-and-privacy).
The applicable acceptance scope is the file-inspection portion of
[A18](../delivery.md#a18-privacy-and-change-monitoring). Account matching,
infrastructure inspection and public change monitoring are outside this slice.
Runtime implementation still depends on [media handling #30](https://github.com/mESmaC/oia/issues/30)
and [export handling #22](https://github.com/mESmaC/oia/issues/22). Neither is
assumed merged or copied into this design.

## 1. User workflow and ownership

1. Select a local file or authorized retained artifact. Inspect a stable,
   read-only snapshot, not an input pathname that can change during processing.
2. Review format-specific findings, inspected regions, unsupported regions,
   resource-limit failures and content-level limitations.
3. Choose a cleanup profile and preview what it removes, retains, cannot inspect
   or would change beyond metadata. No unrelated approval screen is required.
4. Explicitly create a separate cleaned copy. Reopen and re-inspect its exact
   bytes; check original preservation and the profile-specific invariants.
5. Save the verified derivative privately. Sharing or publishing is a separate
   action through the existing export workflow.

Use the existing [artifact/analysis records](../architecture.md#2-identity-versions-and-evidence),
[application services](../architecture.md#7-application-interfaces-and-events),
and [permission boundary](../architecture.md#9-access-collaboration-and-local-operation).
Do not introduce a second evidence store or authorization system.

| Owner | Responsibility |
| --- | --- |
| `oia-domain` | Versioned artifact identity, derivative links, processing/review references and typed inspection results |
| `oia-media` | Bounded local parsing, format profiles, cleanup and reinspection; no network authority |
| `oia-store` | Immutable retained bytes, staged outputs, transactions/recovery and workspace isolation |
| `oia-app` | Authorized inspection queries and explicit, cancellable cleaned-copy commands |
| `oia-export` | Safe output names, omission preview, derivative export and export receipts |

Inspection does not modify retained bytes, resolve embedded URLs, run source
code/macros, invoke models or connect devices. Report filesystem timestamps
separately from embedded timestamps. Operating-system access times may change
during reads; original preservation concerns retained bytes, not an unchanged
external file's access time or continued availability.

## 2. Findings and coverage

An inspection result must identify the input artifact/version, SHA-256 and byte
length, detected format/version, parser/profile version, run time, coverage and
failure reasons. Hash equality establishes byte identity, not authenticity or
truth. Equal bytes do not merge source identities or access grants.

Each finding identifies its structural location, category, detection method and
supported action. Categories include embedded location, creator/author,
timestamps, device/software identifiers, thumbnails/previews, textual properties
and unrecognized extension data. Distinguish decoded fields from opaque
containers; do not infer an identity or location from unknown content.

Raw metadata values, thumbnails and paths are sensitive. Show them only in
authorized details; exclude them from logs, search snippets and default receipts.
Persist detailed findings only under the input's workspace policy and explicit
retention. Source fields containing executable-looking text remain data.

Keep these dimensions separate rather than returning a single clean flag:

| Dimension | Required distinctions |
| --- | --- |
| Execution | Completed, unsupported, malformed, unauthorized, unavailable, resource-limited, cancelled, failed |
| Inspection coverage | Complete for the named profile, partial with uninspected regions/reasons, unknown |
| Finding action | Selected removal, retained by choice, required for rendering, opaque/unsupported, content-level exposure |
| Cleanup verification | Not requested, not attempted, verified for named targets/profile, rejected, incomplete |
| Human review | Not reviewed, or reviewed with actor/time/version reference |

No supported metadata found requires completed inspection, complete coverage for
the named profile and an empty finding set. Partial coverage with zero findings
does not establish absence across the file. An unavailable optional scanner must
not silently downgrade a promised inspection.

Verified means only the named targets were removed under the stated profile.
Reusing the writer's list of removed fields is not reinspection: reopen and parse
the final staged bytes. A shared parser can share blind spots; acceptance evidence
needs a structurally different check or independent reader for the claimed
container absence, with its own limits stated.

Do not claim anonymity or removal of all personal information. Visible content,
faces, text, landmarks, voices, watermarks and steganographic information may
remain; this workflow does not detect or remove them. Cleaning a copy does not
erase other copies, filesystem history or previously published content.

## 3. Format-specific support

A format is supported only for an implemented, validated profile and its exact
subtypes. Neither a filename extension nor a successful metadata-library call
establishes support. Detect and validate the container before offering cleanup.
This table proposes a rollout boundary, not a shipped compatibility list:

| Format/profile | Proposed boundary |
| --- | --- |
| Static PNG, conservative chunk profile | First candidate: enumerate the container; remove selected text, Exif and modification-time chunks; preserve encoded image data and supported rendering chunks; re-inspect output |
| PNG with embedded color profiles or unknown chunks | Inspect recognized regions, disclose the remainder; no complete cleanup claim until retained payload inspection and rendering effects are specified |
| Animated PNG | Outside the static profile; needs separate frame/timing preservation and metadata coverage |
| JPEG/TIFF/HEIF and other media | Unsupported initially; separate profiles must cover metadata containers, previews, orientation and rendering semantics |
| PDF and office documents | Unsupported initially; property deletion alone is insufficient for a file-wide claim without coverage of embedded objects and document structures |
| Archives and encrypted/password-protected files | Unsupported unless separately implemented; no recursive extraction, password attempts or assumed absence of hidden content |

The PNG distinctions follow the W3C
[PNG Third Edition Recommendation](https://www.w3.org/TR/2025/REC-png-3-20250624/),
particularly its chunk definitions and extension rules. A chunk's safe-to-copy
flag is not a privacy assessment.

The implementing maintainer must publish explicit container/chunk retention and
removal lists, parser versions and unsupported conditions. Library defaults must
not define the guarantee implicitly. Do not silently drop opaque data, convert
formats, flatten animation, recompress media, lower resolution/bit depth or
remove color/orientation information to pass verification. Such changes require
a separate, explicitly selected derivative profile.

If selected metadata shares a container with required rendering/orientation
fields, refuse the conservative profile unless those fields can be preserved
and verified. Deleting an entire Exif or profile container is not automatically
a metadata-only change.

For conservative static PNG cleanup, require byte-identical concatenated encoded
image-data payloads, unchanged image parameters and palette/transparency
semantics, and unchanged supported rendering metadata. Validate order, lengths
and checksums in input and output. These checks establish the named structural
invariants, not perceptual equivalence or anonymous visible content.

## 4. Cleaned-copy lifecycle

1. Resolve authorized input to immutable bytes. Reject output aliases, hardlinks
   or symlinks that could overwrite input; use managed paths, not embedded
   filenames. Record original hash and length.
2. Pin the plan to input version, profile, selected targets, retained regions,
   unsupported conditions, parser versions and resource budget. Bind the
   command's idempotency key to this complete plan; different selections need a
   new plan/key.
3. Write to job-owned private staging with bounded size and storage reservation.
   Never rewrite the original or use metadata-supplied destination paths.
4. Reopen and re-inspect staged bytes under the same pinned coverage contract.
   Verify every selected removal, required retained structure, original byte
   integrity and profile-specific content invariant. Parse errors, unresolved
   opaque regions or failed invariants prevent a verified result.
5. At commit, recheck authorization, input/plan identity and cancellation or
   supersession through common store/job contracts. Atomically save derivative,
   provenance and verification receipt before exposing completed output.
   Repeated commands return the same committed result, not duplicate artifacts.
6. On failure/cancellation, preserve the original and existing case. Record an
   actionable failure without raw metadata. Recover/remove only this job's
   staging using the store lifecycle; do not claim secure erasure. Interrupted
   commits recover to one coherent result or no result.

An incomplete inspection can be useful, but an unverified candidate must not
appear under the successful cleaned-copy download action. Any separate
unverified export must be deliberate and labeled with its gaps; it is not an
automatic fallback for failed cleanup.

External-directory export uses the exporter's recoverable no-clobber path and
safe filenames that do not repeat source names, paths, authors or locations.
Cancellation before final publication must not expose partial output; after
commit, report completion honestly. Failed external writes must not delete the
verified managed derivative or overwrite another file.

## 5. Bounded offline execution

Each implemented profile must declare finite limits for input/output bytes,
chunk/container count, individual and cumulative metadata bytes, decoded text,
nesting, image dimensions/pixels, concurrent jobs, wall time and staging storage.
Validate lengths, offsets and arithmetic before reads/allocation. Streaming image
data does not eliminate metadata decompression limits or decoded-pixel limits
when those paths are exercised.

Use shared cancellation/deadline budgets throughout parsing. An uninterruptible
worker requires a terminable bounded execution boundary. Inability to enforce a
required bound makes the profile unavailable, not best-effort supported. No
parser library, production subprocess or numerical default is selected here;
production logic remains Rust under the agreed architecture.

Inspection/cleanup are offline. Do not send bytes, hashes, findings or prompts
to a remote cleaner or fetch references discovered inside files. A future remote
capability needs its own scoped data-handling decision, not a hidden retry.

## 6. Privacy-safe export handoff

Private analysis may link original and derivative versions for reproducibility.
That does not authorize exporting originals, raw findings, source paths/names,
thumbnails, embedded location or credential references. Default shared receipts
contain output hash/length, format/profile/tool versions, removed category names,
retained-category/coverage limits and verification outcome. Exclude original
hashes and stable internal IDs that could correlate output with private evidence.
Export time or original provenance is included only when explicitly selected in
the export preview.

When a destination requires exact evidence/version links, use the existing
authorized export policy and disclose what those links reveal. A minimal
privacy-oriented copy without private lineage is a derivative with omitted
provenance, not a fully reproducible investigation bundle. Preserve the complete
private history; do not fabricate provenance to conceal an omission.

Preview every emitted file, sidecar and receipt, including any originals, source
references, IDs or timestamps. Omit-originals must exclude underlying bytes and
metadata, not merely hide links. The guarantees in
[architecture section 10](../architecture.md#10-exports-and-portability) still
apply; this slice defines no alternative bundle schema and does not settle D05.

## 7. Implementation evidence and remaining work

The runtime implementer owns parser selection, numerical defaults, platform
handling and the actual support list. Record implementing revision, dependencies
and local results. Prefer these focused critical checks, not a broad speculative
test matrix:

- Inspect one synthetic supported file with embedded identifiers and visible
  content. Clean/reopen it; independently verify selected removal, unchanged
  original hash and the conservative profile's retained-content invariants.
- Exercise one opaque region and one bounded malformed/oversized input. Explain
  coverage; neither may yield a misleading complete/verified result or an
  uncontrolled allocation.
- Interrupt/cancel cleanup and attempt an output alias. Establish no original
  overwrite, visible partial copy or late completion; demonstrate coherent
  recovery through the shared storage/job path.
- Export with an identifying source filename, sidecar and private lineage.
  Inspect every emitted byte/record for forbidden disclosure while preserving
  private originals/history.

These are future acceptance obligations, not new fixtures, executed checks or
passing A18 evidence. Full A18 also needs #47/#48's separate work. There is no
Cargo/Lake workspace at this base revision, and this documentation adds none.
Widening support must update the profile, failure/coverage semantics, user-facing
limits and independent evidence together. Profile revisions never rewrite old
receipts or silently enlarge their removal claims.
