"""Extract descriptive metadata from an unmodified Horizons API JSON response.

Shared by fetch_horizons.py (to write the manifest) and verify_sky.py (to
re-derive and compare it). The extraction is descriptive only: it reports
header fields, table columns, row counts, and boundary time tags exactly as
Horizons printed them. It does not parse numbers or convert time scales;
that belongs to the Rust implementation under test.
"""

from __future__ import annotations

import json
import re

HEADER_FIELD = re.compile(r"^([A-Za-z][A-Za-z0-9 .\-/()]{0,30}?)\s*:\s?(.*)$")
SOURCE_TAG = re.compile(r"\{source:\s*([^}]+?)\s*\}")
BANNER = re.compile(r"^Ephemeris / .*$", re.MULTILINE)


class HorizonsFormatError(ValueError):
    pass


def extract_metadata(payload: bytes) -> dict:
    try:
        document = json.loads(payload.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise HorizonsFormatError(f"response is not UTF-8 JSON: {error}") from error
    if not isinstance(document, dict):
        raise HorizonsFormatError("response JSON is not an object")
    signature = document.get("signature")
    if not isinstance(signature, dict):
        raise HorizonsFormatError("response has no signature object")
    result = document.get("result")
    api_error = document.get("error")
    metadata: dict = {
        "api_version": signature.get("version"),
        "api_source": signature.get("source"),
    }
    if not isinstance(result, str):
        metadata["outcome"] = "provider_error"
        metadata["error_text"] = api_error if isinstance(api_error, str) else None
        return metadata

    lines = result.splitlines()
    try:
        start = lines.index("$$SOE")
        end = lines.index("$$EOE")
    except ValueError:
        metadata["outcome"] = "provider_error"
        metadata["error_text"] = (
            api_error
            if isinstance(api_error, str)
            else "\n".join(line.rstrip() for line in lines if line.strip())
        )
        return metadata
    if end <= start:
        raise HorizonsFormatError("$$EOE precedes $$SOE")

    # The column header is the last non-separator line between the final
    # asterisk rule pair preceding $$SOE.
    columns = None
    for line in reversed(lines[:start]):
        stripped = line.strip()
        if stripped and not set(stripped) <= {"*"}:
            columns = [c.strip() for c in stripped.rstrip(",").split(",")]
            break

    header_fields = []
    banner = None
    for line in lines[:start]:
        if banner is None and BANNER.match(line):
            banner = line.rstrip()
            continue
        match = HEADER_FIELD.match(line.rstrip())
        if match and not set(line.strip()) <= {"*"}:
            header_fields.append([match.group(1).strip(), match.group(2).strip()])

    rows = [line for line in lines[start + 1 : end] if line.strip()]
    first_fields = [row.split(",", 1)[0].strip() for row in rows]
    metadata.update({
        "outcome": "ephemeris",
        "provider_banner": banner,
        "ephemeris_sources": sorted(set(SOURCE_TAG.findall(result))),
        "header_fields": header_fields,
        "columns": columns,
        "row_count": len(rows),
        "first_row_label": first_fields[0] if first_fields else None,
        "last_row_label": first_fields[-1] if first_fields else None,
    })
    return metadata
