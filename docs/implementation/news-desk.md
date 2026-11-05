# Local News desk

Implemented, session-only desktop slice of [#156](https://github.com/mESmaC/oia/issues/156), supporting the accounting and inspection subsets of NEWS-01, NEWS-02, NEWS-05, GEO-03 and A06. It replaces the disabled News rail item with a working local dossier reader. It does **not** complete [#20](https://github.com/mESmaC/oia/issues/20), live feed acquisition, source authorization, durable cases, reversible event editing, claim review or full A06/A19. Shared evidence/services remain with #10/#11/#13/#14/#17/#19.

Public feeds now load automatically above this local reader through the separate
[shared collection service](public-collection.md) (#166). This note describes the
secondary local dossier workflow; its offline sample remains dated and partial.

## Use

Run the existing desktop workbench with `cargo run -p oia-app --locked` and select **News**, then expand **Import your own news dossier / offline sample**. **Import news JSON** opens a native file picker. **Load web news snapshot** explicitly loads the real, attributed [example dossier](../../crates/oia-app/examples/news-dossier.json): a dated offline snapshot with partial coverage, not live news. The same file can be imported through the picker. No local dossier is preloaded; the public panel has its own automatic acquisition and source coverage. Filter by exact text, declared region and report category, then select an article version to inspect its text and provenance. **Reset filters** restores the full dossier; **Unload dossier** removes the Rust-held snapshot and presentation view. Replacement discards the previous session snapshot only after success. No files are modified.

The same Rust path can be exercised without a GUI or desktop dependencies:

```text
cargo run -p oia-app --example inspect-news --no-default-features --locked --offline
cargo run -p oia-app --example inspect-news --no-default-features --locked --offline -- crates/oia-app/examples/news-dossier.json Dimorphos
```

With no arguments, that explicit example command inspects the real-news snapshot, including its dated title, partial coverage and provenance. `.gitattributes` disables text conversion for the sample and regression fixture. Output is a JSON view, including retained text, so treat redirected output according to the imported material's permissions. The local reader/CLI does not acquire network content or export bundles; public collection is documented separately above.

## Bundled web-source snapshot (#163)

The sample contains three historical NASA DART articles: [impact announcement](https://www.nasa.gov/news-release/nasas-dart-mission-hits-asteroid-in-first-ever-planetary-defense-test/) (visible publication date September 27, 2022), [orbit-change announcement](https://www.nasa.gov/news-release/nasa-confirms-dart-mission-impact-changed-asteroids-motion-in-space/) (October 11, 2022), and [shape/orbit study](https://www.nasa.gov/missions/dart/nasa-study-asteroids-orbit-shape-changed-after-dart-impact/) (March 19, 2024). These are real publisher statements, not OIA observations or independent corroboration. All use one source ID, `nasa`; origin, event grouping, event time, version history and retraction status are unassessed. The last article is a separate publication, not an invented correction of the earlier articles.

One selected paragraph per page was retrieved on **2026-11-05 UTC**, with exact per-item UTC times in the dossier. A one-off development HTTPS request used a 30-second timeout and 2 MiB response cap per page, without credentials. UTF-8 source paragraph HTML is retained verbatim under `provenance.original_excerpt_html`; Python 3.12 `HTMLParser` decoded character references and removed tags to produce `text`, without whitespace or Unicode normalization. The extraction is a separate representation, not the original HTML. The inspector renders both as inert text. Full page HTML, navigation, other paragraphs, media and captions are deliberately omitted; no full-article or complete-source coverage is claimed. Existing text/dossier SHA-256 values establish byte integrity only, not authenticity or truth. Publication dates preserve the visible day precision, without substituting midnight or retrieval time.

Each item carries its own attribution and [NASA reuse policy](https://www.nasa.gov/nasa-brand-center/images-and-media/) reference. Access was ordinary public reading, processing was local informational text extraction, retention is limited to the selected paragraph and original HTML fragment, and redistribution is informational NASA-content reuse with attribution and no implied endorsement under that policy. No images, logos, third-party quotations or third-party media are included; NASA material is not relicensed under OIA's code license. These conditions do not grant a future polling/scraping integration permission: current source conditions and operational limits must be checked separately then. NOAA was excluded after a direct HTTP 403 and USGS after two direct HTTP 503 responses; no restriction was bypassed, and those omissions do not imply absence of reporting. This narrow selection does not complete A06/A19 or live ingestion on #20.

The former fictional example survives only as the explicitly synthetic [regression fixture](../../crates/oia-app/tests/fixtures/synthetic-news-dossier.json), preserving correction, syndication and missing-page checks. It is not loaded by the sample button or default CLI example.

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
| `retraction_status` | Shared `oia_news_claims::RetractionStatus`: `unknown`, `not_retracted`, `corrected_by_later_version`, `correction_issued` or `retracted`. Use explicit `unknown` when unassessed, not an invented `not_retracted`. Existing dossier states retain their meaning. These remain imported declarations, not a review result. |
| `availability` | Separate shared `AvailabilityState`: `available`, `reference_only`, `missing`, `restricted`, `removed`, `expired` or `corrupt`. Missing local text does not change availability or imply retraction. The shared removed-page/retraction guard applies. |
| `predecessor_version_id` / `superseded_by_version_id` | Optional explicit correction links. Both endpoints must be included, belong to the same record/source and link reciprocally without cycles. Correction states require their corresponding link. No chronology is inferred and all versions remain visible. |
| `region` / `url` | Optional unnormalized place label and reference string. No geocoding, link activation, remote image loading or URL fetch occurs. |
| `published_time` / `updated_time` / `event_time` | Optional separate original time declarations, including uncertain wording/timezones. These are bounded strings, not validated timestamps. OIA does not infer chronology, freshness or event time from publication time. Retrieval time, if known, is separate in `provenance`; language is not represented. |
| `text` | Optional imported article text. Null/omitted means not retained; supplied whitespace-only/empty text is refused. Text is not downloaded, authenticated or interpreted as instructions. |
| `provenance` | Optional object; omission/null means unreported. If present, all seven string fields are required: `retrieved_time` (160-byte cap), `excerpt_locator` (2048), `original_excerpt_html` (65,536), `extraction_note` (2048), `attribution` (512), `rights_url` (2048) and `usage_note` (4096). It declares acquisition time separately from publication/update/event time, identifies the retained excerpt and its extraction, and distinguishes access/processing/retention/redistribution conditions. These are imported declarations, not authorization to fetch or proof of rights. |

All identifiers use 1–128 ASCII letters/digits or `-_.:/@`. Titles, publisher and region are at most 512 UTF-8 bytes; URL and coverage note 2,048 bytes; each time declaration 160 bytes; text 64 KiB per version. Dossiers are regular UTF-8 JSON files, at most 2 MiB and 200 versions. Invalid JSON, duplicate version IDs, inconsistent source labels or record/source identity, excessive sizes and unsupported enums are typed errors. JSON parser nesting limits remain enabled. The 2 MiB byte limit applies before parsing; version and lineage count limits are validated after the bounded JSON parse, not by a streaming visitor. This is an application-local interchange format, **not** the canonical evidence envelope or an A06 manifest importer.

Rust retains the entire original JSON byte sequence and computes its SHA-256. The per-text hash covers the UTF-8 encoding of the decoded JSON text string, **not** the bytes of a fetched article/HTML page. Hash equality establishes byte identity, not authenticity, independence or truth. JavaScript uses text nodes and mirrors `oia_domain::text::requires_visible_unicode_escape` for hidden control/bidi characters, preserving tab/newline and language/emoji ZWNJ/ZWJ joiners without altering the retained Rust bytes. Synthetic metadata, independent of the imported title, labels the News header, shell readout and both session lists as declared synthetic / not observed evidence. No markup, links or source-supplied commands execute.

## Queries, accounting and lifecycle

Search is an exact, case-sensitive substring match against each version's title, publisher or retained text (not across fields). Queries are limited to 256 UTF-8 bytes. Region and category filters combine with search. Results preserve input order, not chronological order. The UI marks edited, unapplied filters, separates retained totals from visible counts, and reports zero matches only within this dossier.

The app calls `oia_news_claims::publication::publication_inventory` for version/record/source accounting, including each visible source roster subset. That shared metadata-only boundary validates unique versions and consistent record ownership. Inventory and corpus verification call the same duplicate-version and availability/retraction guards, retaining their validation order and cooperative stop points. The shared `validate_correction_links` checks reciprocal same-record links and rejects cycles; Serde remote adapters serialize the existing kernel enums rather than defining new domain states. The source roster is an inventory, **not** an assessed `PublisherSummaryInput`.

The optional top-level `lineage` array contains at most 400 declarations: `edge_id`, `relation_kind` (`quotes`, `republishes`, `supersedes`, `supports`), `from_version_id` and `to_version_id`. Edge IDs must be unique and both distinct endpoints must exist in the dossier. The extracted `validate_declared_lineage` is also called by the existing `verify_corpus`, preserving its self-edge, reference and non-independent republishing/quotation rules. The desk supplies no independent-support claim, and shows outgoing declarations in the inspector even if their targets are filtered out.

Origin IDs remain inspectable, but **no origin-group or independence total is calculated**. The UI additionally counts proposed event labels, unknown-origin versions, ungrouped versions and versions without local text as presentation coverage, not domain assessments. Equal text from two publishers remains two records; versions remain separate. No count is an incident frequency, crime rate or truth score. Origin/event IDs may disagree between versions; they are preserved declarations.

The full corpus verifier requires passage anchors, independent-root declarations and assessed publisher windows; the document-lineage kernel requires retained-artifact/retrieval-time contracts; statement comparison requires incident-account identity and coded wording anchors. This dossier cannot honestly supply those inputs. They are not fabricated to run an independence/claim assessment, and no such result is shown. The shared inventory extension handles only the available declarations; it does not authenticate them, invalidate dependent claims, or replace the full kernels.

Native commands stage parsing and a complete view before replacing the held dossier under the workbench's existing single-operation/cancellation publication gate. File dialog cancellation, parsing failure and cancelled work leave the prior snapshot intact. Queries pin the expected dossier hash and reject stale inputs. Work runs off the UI thread with the existing 30-second cooperative processing deadline (dialog time excluded). File reads are byte-bounded; an OS-level blocking read is not forcibly interrupted. The UI serializes its own operations; the shared gate refuses overlapping work from other instruments. No polling, providers, credentials, new permissions, third-party dependencies, toolchain changes or hosted CI are introduced.

## Reference study and deliberate differences

On 2026-11-03, inspected local World Monitor at `fb266469d595b4626e420236bbfa57fed4587263`: `src/components/NewsPanel.ts`, `src/components/news/source-provenance.ts` and `publisher-roster.ts`. Its source-roster/inspection organization informed this design. Its source-tier-derived credibility badges are **not** copied or treated as evidence of truth. The checkout is AGPL-3.0; **no source code, datasets or assets are incorporated**. This is an independent Rust/vanilla-JS implementation of OIA's own contracts, not a port or compatibility layer.

Also inspected God's Eye View at `0d1a68160e532ba65dc34b48ea205de6de4028fc`, `src/data/lifecycle.js` (MIT, Bilawal Sidhu): lifecycle intent, cancellation and unavailable-state separation informed the review. No source code/assets copied. Neither reference application or reference test suite was executed. See [selective reuse inventory](../prior-art.md).

## Verification scope

The compact Rust regressions target evidence/identity preservation, declared reprint inspection, shared availability/retraction states, reciprocal correction links, dangling lineage, correction cycles, withheld origin totals, filter scope, bounds, malformed inputs and cancellation. An application regression exercises the same replacement method as native imports and rejects stale hashes. These assertions cover critical contracts, not speculative coverage goals. The example executes the actual parser and query path for offline inspection.

Execution evidence and environment are recorded on the delivery PR. Native GUI accessibility/layout, OS file dialogs, other platforms, real feeds, durable storage, formal Rust/Lean correspondence and full A06/A19 must not be inferred from parser, DOM-handler or workspace checks. No proof-backed algorithm is changed or new formal proof claimed.

Local checks executed on 2026-11-03, Windows x86_64, Rust 1.99.0 (b940084d7), Node 24.18.0:

- Workspace formatting, all-target Clippy with warnings denied, and workspace tests passed with the existing lockfile and offline cache.
- Three targeted regression functions passed: two news-dossier contracts and the desktop atomic-replacement/stale-hash contract. The actual example parser/query path ran with desktop features disabled; the desktop binary's status command also ran.
- JavaScript syntax checks passed for app, News, Witness and Globe. An in-process Node VM/minimal-DOM probe exercised explicit example loading, selection, missing text, filtering, cancelled/failed import preservation, literal markup/bidi escaping and unloading against Rust-generated views. This was not browser or native WebView rendering.
- Review follow-up: an ephemeral Rust/Node probe compared the actual News escaping function with the shared Rust predicate for all 1,112,064 Unicode scalar values, with the documented tab/newline exception; all matched. Actual News handlers and the extracted shell renderer passed neutral-title synthetic labelling in the header/readout/tray/session list, ordinary and unloaded controls, U+061C/U+E0061 escapes, permitted joiners, literal markup, failed/cancelled import preservation and hash-pinned filtering. These remain minimal-DOM checks, not native WebView validation.
- The existing Lean 4.35.0 gate passed, auditing 290 declarations and matching the Rust/Lean cutoff fixture bytes. Its pre-existing model claims are unchanged.
- Documentation relative-link/fence checks passed. The existing build warns that optional Cesium assets are absent; this does not affect the offline News instrument. No hosted jobs were started.
