"""Verify the integrity of a retained sky reference fixture directory.

Offline and read-only. Checks that:
- the manifest has the expected schema and unique, well-formed case entries;
- the directory holds exactly the declared originals (no missing/undeclared files);
- each original's byte length and SHA-256 match the manifest;
- metadata re-extracted from the original bytes equals the recorded metadata;
- each case's outcome matches its declared expectation (ephemeris vs provider error);
- each recorded query is exactly the encoding of its recorded parameters;
- the manifest matches the case definitions in fetch_horizons.py, so editing a
  case without re-retrieving its reference is reported as drift.

Integrity here means byte and bookkeeping consistency. It does not establish
that Horizons is correct, nor that any OIA calculation agrees with it.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from fetch_horizons import CASES, ENDPOINT, MAX_RESPONSE_BYTES, build_query  # noqa: E402
from horizons_meta import HorizonsFormatError, extract_metadata  # noqa: E402

SCHEMA = "oia.fixture.sky-reference/1"
ALLOWED_TOP_LEVEL = {".gitattributes", "manifest.json", "README.md", "originals"}
MAX_MANIFEST_BYTES = 1024 * 1024
CASE_ID = re.compile(r"^[a-z0-9][a-z0-9-]{0,63}$")
SHA256 = re.compile(r"^[0-9a-f]{64}$")
UTC_STAMP = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$")
OUTCOMES = {"ephemeris", "provider_error"}


def verify(root: Path) -> list[str]:
    errors: list[str] = []
    manifest_path = root / "manifest.json"
    if not manifest_path.is_file():
        return [f"{manifest_path}: missing"]
    if manifest_path.stat().st_size > MAX_MANIFEST_BYTES:
        return [f"{manifest_path}: exceeds {MAX_MANIFEST_BYTES} bytes"]
    try:
        manifest = json.loads(manifest_path.read_bytes().decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        return [f"{manifest_path}: invalid JSON: {error}"]
    if not isinstance(manifest, dict):
        return ["manifest: not an object"]
    if manifest.get("schema") != SCHEMA:
        errors.append(f"manifest: schema is {manifest.get('schema')!r}, expected {SCHEMA!r}")
    if manifest.get("origin_class") != "external_reference":
        errors.append("manifest: origin_class must be 'external_reference'")
    cases = manifest.get("cases")
    if not isinstance(cases, list) or not cases:
        return errors + ["manifest: cases must be a non-empty list"]

    for entry in sorted(root.iterdir()):
        if entry.name not in ALLOWED_TOP_LEVEL:
            errors.append(f"undeclared top-level entry: {entry.name}")

    declared: set[str] = set()
    seen_ids: set[str] = set()
    definitions = {case["id"]: case for case in CASES}
    for index, case in enumerate(cases):
        label = f"cases[{index}]"
        if not isinstance(case, dict):
            errors.append(f"{label}: not an object")
            continue
        case_id = case.get("id")
        if not isinstance(case_id, str) or not CASE_ID.match(case_id):
            errors.append(f"{label}: invalid id {case_id!r}")
            continue
        label = case_id
        if case_id in seen_ids:
            errors.append(f"{label}: duplicate id")
            continue
        seen_ids.add(case_id)
        errors.extend(verify_case(root, case, label, definitions.get(case_id), declared))

    missing_defs = set(definitions) - seen_ids
    if missing_defs:
        errors.append(f"cases defined in fetch_horizons.py but not retained: {sorted(missing_defs)}")

    originals = root / "originals"
    if originals.is_dir():
        for path in sorted(originals.rglob("*")):
            relative = path.relative_to(root).as_posix()
            if path.is_file() and relative not in declared:
                errors.append(f"undeclared file: {relative}")
            elif not path.is_file() and not path.is_dir():
                errors.append(f"unexpected entry: {relative}")
    else:
        errors.append("originals/: missing directory")
    return errors


def verify_case(root: Path, case: dict, label: str, definition: dict | None,
                declared: set[str]) -> list[str]:
    errors: list[str] = []
    expected_outcome = case.get("expected_outcome")
    if expected_outcome not in OUTCOMES:
        errors.append(f"{label}: expected_outcome {expected_outcome!r} not in {sorted(OUTCOMES)}")
    for key in ("requirements", "scenarios"):
        value = case.get(key)
        if not isinstance(value, list) or not value or not all(isinstance(v, str) for v in value):
            errors.append(f"{label}: {key} must be a non-empty list of strings")

    request = case.get("request")
    if not isinstance(request, dict) or not isinstance(request.get("parameters"), dict):
        errors.append(f"{label}: request.parameters missing")
    else:
        if request.get("endpoint") != ENDPOINT or request.get("method") != "GET":
            errors.append(f"{label}: request endpoint/method differs from {ENDPOINT}")
        if request.get("query") != build_query(request["parameters"]):
            errors.append(f"{label}: request.query is not the encoding of request.parameters")
        if definition is None:
            errors.append(f"{label}: not defined in fetch_horizons.py")
        else:
            for key in ("purpose", "requirements", "scenarios", "expected_outcome"):
                if case.get(key) != definition[key]:
                    errors.append(f"{label}: {key} differs from fetch_horizons.py definition")
            if request["parameters"] != definition["params"]:
                errors.append(f"{label}: request parameters differ from fetch_horizons.py "
                              "definition; re-retrieve into a new directory")

    retrieval = case.get("retrieval")
    if not isinstance(retrieval, dict) or not UTC_STAMP.match(
        str(retrieval.get("requested_at_utc", ""))
    ):
        errors.append(f"{label}: retrieval.requested_at_utc must be YYYY-MM-DDTHH:MM:SSZ")

    artifact = case.get("artifact")
    if not isinstance(artifact, dict):
        return errors + [f"{label}: artifact missing"]
    relative = artifact.get("path")
    expected_path = f"originals/{label}.json"
    # The id regex plus this exact form keeps the path inside originals/.
    if relative != expected_path:
        return errors + [f"{label}: artifact.path must be {expected_path!r}"]
    declared.add(relative)
    path = root / relative
    if not path.is_file():
        return errors + [f"{label}: {relative} missing"]
    size = path.stat().st_size
    if size > MAX_RESPONSE_BYTES:
        return errors + [f"{label}: {relative} exceeds {MAX_RESPONSE_BYTES} bytes"]
    payload = path.read_bytes()
    if artifact.get("byte_length") != len(payload):
        errors.append(f"{label}: byte_length {artifact.get('byte_length')} != {len(payload)}")
    if not SHA256.match(str(artifact.get("sha256", ""))):
        errors.append(f"{label}: sha256 is not 64 lowercase hex digits")
    elif artifact["sha256"] != hashlib.sha256(payload).hexdigest():
        errors.append(f"{label}: sha256 mismatch for {relative}")

    try:
        extracted = extract_metadata(payload)
    except HorizonsFormatError as error:
        return errors + [f"{label}: {error}"]
    if extracted != case.get("extracted"):
        errors.append(f"{label}: recorded 'extracted' metadata differs from the original bytes")
    if extracted["outcome"] != expected_outcome:
        errors.append(f"{label}: outcome {extracted['outcome']!r}, expected {expected_outcome!r}")
    if extracted.get("api_source") != "NASA/JPL Horizons API":
        errors.append(f"{label}: unexpected API source {extracted.get('api_source')!r}")
    if extracted["outcome"] == "ephemeris":
        if not extracted.get("row_count"):
            errors.append(f"{label}: ephemeris table is empty")
        if not extracted.get("columns"):
            errors.append(f"{label}: ephemeris table has no column header")
        if not extracted.get("ephemeris_sources"):
            errors.append(f"{label}: no ephemeris source recorded in response header")
    elif not extracted.get("error_text"):
        errors.append(f"{label}: provider error has no error text")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("root", type=Path, nargs="?",
                        default=Path(__file__).resolve().parents[2] / "fixtures" / "sky")
    args = parser.parse_args()
    errors = verify(args.root)
    for error in errors:
        print(f"FAIL {error}", file=sys.stderr)
    if errors:
        print(f"{len(errors)} problem(s) in {args.root}", file=sys.stderr)
        return 1
    print(f"ok: {args.root} is consistent with its manifest and fetch_horizons.py")
    return 0


if __name__ == "__main__":
    sys.exit(main())
