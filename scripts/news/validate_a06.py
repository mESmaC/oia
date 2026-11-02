#!/usr/bin/env python3
"""Deterministic fixture consistency and byte-integrity validator for the A06 news corpus.

Checks `fixtures/news/a06/manifest.json` and `fixtures/news/a06/blobs/*.txt` for internal
consistency and expected outcomes against OIA requirements NEWS-01..NEWS-05, GEO-03,
INTEL-01, INTEL-04, and ADR 0005 (D03). Runtime enforcement in Rust crates is not yet
implemented.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import pathlib
import sys
from urllib.parse import urlparse
from typing import Any

SYNTHETIC_HEADER = "SYNTHETIC A06 FIXTURE - NOT OBSERVED EVIDENCE"

REQUIRED_EPISTEMIC_CATEGORIES = {
    "plan",
    "observation",
    "allegation",
    "arrest",
    "charge",
    "judgment",
    "attributed_statement",
}

REQUIRED_CLAIM_ASSESSMENTS = {
    "supported",
    "contradicted",
    "unsupported_by_available_evidence",
    "materially_misleading",
    "disputed",
    "unresolved",
}

VALID_AVAILABILITY_STATES = {
    "available",
    "reference_only",
    "missing",
    "restricted",
    "removed",
    "expired",
    "corrupt",
}

VALID_REVIEW_STATES = {
    "unreviewed",
    "in_review",
    "reviewed",
    "challenged",
    "superseded",
}


class ValidationError(Exception):
    """Raised when an A06 fixture contract invariant is violated."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValidationError(message)


def require_invalid_tld_url(url_value: Any, context: str) -> None:
    require(isinstance(url_value, str) and bool(url_value), f"{context} must be a non-empty URL string")
    parsed = urlparse(url_value)
    host = (parsed.hostname or "").lower()
    require(
        parsed.scheme in ("http", "https") and host.endswith(".invalid"),
        f"{context} must use a reserved '.invalid' hostname (got {url_value!r})",
    )


def validate_manifest_data(manifest: dict[str, Any], fixture_dir: pathlib.Path) -> dict[str, int]:
    meta = manifest.get("fixture_metadata", {})
    require(
        meta.get("fixture_id") == "a06-news-lineage-and-regional-events",
        "fixture_metadata.fixture_id must be 'a06-news-lineage-and-regional-events'",
    )
    require(meta.get("decision_id") == "D03", "fixture_metadata.decision_id must be 'D03'")
    require(
        meta.get("acceptance_scenario") == "A06",
        "fixture_metadata.acceptance_scenario must be 'A06'",
    )
    require(
        meta.get("is_synthetic_fixture") is True,
        "fixture_metadata must declare is_synthetic_fixture=True",
    )

    repo_root = fixture_dir.parent.parent.parent
    adr_path = repo_root / meta.get("adr_path", "")
    require(adr_path.is_file(), f"Referenced ADR file does not exist: {adr_path}")

    # 1. Validate Sources and SourceUsePolicy (DOC-01, NEWS-01, REPRO-01)
    sources = manifest.get("sources", [])
    require(len(sources) >= 6, "At least 6 distinct sources required for A06")
    source_ids: set[str] = set()
    for src in sources:
        sid = src.get("id")
        require(bool(sid) and sid not in source_ids, f"Duplicate or missing source id: {sid}")
        source_ids.add(sid)
        policy = src.get("use_policy", {})
        for field in (
            "access_method",
            "ordinary_access_transport",
            "automated_processing_policy",
            "retention_policy",
            "redistribution_policy",
            "attribution_required",
        ):
            require(
                bool(policy.get(field)),
                f"Source {sid} missing required use_policy.{field}",
            )
        require(
            src.get("is_synthetic_fixture") is True,
            f"Source {sid} must declare is_synthetic_fixture=True",
        )

    # 2. Validate SourceRecords, SHA-256 byte integrity, synthetic headers, timestamps, geography, and passage anchors
    records = manifest.get("source_records", [])
    require(len(records) >= 8, "At least 8 source_record versions required for A06")
    records_by_version: dict[str, dict[str, Any]] = {}
    passages_by_record_version: dict[str, dict[str, dict[str, Any]]] = {}
    has_multilingual_translation = False
    has_removed_non_retracted = False
    has_gazetteer_centroid = False
    has_polygon_geometry = False

    for rec in records:
        vid = rec.get("version_id")
        require(
            bool(vid) and vid not in records_by_version,
            f"Duplicate or missing SourceRecord version_id: {vid}",
        )
        require(
            rec.get("source_id") in source_ids,
            f"SourceRecord {vid} references unknown source_id {rec.get('source_id')}",
        )
        require(
            rec.get("availability") in VALID_AVAILABILITY_STATES,
            f"SourceRecord {vid} has invalid availability: {rec.get('availability')}",
        )
        require(
            rec.get("review_status") in VALID_REVIEW_STATES,
            f"SourceRecord {vid} has invalid review_status: {rec.get('review_status')}",
        )
        require_invalid_tld_url(rec.get("canonical_url"), f"SourceRecord {vid} canonical_url")

        # Deleted/removed page must NOT be auto-marked retracted (NEWS-04, A06)
        if rec.get("availability") == "removed":
            require(
                rec.get("retraction_status") == "not_retracted",
                f"Removed SourceRecord {vid} must not be auto-marked retracted (got {rec.get('retraction_status')})",
            )
            has_removed_non_retracted = True

        # Verify retained blob bytes, SHA-256, byte length, char length, and synthetic header
        blob_rel = rec.get("blob_path")
        require(bool(blob_rel), f"SourceRecord {vid} missing blob_path")
        blob_file = (fixture_dir / blob_rel).resolve()
        require(
            blob_file.is_relative_to(fixture_dir.resolve()),
            f"SourceRecord {vid} blob_path escapes fixture directory: {blob_rel}",
        )
        require(blob_file.is_file(), f"SourceRecord {vid} missing blob file: {blob_file}")
        raw_bytes = blob_file.read_bytes()
        require(b"\r\n" not in raw_bytes, f"SourceRecord {vid} blob contains CRLF bytes; LF required")
        decoded_text = raw_bytes.decode("utf-8")
        first_line = decoded_text.splitlines()[0] if decoded_text else ""
        require(
            first_line == SYNTHETIC_HEADER,
            f"SourceRecord {vid} blob {blob_rel} must start with {SYNTHETIC_HEADER!r}",
        )
        actual_sha = hashlib.sha256(raw_bytes).hexdigest()
        require(
            actual_sha == rec.get("sha256"),
            f"SHA-256 mismatch for {vid}: expected {rec.get('sha256')}, got {actual_sha}",
        )
        require(
            len(raw_bytes) == rec.get("byte_length"),
            f"byte_length mismatch for {vid}: expected {rec.get('byte_length')}, got {len(raw_bytes)}",
        )
        require(
            len(decoded_text) == rec.get("char_length"),
            f"char_length mismatch for {vid}: expected {rec.get('char_length')}, got {len(decoded_text)}",
        )

        # Verify distinct timestamp fields (event_interval, published_at, retrieved_at, ingested_at)
        ts = rec.get("timestamps", {})
        for req_ts in ("event_interval", "published_at", "retrieved_at", "ingested_at"):
            require(
                req_ts in ts and ts[req_ts] is not None,
                f"SourceRecord {vid} missing timestamps.{req_ts}",
            )
        pub = ts["published_at"]
        require(
            bool(pub.get("utc")) and bool(pub.get("original_timezone")) and bool(pub.get("original_offset")),
            f"SourceRecord {vid} published_at must preserve utc, original_timezone, and original_offset",
        )

        # Verify geography and uncertainty (GEO-01, GEO-03, NEWS-01)
        geo = rec.get("geography", {})
        require(geo.get("crs") == "WGS84", f"SourceRecord {vid} geography.crs must be WGS84")
        require(
            isinstance(geo.get("location_uncertainty_meters"), (int, float))
            and geo["location_uncertainty_meters"] >= 0,
            f"SourceRecord {vid} must declare non-negative location_uncertainty_meters",
        )
        if geo.get("geometry_kind") == "gazetteer_centroid":
            require(
                geo.get("is_gazetteer_centroid") is True,
                f"Gazetteer centroid record {vid} must set is_gazetteer_centroid=True",
            )
            has_gazetteer_centroid = True
        if geo.get("geometry_kind") == "polygon":
            has_polygon_geometry = True

        # Verify passage byte/char spans against immutable UTF-8 payload
        p_map: dict[str, dict[str, Any]] = {}
        for p in rec.get("passages", []):
            pid = p.get("passage_id")
            require(bool(pid) and pid not in p_map, f"Duplicate or missing passage_id {pid} in {vid}")
            c_start, c_end = p.get("char_start"), p.get("char_end")
            b_start, b_end = p.get("byte_start"), p.get("byte_end")
            quote = p.get("quote")
            require(
                isinstance(c_start, int)
                and isinstance(c_end, int)
                and 0 <= c_start < c_end <= len(decoded_text),
                f"Invalid char span [{c_start}, {c_end}) in {vid}:{pid}",
            )
            require(
                isinstance(b_start, int)
                and isinstance(b_end, int)
                and 0 <= b_start < b_end <= len(raw_bytes),
                f"Invalid byte span [{b_start}, {b_end}) in {vid}:{pid}",
            )
            require(
                decoded_text[c_start:c_end] == quote,
                f"Char span quote mismatch in {vid}:{pid}",
            )
            require(
                raw_bytes[b_start:b_end].decode("utf-8") == quote,
                f"Byte span quote mismatch in {vid}:{pid}",
            )
            p_map[pid] = p

        # Check translation anchor link if present (NEWS-01, INTEL-04)
        for p in p_map.values():
            trans = p.get("translation")
            if trans is not None:
                t_pid = trans.get("translation_passage_id")
                require(
                    t_pid in p_map,
                    f"Translation in {vid}:{p['passage_id']} references missing passage {t_pid}",
                )
                require(
                    p_map[t_pid]["quote"] == trans.get("translated_text"),
                    f"Translated text mismatch in {vid}:{p['passage_id']}",
                )
                has_multilingual_translation = True

        records_by_version[vid] = rec
        passages_by_record_version[vid] = p_map

    require(has_multilingual_translation, "A06 corpus must include a multilingual passage with anchored translation")
    require(has_removed_non_retracted, "A06 corpus must include a removed page with retraction_status='not_retracted'")
    require(has_gazetteer_centroid and has_polygon_geometry, "A06 corpus must include both gazetteer_centroid and polygon geometries")

    # 3. Validate Lineage Edges (NEWS-01, INTEL-01)
    edges = manifest.get("lineage_edges", [])
    require(len(edges) >= 5, "At least 5 lineage edges required")
    republish_edges = 0
    supersede_edges = 0
    for edge in edges:
        f_vid = edge.get("from_version_id")
        t_vid = edge.get("to_version_id")
        require(f_vid in records_by_version, f"Lineage edge references unknown from_version_id {f_vid}")
        require(t_vid in records_by_version, f"Lineage edge references unknown to_version_id {t_vid}")
        if edge.get("relation_type") == "republishes":
            republish_edges += 1
            require(
                edge.get("preserves_independent_corroboration") is False,
                f"Syndication/republish edge {edge.get('edge_id')} must not count as independent corroboration",
            )
        if edge.get("relation_type") == "supersedes":
            supersede_edges += 1
    require(republish_edges >= 2, "At least 2 syndication republish edges required")
    require(supersede_edges >= 2, "At least 2 supersession/correction edges required")

    # 4. Validate Event Candidates and Epistemic Categories (NEWS-02)
    candidates = manifest.get("event_candidates", [])
    candidates_by_id: dict[str, dict[str, Any]] = {}
    observed_categories: set[str] = set()
    for cand in candidates:
        cid = cand.get("candidate_id")
        require(bool(cid) and cid not in candidates_by_id, f"Duplicate or missing candidate_id {cid}")
        cat = cand.get("epistemic_category")
        require(
            cat in REQUIRED_EPISTEMIC_CATEGORIES,
            f"Candidate {cid} has invalid epistemic_category {cat}",
        )
        observed_categories.add(cat)
        require(
            cand.get("movement_establishes_intent_or_mission") is False,
            f"Candidate {cid} violates NEWS-02: movement alone must not establish intent, mission, or destination",
        )
        sr_vid = cand.get("source_record_version_id")
        require(sr_vid in records_by_version, f"Candidate {cid} references unknown source_record_version_id {sr_vid}")
        for pid in cand.get("supporting_passage_ids", []):
            require(
                pid in passages_by_record_version[sr_vid],
                f"Candidate {cid} references unknown passage {pid} in {sr_vid}",
            )
        candidates_by_id[cid] = cand

    require(
        observed_categories == REQUIRED_EPISTEMIC_CATEGORIES,
        f"Missing required NEWS-02 epistemic categories: {REQUIRED_EPISTEMIC_CATEGORIES - observed_categories}",
    )

    # 5. Validate Reversible Event Clustering and Independent Source Counts (GEO-03, NEWS-01)
    clusters_block = manifest.get("event_clusters", {})
    active_clusters = clusters_block.get("active_clusters", [])
    require(len(active_clusters) == 2, "Expected 2 active event clusters after split")

    for cl in active_clusters:
        eid = cl.get("event_id")
        members = cl.get("member_source_record_versions", [])
        require(len(members) > 0, f"Cluster {eid} has no member_source_record_versions")
        distinct_article_ids = {records_by_version[m]["id"] for m in members}
        distinct_roots = {
            records_by_version[m]["syndication"]["independent_lineage_root_id"]
            for m in members
        }
        counts = cl.get("counts", {})
        require(
            counts.get("article_versions") == len(members),
            f"Cluster {eid} counts.article_versions ({counts.get('article_versions')}) != len(members) ({len(members)})",
        )
        require(
            counts.get("articles") == len(distinct_article_ids),
            f"Cluster {eid} counts.articles ({counts.get('articles')}) != distinct article ids ({len(distinct_article_ids)})",
        )
        require(
            counts.get("independent_sources") == len(distinct_roots),
            f"Cluster {eid} counts.independent_sources ({counts.get('independent_sources')}) != distinct lineage roots ({len(distinct_roots)})",
        )
        require(
            set(cl.get("independent_lineage_roots", [])) == distinct_roots,
            f"Cluster {eid} independent_lineage_roots mismatch",
        )
        # Ensure syndicated reprints collapsed into fewer independent sources than articles for the channel-hold cluster
        if eid == "evt-vesper-harbor-channel-hold":
            require(
                counts["independent_sources"] < counts["articles"],
                "Syndicated articles in evt-vesper-harbor-channel-hold must collapse to fewer independent_sources than articles",
            )
            require(
                len(cl.get("preserved_conflicts", [])) >= 2,
                "evt-vesper-harbor-channel-hold must preserve conflicting vessel-count and location-uncertainty accounts",
            )

    rev_history = clusters_block.get("cluster_revision_history", [])
    require(len(rev_history) >= 2, "Cluster revision history must record initial merge and reversible split")
    require(
        all(r.get("reversible") is True for r in rev_history),
        "All cluster revisions in GEO-03 must be reversible",
    )
    require(
        bool(rev_history[-1].get("reverse_operation_template")),
        "Latest cluster split revision must include reverse_operation_template",
    )

    # 6. Validate Claims and Assessments (NEWS-03, NEWS-04)
    claims = manifest.get("claims_and_assessments", [])
    observed_assessments: set[str] = set()
    has_quoted_unendorsed = False
    has_temporal_assessment_shift = False

    for clm in claims:
        cid = clm.get("claim_id")
        sr_vid = clm.get("source_record_version_id")
        pid = clm.get("passage_id")
        require(sr_vid in records_by_version, f"Claim {cid} references unknown {sr_vid}")
        require(
            pid in passages_by_record_version[sr_vid],
            f"Claim {cid} references unknown passage {pid} in {sr_vid}",
        )
        pub_assess = clm.get("assessment_at_publication")
        cur_assess = clm.get("current_assessment")
        require(
            pub_assess in REQUIRED_CLAIM_ASSESSMENTS and cur_assess in REQUIRED_CLAIM_ASSESSMENTS,
            f"Claim {cid} has invalid assessment state ({pub_assess}, {cur_assess})",
        )
        observed_assessments.add(pub_assess)
        observed_assessments.add(cur_assess)
        if clm.get("quotes_claim_without_endorsing") is True:
            has_quoted_unendorsed = True
        if pub_assess != cur_assess:
            has_temporal_assessment_shift = True
        for ev_list_key in ("supporting_evidence", "contrary_evidence"):
            for ev in clm.get(ev_list_key, []):
                ev_vid = ev.get("source_record_version_id")
                ev_pid = ev.get("passage_id")
                require(
                    ev_vid in records_by_version and ev_pid in passages_by_record_version[ev_vid],
                    f"Claim {cid} {ev_list_key} references invalid anchor {ev_vid}:{ev_pid}",
                )

    require(
        observed_assessments == REQUIRED_CLAIM_ASSESSMENTS,
        f"Missing required NEWS-04 claim assessment states: {REQUIRED_CLAIM_ASSESSMENTS - observed_assessments}",
    )
    require(has_quoted_unendorsed, "Claims must include an attributed allegation where quotes_claim_without_endorsing=True")
    require(has_temporal_assessment_shift, "Claims must distinguish publication-time assessment from post-update assessment")

    # 7. Validate Dependency Invalidations (NEWS-05, architecture §2.3)
    invalidations = manifest.get("dependency_invalidations", [])
    require(len(invalidations) >= 2, "At least 2 dependency invalidation records required")
    for inv in invalidations:
        trig = inv.get("trigger_version_id")
        require(trig in records_by_version, f"Invalidation {inv.get('invalidation_id')} references unknown trigger {trig}")
        affected = inv.get("affected_objects", [])
        require(len(affected) > 0, f"Invalidation {inv.get('invalidation_id')} has no affected_objects")
        for obj in affected:
            require(
                obj.get("status_set") == "needs_reassessment" and obj.get("prior_issued_version_preserved") is True,
                f"Invalidation {inv.get('invalidation_id')} must set needs_reassessment and preserve prior issued version",
            )
            oid = obj.get("object_id")
            if obj.get("object_type") == "SourceRecord":
                require(
                    records_by_version[oid].get("needs_reassessment") is True,
                    f"SourceRecord {oid} must have needs_reassessment=True after invalidation",
                )
            elif obj.get("object_type") == "EventCandidate":
                require(
                    candidates_by_id[oid].get("needs_reassessment") is True,
                    f"EventCandidate {oid} must have needs_reassessment=True after invalidation",
                )

    # 8. Validate Publisher Summaries and Crime-Rate Volume Guards (NEWS-05)
    pub_summaries = manifest.get("publisher_summaries", [])
    require(len(pub_summaries) >= 3, "At least 3 publisher summaries required")
    for ps in pub_summaries:
        ps_id = ps.get("summary_id")
        require(ps.get("source_id") in source_ids, f"Publisher summary {ps_id} references unknown source_id")
        require(
            ps.get("sample_size") == len(ps.get("evaluated_source_record_versions", []))
            and ps.get("sample_size", 0) > 0,
            f"Publisher summary {ps_id} sample_size must match evaluated_source_record_versions length",
        )
        require(bool(ps.get("sampling_method")), f"Publisher summary {ps_id} missing sampling_method")
        require(
            isinstance(ps.get("unresolved_assessment_count"), int),
            f"Publisher summary {ps_id} missing unresolved_assessment_count",
        )
        guard = ps.get("volume_interpretation_guard", {})
        require(
            guard.get("represents_incident_frequency_or_crime_rate") is False,
            f"Publisher summary {ps_id} violates NEWS-05: news volume must not be equated with incident frequency or crime rate",
        )
        require(
            bool(guard.get("methodology_note")),
            f"Publisher summary {ps_id} missing volume_interpretation_guard.methodology_note",
        )

    return {
        "sources": len(sources),
        "source_record_versions": len(records),
        "lineage_edges": len(edges),
        "event_candidates": len(candidates),
        "active_clusters": len(active_clusters),
        "claims": len(claims),
        "dependency_invalidations": len(invalidations),
        "publisher_summaries": len(pub_summaries),
    }


def run_self_tests(manifest: dict[str, Any], fixture_dir: pathlib.Path) -> int:
    """Verify that concrete contract violations are rejected by the validator."""
    cases: list[tuple[str, Any]] = []

    # 1. Corrupted SHA-256 hash
    m1 = copy.deepcopy(manifest)
    m1["source_records"][0]["sha256"] = "0" * 64
    cases.append(("corrupted_sha256", m1))

    # 2. Deleted page auto-marked retracted (violates NEWS-04 / A06)
    m2 = copy.deepcopy(manifest)
    for r in m2["source_records"]:
        if r["availability"] == "removed":
            r["retraction_status"] = "retracted"
    cases.append(("deleted_page_auto_retracted", m2))

    # 3. Syndicated reprint counted as independent source (violates NEWS-01 / GEO-03)
    m3 = copy.deepcopy(manifest)
    m3["event_clusters"]["active_clusters"][0]["counts"]["independent_sources"] = 6
    cases.append(("syndication_inflates_independent_sources", m3))

    # 4. Movement promoted to intent/mission (violates NEWS-02)
    m4 = copy.deepcopy(manifest)
    m4["event_candidates"][0]["movement_establishes_intent_or_mission"] = True
    cases.append(("movement_promoted_to_intent", m4))

    # 5. Publisher summary equates article volume with crime rate (violates NEWS-05)
    m5 = copy.deepcopy(manifest)
    m5["publisher_summaries"][0]["volume_interpretation_guard"][
        "represents_incident_frequency_or_crime_rate"
    ] = True
    cases.append(("article_volume_equated_with_crime_rate", m5))

    # 6. Broken passage byte span
    m6 = copy.deepcopy(manifest)
    m6["source_records"][1]["passages"][1]["byte_end"] += 1
    cases.append(("broken_utf8_passage_byte_span", m6))

    # 7. Non-.invalid URL in synthetic fixture (violates synthetic-fixture hygiene)
    m7 = copy.deepcopy(manifest)
    m7["source_records"][0]["canonical_url"] = "https://www.navcen.uscg.gov/lnm"
    cases.append(("real_world_url_in_synthetic_fixture", m7))

    for name, mutated in cases:
        try:
            validate_manifest_data(mutated, fixture_dir)
        except ValidationError:
            continue
        raise AssertionError(f"Self-test failed: mutation '{name}' was not rejected")

    return len(cases)


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--fixture-dir",
        type=pathlib.Path,
        default=pathlib.Path(__file__).resolve().parents[2] / "fixtures" / "news" / "a06",
        help="Path to the A06 fixture directory",
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="Run negative mutation checks to verify contract enforcement",
    )
    args = parser.parse_args()

    manifest_path = args.fixture_dir / "manifest.json"
    if not manifest_path.is_file():
        print(f"ERROR: Manifest not found at {manifest_path}", file=sys.stderr)
        return 1

    manifest = json.loads(manifest_path.read_bytes().decode("utf-8"))
    summary = validate_manifest_data(manifest, args.fixture_dir)

    neg_count = 0
    if args.self_test:
        neg_count = run_self_tests(manifest, args.fixture_dir)

    print(
        json.dumps(
            {
                "status": "ok",
                "fixture_id": manifest["fixture_metadata"]["fixture_id"],
                "validated_counts": summary,
                "negative_contract_checks_passed": neg_count,
            },
            indent=2,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
