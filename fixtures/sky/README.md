# Sky and solar-system reference fixtures

Independent reference data for [#25](https://github.com/mESmaC/oia/issues/25) (SPACE-04, SPACE-05, GEO-05) and the physical-calculation checks in acceptance scenarios A05 and A04 ([delivery](../../docs/delivery.md#a05-space-observations-and-planetary-context)).

**Origin class: external reference.** These are unmodified responses from the NASA/JPL Horizons API. They are not observations, OIA output, or a selection of OIA's runtime ephemeris provider: decision D06 remains open. Horizons appears here only as an authoritative implementation to compare a future OIA calculation against.

## Contents

- `originals/<case>.json`: response bytes exactly as received from `https://ssd.jpl.nasa.gov/api/horizons.api`. Never edit them.
- `manifest.json`: for each case, the purpose, requirement and scenario IDs, the exact request parameters and encoded query, retrieval time, byte length, SHA-256, and metadata extracted from the bytes (API version, ephemeris sources, header fields, column names, row count, first and last time tags). Extracted metadata is descriptive; it is re-derived and checked by the validator.

Original set retrieval: 2026-11-02 02:22:54Z to 02:23:20Z UTC. The two SSB Mars cases were added from responses retrieved on 2026-11-03 at 03:23:02Z and 03:23:03Z; their exact citations travel in their case entries. Horizons API version 1.2. Ephemeris sources named in the responses: `DE441` (planets, Moon, Sun, SSB), `mar099` (Mars system), `jup365_merged` (Jupiter system). Earth-orientation file: `eop.261030.p270126` (data-based through 2026-10-30, so all post-1962 epochs used here precede its prediction interval).

## Cases

| Case | Isolates |
| --- | --- |
| `geo-moon-eclipse`, `geo-sun-eclipse`, `geo-mars-eclipse` | Geocentric astrometric (ICRF) versus airless apparent RA/Dec; range, range-rate, one-way light time; illumination and S-T-O angle; TDB−UT. 2024-04-08 17:00–19:00 UT, 30 min steps. |
| `topo-dallas-moon-airless`, `topo-dallas-sun-airless` | Topocentric positions inside the 2024-04-08 totality path. The Sun's and Moon's apparent RA/Dec agree to about 0.008° at 18:42 UTC; the geocentric cases do not, because of lunar parallax. |
| `topo-dallas-sun-refracted` | Identical to `topo-dallas-sun-airless` except for Horizons' refraction model. Azimuth matches; elevation differs. |
| `topo-alert-sun-solstice` | Polar observer (82.5° N) over 24 h on 2024-06-20; the Sun stays above the horizon. |
| `topo-dateline-east-sun`, `topo-dateline-west-sun` | Observers 0.1° either side of the antimeridian at identical UTC instants near local sunrise. Their civil dates differ; their sky positions are continuous. |
| `topo-washington-mars-1952` | Pre-1962 epoch: time tags are UT1, not UTC, and TDB−UT is about 29.88 s. Mars sets during the window. |
| `moon-phase-lunation` | Geocentric lunar illumination, S-T-O angle, and phase angle/bisector, daily over 2024-04-01 to 2024-05-01. |
| `earth-subsolar-point` | Sub-solar point on Earth (Earth viewed from the Sun's centre), for illumination and terminator checks. |
| `vec-helio-{earth,mars,jupiter}-geometric` | Heliocentric geometric state vectors (no light-time correction), ecliptic of J2000.0, AU and AU/day, at TDB epochs. |
| `vec-helio-mars-lt` | Same as `vec-helio-mars-geometric` with one-way light-time correction. A heliocentric origin is not inertial; this pair is not used as an inertial light-time oracle. |
| `vec-ssb-mars-geometric`, `vec-ssb-mars-lt` | Mars relative to the solar-system barycenter, geometric versus one-way-light-time-corrected. These retain an inertial-center reference for the light-time solver. |
| `vec-geo-moon-geometric` | Geocentric geometric state vector of the Moon. |
| `error-jwst-before-launch` | An unsupported date range (JWST before its ephemeris coverage). The expected result is an explicit provider error, which must not be presented as an empty or interpolated result. |

## Conventions consumers must handle

Each response explains its own columns and flags after the table (`Column meaning`, `SOLAR PRESENCE`, `LUNAR PRESENCE`). Read those sections rather than inferring meaning from column names. The points below are the ones most likely to cause false agreement or disagreement:

- **Time scales.** Observer-table time tags are UT: UTC for 1962 onward and UT1 before 1962. The `TDB-UT` column gives the offset for each row. Vector-table epochs are TDB (`JDTDB` and `Calendar Date (TDB)`).
- **Astrometric versus apparent.** `R.A.___(ICRF)` and `DEC____(ICRF)` are astrometric: ICRF, corrected for light time only. The `(a-app)` columns are airless apparent positions relative to the true equator and equinox of date (IAU76/80, EOP-corrected). Horizons notes that this equinox is offset −53 mas from the IAU 2006/2000A frame.
- **Refraction.** `(rfct-app)` and `(r-app)` columns use Horizons' Earth refraction model. The model still appears to be applied below the horizon. At 18:00 UTC, `topo-dateline-east-sun` gives a refracted Sun RA of 17.270° at −2.0° elevation. The airless apparent RA at the same instant in `topo-dallas-sun-airless` is 17.893°, and solar parallax between the sites is under 9″, so the difference of about 0.62° comes from the model. Treat refracted values at or below the horizon as model output, not a physical truth target, and do not use them to set tolerances.
- **Observer sites.** `SITE_COORD` is east-positive longitude (degrees), geodetic latitude (degrees), and height (km) on Horizons' Earth model. The header field `Center geodetic` echoes the value as interpreted; for example, Dallas is shown as 263.203° E.
- **Longitudes on Earth.** `earth-subsolar-point` longitudes are ITRF93, east-positive, 0–360°.
- **Printed digits are not accuracy.** `EXTRA_PREC` increases printed precision, not reference accuracy. Comparison tolerances for OIA must be justified in #25 against Horizons' documented ephemeris accuracy and the assumptions being compared (frame, light time, aberration, refraction, and EOP). They are not established here.

## Verify and regenerate

The scripts in `scripts/sky/` are development tools for preparing and checking this fixture. No OIA runtime path calls them, and they are not an ephemeris implementation. They need Python 3.10 or later; they were tested with Python 3.12.10 on Windows 11.

```text
python -I scripts/sky/verify_sky.py
```

This checks offline that every declared original is present, with no extra files, and that byte length, SHA-256, and re-extracted metadata match the manifest. It also checks that each recorded query encodes its parameters and that the manifest's case definitions and provider citation and terms still match `scripts/sky/fetch_horizons.py`. A passing check establishes byte and bookkeeping integrity only. It does not show that Horizons is correct or that OIA agrees with it.

To re-retrieve, write to a new directory and compare. Never overwrite this one:

```text
python -I scripts/sky/fetch_horizons.py <new-directory>
python -I scripts/sky/verify_sky.py <new-directory>
```

The fetcher uses the Python standard library only. It issues one sequential request per case with a 60 s timeout and a 2 MiB response cap, and it does not retry. Each retrieval differs at least in the first line of the `Ephemeris / API_USER …` banner, which contains the generation time. On 2026-11-02, two retrievals of `geo-moon-eclipse` a few minutes apart differed only in that line. Other cases were not compared. Investigate and record any difference inside a table or header (for example, from a new EOP file or ephemeris version) before replacing this reference.

## Citation and terms

Cite these data as the Horizons manual recommends:

> Giorgini, JD and JPL Solar System Dynamics Group, NASA/JPL Horizons On-Line Ephemeris System, <https://ssd.jpl.nasa.gov/horizons/>, data retrieved 2026-Nov-02.

The [SSD About page](https://ssd.jpl.nasa.gov/about/) gives this form for data downloaded from the site:

> Solar System Dynamics. (Downloaded 2026, November 2). (Horizons System). https://ssd.jpl.nasa.gov

Horizons was developed at the Jet Propulsion Laboratory (Solar System Dynamics Group), California Institute of Technology, under contract with NASA.

**Terms checked on 2026-11-02 (UTC):**
- Sources: the [Horizons manual](https://ssd.jpl.nasa.gov/horizons/manual.html) (Acknowledgements and "References for the Horizons system") and the [SSD About page](https://ssd.jpl.nasa.gov/about/).
- What they ask: citation in the form above, including the retrieval date.
- Neither page states a copyright notice, license, or redistribution restriction. This is an absence of stated terms, not an explicit grant.
- The manual calls files that users produce "informal file releases". They must not be used for purposes affecting spacecraft hardware safety or mission success without first contacting the JPL Solar System Dynamics Group. OIA uses them only as test references.

The same citation and terms summary is recorded in `manifest.json` under `provider`, so it travels with the data. Cases retrieved on another UTC date carry their own matching Horizons and SSD citations under `retrieval`; the validator checks those against the actual case timestamp. Formal third-party notices for bundled datasets remain to be settled with D06 or the repository licensing work in #9.

## Limitations

Not covered: satellite or TLE pass geometry and orbital-element age, planetary rotation and orientation models other than Earth's sub-solar point, observer height uncertainty, and dates outside 1952–2024.
