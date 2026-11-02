# Report export and portable investigation contract

Status: implementation design for [#22](https://github.com/mESmaC/oia/issues/22),
selected by [D05 / ADR 0006](../adr/0006-d05-report-and-bundle-formats.md).
No runtime capability or passing acceptance scenario is claimed here. The
[architecture](../architecture.md) remains the owner of shared domain, permission,
storage, and job contracts. This document defines export/import behavior, not a
replacement domain schema or UI design.

## 1. Selection, authorization, and snapshots

The application service receives the workspace authorization context, selected
case and report versions, target format, requested evidence selection, redaction
choices, output destination, cancellation/deadline, and resource limits. A query
for an export preview must not write files or publish anything.

Resolve and pin the selected versions and their reference closure before rendering.
The preview identifies what will be included, what is reference-only, what will be
omitted, and which conclusions lack exportable support. A report snapshot includes
the selection, coverage, redaction policy/version, and creation time. It cannot
silently follow a newer source or report revision during export.

Authorization covers records, snippets, citations, blobs, embedded metadata, and
derived outputs. Recheck access before committing the export. If access is revoked
or the selected snapshot can no longer be exported under current policy, fail the
export and require a newly computed preview; do not silently change its contents.
Shared services must provide a revision/lease or equivalent commit boundary for
this check. A renderer cannot bypass it by reading the blob store directly.

Changes after a successful export cannot retract an offline copy. Later correction
or redaction creates a new export snapshot. Previously issued report versions stay
identifiable, subject to retention/deletion policy; no export updates them in place.

## 2. Editable reports and citations

The authoritative report contains structured, ordered blocks with stable block and
statement identities, immutable report versions, actor attribution, and the shared
generated/review/assessment states. Renderers consume an authorized projection of
these records. Editing Markdown outside OIA does not automatically update the
structured report or preserve its review status.

The projection covers question/scope, coverage, chronology, source lineage,
relevant context/media, hypotheses, supporting and conflicting evidence, bounded
conclusions, open questions, citations, and the evidence inventory. Empty sections
use an honest state such as no material selected; they must not generate findings.
Source limitations and uncertainty remain adjacent to the claims they qualify.

Each statement-to-evidence link retains the exact source record version and, where
applicable, artifact/extraction version and typed anchor. Retain the anchor's
coordinate space: UTF-8 byte range, extracted passage, page region, frame/sample,
or another registered kind. A text offset into an extraction is not an offset into
the original PDF. Do not recalculate anchors against normalized export text.

On export, validate references and anchor bounds against the pinned target when
that target is available. A missing/restricted target keeps an explicit availability
state and permitted citation metadata. It is not reported as a validated anchor.
Unsupported anchor types require a visible limitation. Structural resolution does
not establish semantic support or promote generated text to human-reviewed text.

Readable citations show a stable citation label, source/release identity and version,
anchor description, and availability. Internal HTML fragment IDs are exporter-owned
opaque identifiers, never raw source markup or filesystem paths. Markdown retains
the same information in visible citation entries even without clickable anchors.
Different sources with identical bytes keep different citation identities.

A correction flags dependent report versions for reassessment through the shared
dependency service. An export states the reassessment status known at snapshot time;
an offline report cannot promise knowledge of later corrections. Import preserves
the origin snapshot and maps exact versions without substituting current local ones.

## 3. Redaction and omission

Redaction is a projection, never an edit of retained original bytes. A redacted
artifact is a separate conventional derivative with permitted processing history.
Its citations identify that derivative and its own anchors; original byte ranges
cannot be applied to rewritten text. Excluded original bytes must not be bundled
beside a redacted preview unless separately authorized for the same audience.

Remove excluded content from text, hidden layers, alternate text, embedded metadata,
thumbnails, filenames, URLs, link targets, structured records, and reproduction
parameters. Strip credential references and local endpoint/path configuration;
export only permitted provider identity and non-secret method configuration.
Use an allowlisted export projection rather than serializing application state.

Retain permitted omission reasons: not selected, retention prohibited,
redistribution prohibited, missing, removed, restricted, or unsupported. Availability
and omission reason are separate from truth, claim assessment, and review state.
For authorized-but-unexportable evidence, retain only metadata permitted for that
recipient. If even existence, count, identifier, or reason is sensitive, omit those
details too. A generic coverage limitation may be shown only if permitted; the
projection must not leak an excluded record through a dangling reference.

If a conclusion cannot be exported without exposing sensitive evidence, the preview
must require removing/redacting the conclusion or choosing an authorized audience.
It must not launder the conclusion into an unsupported-looking statement while
silently including the original sensitive payload elsewhere.

## 4. Readable output

Both renderers use the same pinned projection and retain generated, predictive,
simulated, and review labels as text. Color or transparency is never their only cue.
Chronology preserves original time precision and time references. A date-only source
must not become an exact midnight observation in an export.

HTML uses semantic headings, visible focus for links, readable print styles, and
escaped source text in text/attribute contexts. No script, event handlers, forms,
iframes, plugins, executable URLs, active SVG, or automatic external requests are
allowed. Inline styles come from the renderer, not source material. A restrictive
document policy is defense in depth, not a substitute for safe rendering. Permitted
raster previews are separate derivatives; unsupported/oversized media are explicit
references, not silently transcoded replacements for originals.

Markdown must neutralize raw HTML and escape source-controlled Markdown syntax so
quoted evidence cannot create images or arbitrary links. Images are not remotely
embedded. Deliberate external references may use sanitized HTTP(S) links, require
reader activation, and are labeled external; credential-bearing URLs and private
endpoints are excluded. Untrusted URLs can remain plain text when safe to disclose.
Different Markdown viewers can implement link previews; OIA's renderer never
fetches them, and OIA cannot guarantee behavior of an arbitrary external viewer.

Do not include local absolute paths in either output. HTML includes no network font
dependency or decorative live instrumentation. Print and reduced-effects rendering
retain the same evidence, labels, attribution, and readable citation text. Original
media bytes remain in the bundle when permitted, independently of report previews.

## 5. Portable directory bundle

The initial container is an uncompressed directory with this logical layout:

```text
manifest.json
records/          # Shared versioned domain records and relations
assets/           # Permitted original/derivative bytes, addressed by SHA-256
reports/          # Optional readable report snapshots
reproduction/     # Data-only run/dependency manifests
```

The manifest declares these semantic fields. Exact wire names and domain schema
versions must be finalized after prerequisite contracts merge, before implementation
claims format compatibility.

| Field group | Required meaning |
| --- | --- |
| Format identity | Bundle format/version, package identity, creation instant/time scale, producer identity/version. Package identity is not trusted authority. |
| Capabilities | Required and optional record, anchor, analysis, and schema capabilities with versions. Unsupported required capabilities block import. |
| Entry inventory | Every payload's relative path, role, media type, byte length, SHA-256, and schema version where structured. The manifest itself is the bounded inventory root, not an entry hashing itself. |
| Record inventory | Origin namespace and exact object/version identities with ownership context sufficient for permitted provenance, never executable grants. |
| References | Included targets or explicit permitted external/reference-only targets; content hashes do not replace source identities. |
| Omissions | Permitted excluded assets, reasons, coverage effects, and unreproducible steps, subject to section 3's disclosure limits. |
| Use conditions | Per-asset permission/attribution and retention/redistribution conditions, preserved separately from the OIA software license. |
| Reproduction | Input versions, queries, grouping decisions, calibration, analysis method/model/configuration identifiers, forecasts, and environment limits where selected. No secrets or executable hooks. |

Each required reference resolves to an included version or a typed, permitted
unavailable/reference-only target. An unexplained dangling reference is malformed.
Original source identities, immutable versions, origin classes, review history,
timestamps/precision, and uncertainty survive mapping. Imported actor names and
review assertions remain attributed to their source namespace; importing them does
not authenticate those people or authorize them in the destination workspace.

Identical blobs may share storage under destination isolation rules. Distinct
releases, sources, claims, and versions must not merge because bytes match. Imported
IDs map into a fresh local namespace while retaining permitted origin IDs and
version links. A conflicting duplicate identity in one package fails validation.
Reimport creates another private case or returns an explicit duplicate result under
the application's request-id contract; it never overwrites an existing case.

## 6. Bounded, staged import and export

Both operations require configured finite limits for total bytes, individual file
bytes, entries/records, references, string length, JSON nesting, and elapsed work.
The first implementation must publish defaults and configurable ceilings with its
validation evidence. No numerical limit is presented here as a measured safe value.
Enforce limits while reading/decoding, before unbounded allocation; advertised sizes
are untrusted. Cancellation checks are required during work and at publication.

Importer sequence:

1. Validate the bounded manifest and supported format/capabilities. Reject duplicate
   JSON object keys and non-finite numeric encodings; decode numeric values using the
   shared types without silently rounding exact financial quantities or timestamps.
2. Validate the complete inventory. Paths use relative `/`-separated components;
   reject empty/dot/parent components, absolute/drive/UNC paths, backslashes, colons,
   NUL/control characters, Windows device names, trailing dots/spaces, and colliding
   names under supported platform rules. Asset names must match their declared hash.
   Do not URL-decode paths. Reject duplicate entries and unlisted payload files.
3. Read payloads only from regular files within the selected root; layout directories
   must be ordinary directories. Reject symlinks, junctions, and reparse points in
   the root and every traversed component, and reject other non-regular entries.
   A pre-scan followed by unchecked
   path reopening is insufficient: use handle-relative/no-follow access or an
   equivalent platform-safe snapshot. Fail explicitly where safe access is unavailable.
4. Stream copies into a new, bounded, private staging area. Verify actual lengths
   and hashes over the staged bytes, rejecting changes, truncation, or excess input.
   Parse and validate those same staged bytes; do not later reopen untrusted input
   as the verified payload. Validate record schemas, IDs, references, typed anchors,
   origin classes, and supported numeric/time/geometry domains.
5. Construct the fresh local ID mapping and reference graph, authorize destination
   case creation, and validate the mapped graph. Required unsupported features fail;
   optional unsupported data may be retained as inert opaque bytes only if explicitly
   supported by the store, otherwise report it as omitted. Any permitted references
   to omitted data must retain a typed unavailable target; if a required capability
   depends on that data, fail import instead. Never label it executed or successfully
   replayed. Known records with invalid data fail even if optional.
6. Recheck cancellation/deadline, job lease, destination access, and resource budget.
   Commit the case, records, references, and blob registrations through the shared
   storage transaction/recovery protocol. Publish visibility only after a durable
   commit. Staged blobs remain unreachable on failure and are cleaned up through
   recovery; existing cases remain unchanged.

Export follows the same publication discipline: create a private staged output,
render/copy from the pinned authorized snapshot, compute inventory hashes, validate
references and limits, recheck authorization/job ownership, then publish to a fresh
destination without replacing existing files. Directory publication requires a
supported atomic same-filesystem move or equivalent storage protocol. Report an
unsupported destination if that guarantee cannot be supplied. Single-file reports
use an equivalent staged file publication. Do not leave a partial output looking
complete after disk exhaustion, cancellation, or restart.

Opening/importing never fetches references, installs models, runs analyses, executes
scripts, enables subscriptions, contacts providers, or connects devices. Readable
reports in an imported package remain untrusted; OIA must regenerate from validated
records or display through an inert viewer rather than execute supplied HTML. Replay
is a separate authorized application command with supported methods and pinned inputs.

The importer reports structured outcomes for unsupported format/capability,
invalid path/schema/reference/anchor, identity collision, integrity mismatch,
limit exceeded, access denied, input changed, cancelled, storage failure, and
successful import with declared limitations. A syntactically valid bundle with
permitted missing assets can succeed with limitations; a corrupt included asset
cannot be silently downgraded to missing. Error detail must not expose excluded
content, private paths, secrets, or another workspace's existence.

## 7. Implementation handoff and acceptance evidence

The following are planned checks for the runtime implementer, not a new test-suite
prerequisite and not executed acceptance results. Use compact fixtures and the
existing A03 corpus where it supplies original/citation cases; its fixture manifest
is not the production bundle schema.

| Contract / requirement | Evidence required before claiming runtime support |
| --- | --- |
| COLLAB-02; A16 | Export an editable report containing generated and reviewed blocks, conflicting evidence, coverage, and open questions. Confirm the same labels and exact citation targets in Markdown and HTML. |
| COLLAB-03; A16 | Preview redactions and export privately without publishing; inspect all resulting bytes/metadata for excluded content. Correct a source and show reassessment without replacing the issued snapshot. |
| REPRO-01; A17 | Round-trip originals, two sources sharing bytes, revised releases, references, omissions, calibration, queries, grouping, analyses, forecasts, and report records. Compare hashes and mapped version/anchor identities. |
| REPRO-02; A17 | Reject traversal, platform name aliases, links/reparse escapes, duplicate keys/IDs, corrupt bytes, oversized/nested input, unsupported required schemas, and a changed input during staging. Reject archive containers, including compressed bombs, without unpacking. Verify the existing case/store state remains unchanged. |
| REPRO-02; recovery | Interrupt at staging and commit boundaries, cancel/revoke the lease before publication, and exhaust destination space. Restart and confirm no partial visible case or complete-looking output. |
| Isolation; A16/A17 | Exercise recipient scopes and access revocation at commit; include references whose existence is sensitive. Confirm no hidden payload, credentials, origin permissions, or cross-workspace deduplication signal escapes. |
| UX-05 | Inspect HTML on screen and in print with keyboard navigation; inspect plain Markdown. Citation text, attribution, uncertainty, and status labels remain legible without effects or color. |
| Inert content; A17 | Open an imported report containing hostile markup and a bundle containing tool/model instructions. Confirm no automatic requests, code execution, model loading, device activity, or replay. |

Full A16 also depends on shared roles and concurrent editing; full A17 includes a
supported analysis replay. Neither is completed by this export design. The runtime
PR must name its actual commands, platform/toolchain, selected limits, fixtures,
results, and unsupported capabilities. Rust baseline checks apply once integrated;
this document introduces no Lean proof claim.
