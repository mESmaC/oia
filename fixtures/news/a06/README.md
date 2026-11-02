# A06 Fixture Corpus: Regional News Lineage, Event Clustering, and Claim Assessment

Deterministic offline verification corpus for [Work Package #20](https://github.com/mESmaC/oia/issues/20) and [Work Package #21](https://github.com/mESmaC/oia/issues/21), governed by [ADR 0004 (`D03`)](../../../docs/adr/0004-seed-corpus-and-news-sources.md).

## 1. Purpose and Verification Status

- **Implemented:** Deterministic fixture corpus (`manifest.json` + 9 immutable UTF-8 source payloads in `blobs/`) and local contract validator (`scripts/news/validate_a06.py`).
- **Proposed/target:** Runtime ingestion and clustering execution in `oia-ingest`, `oia-time-geo`, `oia-domain`, and `oia-analysis` once prerequisites `#10`, `#12`, `#13`, `#17`, and `#19` merge.

## 2. Covered Requirements and A06 Scenario Elements

| Requirement / Scenario | Fixture Coverage in `manifest.json` |
| --- | --- |
| `NEWS-01` | 8 registered sources (`official_bulletin`, `specialist_operational_log`, `wire_service`, `regional_news`, `specialist_news`, `official_enforcement_bulletin`, `official_judicial_docket`, `local_commentary`) and 9 immutable `SourceRecord` versions with distinct `event_interval`, `published_at`, `revised_at`, `retrieved_at`, and `ingested_at` timestamps, original timezone/offset preservation, geographic uncertainty (`polygon`, `point_with_uncertainty_radius`, `gazetteer_centroid`), and multilingual Japanese-to-English passage anchoring (`sr-harbor-coop-log@v1`). |
| `NEWS-02` | 7 `event_candidates` covering all required epistemic categories: `plan` (`cand-w188b-calibration-plan`), `observation` (`cand-west-channel-unlit-craft`), `attributed_statement` (`cand-kealoha-pilotage-statement`), `allegation` (`cand-contractor-payload-allegation`), `arrest` (`cand-reef-scanner-detention`), `charge` (`cand-reef-scanner-charges`), and `judgment` (`cand-reef-scanner-judgment`). Enforces `movement_establishes_intent_or_mission: false`. |
| `NEWS-03` | Passage-level fault annotations covering `numerical_error` (`14 vessels` vs `14-meter survey launch`), `headline_caption_mismatch`, `distorted_quotation` (`"harbor control confirmed a full military lockdown"` fabricated from `"harbor control held inbound pilotage for 35 minutes"`), `factual_contradiction`, `unsupported_certainty`, `selective_comparison`, `materially_missing_context`, and `news_volume_equated_with_crime_rate`. |
| `NEWS-04` | All 6 claim assessment states represented (`supported`, `contradicted`, `unsupported_by_available_evidence`, `materially_misleading`, `disputed`, `unresolved`) with separate `review_status` and `availability`. Distinguishes quoting an allegation from endorsing it (`claim-03-contractor-payload-allegation`) and publication-time assessment (`disputed`) from post-adjudication assessment (`contradicted` in `claim-06-willful-fairway-obstruction`). Verifies that a removed page (`sr-regional-blog-lights@v1`, HTTP `410 Gone`, `availability: removed`) retains `retraction_status: not_retracted`. |
| `NEWS-05` | Versioned correction propagation (`sr-pacific-wire-884@v2` superseding `v1` and `sr-district-docket-0412@v1` updating `sr-port-authority-0412@v1`) marks dependent records, candidates, claims, and draft reports `needs_reassessment: true` without overwriting prior versions. Publisher summaries expose `sample_size`, `sampling_method`, `time_window`, `unresolved_assessment_count`, `correction_history`, and `represents_incident_frequency_or_crime_rate: false`. |
| `GEO-03` | Reversible event clustering with explicit matching criteria, preserved conflicts (`14` vs `4` vessels; Oahu gazetteer centroid vs Buoy #3 coordinates), separate counts (`articles`, `accounts`, `independent_sources`, `proposed_events`), syndication deduplication (`sr-pacific-wire-884`, `sr-island-tribune-884`, and `sr-maritime-daily-884` share `lineage-root-harbor-coop-log`), and a reversible split/merge history (`cluster-rev-1` -> `cluster-rev-2`). |

## 3. Local Validation Command

Run from the repository root:

```text
python scripts/news/validate_a06.py --self-test
```
