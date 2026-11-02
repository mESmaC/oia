"""Validate the checked-in document lineage, release-comparison, and correction-propagation fixture corpus.

This script validates fixture integrity and reference contract invariants for issue #19
(DOC-02, DOC-04, DOC-05, INTEL-01; scenarios A03 and A08). It does not claim that OIA's
production Rust runtime or Lean proof suite has been executed.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
from pathlib import Path, PurePosixPath

MAX_FILE_BYTES = 1024 * 1024
MAX_GRAPH_NODES = 200
MAX_GRAPH_EDGES = 500
LINEAGE_EDGE_KINDS = ("republishes", "quotes", "cites", "translates")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def read_bounded(path: Path) -> bytes:
    with path.open("rb") as stream:
        payload = stream.read(MAX_FILE_BYTES + 1)
    require(len(payload) <= MAX_FILE_BYTES, f"Oversized fixture file: {path.name}")
    return payload


def indexed(items: list, key: str) -> dict:
    require(
        isinstance(items, list) and len(items) <= MAX_GRAPH_NODES,
        f"Invalid or excessive fixture list for key {key}",
    )
    result = {}
    for item in items:
        require(isinstance(item, dict) and key in item, f"Missing key {key}")
        identity = item[key]
        require(
            isinstance(identity, str) and identity and identity not in result,
            f"Duplicate or invalid {key}: {identity!r}",
        )
        result[identity] = item
    return result


def compute_exact_duplicate_groups(records: dict, artifacts: dict) -> list[dict]:
    by_hash: dict[str, list[dict]] = {}
    for rec in records.values():
        if rec["retention"] != "retained":
            continue
        for page in rec["pages"]:
            art_id = page["artifact_id"]
            if art_id is not None:
                sha = artifacts[art_id]["sha256"]
                by_hash.setdefault(sha, []).append(rec)
    groups = []
    for sha, members in sorted(by_hash.items()):
        if len(members) > 1:
            art_ids = {m["pages"][0]["artifact_id"] for m in members}
            require(len(art_ids) == 1, "Identical sha256 mapped to multiple artifact IDs")
            art_id = next(iter(art_ids))
            member_versions = [m["version_id"] for m in members]
            source_ids = [m["source_id"] for m in members]
            require(
                len(set(member_versions)) == len(member_versions),
                "Duplicate record version in exact-duplicate group",
            )
            require(
                len(set(source_ids)) == len(source_ids),
                "Exact-byte duplicate group collapsed distinct source_ids",
            )
            groups.append(
                {
                    "artifact_id": art_id,
                    "sha256": sha,
                    "member_record_versions": member_versions,
                    "distinct_source_ids": source_ids,
                }
            )
    return groups


def verify_release_comparisons(
    comparisons: list[dict], records: dict, payloads: dict[str, bytes]
) -> None:
    allowed_kinds = {
        "unchanged_unredacted",
        "newly_disclosed_in_right",
        "newly_redacted_in_right",
        "differently_redacted",
        "revised_unredacted",
        "coverage_gap",
    }
    for comp in comparisons:
        left_rec = records[comp["left_record_version_id"]]
        right_rec = records[comp["right_record_version_id"]]
        require(
            left_rec["version_id"] != right_rec["version_id"],
            "Release comparison must reference distinct record versions",
        )
        left_pages = {p["number"]: p["artifact_id"] for p in left_rec["pages"]}
        right_pages = {p["number"]: p["artifact_id"] for p in right_rec["pages"]}
        for page_num, art_id in left_pages.items():
            left_raw = payloads[art_id].decode("utf-8")
            for forbidden in comp.get("forbidden_cleartext_in_left", []):
                require(
                    forbidden not in left_raw,
                    f"Hidden cleartext {forbidden!r} leaked in redacted artifact {art_id}",
                )
        require(comp.get("segments"), f"Empty segments in {comp['comparison_id']}")
        for seg in comp["segments"]:
            kind = seg["segment_kind"]
            require(kind in allowed_kinds, f"Unsupported segment_kind: {kind}")
            page = seg["page"]
            l_start, l_end = seg["left_byte_range"]
            r_start, r_end = seg["right_byte_range"]
            l_raw = payloads[left_pages[page]]
            r_raw = payloads[right_pages[page]]
            require(
                0 <= l_start < l_end <= len(l_raw) and 0 <= r_start < r_end <= len(r_raw),
                f"Invalid release comparison byte range in {comp['comparison_id']}",
            )
            l_slice = l_raw[l_start:l_end].decode("utf-8")
            r_slice = r_raw[r_start:r_end].decode("utf-8")
            require(
                l_slice == seg["left_text"] and r_slice == seg["right_text"],
                f"Release comparison text mismatch in {comp['comparison_id']}",
            )
            if kind == "unchanged_unredacted":
                require(l_slice == r_slice, "unchanged_unredacted segments must match byte-for-byte")
            elif kind == "newly_disclosed_in_right":
                require(
                    "REDACTED" in l_slice and "REDACTED" not in r_slice and "WITHHELD" not in r_slice,
                    "newly_disclosed_in_right must transition from redacted marker to disclosed text",
                )
            elif kind == "newly_redacted_in_right":
                require(
                    "REDACTED" not in l_slice and ("REDACTED" in r_slice or "WITHHELD" in r_slice),
                    "newly_redacted_in_right must transition from disclosed text to redacted marker",
                )
            elif kind == "differently_redacted":
                require(
                    l_slice != r_slice
                    and ("REDACTED" in l_slice or "WITHHELD" in l_slice)
                    and ("REDACTED" in r_slice or "WITHHELD" in r_slice),
                    "differently_redacted segments must both be redaction markers that differ",
                )


def compute_lineage_sccs(
    node_ids: list[str], relations: list[dict]
) -> tuple[dict[str, set[str]], list[list[str]], dict[str, list[str]]]:
    require(len(relations) <= MAX_GRAPH_EDGES, "Lineage graph edge budget exceeded")
    adj: dict[str, list[str]] = {n: [] for n in sorted(node_ids)}
    for rel in relations:
        if rel["kind"] in LINEAGE_EDGE_KINDS:
            src, dst = rel["from"], rel["to"]
            require(src in adj and dst in adj, f"Dangling lineage edge: {src} -> {dst}")
            adj[src].append(dst)
    for n in adj:
        adj[n].sort()

    index = 0
    indices: dict[str, int] = {}
    lowlink: dict[str, int] = {}
    stack: list[str] = []
    on_stack: set[str] = set()
    sccs: list[list[str]] = []

    def strongconnect(node: str) -> None:
        nonlocal index
        indices[node] = index
        lowlink[node] = index
        index += 1
        stack.append(node)
        on_stack.add(node)
        for neighbor in adj[node]:
            if neighbor not in indices:
                strongconnect(neighbor)
                lowlink[node] = min(lowlink[node], lowlink[neighbor])
            elif neighbor in on_stack:
                lowlink[node] = min(lowlink[node], indices[neighbor])
        if lowlink[node] == indices[node]:
            component = []
            while True:
                w = stack.pop()
                on_stack.remove(w)
                component.append(w)
                if w == node:
                    break
            sccs.append(sorted(component))

    for n in sorted(adj):
        if n not in indices:
            strongconnect(n)

    cyclic_sccs: list[list[str]] = []
    cycle_witnesses: dict[str, list[str]] = {}
    for comp in sccs:
        is_cycle = len(comp) > 1 or (len(comp) == 1 and comp[0] in adj[comp[0]])
        if is_cycle:
            cyclic_sccs.append(comp)
            start = comp[0]
            comp_set = set(comp)
            witness = [start]
            curr = start
            visited = {start}
            while True:
                next_candidates = [nb for nb in adj[curr] if nb in comp_set]
                require(bool(next_candidates), "Broken cycle SCC")
                if start in next_candidates:
                    witness.append(start)
                    break
                chosen = next((nb for nb in next_candidates if nb not in visited), next_candidates[0])
                witness.append(chosen)
                if chosen in visited:
                    break
                visited.add(chosen)
                curr = chosen
            for member in comp:
                cycle_witnesses[member] = witness

    return {k: set(v) for k, v in adj.items()}, cyclic_sccs, cycle_witnesses


def evaluate_corroboration_query(
    query: dict,
    records: dict,
    derivations: dict,
    relations: list[dict],
    artifacts: dict,
) -> None:
    adj, cyclic_sccs, cycle_witnesses = compute_lineage_sccs(list(records.keys()), relations)
    cyclic_members = {m for comp in cyclic_sccs for m in comp}

    supporting_records: list[str] = []
    excluded_derivatives: list[str] = []
    for item_id in query["citing_inputs"]:
        if item_id in derivations:
            deriv = derivations[item_id]
            require(
                deriv["origin_class"] != "simulation",
                "Derivation must declare analytical_inference or extracted_claim origin_class",
            )
            excluded_derivatives.append(item_id)
        elif item_id in records:
            supporting_records.append(item_id)
        else:
            raise ValueError(f"Unknown citing input in corroboration query: {item_id}")

    supporting_records.sort()
    excluded_derivatives.sort()
    require(
        supporting_records == query["expected_supporting_record_versions"],
        f"Supporting records mismatch: {supporting_records}",
    )
    require(
        excluded_derivatives == query["expected_excluded_derivative_runs"],
        f"Excluded derivatives mismatch: {excluded_derivatives}",
    )

    # Trace upstream roots for each supporting record
    circular_only: list[str] = []
    upstream_roots: set[str] = set()
    for rec_id in supporting_records:
        visited: set[str] = set()
        stack = [rec_id]
        reached_roots: set[str] = set()
        hit_cycle = False
        while stack:
            curr = stack.pop()
            if curr in visited:
                continue
            visited.add(curr)
            if curr in cyclic_members:
                hit_cycle = True
            outs = adj[curr]
            if not outs:
                if curr not in cyclic_members:
                    reached_roots.add(curr)
            else:
                for nxt in outs:
                    if nxt not in visited:
                        stack.append(nxt)
        if not reached_roots and hit_cycle:
            circular_only.append(rec_id)
        upstream_roots.update(reached_roots)

    circular_only.sort()
    require(
        circular_only == query["expected_circular_only_record_versions"],
        f"Circular-only records mismatch: {circular_only}",
    )
    if circular_only:
        witness = cycle_witnesses[circular_only[0]]
        require(
            witness == query["expected_cycle_witness"],
            f"Cycle witness mismatch: {witness} != {query['expected_cycle_witness']}",
        )

    # Collapse any exact-byte duplicates among upstream roots by SHA-256
    deduped_roots: list[str] = []
    seen_hashes: set[str] = set()
    for root_id in sorted(upstream_roots):
        rec = records[root_id]
        page_hashes = tuple(
            artifacts[p["artifact_id"]]["sha256"]
            for p in rec["pages"]
            if p["artifact_id"] is not None
        )
        hash_key = "|".join(page_hashes) if page_hashes else f"no-bytes:{root_id}"
        if hash_key not in seen_hashes:
            seen_hashes.add(hash_key)
            deduped_roots.append(root_id)

    require(
        deduped_roots == query["expected_independent_supporting_roots"],
        f"Independent supporting roots mismatch: {deduped_roots}",
    )
    require(
        len(deduped_roots) == query["expected_independent_root_count"],
        "Independent root count mismatch",
    )

    # Check earliest-known-source view semantics (no invented timestamps for undated roots)
    dated_roots = [
        r for r in deduped_roots if records[r]["publication_time"] is not None
    ]
    undated_roots = [
        r for r in deduped_roots if records[r]["publication_time"] is None
    ]
    for u in undated_roots:
        require(
            bool(records[u]["unknown_time_reason"]),
            f"Undated root {u} missing unknown_time_reason",
        )
    dated_roots.sort(key=lambda r: (records[r]["publication_time"], r))
    earliest_view = query["earliest_known_source_view"]
    if undated_roots and dated_roots:
        require(
            earliest_view["resolution"] == "ambiguous_candidates",
            "Presence of undated independent root must report ambiguous_candidates",
        )
    require(
        earliest_view["earliest_dated_root"] == (dated_roots[0] if dated_roots else None),
        "Earliest dated root mismatch",
    )
    require(
        earliest_view["undated_candidate_roots"] == undated_roots,
        "Undated candidate roots mismatch",
    )
    ref_only = sorted(
        r["version_id"] for r in records.values() if r["retention"] == "reference_only"
    )
    cov = earliest_view["coverage_qualification"]
    require(
        cov["omitted_reference_only_records"] == ref_only
        and cov["detected_cycle_count"] == len(cyclic_sccs),
        "Earliest-known-source coverage qualification mismatch",
    )


def compute_invalidation_propagation(
    records: dict, derivations: dict, relations: list[dict]
) -> tuple[list[dict], list[str], list[str]]:
    # 1. Verify derived_from forms a strict DAG (no cycles allowed in derivations)
    all_nodes = set(records.keys()) | set(derivations.keys())
    visiting: set[str] = set()
    visited: set[str] = set()

    def check_dag(node_id: str) -> None:
        if node_id in visited:
            return
        require(node_id not in visiting, f"DerivationCycleDetected at {node_id}")
        visiting.add(node_id)
        if node_id in derivations:
            parents = derivations[node_id]["derived_from"]
            require(isinstance(parents, list) and parents, f"Empty derived_from on {node_id}")
            for parent_id in parents:
                require(parent_id in all_nodes, f"Unknown derived_from target {parent_id}")
                check_dag(parent_id)
        visiting.remove(node_id)
        visited.add(node_id)

    for d_id in sorted(derivations):
        check_dag(d_id)

    # 2. Identify invalidating transitions on SourceRecords (supersedes and retracted_by)
    triggers: dict[str, dict] = {}
    for rel in relations:
        if rel["kind"] == "supersedes":
            old_id, new_id = rel["to"], rel["from"]
            require(
                records[old_id]["lifecycle_state"] == "superseded_by_correction",
                f"Superseded record {old_id} must have lifecycle_state superseded_by_correction",
            )
            triggers[old_id] = {
                "trigger_record_version_id": old_id,
                "replacement_record_version_id": new_id,
                "reason": "source_corrected",
                "preserve_old_citations": records[old_id]["retention"] == "retained",
            }
        elif rel["kind"] == "retracted_by":
            old_id, new_id = rel["to"], rel["from"]
            require(
                records[old_id]["lifecycle_state"] == "retracted",
                f"Retracted record {old_id} must have lifecycle_state retracted",
            )
            triggers[old_id] = {
                "trigger_record_version_id": old_id,
                "replacement_record_version_id": new_id,
                "reason": "source_retracted",
                "preserve_old_citations": records[old_id]["retention"] == "retained",
            }

    # 3. Verify removed endpoints are NEVER auto-marked retracted (DOC-05, NEWS-05, A06)
    removed_not_retracted = []
    for rec in records.values():
        if rec["availability"] == "removed":
            require(
                rec["lifecycle_state"] != "retracted" and rec["version_id"] not in triggers,
                f"Removed record {rec['version_id']} must not be auto-marked retracted",
            )
            removed_not_retracted.append(rec["version_id"])
    removed_not_retracted.sort()

    # 4. Walk downstream derivations transitively
    def upstream_triggers(node_id: str) -> list[dict]:
        found: dict[str, dict] = {}
        stack = [node_id]
        seen: set[str] = set()
        while stack:
            curr = stack.pop()
            if curr in seen:
                continue
            seen.add(curr)
            if curr in triggers:
                found[curr] = triggers[curr]
            if curr in derivations:
                for parent in derivations[curr]["derived_from"]:
                    stack.append(parent)
        return [found[k] for k in sorted(found)]

    needs_reassessment = []
    fresh_versions = []
    for d_id, deriv in derivations.items():
        active_triggers = upstream_triggers(d_id)
        if active_triggers:
            for trig in active_triggers:
                needs_reassessment.append(
                    {
                        "target_version_id": d_id,
                        "trigger_record_version_id": trig["trigger_record_version_id"],
                        "replacement_record_version_id": trig["replacement_record_version_id"],
                        "reason": trig["reason"],
                        "preserve_old_citations": trig["preserve_old_citations"],
                    }
                )
        else:
            fresh_versions.append(d_id)

    return needs_reassessment, sorted(fresh_versions), removed_not_retracted


def verify_a03_cross_check(a03_root: Path) -> None:
    if not a03_root.exists():
        return
    a03_manifest = json.loads(read_bounded(a03_root / "manifest.json"))
    a03_records = indexed(a03_manifest["records"], "id")
    a03_relations = a03_manifest["relations"]
    _, cyclic_sccs, _ = compute_lineage_sccs(list(a03_records.keys()), a03_relations)
    require(cyclic_sccs == [], "Unexpected cycle in base A03 fixture relations")
    corr = a03_manifest["expectations"]["correction"]
    require(
        corr["dependent_result_state"] == "needs_reassessment"
        and corr["preserve_old_citations"] is True,
        "A03 correction expectation out of sync with lineage contract",
    )


def verify_manifest_data(
    manifest: dict, payloads: dict[str, bytes]
) -> dict:
    require(
        manifest.get("fixture_schema_version") == 1
        and manifest.get("classification") == "simulation",
        "Unsupported or unlabeled lineage fixture",
    )
    require(
        manifest.get("requirements") == ["DOC-02", "DOC-04", "DOC-05", "INTEL-01"],
        "Unexpected fixture requirement list",
    )
    sources = indexed(manifest["sources"], "id")
    artifacts = indexed(manifest["artifacts"], "id")
    records = indexed(manifest["records"], "version_id")
    derivations = indexed(manifest["derivations"], "version_id")
    require(
        not (set(records.keys()) & set(derivations.keys())),
        "Record and derivation version_ids must be disjoint",
    )

    for rec in records.values():
        require(
            rec["source_id"] in sources and rec["origin_class"] == "simulation",
            f"Invalid record source or origin_class on {rec['version_id']}",
        )
        require(rec["review_status"] == "unreviewed", "Fixture source record asserts review")
        if rec["event_time"] is None or rec["publication_time"] is None:
            require(
                isinstance(rec["unknown_time_reason"], str) and bool(rec["unknown_time_reason"]),
                f"Record {rec['version_id']} with null timestamp must state unknown_time_reason",
            )
        if rec["predecessor_version_id"] is not None:
            require(
                rec["predecessor_version_id"] in records,
                f"Missing predecessor {rec['predecessor_version_id']}",
            )
        if rec["retention"] == "reference_only":
            require(
                not rec["pages"] and bool(rec["omission_reason"]),
                f"Reference-only record {rec['version_id']} must not retain page bytes",
            )
        else:
            require(
                rec["retention"] == "retained"
                and bool(rec["pages"])
                and rec["omission_reason"] is None,
                f"Invalid retained record {rec['version_id']}",
            )
            for page in rec["pages"]:
                require(
                    page["artifact_id"] in artifacts
                    and page["expected_extraction"] == "text_available",
                    f"Invalid page entry on {rec['version_id']}",
                )

    expectations = manifest["expectations"]

    # 1. Exact-byte duplicate grouping
    dup_groups = compute_exact_duplicate_groups(records, artifacts)
    require(
        dup_groups == expectations["exact_duplicate_groups"],
        f"Exact duplicate groups mismatch: {dup_groups}",
    )

    # 2. Version-bound UTF-8 byte-range citations
    for citation in expectations["citations"]:
        rec = records[citation["record_version_id"]]
        require(
            any(
                p["artifact_id"] == citation["artifact_id"] and p["number"] == citation["page"]
                for p in rec["pages"]
            ),
            f"Citation artifact/page mismatch for {citation['record_version_id']}",
        )
        require(citation["anchor_kind"] == "utf8_byte_range", "Unsupported anchor_kind")
        raw = payloads[citation["artifact_id"]]
        start, end = citation["start"], citation["end"]
        require(
            type(start) is int and type(end) is int and 0 <= start < end <= len(raw),
            f"Invalid byte anchor range [{start}, {end})",
        )
        decoded = raw[start:end].decode("utf-8")
        require(
            decoded == citation["quote"],
            f"Citation quote mismatch on {citation['record_version_id']}: {decoded!r}",
        )

    # 3. Release and redaction comparisons
    verify_release_comparisons(expectations["release_comparisons"], records, payloads)

    # 4. Lineage graph, circular citations, earliest-known-source, and corroboration independence
    for query in expectations["corroboration_queries"]:
        evaluate_corroboration_query(
            query, records, derivations, manifest["relations"], artifacts
        )

    # 5. Acyclic derivation DAG & correction/retraction invalidation propagation
    needs_reassessment, fresh_versions, removed_not_retracted = (
        compute_invalidation_propagation(records, derivations, manifest["relations"])
    )
    inv_exp = expectations["invalidation_propagation"]
    require(
        needs_reassessment == inv_exp["needs_reassessment"],
        f"needs_reassessment mismatch: {needs_reassessment}",
    )
    require(
        fresh_versions == inv_exp["unaffected_fresh_versions"],
        f"unaffected_fresh_versions mismatch: {fresh_versions}",
    )
    require(
        removed_not_retracted == inv_exp["removed_not_auto_retracted_record_versions"],
        f"removed_not_auto_retracted mismatch: {removed_not_retracted}",
    )

    return {
        "artifacts": len(artifacts),
        "source_records": len(records),
        "derivations": len(derivations),
        "exact_citations": len(expectations["citations"]),
        "release_comparisons": len(expectations["release_comparisons"]),
        "corroboration_queries": len(expectations["corroboration_queries"]),
    }


def verify(root: Path) -> dict:
    root = root.resolve(strict=True)
    gitattributes = read_bounded(root / ".gitattributes").decode("utf-8")
    require(
        "originals/** -text" in gitattributes and "manifest.json text eol=lf" in gitattributes,
        "Missing required .gitattributes rules for byte-exact originals",
    )
    manifest_raw = read_bounded(root / "manifest.json")
    require(b"\r\n" not in manifest_raw, "manifest.json must use LF line endings")
    manifest = json.loads(manifest_raw)
    artifacts = indexed(manifest["artifacts"], "id")
    payloads: dict[str, bytes] = {}
    paths: set[Path] = set()
    for key, artifact in artifacts.items():
        relative = PurePosixPath(artifact["path"])
        require(
            not relative.is_absolute()
            and ".." not in relative.parts
            and "\\" not in str(relative)
            and ":" not in str(relative)
            and relative.parts[0] == "originals",
            f"Invalid fixture path: {artifact['path']}",
        )
        path = root.joinpath(*relative.parts).resolve(strict=True)
        require(
            path.is_relative_to(root / "originals") and path not in paths,
            f"Escaping or duplicate fixture path: {path}",
        )
        paths.add(path)
        raw = read_bounded(path)
        require(b"\r\n" not in raw, f"CRLF line ending corruption in {key}")
        require(len(raw) == artifact["byte_length"], f"Byte length mismatch: {key}")
        require(hashlib.sha256(raw).hexdigest() == artifact["sha256"], f"Hash mismatch: {key}")
        require(artifact["origin_class"] == "simulation", f"Lost simulation label: {key}")
        require(
            artifact["media_type"] == "text/plain; charset=utf-8",
            f"Unsupported media type: {artifact['media_type']}",
        )
        text = raw.decode("utf-8")
        require(
            text.startswith("SYNTHETIC LINEAGE FIXTURE — NOT OBSERVED EVIDENCE\n"),
            f"Missing visible synthetic fixture header with non-ASCII prefix in {key}",
        )
        payloads[key] = raw

    actual_files = {p.resolve() for p in (root / "originals").rglob("*") if p.is_file()}
    require(paths == actual_files, "Unmanifested original file in originals/")

    summary = verify_manifest_data(manifest, payloads)
    verify_a03_cross_check(root.parent / "a03")
    return summary


def run_self_test(root: Path) -> None:
    root = root.resolve(strict=True)
    manifest = json.loads(read_bounded(root / "manifest.json"))
    payloads = {
        a["id"]: read_bounded(root / a["path"]) for a in manifest["artifacts"]
    }

    # Case 1: Shifted UTF-8 byte anchor must be rejected
    bad_anchor = copy.deepcopy(manifest)
    bad_anchor["expectations"]["citations"][0]["start"] += 1
    try:
        verify_manifest_data(bad_anchor, payloads)
    except ValueError:
        pass
    else:
        raise AssertionError("Self-test failed: shifted citation byte range was not rejected")

    # Case 2: Hidden cleartext inside redacted artifact must be rejected
    bad_payloads = dict(payloads)
    bad_payloads["foia-release-2024-redacted.txt"] += b"\nHidden: Sector-7 Telemetry\n"
    try:
        verify_manifest_data(manifest, bad_payloads)
    except ValueError:
        pass
    else:
        raise AssertionError("Self-test failed: hidden cleartext in redacted artifact was not rejected")

    # Case 3: Cycle in derived_from DAG must be rejected
    bad_dag = copy.deepcopy(manifest)
    bad_dag["derivations"][0]["derived_from"].append("report-coastal-dossier-v1")
    try:
        verify_manifest_data(bad_dag, payloads)
    except ValueError:
        pass
    else:
        raise AssertionError("Self-test failed: derivation cycle was not rejected")

    # Case 4: Removed endpoint auto-marked retracted must be rejected
    bad_removed = copy.deepcopy(manifest)
    for rec in bad_removed["records"]:
        if rec["version_id"] == "removed-endpoint-v1":
            rec["lifecycle_state"] = "retracted"
    try:
        verify_manifest_data(bad_removed, payloads)
    except ValueError:
        pass
    else:
        raise AssertionError("Self-test failed: auto-retracted removed endpoint was not rejected")

    # Case 5: Counting circular citation cluster as independent root must be rejected
    bad_cycle_count = copy.deepcopy(manifest)
    bad_cycle_count["expectations"]["corroboration_queries"][0]["expected_independent_root_count"] = 3
    try:
        verify_manifest_data(bad_cycle_count, payloads)
    except ValueError:
        pass
    else:
        raise AssertionError("Self-test failed: inflated corroboration root count was not rejected")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "root",
        nargs="?",
        type=Path,
        default=Path(__file__).resolve().parents[2] / "fixtures/documents/lineage",
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="Run negative/adversarial contract checks in memory after validating the corpus.",
    )
    args = parser.parse_args()
    try:
        result = verify(args.root)
        if args.self_test:
            run_self_test(args.root)
    except (OSError, ValueError, KeyError, TypeError, IndexError, UnicodeDecodeError) as error:
        parser.exit(1, f"Document lineage fixture invalid: {error}\n")
    print(f"Document lineage fixture integrity passed: {json.dumps(result, sort_keys=True)}")
    if args.self_test:
        print("Self-test passed: 5 adversarial contract violations rejected.")
    print("Runtime Rust ingestion, persistence, and end-to-end A03/A08 acceptance have NOT been tested.")
