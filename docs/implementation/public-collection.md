# Public news and archival report collection

Implemented session-only slice of [#166](https://github.com/mESmaC/oia/issues/166),
supporting SOURCE-01–SOURCE-04, NEWS-01, GEO-02 and the interactive feed/API subset
of A19. Public collection is the primary News and Witness experience. Local
[news dossiers](news-desk.md) and personal [witness capture](witness-capture.md)
remain available as secondary, expandable controls.

## User workflow

Opening **News** starts collection from NASA and BBC World RSS. Opening
**Witness** searches The National Archives Discovery catalogue for `UFO sighting`.
No API key, manually authored article, JSON file or OIA account is needed.
The report search can request other UFO-related keywords; source request intervals
still apply, and the exact request that produced the displayed result remains visible.
Changing keywords does not relabel earlier results as a new search.

While the active tab is visible, a checked control requests refresh every 15 minutes.
Switching tabs, hiding the app or unchecking the control stops that timer. NASA's
hourly interval and all provider backoff/cache directives take precedence. Manual
refresh uses the same interval controls; it cannot bypass a restriction or rate limit.
This is a foreground session timer, not a durable background scheduler.

Filter collected titles/excerpts locally and choose a source. Results preserve source
order and the provider's item order; they are not globally sorted by inferred dates.
Select an item to read the supplied excerpt or catalogue description. Inspect
publication/update strings, the original retrieval time, last successful source check,
request URL, exact item byte range, response/item hashes, extraction version and
coverage. Source URLs are copyable text; they are not loaded in the privileged WebView.
Source HTML is displayed as text, never executed. Hidden controls use the existing
domain prose-display policy; original bytes remain unchanged.

Each source expands to its health, next eligible request time, separate use conditions
and retained-response history. **Inspect retained original response** shows the exact
UTF-8 response and its canonical evidence ledger, with controls escaped for display.
It does not export or publish either. Explicit clearing discards the instrument's
retained responses and extractions, while preserving cadence/backoff reservations.

**Add a public RSS / Atom source** registers a user-selected public HTTPS feed under
the same parser, network constraints and budgets. Seeds are not a permanent host
allow-list. This operation supports personal feed reading and a bounded memory cache;
unknown reuse rights remain explicit, and no redistribution/model processing is added.
Source registrations and all collected material are discarded on exit.

## Initial source profiles and conditions

Evaluated on 2026-11-05 UTC with ordinary public GETs, without credentials or access
bypass. Live availability is an observation from that check, not a support guarantee.

| Source | Access and automated use | Retention | Redistribution / limits |
| --- | --- | --- | --- |
| [NASA RSS](https://www.nasa.gov/feed/) | Publisher-provided RSS; one GET at most hourly; informational reading | Exact response in bounded session memory | [NASA media guidance](https://www.nasa.gov/nasa-brand-center/images-and-media/) generally permits informational use; third-party credits and protected marks remain separate. This workflow exports no payload or images. |
| [BBC World RSS](https://feeds.bbci.co.uk/news/world/rss.xml) | Publisher-provided feed for personal, non-commercial reading; at most every 15 minutes | Exact feed response in bounded session memory; full article pages are not fetched | BBC copyright and [metadata/RSS terms](https://www.bbc.co.uk/usingthebbc/terms-of-use/#15metadataandrssfeeds). No republication, model input or payload export. |
| [National Archives Discovery API](https://discovery.nationalarchives.gov.uk/API/help) | Public documented search API; one first-page `UFO` query at most every 10 seconds (automatic refresh every 15 minutes) | Exact JSON catalogue response in bounded session memory | [Published reuse terms](https://www.nationalarchives.gov.uk/legal/terms-of-use/); catalogue metadata does not grant rights to underlying documents/scans. No scans, full testimony or payload exports are acquired. |

NUFORC's ordinary public requests returned HTTP 403 during evaluation. It is not a
registered provider, and no alternate access or bypass is attempted. The Witness
catalogue contains descriptions of historical report files, sometimes with only a
title, date range and archive reference. It is not the full witness account, a current
sighting feed, a verified observation or evidence of independent corroboration.
Catalogue covering dates belong to the file and never populate an event time.

## Ownership, evidence and failure behavior

`oia-ingest::collection::Registry` owns source definitions, policies, normalized
batches, source health, response history and request eligibility. `transport::Transport`
is injected; fixtures and `HttpTransport` use the same parser and normalization path.
`oia-app::collection` composes that service with the existing worker, cancellation and
publication gate. Read-only `collection_view` and `collection_original` issue no
network requests; mutations have separate commands.

Every retained response is a canonical `SourceRecord` with
`AcquiredSourceMaterial` origin and `Unreviewed` status. Extracted feed/catalogue
items are `Document` / `ConventionalDerivative` records with
`public-metadata-extraction-v1` provenance linking the exact acquired response version.
Neither pathway calls the personal-testimony capture command or creates observations,
claims assessed as true, geographic positions or event clusters. Exact original XML/JSON
and item byte anchors remain available; hashes establish integrity, not authenticity.
Provider GUID/ID (or the supported article URL when no GUID exists) determines the
source-scoped record identity; changed item bytes produce another extraction version.
Missing dates, narrative and locations remain unknown rather than inferred defaults.

A refresh stages a registry clone. Only the existing application publication gate
can install it after the final cancellation/deadline check. Cancelled/expired work
cannot replace the previous collection. One source's failure can accompany another
source's success. Failures preserve earlier originals and display their actual retrieval
time with a **stale retained response** label. Empty, not collected, unavailable,
unauthorized, restricted, missing/removed, rate limited, malformed, byte-limit,
redirect-blocked, reference-only and cache-full outcomes remain distinct. Missing
pages do not imply retraction; an empty feed does not imply absence elsewhere.

HTTP validators are sent only for the exact request URL whose response is held. A
304 revalidates that response without changing its original acquisition timestamp or
creating another snapshot. Cache-Control `max-age` extends request spacing; `no-store`
produces a reference-only outcome without retaining that response body. Numeric and
HTTP-date Retry-After values extend backoff; an invalid value, or 429/503 without a
usable value, conservatively backs off for 24 hours. Transport reservations survive
cancellation and clearing, including backoff already received from the provider.

## Bounds and destination isolation

- Three registered sources per instrument, one request at a time; no crawl, pagination,
  enclosure/image fetching, automatic retries, credentials or browser execution.
- A two-second DNS wait, three-second connection timeout and eight-second total HTTP
  timeout per source, inside the existing 30-second application deadline. Cancellation
  is checked between reads, during parsing and immediately before publication; a
  blocking OS/network call may finish up to its bounded timeout before returning.
- One DNS worker with a one-slot queue; stalled resolution cannot spawn unbounded
  workers. At most 32 DNS addresses; every address must pass a conservative public-IP
  policy before it is pinned into the HTTPS client. Proxies and redirects are disabled.
- Public HTTPS on port 443, no URL credentials/fragments; local/private, loopback,
  link-local, multicast, documentation and transition address ranges are refused.
  Relative item links resolve against the response URL; non-HTTPS or otherwise unsafe
  links are omitted. Feed links do not grant authority for further requests.
- Two MiB per response; oversized bodies are refused, never accepted as a truncated
  success. UTF-8 only; RSS/Atom XML depth 32, DTDs refused; provider fields are bounded.
  JSON uses the bounded response and serde's recursion limit.
- Thirty displayed items per source response with exclusions and total catalogue
  matches exposed separately. No complete archive/news-coverage claim is made.
- Four distinct original responses per source, at most 48 MiB of original payload
  across six sources, plus bounded metadata/extractions. Identical responses deduplicate.
  A fifth distinct response leaves earlier originals intact and reports `cache_full`;
  explicit clear is required. There is no silent eviction of evidence versions.

## Validation and remaining scope

Three focused ingestion checks cover exact original/item anchors and linked evidence
classes, retry/revision/failure preservation, bounded malformed input, and private
destination refusal. The live example uses the same registry and transport:

```text
cargo run -p oia-ingest --example collect-public --locked --offline
```

`--offline` applies to Cargo dependency resolution; the example intentionally uses
the internet. It reports structured source outcomes and does not turn an unavailable
source into a success. The desktop data/UI integration is separately exercised through
a development-only DOM probe with captured live response views and stubbed IPC; that
probe is not a native WebView or live end-to-end desktop measurement.

Durable source/evidence storage and jobs/authorization remain #11/#14. Broader source
discovery, credentialed providers, scraping/crawling and full acquisition contracts remain
#13/#17/#36. Translation, correction/retraction propagation and the collected-news event
grouping workflow remain #19/#20. Full archived testimony, durable witness capture,
sky sketches and event linkage remain #27. Integrated A01/A06/A13/A19 and broader
platform validation remain open. This slice establishes no Lean or formal Rust proof.

Validation on Windows x86_64, Rust 1.99.0, after rebasing onto main `f039c66`:

- `cargo fmt --all -- --check` and workspace Clippy (`--all-targets --locked -- -D warnings`) passed.
- `cargo test --workspace --locked --offline`: **230 passed, 0 failed**, including
  three focused collection contracts and the existing Rust/Lean cutoff correspondence.
- `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run.ps1 proofs`
  passed the pinned Lean 4.35.0 build, axiom audit and fixture synchronization gate.
- Final live collector probe: NASA **10**, BBC World **23**, Discovery **30** items,
  all three outcomes `ready`. Counts are observations from this run, not guarantees.
- Node 24 syntax checks and a development-only jsdom 26.1.0 probe passed automatic
  activation, retained-live-view rendering, filtering/selection, original inspection,
  inert source HTML, failed-refresh retention and secondary local-entry structure.
  This probe stubs IPC; no native desktop walkthrough or visual/accessibility
  conformance is claimed. Its dependency/cache and captured data are not committed.
- Changed Markdown relative links/fences and `git diff --check` passed. No hosted
  CI was added or triggered. This checkout lacks the separately fetched Cesium vendor
  assets; the unrelated Globe renderer was not validated by this slice.
