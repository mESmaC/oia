"""Validate the checked-in synthetic corpus, not OIA's pending runtime behavior."""

import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import struct
import zlib

MAX_FILE_BYTES = 1024 * 1024


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def read_bounded(path: Path) -> bytes:
    with path.open("rb") as stream:
        payload = stream.read(MAX_FILE_BYTES + 1)
    require(len(payload) <= MAX_FILE_BYTES, f"Oversized fixture: {path.name}")
    return payload


def indexed(items: list, key: str) -> dict:
    require(isinstance(items, list) and len(items) <= 100, "Invalid or excessive fixture entries")
    result = {}
    for item in items:
        identity = item[key]
        require(isinstance(identity, str) and identity and identity not in result, f"Duplicate/invalid {key}")
        result[identity] = item
    return result


def verify_png(raw: bytes) -> None:
    require(raw[:8] == b"\x89PNG\r\n\x1a\n", "Invalid PNG signature")
    offset, chunks = 8, []
    while offset < len(raw):
        require(offset + 12 <= len(raw), "Truncated PNG chunk")
        length = struct.unpack_from(">I", raw, offset)[0]
        end = offset + 12 + length
        require(end <= len(raw), "Truncated PNG data")
        kind, data = raw[offset + 4:offset + 8], raw[offset + 8:end - 4]
        crc = struct.unpack_from(">I", raw, end - 4)[0]
        require(zlib.crc32(kind + data) == crc, "Invalid PNG checksum")
        require(kind in (b"IHDR", b"IDAT", b"IEND"), "Unexpected PNG metadata/text layer")
        chunks.append((kind, data))
        offset = end
    require(chunks and chunks[0][0] == b"IHDR" and chunks[-1] == (b"IEND", b""), "Invalid PNG structure")
    require(len(chunks[0][1]) == 13, "Invalid PNG header")
    require(struct.unpack(">IIBBBBB", chunks[0][1]) == (960, 540, 8, 0, 0, 0, 0), "Unexpected raster format")
    require([kind for kind, _ in chunks[1:-1]] and all(kind == b"IDAT" for kind, _ in chunks[1:-1]), "Invalid PNG chunk order")
    decoder = zlib.decompressobj()
    expected_length = 540 * 961
    pixels = decoder.decompress(b"".join(data for kind, data in chunks if kind == b"IDAT"), expected_length + 1)
    require(len(pixels) == expected_length and decoder.eof and not decoder.unused_data, "Invalid raster data length")


def verify(root: Path) -> dict:
    root = root.resolve(strict=True)
    manifest = json.loads(read_bounded(root / "manifest.json"))
    require(manifest["fixture_schema_version"] == 1 and manifest["classification"] == "simulation", "Unsupported/unlabeled fixture")
    sources = indexed(manifest["sources"], "id")
    artifacts = indexed(manifest["artifacts"], "id")
    records = indexed(manifest["records"], "id")
    indexed(manifest["records"], "version_id")
    payloads, paths = {}, set()
    for key, artifact in artifacts.items():
        relative = PurePosixPath(artifact["path"])
        require(not relative.is_absolute() and ".." not in relative.parts and "\\" not in str(relative)
                and ":" not in str(relative) and relative.parts[0] == "originals", "Invalid fixture path")
        path = root.joinpath(*relative.parts).resolve(strict=True)
        require(path.is_relative_to(root / "originals") and path not in paths, "Escaping or duplicate fixture path")
        paths.add(path)
        raw = read_bounded(path)
        require(len(raw) == artifact["byte_length"], f"Byte length mismatch: {key}")
        require(hashlib.sha256(raw).hexdigest() == artifact["sha256"], f"Hash mismatch: {key}")
        require(artifact["origin_class"] == "simulation", "Lost simulation label")
        if artifact["media_type"] == "image/png":
            verify_png(raw)
        else:
            require(artifact["media_type"] == "text/plain; charset=utf-8", "Unsupported fixture media type")
            require(raw.decode("utf-8").startswith("SYNTHETIC A03 FIXTURE"), "Missing visible fixture label")
        payloads[key] = raw
    require(paths == {p.resolve() for p in (root / "originals").rglob("*") if p.is_file()}, "Unmanifested original")
    for record in records.values():
        require(record["source_id"] in sources and record["origin_class"] == "simulation", "Invalid record origin")
        require(record["review_status"] == "unreviewed", "Fixture asserts review")
        require(record["event_time"] is None and record["publication_time"] is None and record["unknown_time_reason"], "Invented source time")
        require(record["predecessor_record_id"] is None or record["predecessor_record_id"] in records, "Missing predecessor")
        page_numbers = set()
        for page in record["pages"]:
            require(type(page["number"]) is int and page["number"] > 0 and page["number"] not in page_numbers, "Invalid page number")
            page_numbers.add(page["number"])
            if page["expected_extraction"] == "missing":
                require(page["artifact_id"] is None and page["omission_reason"], "Missing page has invented bytes")
            else:
                require(page["artifact_id"] in artifacts and page["expected_extraction"] in ("text_available", "ocr_required", "unreadable"), "Invalid page artifact/status")
        require(record["retention"] in ("retained", "reference_only"), "Invalid retention state")
        if record["retention"] == "reference_only":
            require(not record["pages"] and record["omission_reason"], "Reference-only source retained bytes")
        else:
            require(record["pages"] and record["omission_reason"] is None, "Invalid retained record")

    expectations = manifest["expectations"]
    for citation in expectations["citations"]:
        record = records[citation["record_id"]]
        require(citation["record_version_id"] == record["version_id"], "Citation version mismatch")
        require(any(p["artifact_id"] == citation["artifact_id"] and p["number"] == citation["page"] for p in record["pages"]), "Citation outside source record")
        require(citation["anchor_kind"] == "utf8_byte_range", "Unsupported anchor")
        raw = payloads[citation["artifact_id"]]
        start, end = citation["start"], citation["end"]
        require(type(start) is int and type(end) is int and 0 <= start < end <= len(raw), "Invalid anchor range")
        require(raw[start:end].decode("utf-8") == citation["quote"], "Citation quote mismatch")

    relations = {(r["from"], r["to"], r["kind"]) for r in manifest["relations"]}
    require(all(a in records and b in records for a, b, _ in relations), "Dangling relation")
    require(relations == {
        ("report-republication", "report-original", "republishes"),
        ("report-correction", "report-original", "supersedes"),
        ("release-b", "release-a", "alternate_redaction"),
        ("conflicting-account", "report-original", "contradicts"),
    }, "Required source relationships changed")
    original, mirror = records["report-original"], records["report-republication"]
    require(original["source_id"] != mirror["source_id"] and original["pages"] == mirror["pages"], "Duplicate bytes lost separate source identities")
    require(payloads["report-v1.txt"] != payloads["report-v2.txt"] and payloads["redacted-a.txt"] != payloads["redacted-b.txt"], "Revisions/redactions collapsed")
    scan = expectations["scan"]
    require(scan["coverage"] == "partial" and scan["requested_pages"] == [1, 2, 3]
            and scan["readable_pages"] == [1] and scan["unreadable_pages"] == [2]
            and scan["missing_pages"] == [3], "Lost partial coverage")
    require([p["expected_extraction"] for p in records[scan["record_id"]]["pages"]] == ["ocr_required", "unreadable", "missing"], "Scan coverage mismatch")
    require(expectations["correction"] == {
        "input_record_id": "report-original", "replacement_record_id": "report-correction",
        "dependent_result_state": "needs_reassessment", "preserve_old_citations": True,
    }, "Correction expectation changed")
    require(expectations["untrusted_source_allowed_actions"] == [], "Source text grants actions")
    return {"artifacts": len(artifacts), "source_records": len(records), "exact_citations": len(expectations["citations"])}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", nargs="?", type=Path,
                        default=Path(__file__).resolve().parents[2] / "fixtures/documents/a03")
    args = parser.parse_args()
    try:
        result = verify(args.root)
    except (OSError, ValueError, KeyError, TypeError, IndexError, struct.error, zlib.error) as error:
        parser.exit(1, f"A03 fixture invalid: {error}\n")
    print(f"A03 fixture integrity passed: {json.dumps(result, sort_keys=True)}")
    print("Runtime ingestion, OCR, citation invalidation, and A03 acceptance have NOT been tested.")
