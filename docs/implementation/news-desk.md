# Local News desk

Implemented, session-only desktop slice of [#156](https://github.com/mESmaC/oia/issues/156), supporting the accounting and inspection subsets of NEWS-01, NEWS-02, NEWS-05, GEO-03 and A06. It replaces the disabled News rail item with a working local dossier reader. It does **not** complete [#20](https://github.com/mESmaC/oia/issues/20), live feed acquisition, source authorization, durable cases, reversible event editing, claim review or full A06/A19. Shared evidence/services remain with #10/#11/#13/#14/#17/#19.

## Use

Run the existing desktop workbench with `cargo run -p oia-app --locked` and select **News**. **Import news JSON** opens a native file picker. **Load synthetic example** explicitly loads the fictional [example dossier](../../crates/oia-app/examples/news-dossier.json); nothing is preloaded or presented as a live feed. Filter by exact text, declared region and report category, then select an article version to inspect its text and provenance. **Reset filters** restores the full dossier; **Unload dossier** removes the Rust-held snapshot and presentation view. Replacement discards the previous session snapshot only after success. No files are modified.

The same Rust path can be exercised without a GUI or desktop dependencies:

```text
cargo run -p oia-app --example inspect-news --no-default-features --locked --offline
cargo run -p oia-app --example inspect-news --no-default-features --locked --offline -- crates/oia-app/examples/news-dossier.json fourteen
```

With no arguments, that explicit example command inspects the synthetic sample. Output is a JSON view, including retained text, so treat redirected output according to the imported material's permissions. No network acquisition or bundle export is implemented.

## Dossier contract

The top-level object requires `schema: "oia-news-dossier/1"`, `title`, `synthetic` (boolean), `collection_coverage` (`complete`, `partial` or `unknown`), a nonempty `coverage_note` describing the selected collection's scope/omissions, and `articles`. Unknown fields and unknown enum values are rejected rather than discarded. An empty article array is valid and distinct from missing collection coverage. Even `complete` is only an imported declaration about the stated collection, never a claim of complete coverage of the world or a region.

Each article version has:

| Field | Meaning |
| --- | --- |
| `record_id` / `version_id` | Stable article-record declaration and globally unique version declaration within this dossier. Versions are never merged or overwritten. A record must retain its source ID. |
| `source_id` / `publisher` | Declared source identity and consistent display name. Separate source IDs do not establish separate independent witnesses. |
| `title` | Imported headline, displayed as text. |
| `origin_id` | Optional common-underlying-source grouping declaration; null/omitted means unknown, not independent. |
| `event_id` | Optional proposed event grouping declaration; null/omitted means ungrouped. No geographic or event matching is performed. |
| `kind` | `plan`, `reported_observation`, `allegation`, `arrest`, `charge`, `judgment` or `attributed_statement`. Categories remain imported assertions, not OIA findings. |
| `publication_state` | Declared `published`, `corrected`, `retracted` or `unavailable`. Missing text or an unavailable page does not cause retraction. No automatic supersession, latest-version selection or downstream invalidation. |
| `region` / `url` | Optional unnormalized place label and reference string. No geocoding, link activation, remote image loading or URL fetch occurs. |
| `published_time` / `updated_time` / `event_time` | Optional separate original time declarations, including uncertain wording/timezones. These are bounded strings, not validated timestamps. OIA does not infer chronology, freshness or event time from publication time. |
| `text` | Optional imported article text. Null/omitted means not retained; supplied whitespace-only/empty text is refused. Text is not downloaded, authenticated or interpreted as instructions. |

All identifiers use 1–128 ASCII letters/digits or `-_.:/@`. Titles, publisher and region are at most 512 UTF-8 bytes; URL and coverage note 2,048 bytes; each time declaration 160 bytes; text 64 KiB per version. Dossiers are regular UTF-8 JSON files, at most 2 MiB and 200 versions. Invalid JSON, duplicate version IDs, inconsistent source labels or record/source identity, excessive sizes and unsupported enums are typed errors. JSON parser nesting limits remain enabled. This is an application-local interchange format, **not** the canonical evidence envelope or an A06 manifest importer.

Rust retains the entire original JSON byte sequence and computes its SHA-256. The per-text hash covers the UTF-8 encoding of the decoded JSON text string, **not** the bytes of a fetched article/HTML page. Hash equality establishes byte identity, not authenticity, independence or truth. JavaScript uses text nodes and visible escapes for hidden control/bidi characters without altering the retained Rust bytes. No markup, links or source-supplied commands execute.

## Queries, accounting and lifecycle

Search is an exact, case-sensitive substring match against each version's title, publisher or retained text (not across fields). Queries are limited to 256 UTF-8 bytes. Region and category filters combine with search. Results preserve input order, not chronological order. The UI marks edited, unapplied filters, separates retained totals from visible counts, and reports zero matches only within this dossier.

Rust separately counts article versions, distinct article records, declared source IDs, declared origin groups, proposed event groups, unknown-origin versions, ungrouped versions and versions lacking text. The visible source roster reports counts per declared source. Equal text from two publishers remains two records; multiple versions of one record stay multiple versions. Unknown origins are never silently assigned unique roots or counted as independent corroboration. No count is an incident frequency, crime rate or truth score. Origin/event IDs may disagree between versions; these are preserved declarations, not reconciled domain state.

Native commands stage parsing and a complete view before replacing the held dossier under the workbench's existing single-operation/cancellation publication gate. File dialog cancellation, parsing failure and cancelled work leave the prior snapshot intact. Queries pin the expected dossier hash and reject stale inputs. Work runs off the UI thread with the existing 30-second cooperative processing deadline (dialog time excluded). File reads are byte-bounded; an OS-level blocking read is not forcibly interrupted. The UI serializes its own operations; the shared gate refuses overlapping work from other instruments. No polling, providers, credentials, new permissions, dependencies, toolchain changes or hosted CI are introduced.

## Reference study and deliberate differences

On 2026-11-03, inspected local World Monitor at `fb266469d595b4626e420236bbfa57fed4587263`: `src/components/NewsPanel.ts`, `src/components/news/source-provenance.ts` and `publisher-roster.ts`. Its source-roster/inspection organization informed this design. Its source-tier-derived credibility badges are **not** copied or treated as evidence of truth. The checkout is AGPL-3.0; **no source code, datasets or assets are incorporated**. This is an independent Rust/vanilla-JS implementation of OIA's own contracts, not a port or compatibility layer.

Also inspected God's Eye View at `0d1a68160e532ba65dc34b48ea205de6de4028fc`, `src/data/lifecycle.js` (MIT, Bilawal Sidhu): lifecycle intent, cancellation and unavailable-state separation informed the review. No source code/assets copied. Neither reference application or reference test suite was executed. See [selective reuse inventory](../prior-art.md).

## Verification scope

The compact Rust regressions target evidence/identity preservation, syndication accounting, missing-versus-unavailable state, filter scope, bounds, malformed inputs and cancellation. An application regression exercises the same replacement method as native imports and rejects stale hashes. These assertions cover critical contracts, not speculative coverage goals. The example executes the actual parser and query path for offline inspection.

Execution evidence and environment are recorded on the delivery PR. Native GUI accessibility/layout, OS file dialogs, other platforms, real feeds, durable storage, formal Rust/Lean correspondence and full A06/A19 must not be inferred from parser, DOM-handler or workspace checks. No proof-backed algorithm is changed or new formal proof claimed.

Local checks executed on 2026-11-03, Windows x86_64, Rust 1.99.0 (b940084d7), Node 24.18.0:

- Workspace formatting, all-target Clippy with warnings denied, and workspace tests passed with the existing lockfile and offline cache.
- Three targeted regression functions passed: two news-dossier contracts and the desktop atomic-replacement/stale-hash contract. The actual example parser/query path ran with desktop features disabled; the desktop binary's status command also ran.
- JavaScript syntax checks passed for app, News, Witness and Globe. An in-process Node VM/minimal-DOM probe exercised explicit example loading, selection, missing text, filtering, cancelled/failed import preservation, literal markup/bidi escaping and unloading against Rust-generated views. This was not browser or native WebView rendering.
- The existing Lean 4.35.0 gate passed, auditing 290 declarations and matching the Rust/Lean cutoff fixture bytes. Its pre-existing model claims are unchanged.
- Documentation relative-link/fence checks passed. The existing build warns that optional Cesium assets are absent; this does not affect the offline News instrument. No hosted jobs were started.
