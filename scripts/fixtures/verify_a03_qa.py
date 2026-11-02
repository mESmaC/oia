"""Validate the A03 search, cited summary, and grounded QA fixture expectations (#18).

Uses only the Python standard library (`python -I scripts/fixtures/verify_a03_qa.py --self-test`).
Validates fixture consistency and contract rules; does not claim OIA runtime services are implemented.
"""

import argparse
import copy
import hashlib
import json
import math
from pathlib import Path, PurePosixPath

MAX_FILE_BYTES = 1024 * 1024
REQUIRED_SUMMARY_SECTIONS = (
    "document_identity",
    "subject",
    "claims_and_findings",
    "methods",
    "limitations",
    "unresolved_questions",
)
ALLOWED_EPISTEMIC_STATUSES = {
    "attributed_source_claim",
    "method_summary",
    "limitation_note",
    "insufficient_material_notice",
    "quoted_untrusted_source_text",
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def read_bounded(path: Path) -> bytes:
    with path.open("rb") as stream:
        payload = stream.read(MAX_FILE_BYTES + 1)
    require(len(payload) <= MAX_FILE_BYTES, f"Oversized fixture file: {path.name}")
    return payload


def indexed(items: list, key: str) -> dict:
    require(isinstance(items, list) and 0 < len(items) <= 100, f"Invalid list for key {key}")
    table = {}
    for item in items:
        identity = item.get(key)
        require(isinstance(identity, str) and identity and identity not in table, f"Duplicate/invalid {key}: {identity}")
        table[identity] = item
    return table


def load_base_corpus(root: Path) -> tuple[dict, dict, dict, dict, bytes]:
    manifest_raw = read_bounded(root / "manifest.json")
    manifest = json.loads(manifest_raw.decode("utf-8"))
    require(manifest.get("fixture_schema_version") == 1, "Unsupported base manifest schema")
    require(manifest.get("fixture_id") == "oia-a03-synthetic-v1", "Unexpected base fixture_id")
    require(manifest.get("classification") == "simulation", "Base corpus must be labeled simulation")

    sources = indexed(manifest["sources"], "id")
    artifacts = indexed(manifest["artifacts"], "id")
    records = indexed(manifest["records"], "id")
    records_by_version = indexed(manifest["records"], "version_id")

    payloads = {}
    for artifact_id, artifact in artifacts.items():
        rel = PurePosixPath(artifact["path"])
        require(
            not rel.is_absolute() and ".." not in rel.parts and rel.parts[0] == "originals",
            f"Invalid artifact path: {artifact['path']}",
        )
        raw = read_bounded(root.joinpath(*rel.parts).resolve(strict=True))
        require(len(raw) == artifact["byte_length"], f"Byte length mismatch on {artifact_id}")
        require(hashlib.sha256(raw).hexdigest() == artifact["sha256"], f"SHA-256 mismatch on {artifact_id}")
        payloads[artifact_id] = raw

    ocr_transcript_bytes = manifest["expectations"]["scan"]["page_1_transcript"].encode("utf-8")
    return (
        {"manifest": manifest, "sources": sources, "artifacts": artifacts},
        records,
        records_by_version,
        payloads,
        ocr_transcript_bytes,
    )


def verify_citation(
    citation: dict,
    records: dict,
    payloads: dict,
    ocr_transcript_bytes: bytes,
    allowed_version_ids: set[str] | None = None,
    require_validation_status: bool = True,
) -> None:
    record_id = citation.get("record_id")
    version_id = citation.get("record_version_id")
    artifact_id = citation.get("artifact_id")
    page_num = citation.get("page")
    anchor_kind = citation.get("anchor_kind")
    start = citation.get("start")
    end = citation.get("end")
    quote = citation.get("quote")

    require(record_id in records, f"Citation references unknown record_id: {record_id}")
    record = records[record_id]
    require(record["version_id"] == version_id, f"Citation version mismatch on {record_id}: {version_id}")
    if allowed_version_ids is not None:
        require(version_id in allowed_version_ids, f"Citation version {version_id} outside pinned input versions")

    require(record["retention"] == "retained", f"Citation cannot reference non-retained record {record_id}")
    matching_pages = [p for p in record["pages"] if p["number"] == page_num]
    require(len(matching_pages) == 1, f"Citation references nonexistent page {page_num} on {record_id}")
    page = matching_pages[0]
    require(
        page["expected_extraction"] in ("text_available", "ocr_required"),
        f"Citation cannot reference {page['expected_extraction']} page {page_num} on {record_id}",
    )
    require(page["artifact_id"] == artifact_id, f"Citation artifact mismatch on {record_id} page {page_num}")

    if require_validation_status:
        require(
            citation.get("validation_status") == "structurally_validated",
            "Persisted citation must have validation_status == 'structurally_validated'",
        )

    require(type(start) is int and type(end) is int and 0 <= start < end, "Invalid citation byte offsets")
    require(isinstance(quote, str) and len(quote) > 0, "Empty citation quote")

    if anchor_kind == "utf8_byte_range":
        require(page["expected_extraction"] == "text_available", "utf8_byte_range requires text_available page")
        raw = payloads[artifact_id]
        require(end <= len(raw), f"Citation byte range [{start}, {end}) exceeds {artifact_id} length {len(raw)}")
        extracted = raw[start:end].decode("utf-8")
        require(extracted == quote, f"Citation quote mismatch on {artifact_id}[{start}:{end}]")
    elif anchor_kind == "ocr_transcript_utf8_byte_range":
        require(
            record_id == "scanned-report" and page_num == 1 and page["expected_extraction"] == "ocr_required",
            "ocr_transcript_utf8_byte_range only valid on scanned-report page 1",
        )
        require(end <= len(ocr_transcript_bytes), "OCR transcript byte range out of bounds")
        extracted = ocr_transcript_bytes[start:end].decode("utf-8")
        require(extracted == quote, "OCR transcript citation quote mismatch")
    else:
        raise ValueError(f"Unsupported anchor_kind: {anchor_kind}")


def verify_statement_block(
    stmt_obj: dict,
    records: dict,
    payloads: dict,
    ocr_transcript_bytes: bytes,
    allowed_version_ids: set[str],
) -> None:
    require(isinstance(stmt_obj.get("statement"), str) and stmt_obj["statement"].strip(), "Empty statement")
    require(
        stmt_obj.get("epistemic_status") in ALLOWED_EPISTEMIC_STATUSES,
        f"Invalid epistemic_status: {stmt_obj.get('epistemic_status')}",
    )
    require(
        stmt_obj.get("established_oia_fact") is False,
        "AI draft or source assertion must not set established_oia_fact=True",
    )
    citations = stmt_obj.get("citations")
    require(isinstance(citations, list) and len(citations) > 0, "Substantive statement missing citations")
    for citation in citations:
        verify_citation(
            citation,
            records,
            payloads,
            ocr_transcript_bytes,
            allowed_version_ids=allowed_version_ids,
            require_validation_status=True,
        )


def verify_expectations_data(
    data: dict,
    base_info: dict,
    records: dict,
    records_by_version: dict,
    payloads: dict,
    ocr_transcript_bytes: bytes,
) -> dict:
    manifest = base_info["manifest"]
    require(data.get("fixture_schema_version") == 1, "Unsupported QA fixture schema version")
    require(data.get("fixture_id") == "oia-a03-search-summary-qa-v1", "Unexpected QA fixture_id")
    require(data.get("base_corpus_fixture_id") == manifest["fixture_id"], "Base corpus fixture_id mismatch")
    require(data.get("classification") == "simulation", "QA fixture must retain simulation classification")
    require(
        data.get("requirements") == ["CASE-05", "DOC-03", "DOC-04", "DOC-05"],
        "QA fixture requirements list mismatch",
    )

    superseded_records = {
        rel["to"]: rel["from"]
        for rel in manifest["relations"]
        if rel["kind"] == "supersedes"
    }

    # 1. Verify search cases (CASE-05, DOC-04)
    search_cases = indexed(data.get("search_cases", []), "id")
    for case_id, sc in search_cases.items():
        query = sc["query"]
        require(query["retrieval_mode"] in ("lexical_only", "hybrid_lexical_and_semantic"), "Invalid retrieval_mode")
        pagination = query["pagination"]
        require(type(pagination["limit"]) is int and 1 <= pagination["limit"] <= 500, "Invalid pagination limit")
        if query["retrieval_mode"] == "hybrid_lexical_and_semantic" and sc["semantic_index_status"] != "active":
            require(
                sc["expected_outcome"] == "partial" and sc.get("degradation_reason"),
                f"Hybrid search {case_id} with inactive semantic index must report partial outcome and reason",
            )
        eval_versions = set(sc["coverage"]["evaluated_record_versions"])
        require(eval_versions.issubset(records_by_version.keys()), "Unknown evaluated_record_version in search")
        if query["version_policy"] == "latest_only":
            for sup_rec_id in superseded_records:
                require(
                    records[sup_rec_id]["version_id"] not in eval_versions,
                    f"latest_only search {case_id} included superseded record {sup_rec_id}",
                )

        unsearchable = {
            (u["record_id"], u["record_version_id"], u["page"], u["reason"])
            for u in sc["coverage"]["unsearchable_or_partial_records"]
        }
        expected_unsearchable = {
            ("scanned-report", "scanned-report-version-1", 1, "ocr_required"),
            ("scanned-report", "scanned-report-version-1", 2, "unreadable"),
            ("scanned-report", "scanned-report-version-1", 3, "missing"),
            ("reference-only", "reference-only-version-1", None, "reference_only"),
        }
        require(
            unsearchable == expected_unsearchable,
            f"Search case {case_id} failed to disclose exact unsearchable/reference-only coverage gaps",
        )

        require(isinstance(sc["expected_hits"], list) and len(sc["expected_hits"]) > 0, "Empty search hits")
        for hit in sc["expected_hits"]:
            verify_citation(
                hit,
                records,
                payloads,
                ocr_transcript_bytes,
                allowed_version_ids=eval_versions,
                require_validation_status=False,
            )
            require(query["text"] in hit["quote"], f"Search hit in {case_id} does not contain query text")
            expected_sup = superseded_records.get(hit["record_id"])
            require(hit["superseded_by_record_id"] == expected_sup, f"Incorrect supersession tag on {hit['record_id']}")

    # 2. Verify saved searches (CASE-05)
    saved_searches = indexed(data.get("saved_searches", []), "version_id")
    for ss_ver, ss in saved_searches.items():
        require(ss["query_ref"] in search_cases, f"Saved search {ss_ver} references unknown query_ref")
        pred = ss["predecessor_version_id"]
        require(pred is None or pred in saved_searches, f"Saved search {ss_ver} has invalid predecessor {pred}")
        for rv in ss["pinned_collection_record_version_ids"]:
            require(rv in records_by_version, f"Saved search {ss_ver} pins unknown record_version_id {rv}")

    # 3. Verify summary drafts (DOC-03, DOC-05)
    summary_drafts = indexed(data.get("summary_drafts", []), "id")
    for draft_id, draft in summary_drafts.items():
        require(draft["origin_class"] == "analytical_inference", f"Draft {draft_id} must be analytical_inference")
        require(draft["draft_label"] == "ai_generated_draft", f"Draft {draft_id} missing ai_generated_draft label")
        require(draft["review_status"] == "unreviewed", f"Model draft {draft_id} cannot mark itself reviewed")
        require(draft["freshness_state"] in ("current", "needs_reassessment"), "Invalid freshness_state")

        prov = draft["model_provenance"]
        for key in ("provider_id", "model_id", "model_version", "prompt_template_id", "egress_mode", "generated_at"):
            require(isinstance(prov.get(key), str) and prov[key], f"Draft {draft_id} missing provenance {key}")
        require(type(prov.get("prompt_template_version")) is int, "Invalid prompt_template_version")
        temp = prov.get("temperature")
        require(
            type(temp) in (int, float) and math.isfinite(temp) and 0.0 <= float(temp) <= 2.0,
            f"Draft {draft_id} temperature must be finite in [0.0, 2.0]",
        )
        require(prov.get("seed") is None or type(prov.get("seed")) is int, f"Draft {draft_id} seed must be int or None")
        budget = prov.get("token_budget")
        require(type(budget) is int and budget > 0, f"Draft {draft_id} token_budget must be positive int")

        target_rec = records[draft["target_record_id"]]
        target_ver = draft["target_record_version_id"]
        require(target_rec["version_id"] == target_ver, f"Draft {draft_id} target version mismatch")

        coverage = draft["coverage"]
        included_pages = {p["page"] for p in coverage["included_pages"]}
        omitted_pages = {(p["page"], p["reason"]) for p in coverage["omitted_pages"]}
        expected_omitted = {
            (p["number"], p["expected_extraction"])
            for p in target_rec["pages"]
            if p["expected_extraction"] in ("unreadable", "missing")
        }
        require(
            omitted_pages == expected_omitted,
            f"Draft {draft_id} omitted_pages {omitted_pages} does not match expected {expected_omitted}",
        )
        if expected_omitted:
            require(coverage["completeness"] == "partial", f"Draft {draft_id} with omitted pages must be partial")
        else:
            require(coverage["completeness"] == "complete", f"Draft {draft_id} without omissions must be complete")
        require(
            included_pages == {p["number"] for p in target_rec["pages"] if p["number"] not in {n for n, _ in expected_omitted}},
            f"Draft {draft_id} included_pages mismatch",
        )

        sections = draft["sections"]
        require(tuple(sections.keys()) == REQUIRED_SUMMARY_SECTIONS, f"Draft {draft_id} missing required DOC-03 sections")
        for sec_name in REQUIRED_SUMMARY_SECTIONS[:-1]:
            sec_items = sections[sec_name]
            require(isinstance(sec_items, list) and len(sec_items) > 0, f"Draft {draft_id} empty section {sec_name}")
            for stmt_obj in sec_items:
                verify_statement_block(stmt_obj, records, payloads, ocr_transcript_bytes, {target_ver})
        unresolved = sections["unresolved_questions"]
        require(
            isinstance(unresolved, list) and len(unresolved) > 0 and all(isinstance(q, str) and q for q in unresolved),
            f"Draft {draft_id} must include non-empty unresolved_questions",
        )

    # 4. Verify grounded QA cases (DOC-04, DOC-05)
    qa_cases = indexed(data.get("qa_cases", []), "id")
    for qa_id, qa in qa_cases.items():
        require(qa["origin_class"] == "analytical_inference", f"QA {qa_id} must be analytical_inference")
        require(qa["review_status"] == "unreviewed", f"QA {qa_id} cannot self-review")
        queried_versions = set(qa["queried_record_version_ids"])
        require(
            len(queried_versions) > 0 and queried_versions.issubset(records_by_version.keys()),
            f"QA {qa_id} has invalid queried_record_version_ids",
        )
        status = qa["answer_status"]
        require(status in ("grounded_answer", "insufficient_material", "provider_unavailable"), "Invalid answer_status")
        require(isinstance(qa["statements"], list) and len(qa["statements"]) > 0, f"QA {qa_id} has no statements")
        for stmt_obj in qa["statements"]:
            verify_statement_block(stmt_obj, records, payloads, ocr_transcript_bytes, queried_versions)
            if status == "insufficient_material":
                require(
                    stmt_obj["epistemic_status"] == "insufficient_material_notice",
                    f"Insufficient-material QA {qa_id} cannot emit grounded claims",
                )
        if status == "insufficient_material":
            require(
                isinstance(qa.get("insufficiency_reasons"), list) and len(qa["insufficiency_reasons"]) > 0,
                f"QA {qa_id} must list insufficiency_reasons",
            )

        expected_qa_omissions = set()
        for rv in queried_versions:
            rec = records_by_version[rv]
            if rec["retention"] == "reference_only":
                expected_qa_omissions.add((rec["id"], rv, None, "reference_only"))
            for p in rec["pages"]:
                if p["expected_extraction"] in ("unreadable", "missing"):
                    expected_qa_omissions.add((rec["id"], rv, p["number"], p["expected_extraction"]))
        actual_qa_omissions = {
            (o["record_id"], o["record_version_id"], o["page"], o["reason"])
            for o in qa["omitted_records_or_pages"]
        }
        require(
            actual_qa_omissions == expected_qa_omissions,
            f"QA {qa_id} omitted_records_or_pages mismatch: {actual_qa_omissions} != {expected_qa_omissions}",
        )

    # 5. Verify correction reassessment (DOC-05, A03)
    corr = data["correction_reassessment"]
    require(
        corr["superseded_record_id"] == "report-original"
        and corr["superseded_record_version_id"] == "report-original-version-1"
        and corr["replacement_record_id"] == "report-correction"
        and corr["replacement_record_version_id"] == "report-correction-version-1",
        "Correction reassessment record/version binding mismatch",
    )
    require(corr["invalidated_summary_id"] in summary_drafts, "Invalidated summary ID missing")
    for inv_qa in corr["invalidated_qa_ids"]:
        require(inv_qa in qa_cases, f"Invalidated QA ID {inv_qa} missing")
    post_state = corr["post_correction_state_of_prior_outputs"]
    require(
        post_state == {
            "freshness_state": "needs_reassessment",
            "invalidated_by_record_version_id": "report-correction-version-1",
            "invalidation_reason": "source_record_superseded",
            "prior_citations_remain_resolvable": True,
        },
        "Post-correction invalidation state violated",
    )
    # Confirm prior citations in the invalidated summary still resolve against report-v1.txt
    prior_summary = summary_drafts[corr["invalidated_summary_id"]]
    for sec_name in REQUIRED_SUMMARY_SECTIONS[:-1]:
        for stmt_obj in prior_summary["sections"][sec_name]:
            verify_statement_block(
                stmt_obj, records, payloads, ocr_transcript_bytes, {"report-original-version-1"}
            )
    reassessed = corr["reassessed_summary"]
    require(
        reassessed["predecessor_summary_id"] == corr["invalidated_summary_id"]
        and reassessed["target_record_version_id"] == "report-correction-version-1"
        and reassessed["origin_class"] == "analytical_inference"
        and reassessed["review_status"] == "unreviewed"
        and reassessed["freshness_state"] == "current",
        "Reassessed summary envelope invalid",
    )
    verify_statement_block(
        reassessed["updated_claim_statement"],
        records,
        payloads,
        ocr_transcript_bytes,
        {"report-correction-version-1"},
    )

    # 6. Verify prompt-injection isolation (DOC-05, A03)
    inj = data["prompt_injection_isolation"]
    require(
        inj["record_id"] == "untrusted-source"
        and inj["record_version_id"] == "untrusted-source-version-1"
        and inj["artifact_id"] == "untrusted.txt",
        "Prompt injection target record mismatch",
    )
    span_citation = {
        "record_id": inj["record_id"],
        "record_version_id": inj["record_version_id"],
        "artifact_id": inj["artifact_id"],
        **inj["adversarial_span"],
    }
    verify_citation(
        span_citation,
        records,
        payloads,
        ocr_transcript_bytes,
        allowed_version_ids={"untrusted-source-version-1"},
        require_validation_status=False,
    )
    require(inj["treated_as"] == "untrusted_research_data", "Untrusted source must be treated_as untrusted_research_data")
    require(
        inj["executed_actions"] == manifest["expectations"]["untrusted_source_allowed_actions"] == []
        and inj["granted_permissions"] == []
        and inj["network_requests_issued"] == []
        and inj["case_visibility_changed"] is False,
        "Prompt injection must execute zero actions, network requests, or permission/visibility changes",
    )
    verify_statement_block(
        inj["safe_summary_statement"],
        records,
        payloads,
        ocr_transcript_bytes,
        {"untrusted-source-version-1"},
    )
    require(
        inj["safe_summary_statement"]["epistemic_status"] == "quoted_untrusted_source_text",
        "Prompt-injection summary statement must use quoted_untrusted_source_text epistemic status",
    )

    return {
        "search_cases": len(search_cases),
        "saved_searches": len(saved_searches),
        "summary_drafts": len(summary_drafts),
        "qa_cases": len(qa_cases),
        "reassessment_verified": True,
        "prompt_injection_isolated": True,
    }


def run_self_tests(
    data: dict,
    base_info: dict,
    records: dict,
    records_by_version: dict,
    payloads: dict,
    ocr_transcript_bytes: bytes,
) -> int:
    mutations = []

    def mut_shifted_anchor(d: dict) -> None:
        d["summary_drafts"][0]["sections"]["claims_and_findings"][0]["citations"][0]["start"] += 1

    def mut_cite_unreadable_page(d: dict) -> None:
        d["summary_drafts"][1]["sections"]["limitations"][0]["citations"][0]["page"] = 2
        d["summary_drafts"][1]["sections"]["limitations"][0]["citations"][0]["artifact_id"] = "scan-page-2.png"

    def mut_hide_omitted_page(d: dict) -> None:
        d["summary_drafts"][1]["coverage"]["omitted_pages"].pop()

    def mut_promote_fact_or_self_review(d: dict) -> None:
        d["summary_drafts"][0]["sections"]["claims_and_findings"][0]["established_oia_fact"] = True

    def mut_skip_reassessment(d: dict) -> None:
        d["correction_reassessment"]["post_correction_state_of_prior_outputs"]["freshness_state"] = "current"

    def mut_allow_injected_action(d: dict) -> None:
        d["prompt_injection_isolation"]["executed_actions"] = ["publish_case"]

    def mut_invalid_temperature(d: dict) -> None:
        d["summary_drafts"][0]["model_provenance"]["temperature"] = math.nan

    mutations.extend([
        ("shifted_anchor", mut_shifted_anchor),
        ("cite_unreadable_page", mut_cite_unreadable_page),
        ("hide_omitted_page", mut_hide_omitted_page),
        ("promote_fact_or_self_review", mut_promote_fact_or_self_review),
        ("skip_reassessment", mut_skip_reassessment),
        ("allow_injected_action", mut_allow_injected_action),
        ("invalid_temperature", mut_invalid_temperature),
    ])

    for name, fn in mutations:
        mutated = copy.deepcopy(data)
        fn(mutated)
        try:
            verify_expectations_data(
                mutated, base_info, records, records_by_version, payloads, ocr_transcript_bytes
            )
        except ValueError:
            continue
        raise ValueError(f"Self-test failed: negative mutation '{name}' was not rejected")

    return len(mutations)


def verify(root: Path, run_negative_tests: bool = False) -> dict:
    root = root.resolve(strict=True)
    base_info, records, records_by_version, payloads, ocr_transcript_bytes = load_base_corpus(root)
    qa_raw = read_bounded(root / "qa_summary_expectations.json")
    data = json.loads(qa_raw.decode("utf-8"))
    summary = verify_expectations_data(
        data, base_info, records, records_by_version, payloads, ocr_transcript_bytes
    )
    if run_negative_tests:
        summary["negative_self_tests_passed"] = run_self_tests(
            data, base_info, records, records_by_version, payloads, ocr_transcript_bytes
        )
    return summary


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "root",
        nargs="?",
        type=Path,
        default=Path(__file__).resolve().parents[2] / "fixtures/documents/a03",
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="Run negative contract-mutation tests in addition to positive validation",
    )
    args = parser.parse_args()
    try:
        result = verify(args.root, run_negative_tests=args.self_test)
    except (OSError, ValueError, KeyError, TypeError, IndexError, UnicodeDecodeError) as error:
        parser.exit(1, f"A03 search/summary/QA fixture invalid: {error}\n")
    print(f"A03 search/summary/QA fixture passed: {json.dumps(result, sort_keys=True)}")
    print("Runtime search, synthesis adapters, and end-to-end A03 acceptance have NOT been executed.")
