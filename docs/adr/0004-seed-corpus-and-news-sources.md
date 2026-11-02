# ADR 0004: Seed corpus and regional news source architecture (D03)

- **Status:** Accepted
- **Date:** 2026-11-01
- **Decision ID:** D03 ([docs/delivery.md](../delivery.md) §6)
- **Work Package:** [#20 — Construct regional news events from multiple sources](https://github.com/mESmaC/oia/issues/20)
- **Affected Requirements:** `DOC-01`, `DOC-02`, `NEWS-01`, `NEWS-02`, `NEWS-03`, `NEWS-04`, `NEWS-05`, `GEO-03`, `INTEL-01`, `INTEL-04`, `REPRO-01`, `REPRO-02`
- **Verification Status:**
  - **Implemented:** Decision record, data contract schema for A06 news lineage/event clustering/claim assessment, deterministic fixture corpus (`fixtures/news/a06/`), and local integrity/contract validator (`scripts/news/validate_a06.py`).
  - **Proposed/target:** Runtime Rust source adapters in `oia-ingest` and regional event clustering/claim-checking services in `oia-domain`, `oia-time-geo`, and `oia-analysis` (dependent on P0 #10, #12, #13 and P1 #17, #19).

## 1. Context and problem statement

Open Intelligence Agency (OIA) P1 requires a small, documented seed collection covering UAP/public-disclosure documents and regional news/official reporting (`D03`), together with deterministic verification corpora for Acceptance Scenarios `A03` (cited document research) and `A06` (news lineage and claim assessment).

Several constraints from [docs/spec.md](../spec.md), [docs/architecture.md](../architecture.md), and [AGENTS.md](../../AGENTS.md) govern this choice:

1. **Separate access, processing, retention, and redistribution rights (`DOC-01`, `NEWS-01`, `REPRO-01`):** Ordinary public viewing access does not grant permission to redistribute third-party articles in git repositories or exported investigation bundles. Every source registration and `SourceRecord` must carry separate policy fields for access method, automated processing, local retention, and bundle redistribution.
2. **Syndication versus independent corroboration (`NEWS-01`, `GEO-03`, `INTEL-01`):** Multiple regional outlets republishing a wire dispatch or quoting the same press briefing are not independent observations. Event clusters must maintain distinct counts of `articles`, `accounts`, `independent_sources`, and `proposed_events`.
3. **Epistemic discipline on regional events (`NEWS-02`):** Military movements, exercises, deployments, public-safety incidents, and transport disruptions must keep `plan`, `observation`, `allegation`, `arrest`, `charge`, `judgment`, and `attributed_statement` distinct. Reported movement alone must never be promoted to established intent, mission, or destination.
4. **Time and geographic uncertainty (`NEWS-01`, `GEO-01`, `GEO-03`):** Event/capture time, publication time, provider revision time, and OIA retrieval/ingestion time must remain distinct, with original timezone offsets and ambiguity preserved. Gazetteer centroids, bounded operational areas, and exact coordinates must carry explicit geometry and altitude reference kinds.
5. **Correction propagation and removed-page semantics (`NEWS-03`, `NEWS-04`, `NEWS-05`):** Corrections, retractions, and case outcomes must create new versioned records and mark downstream clusters, summaries, and reports `needs_reassessment` without overwriting issued versions. A removed or HTTP-404/410 source page (`availability: removed`) must not be automatically marked `retracted`.
6. **Publisher summary guards (`NEWS-05`):** Publisher-level summaries must expose sample scope, evaluation method, time window, unresolved assessments, and correction history, and must structurally prohibit presenting news article volume as incident frequency, crime rate, or community criminality.

## 2. Alternatives considered

### Alternative A: Unconstrained live news-aggregator scraping without bundled fixtures

- **Description:** Query live commercial news search APIs or scrape arbitrary regional portals on demand during development and testing.
- **Rejected because:** Live feeds change continuously, cannot provide deterministic regression verification for `A06`, risk violating source terms or rate limits, and lack controlled ground-truth lineage for syndication cycles, silent revisions, deleted pages, and distorted quotations.

### Alternative B: Commit verbatim copyrighted commercial wire and newspaper corpora into the repository

- **Description:** Store scraped commercial news articles directly in the repository as test fixtures.
- **Rejected because:** Public web readability does not grant a redistribution license under repository terms (`AGENTS.md` §6–§7, `REPRO-01`).

### Alternative C: Two-tier seed corpus architecture — bundled deterministic fixtures plus capability-scoped open/official source profiles (Selected)

- **Description:**
  1. **Tier 1 (Bundled offline deterministic corpora):** Maintain self-contained, repository-redistributable fixture corpora (`fixtures/documents/a03/` for `A03` and `fixtures/news/a06/` for `A06`) with content-addressed SHA-256 original payloads, versioned provenance envelopes, explicit lineage graphs, multilingual passages, reversible event clusters, and passage-level claim assessments.
  2. **Tier 2 (Declared P1 seed source profiles):** Scope initial P1 adapters to documented public-record, scientific-preprint, official-bulletin, and user-configured syndication sources with explicit `SourceUsePolicy` enforcement.

## 3. Decision

We adopt **Alternative C**.

### 3.1 Tier 1: Deterministic A06 regional news and claim-assessment fixture corpus

The repository includes `fixtures/news/a06/` as the canonical offline fixture set for `#20` and `#21` (`A06`), validated locally by `scripts/news/validate_a06.py`.

The fixture scenario models a regional coastal/maritime and airspace sequence in the North-Central Pacific / Hawaiian Islands approaches (`2026-10-14` through `2026-10-18`, straddling `UTC-10:00` HST, `UTC+12:00` / `UTC-12:00` dateline-adjacent maritime notices, and `UTC`), combining:

1. **Official Planned Exercise Notice (`src-uscg-lnm-2026-41`):** USCG/Navy hydrographic and airspace advisory announcing a scheduled maritime sensor calibration exercise (`category: plan`, bounded polygon geometry, explicit window `[2026-10-15T18:00:00Z, 2026-10-16T04:00:00Z)`).
2. **Original Local Harbor Account (`src-harbor-coop-log-v1`):** Independent port-pilot cooperative logbook entry reporting an observed unlit fast surface craft and temporary channel closure (`category: observation`, multilingual English + Japanese radio log excerpt with original passage and translation anchored separately per `NEWS-01` / `INTEL-04`).
3. **Wire Service Dispatch — v1 and v2 Correction (`src-pacific-wire-dispatch-v1`, `src-pacific-wire-dispatch-v2`):**
   - `v1` quotes the harbor cooperative log and adds an uncorroborated anonymous allegation (`category: allegation`) plus a numerical transcription error (`14 vessels` instead of `4 vessels`).
   - `v2` issues a formal correction fixing the vessel count (`4 vessels`) and clarifying that the anonymous allegation was retracted by the speaker, triggering `needs_reassessment` on downstream claims, event cluster summaries, and draft report statements without mutating `v1`.
4. **Syndicated Regional Republications (`src-island-tribune-syndicated-v1`, `src-maritime-daily-syndicated-v1`):**
   - Republish `src-pacific-wire-dispatch-v1` (`relation: republishes` / `quotes`).
   - `src-island-tribune-syndicated-v1` attaches an overstated, materially misleading headline (*"Naval Blockade Halts Commercial Shipping Across Pacific Corridor"*) that contradicts the body text's routine 4-hour single-channel advisory (`NEWS-03`, `NEWS-04`: `materially_misleading`).
   - Both syndicated articles share lineage with `src-pacific-wire-dispatch-v1` so the cluster's `independent_sources` count is NOT incremented by wire reprints.
5. **Independent Official Safety & Enforcement Release — Arrest/Charge vs Judgment (`src-port-authority-bulletin-v1`, `src-district-court-docket-v1`):**
   - `src-port-authority-bulletin-v1` records a harbor-safety citation and operator detention (`category: arrest` and `category: charge`) for an unregistered commercial survey launch.
   - `src-district-court-docket-v1` records the subsequent administrative dismissal of the willful-obstruction count and a civil equipment fine (`category: judgment`), flagging earlier summaries that referenced only the initial charge.
6. **Removed / Deleted Bulletin Page (`src-regional-blog-mirror-v1`):**
   - Previously acquired snapshot retained with SHA-256 hash; a later poll records HTTP `410 Gone` (`availability: removed`).
   - Per `NEWS-04` and `A06`, its status remains `availability: removed` and `retraction_status: not_retracted` (deleted pages are never auto-marked `retracted`).
7. **Reversible Event Clustering (`GEO-03`):**
   - Two candidate clusters (`evt-cluster-channel-closure` and `evt-cluster-scheduled-calibration`) plus a recorded split/merge decision log showing why the scheduled offshore Navy calibration exercise and the inshore survey-launch safety incident were split after reviewing spatial bounds, time offsets, and source lineage, while preserving all conflicting accounts and explicit counts (`articles: 7`, `accounts: 4`, `independent_sources: 4`, `proposed_events: 2`).
8. **Publisher-Level Summary (`NEWS-05`):**
   - Evaluates sampled items per publisher with explicit `sample_size`, `sampling_method`, `time_window`, `unresolved_assessment_count`, `correction_history`, and `volume_interpretation_guard` (`represents_incident_or_crime_rate: false`).

### 3.2 Tier 2: Declared P1 seed source profiles and `SourceUsePolicy` contract

When runtime adapters land in `oia-ingest` (`#13`, `#17`, `#20`), P1 seed registrations are scoped to four explicit source classes:

| Profile ID | Source Class | Representative Seed Endpoints / Formats | Ordinary Access Method | Default Bundle Redistribution Policy |
| --- | --- | --- | --- | --- |
| `seed-gov-disclosure` | Public government disclosure & archival records (`DOC-01`) | Local file/bundle import and public federal reading-room releases (NARA/AARO public reports, PDF/HTML/JSON) | Direct HTTP GET / local import; no authentication bypass | `bundle_bytes_permitted` for verified U.S. federal public-domain works; `reference_and_anchors_only` when third-party enclosures lack redistribution notice |
| `seed-arxiv-preprints` | Open scientific preprints (`DOC-01`, `DOC-02`) | arXiv versioned metadata/preprints (`v1`, `v2`, ...) via documented public export formats | Bounded HTTP GET with explicit rate-limit budget and User-Agent attribution | Determined per record license (`CC-BY`/`CC0` -> `bundle_bytes_permitted`; arXiv non-exclusive distribution license -> `reference_and_anchors_only`) |
| `seed-official-bulletins` | Official weather, maritime, aviation & municipal notices (`NEWS-01`, `NEWS-02`) | CAP/Atom/RSS advisories, Local Notices to Mariners, municipal/court press bulletins | Bounded polling with `ETag`/`Last-Modified` and injected transport | `bundle_bytes_permitted` for public-domain federal notices; `reference_and_anchors_only` unless municipal terms permit redistribution |
| `seed-syndicated-news` | Regional, wire, and specialist journalism (`NEWS-01`–`NEWS-05`) | User-configured RSS/Atom/JSON Feed and individual article URLs | Bounded HTTP GET of ordinarily accessible pages; no paywall/credential bypass | `reference_and_anchors_only` by default (retains local evidence blob per local retention policy, exports bibliographic metadata, hashes, version anchors, and bounded quotation spans only) |

Every `Source` and `SourceRecord` must carry:

- `access_method`: `local_fixture` | `direct_http_get` | `rss_atom_poll` | `oai_pmh_harvest`
- `automated_processing_policy`: `permitted_with_rate_limit` | `manual_import_only` | `prohibited`
- `retention_policy`: `pinned_case_evidence` | `ephemeral_cache_only` | `metadata_and_anchors_only`
- `redistribution_policy`: `bundle_bytes_permitted` | `reference_and_anchors_only` | `prohibited`

## 4. Consequences and boundaries

- **Immediate local verification:** `#20` and `#21` have a rich, deterministic, zero-network fixture corpus (`fixtures/news/a06/`) and automated validator (`scripts/news/validate_a06.py`) that enforce every data-contract rule in `A06`, `NEWS-01`–`NEWS-05`, and `GEO-03` today.
- **No false runtime claims:** This ADR and fixture set do not claim that `oia-ingest` or `oia-analysis` runtime crates exist yet. Runtime adapters and clustering services will consume `fixtures/news/a06/manifest.json` directly once `#10`, `#12`, `#13`, `#17`, and `#19` merge.
- **Preserved original bytes and lineage:** Every fixture source version in `fixtures/news/a06/blobs/` is immutable and content-addressed by SHA-256; revisions, corrections, translations, cluster splits, and claim assessments reference exact `source_record_version_id` and character/byte spans.
