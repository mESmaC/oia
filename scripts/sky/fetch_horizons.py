"""Retrieve JPL Horizons reference responses into a NEW directory.

Role: development tool for issue #25 (SPACE-04, SPACE-05, GEO-05; scenarios
A05 and A04). It is not production code and is not OIA's runtime ephemeris
provider; decision D06 remains open. Horizons serves here as an independent
reference that a future Rust implementation is compared against.

Original response bytes are written unchanged. The manifest records the exact
request, retrieval time, digest, and metadata extracted from those bytes. The
script never rewrites an existing fixture directory; regenerate into a new
directory and compare with verify_sky.py instead.

Standard library only (Python 3.10+). Network use is bounded: sequential
requests, fixed timeout, capped response size, no retries.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from horizons_meta import extract_metadata  # noqa: E402

ENDPOINT = "https://ssd.jpl.nasa.gov/api/horizons.api"
TIMEOUT_SECONDS = 60
MAX_RESPONSE_BYTES = 2 * 1024 * 1024
DELAY_BETWEEN_REQUESTS_SECONDS = 1.0
USER_AGENT = "oia-sky-reference-fixtures/1 (+https://github.com/mESmaC/oia/issues/25)"

# Observer sites. Longitude east-positive degrees, geodetic latitude degrees,
# height above the WGS84 ellipsoid in km (Horizons SITE_COORD convention).
DALLAS = "-96.7970,32.7767,0.139"  # inside 2024-04-08 totality path
ALERT_NU = "-62.3481,82.5018,0.030"  # polar: midnight sun near solstice
DATELINE_EAST = "179.9000,-16.8000,0.000"  # just east of the antimeridian
DATELINE_WEST = "-179.9000,-16.8000,0.000"  # just west, same UTC instant
WASHINGTON_DCA = "-77.0402,38.8512,0.005"  # pre-1962 epoch (UT1 time tags)

OBSERVER_COMMON = {
    "EPHEM_TYPE": "OBSERVER",
    "ANG_FORMAT": "DEG",
    "EXTRA_PREC": "YES",
    "CSV_FORMAT": "YES",
    "OBJ_DATA": "NO",
    "TIME_DIGITS": "SECONDS",
    "TIME_TYPE": "UT",
}

VECTOR_COMMON = {
    "EPHEM_TYPE": "VECTORS",
    "REF_SYSTEM": "ICRF",
    "REF_PLANE": "ECLIPTIC",
    "VEC_TABLE": "3",
    "OUT_UNITS": "AU-D",
    "VEC_LABELS": "YES",
    "CSV_FORMAT": "YES",
    "OBJ_DATA": "NO",
    "TIME_DIGITS": "SECONDS",
}

ECLIPSE_WINDOW = {
    "START_TIME": "2024-04-08 17:00",
    "STOP_TIME": "2024-04-08 19:00",
    "STEP_SIZE": "30m",
}
TOTALITY_WINDOW = {
    "START_TIME": "2024-04-08 18:00",
    "STOP_TIME": "2024-04-08 19:00",
    "STEP_SIZE": "6m",
}
VECTOR_EPOCH = {
    "START_TIME": "2024-04-08 18:00 TDB",
    "STOP_TIME": "2024-04-08 19:00",  # scale set by START only
    "STEP_SIZE": "1h",
}


def topo(site: str) -> dict[str, str]:
    return {"CENTER": "coord@399", "COORD_TYPE": "GEODETIC", "SITE_COORD": site}


CASES: list[dict] = [
    {
        "id": "geo-moon-eclipse",
        "purpose": "Geocentric Moon: astrometric (ICRF) versus airless apparent RA/Dec, "
        "illumination, angular diameter, range/range-rate, light time, S-T-O angle, TDB-UT.",
        "requirements": ["SPACE-04", "SPACE-05"],
        "scenarios": ["A05"],
        "expected_outcome": "ephemeris",
        "params": {**OBSERVER_COMMON, **ECLIPSE_WINDOW, "COMMAND": "301",
                   "CENTER": "500@399", "QUANTITIES": "1,2,10,13,20,21,24,30",
                   "APPARENT": "AIRLESS"},
    },
    {
        "id": "geo-sun-eclipse",
        "purpose": "Geocentric Sun: astrometric versus airless apparent RA/Dec, angular "
        "diameter, range, light time, TDB-UT.",
        "requirements": ["SPACE-04", "SPACE-05"],
        "scenarios": ["A05"],
        "expected_outcome": "ephemeris",
        "params": {**OBSERVER_COMMON, **ECLIPSE_WINDOW, "COMMAND": "10",
                   "CENTER": "500@399", "QUANTITIES": "1,2,13,20,21,30",
                   "APPARENT": "AIRLESS"},
    },
    {
        "id": "geo-mars-eclipse",
        "purpose": "Geocentric Mars: astrometric versus apparent RA/Dec with multi-minute "
        "light time, illumination, S-T-O angle.",
        "requirements": ["SPACE-04", "SPACE-05"],
        "scenarios": ["A05"],
        "expected_outcome": "ephemeris",
        "params": {**OBSERVER_COMMON, **ECLIPSE_WINDOW, "COMMAND": "499",
                   "CENTER": "500@399", "QUANTITIES": "1,2,10,13,20,21,24,30",
                   "APPARENT": "AIRLESS"},
    },
    {
        "id": "topo-dallas-moon-airless",
        "purpose": "Topocentric Moon from Dallas through totality: lunar parallax versus "
        "the geocentric case; airless Az/El.",
        "requirements": ["SPACE-05", "GEO-05"],
        "scenarios": ["A05", "A04"],
        "expected_outcome": "ephemeris",
        "params": {**OBSERVER_COMMON, **TOTALITY_WINDOW, **topo(DALLAS), "COMMAND": "301",
                   "QUANTITIES": "1,2,4,10,13,20,30", "APPARENT": "AIRLESS"},
    },
    {
        "id": "topo-dallas-sun-airless",
        "purpose": "Topocentric Sun from Dallas through totality, airless Az/El.",
        "requirements": ["SPACE-05", "GEO-05"],
        "scenarios": ["A05", "A04"],
        "expected_outcome": "ephemeris",
        "params": {**OBSERVER_COMMON, **TOTALITY_WINDOW, **topo(DALLAS), "COMMAND": "10",
                   "QUANTITIES": "1,2,4,13,20,30", "APPARENT": "AIRLESS"},
    },
    {
        "id": "topo-dallas-sun-refracted",
        "purpose": "Same request as topo-dallas-sun-airless with Horizons refraction "
        "model enabled, isolating the refraction assumption.",
        "requirements": ["SPACE-05", "GEO-05"],
        "scenarios": ["A05", "A04"],
        "expected_outcome": "ephemeris",
        "params": {**OBSERVER_COMMON, **TOTALITY_WINDOW, **topo(DALLAS), "COMMAND": "10",
                   "QUANTITIES": "1,2,4,13,20,30", "APPARENT": "REFRACTED"},
    },
    {
        "id": "topo-alert-sun-solstice",
        "purpose": "Polar observer (Alert, Nunavut) over 24 h near the June solstice: "
        "the Sun stays above the horizon; refracted Az/El.",
        "requirements": ["SPACE-05", "GEO-05"],
        "scenarios": ["A04"],
        "expected_outcome": "ephemeris",
        "params": {**OBSERVER_COMMON, **topo(ALERT_NU), "COMMAND": "10",
                   "START_TIME": "2024-06-20 00:00", "STOP_TIME": "2024-06-21 00:00",
                   "STEP_SIZE": "3h", "QUANTITIES": "4,30", "APPARENT": "REFRACTED"},
    },
    {
        "id": "topo-dateline-east-sun",
        "purpose": "Observer 0.1 deg east of the antimeridian near local sunrise; pair with "
        "topo-dateline-west-sun at the same UTC instants (civil dates differ, sky does not).",
        "requirements": ["SPACE-05", "GEO-05"],
        "scenarios": ["A04"],
        "expected_outcome": "ephemeris",
        "params": {**OBSERVER_COMMON, **ECLIPSE_WINDOW, **topo(DATELINE_EAST), "COMMAND": "10",
                   "QUANTITIES": "2,4,30", "APPARENT": "REFRACTED"},
    },
    {
        "id": "topo-dateline-west-sun",
        "purpose": "Observer 0.1 deg west of the antimeridian; counterpart of "
        "topo-dateline-east-sun.",
        "requirements": ["SPACE-05", "GEO-05"],
        "scenarios": ["A04"],
        "expected_outcome": "ephemeris",
        "params": {**OBSERVER_COMMON, **ECLIPSE_WINDOW, **topo(DATELINE_WEST), "COMMAND": "10",
                   "QUANTITIES": "2,4,30", "APPARENT": "REFRACTED"},
    },
    {
        "id": "topo-washington-mars-1952",
        "purpose": "Historical pre-1962 epoch where Horizons time tags are UT1, not UTC; "
        "refracted Az/El of Mars from Washington National Airport as it sets (~04:50 UT1).",
        "requirements": ["SPACE-05", "GEO-05"],
        "scenarios": ["A04"],
        "expected_outcome": "ephemeris",
        "params": {**OBSERVER_COMMON, **topo(WASHINGTON_DCA), "COMMAND": "499",
                   "START_TIME": "1952-07-20 02:00", "STOP_TIME": "1952-07-20 05:00",
                   "STEP_SIZE": "30m", "QUANTITIES": "1,2,4,10,30", "APPARENT": "REFRACTED"},
    },
    {
        "id": "moon-phase-lunation",
        "purpose": "Geocentric lunar illumination and phase angle across one lunation "
        "(daily, 2024-04-01 to 2024-05-01 UT).",
        "requirements": ["SPACE-04"],
        "scenarios": ["A05"],
        "expected_outcome": "ephemeris",
        "params": {**OBSERVER_COMMON, "COMMAND": "301", "CENTER": "500@399",
                   "START_TIME": "2024-04-01 00:00", "STOP_TIME": "2024-05-01 00:00",
                   "STEP_SIZE": "1d", "QUANTITIES": "10,24,43", "APPARENT": "AIRLESS"},
    },
    {
        "id": "earth-subsolar-point",
        "purpose": "Sub-solar point on Earth (Earth seen from the Sun's centre) for "
        "illumination/terminator checks across the eclipse window.",
        "requirements": ["SPACE-04"],
        "scenarios": ["A05"],
        "expected_outcome": "ephemeris",
        "params": {**OBSERVER_COMMON, **ECLIPSE_WINDOW, "COMMAND": "399",
                   "CENTER": "500@10", "QUANTITIES": "14,30", "APPARENT": "AIRLESS"},
    },
    *[
        {
            "id": f"vec-helio-{name}-geometric",
            "purpose": f"Heliocentric geometric (no light-time correction) ICRF ecliptic "
            f"state vector of {label}; TDB epochs.",
            "requirements": ["SPACE-04", "SPACE-05"],
            "scenarios": ["A05"],
            "expected_outcome": "ephemeris",
            "params": {**VECTOR_COMMON, **VECTOR_EPOCH, "COMMAND": command,
                       "CENTER": "500@10", "VEC_CORR": "NONE"},
        }
        for name, label, command in [
            ("earth", "Earth", "399"),
            ("mars", "Mars", "499"),
            ("jupiter", "Jupiter", "599"),
        ]
    ],
    {
        "id": "vec-helio-mars-lt",
        "purpose": "Same as vec-helio-mars-geometric with one-way light-time correction, "
        "separating geometric from light-time-corrected position.",
        "requirements": ["SPACE-05"],
        "scenarios": ["A05"],
        "expected_outcome": "ephemeris",
        "params": {**VECTOR_COMMON, **VECTOR_EPOCH, "COMMAND": "499",
                   "CENTER": "500@10", "VEC_CORR": "LT"},
    },
    {
        "id": "vec-geo-moon-geometric",
        "purpose": "Geocentric geometric ICRF ecliptic state vector of the Moon (lunar "
        "orbit); TDB epochs.",
        "requirements": ["SPACE-04"],
        "scenarios": ["A05"],
        "expected_outcome": "ephemeris",
        "params": {**VECTOR_COMMON, **VECTOR_EPOCH, "COMMAND": "301",
                   "CENTER": "500@399", "VEC_CORR": "NONE"},
    },
    {
        "id": "error-jwst-before-launch",
        "purpose": "Unsupported date: JWST (-170) requested before its ephemeris coverage. "
        "Retained so an unavailable range stays an explicit error, not an empty or "
        "interpolated result.",
        "requirements": ["SPACE-05"],
        "scenarios": ["A05"],
        "expected_outcome": "provider_error",
        "params": {**OBSERVER_COMMON, "COMMAND": "-170", "CENTER": "500@399",
                   "START_TIME": "2020-01-01 00:00", "STOP_TIME": "2020-01-02 00:00",
                   "STEP_SIZE": "1d", "QUANTITIES": "1,20", "APPARENT": "AIRLESS"},
    },
]


def quote(value: str) -> str:
    return f"'{value}'"


def build_query(params: dict[str, str]) -> str:
    ordered = [("format", "json"), ("MAKE_EPHEM", "YES")]
    ordered += [(key, quote(value)) for key, value in params.items()]
    return urllib.parse.urlencode(ordered, quote_via=urllib.parse.quote)


def fetch(query: str) -> tuple[bytes, str]:
    request = urllib.request.Request(f"{ENDPOINT}?{query}", headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request, timeout=TIMEOUT_SECONDS) as response:
        if response.status != 200:
            raise RuntimeError(f"HTTP {response.status}")
        payload = response.read(MAX_RESPONSE_BYTES + 1)
        if len(payload) > MAX_RESPONSE_BYTES:
            raise RuntimeError(f"response exceeds {MAX_RESPONSE_BYTES} bytes")
        return payload, response.headers.get("Date", "")


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")


def run(destination: Path, only: set[str] | None) -> int:
    ids = [case["id"] for case in CASES]
    if len(ids) != len(set(ids)):
        raise ValueError("duplicate case id")
    if only:
        unknown = only - set(ids)
        if unknown:
            raise ValueError(f"unknown case ids: {sorted(unknown)}")
    destination.mkdir(parents=True, exist_ok=False)
    (destination / "originals").mkdir()
    entries = []
    failures = 0
    for index, case in enumerate(c for c in CASES if not only or c["id"] in only):
        if index:
            time.sleep(DELAY_BETWEEN_REQUESTS_SECONDS)
        query = build_query(case["params"])
        requested_at = utc_now()
        try:
            payload, http_date = fetch(query)
        except (urllib.error.URLError, RuntimeError, TimeoutError) as error:
            print(f"FAIL {case['id']}: {error}", file=sys.stderr)
            failures += 1
            continue
        relative = f"originals/{case['id']}.json"
        (destination / relative).write_bytes(payload)
        metadata = extract_metadata(payload)
        if metadata["outcome"] != case["expected_outcome"]:
            print(f"UNEXPECTED {case['id']}: outcome {metadata['outcome']}", file=sys.stderr)
            failures += 1
        entries.append({
            "id": case["id"],
            "purpose": case["purpose"],
            "requirements": case["requirements"],
            "scenarios": case["scenarios"],
            "expected_outcome": case["expected_outcome"],
            "request": {
                "method": "GET",
                "endpoint": ENDPOINT,
                "parameters": {k: v for k, v in case["params"].items()},
                "query": query,
            },
            "retrieval": {"requested_at_utc": requested_at, "http_date": http_date},
            "artifact": {
                "path": relative,
                "media_type": "application/json",
                "byte_length": len(payload),
                "sha256": hashlib.sha256(payload).hexdigest(),
            },
            "extracted": metadata,
        })
        print(f"ok   {case['id']} ({len(payload)} bytes, {metadata['outcome']})")
    manifest = {
        "schema": "oia.fixture.sky-reference/1",
        "origin_class": "external_reference",
        "description": "Unmodified JPL Horizons API responses used as an independent "
        "reference for sky and solar-system calculations. Not observations; not OIA output.",
        "provider": {
            "name": "NASA/JPL Solar System Dynamics Group, Horizons On-Line Ephemeris System",
            "documentation": "https://ssd-api.jpl.nasa.gov/doc/horizons.html",
            "manual": "https://ssd.jpl.nasa.gov/horizons/manual.html",
        },
        "generator": "scripts/sky/fetch_horizons.py",
        "cases": entries,
    }
    (destination / "manifest.json").write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n"
    )
    return 1 if failures else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("destination", type=Path, help="new directory to create")
    parser.add_argument("--only", nargs="+", help="fetch only these case ids")
    args = parser.parse_args()
    return run(args.destination, set(args.only) if args.only else None)


if __name__ == "__main__":
    sys.exit(main())
