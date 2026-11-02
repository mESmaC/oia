# Observer sky geometry kernel

Status: implemented in the `oia-sky` workspace package, which has no dependencies. It is not
integrated with application services, the investigation clock, or any
ephemeris. This record covers the provider-independent observer-geometry slice
of [#25](https://github.com/mESmaC/oia/issues/25). It supports
[GEO-05](../spec.md#42-geographic-events-and-uap-investigations) (observer-specific
celestial positions) and [SPACE-05](../spec.md#43-space-imagery-and-solar-system)
(geometric versus apparent positions, observer location, time standard,
orientation and refraction assumptions). It checks the horizontal-coordinate
subset of the physical-calculation checks in
[A04](../delivery.md#a04-geographic-reconstruction) and
[A05](../delivery.md#a05-space-observations-and-planetary-context) against the
merged [Horizons reference fixtures](../../fixtures/sky/README.md).

## Boundary

The kernel takes a **geocentric apparent place**, referred to the true equator
and equinox of date, from whatever ephemeris the caller uses. It returns where
that target appears to one observer.

| Step | Model | Inputs the caller must supply |
| --- | --- | --- |
| Earth rotation | GMST (IAU 1982) plus the equation of the equinoxes Δψ·cos ε₀. Δψ comes from the four largest IAU 1980 nutation terms, and ε₀ is the IAU 1980 mean obliquity. | UT1 and TT Julian dates |
| Observer state | WGS84 ellipsoid, rotated by GAST + longitude. Velocity is ω × r. Polar motion is neglected. | Geodetic latitude, east longitude, ellipsoidal height |
| Topocentric place | Parallax by vector subtraction (finite distances only), then first-order diurnal aberration. Targets nearer than 100,000 km are refused; for them, pass a provider-computed topocentric place to `TopocentricPlace::new`. | Geocentric distance, or the explicit `ParallaxNegligible` |
| Horizontal | Hour angle = GAST + λ − α. Elevation is measured from the ellipsoid normal (geodetic vertical); azimuth runs from north through east. | — |
| Refraction (optional) | Saemundsson: R = 1.02′ cot(h + 10.3/(h + 5.11)), scaled by (P/1010)(283/(273 + T)) | Pressure and temperature, or the named `Atmosphere::conventional_reference()` (1010 hPa, 10 °C) |

`MODEL_ID` names this combination. Record it with any result so the result can
be reassessed if a model changes.

**Not in this kernel:**
- Choosing, evaluating, or parsing an ephemeris (decision D06 stays open).
- Converting UTC to UT1 or TT, and Earth-orientation data. These belong to #12.
- Light time (including the topocentric light-time difference), planetary or
  satellite positions, and TLEs.
- The globe or sky renderer (#23), and a Lean proof.

Horizons responses are parsed only in test code, as a reference reader.

## Contract

- **Validated inputs.** Every input is checked before computation; a failure
  returns `SkyError::NonFinite` or `SkyError::OutOfRange`:
  - UT1: 1900-01-01 to 2100-01-01.
  - TT − UT1: −10 s to 400 s. This is only a gross-error check:
    - It rejects swapped UT1 and TT arguments when ΔT exceeds 10 s, that is
      after about 1910.
    - It cannot detect UTC or UT1 passed as TT, because UTC − UT1 is within
      ±0.9 s. That mistake moves the nutation arguments by about 69 s, which
      changes the result by ≲ 0.0001″.
  - Latitude: [−90, 90]. Longitude: [−180, 360], normalised to [−180, 180).
  - Height: −12 km to 100 km.
  - RA: [0, 360). Dec: [−90, 90].
  - Finite geocentric distance: 100,000 km to 10¹⁵ km (about 106 light-years).
    The lower bound keeps the light-time term in the budget below 0.1″.
    Beyond the upper bound, state `ParallaxNegligible`.
  - A provider-supplied `TopocentricPlace`: RA and Dec as above, and a positive
    distance up to 10¹⁵ km, if known.
  - `EarthRotation` and `TopocentricPlace` have private fields, so a NaN
    can't reach `horizontal` through them.
  - Pressure: 0–1,100 hPa. Temperature: −90 to 60 °C.

  A target that coincides with the observer is `DegenerateGeometry`.
- **No invented distance.** A target without a known distance needs the
  explicit `TargetDistance::ParallaxNegligible`. That is a stated assumption,
  not a default, and the result then reports no topocentric distance.
- **Refraction is withheld outside the model domain.** Below an airless
  elevation of −1°, `refract` returns `Refraction::BelowModelDomain` and no
  refracted value; the airless position stays valid. Horizons prints a
  refracted value there, and the fixtures show that it is a constant 38.79′,
  the formula's value at −1°. The kernel deliberately does not reproduce that
  clamp.
- **Stated conditions.** Refraction uses the conditions the caller states.
  The conventional reference atmosphere is an assumption, not a measurement of
  local conditions. Near the horizon, real refraction can differ from any
  standard model by several arcminutes.
- **Azimuth at the zenith and nadir** is reported as `atan2` computes it and
  has no meaning there.
- **At latitude ±90°** "north" is undefined. The azimuth is measured from the
  meridian of the longitude supplied, so it changes with that longitude.
- **No I/O, allocation, or side effects.** Every function is a deterministic
  computation in `f64`. Results are not claimed to be bit-identical across
  platforms.

## Accuracy budget

These are derived bounds for the kernel's own approximations, in addition to
errors in the inputs.

**Sources:**
- The GMST polynomial, the four-term Δψ and its ≤ 0.5″ bound, and the IAU 1980
  mean obliquity: J. Meeus, *Astronomical Algorithms*, 2nd ed. (1998), eqs.
  12.4 and 22.2 and ch. 22.
- The refraction formula: Saemundsson, *Sky & Telescope* 72, 70 (1986), as
  given in Meeus ch. 16.

| Source | Effect on the computed direction |
| --- | --- |
| Δψ truncated to four terms (Meeus) | ≤ 0.5″ in Δψ, so ≤ 0.46″ in GAST and the hour angle |
| IAU 1994 complementary terms of the equation of the equinoxes, omitted | ≤ 0.0027″ |
| ε₀ used instead of the true obliquity | ≤ 0.0004″ |
| GMST IAU 1982 versus IAU 2006 | Small compared with the truncation above near J2000; not separately measured |
| Polar motion neglected | typically ≤ 0.5″ in the observer frame |
| First-order diurnal aberration (≤ 0.32″) | second-order terms ≪ 0.001″ |
| Topocentric light-time difference, not corrected | The geocentric apparent place carries the geocentric light time. The light time from the Earth's own motion cancels against aberration, so what remains is about u⊥·(d − ρ)/(c·ρ). Here u⊥ is the target's transverse velocity relative to the Earth, d − ρ ≤ 6,378 km, and ρ is the topocentric distance. Values: Moon about 0.013″ (measured 0.016″); a bound orbit at the 100,000 km minimum ≲ 0.1″. GEO (0.38″) and LEO (about 55″ at 600 km) are below the minimum and refused. |
| **Caller-supplied UT1 error** | 1.5″ per 0.1 s |
| **Geodetic versus astronomical vertical** (deflection of the vertical) | not modelled. Typically several arcseconds, up to about 1′ in mountains. |
| **Refraction model versus the real atmosphere** | not bounded by the kernel. Arcminutes near the horizon. |

So, given exact inputs, a correct UT1, a source frame consistent with
IAU 1976/1980, and a target within the accepted distances, the kernel's own
geometric error is about 1″.

For satellites and other near-Earth targets, the provider must supply a
topocentric apparent place. The kernel then adds no parallax or light-time
error. The atmosphere
and the local vertical usually dominate real sightings. The UAP and witness
workflows (A04) should therefore carry the observer's position and time
uncertainty separately from this budget.

## Run locally

From the repository root. `rust-toolchain.toml` pins Rust 1.99.0.

```text
cargo run -p oia-sky --example sky-position --locked --offline -- 2460409.25 2460409.250800760 32.7767 -96.7970 139 17.577209049 7.814867384 359780.726 1010 10
```

The arguments, in order:
- `JD_UT1`, `JD_TT`.
- `LAT_DEG`, `LON_EAST_DEG`, `HEIGHT_M` (ellipsoidal).
- `RA_DEG`, `DEC_DEG`: geocentric apparent, true equator and equinox of date.
- `DISTANCE_KM`, or `parallax-negligible`.
- Optionally `PRESSURE_HPA TEMP_C`. Without them, refraction is not applied.

The command above uses the 2024-04-08 18:00 UT geocentric Moon row of
`geo-moon-eclipse`. It prints:
- the model ID and echoed inputs;
- GMST, the equation of the equinoxes, and GAST;
- the topocentric place and distance;
- airless azimuth, elevation and hour angle;
- the refracted elevation, or the reason none was produced.

Exit code `0` means success, `1` an invalid argument. The example is a
read-only calculator, not a supported application command.

## Local verification

**Environment:**
- Windows 11 Pro x86_64, Rust 1.99.0 (`x86_64-pc-windows-msvc`), offline.
- Developed on `main` at `3a9d5d3`. Rechecked after merging `main` at `b3b0479`, which includes the root workspace from #56.
- Fixtures: `fixtures/sky` as merged in #52 (Horizons API 1.2, DE441, EOP
  `eop.261030.p270126`), read in place and not modified.

**Commands:**

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo test --workspace --locked --offline
cargo test -p oia-sky --locked --offline -- --nocapture --test-threads 1
scripts/run.sh check
python -I scripts/sky/verify_sky.py
```

`tests/horizons_reference.rs` has four tests. Each covers a numerical or
input-validation risk that ordinary inspection would miss.

The tests print every residual. The "Measured maximum" column is the largest
absolute value printed by the `--nocapture` run above. The fixture reader
asserts that every row has as many fields as the header, and the row counts
are asserted.

| Test | Reference rows | Tolerance | Measured maximum |
| --- | --- | --- | --- |
| Horizontal transform (topocentric RA/Dec → Az/El) | 11 Dallas Moon rows (2024, UTC tags), 7 Washington Mars rows (1952, UT1 tags, refracted RA/Dec → refracted Az/El, down to −1.2°) | 1.0″ | Az·cos el 0.293″ and El 0.075″ (Dallas); 0.104″ and 0.088″ (1952) |
| Parallax + diurnal aberration (geocentric → topocentric) | 3 Moon epochs where `geo-moon-eclipse` and the Dallas rows coincide; the correction itself is > 0.4° | 0.1″ | 0.016″ |
| Full chain with refraction | 10 rows from `geo-sun-eclipse` for observers at 179.9° E and 180.1° E through sunrise | 1.0″ | Az 0.057″; refracted El 0.045″ (at 4.6° and 11.6°). The 6 rows below −1° are withheld as `BelowModelDomain` |
| Refraction amount alone | 11 Dallas Sun rows, airless versus refracted (~64°) | 0.5″ | 0.116″ |
| Input validation | — | — | Rejected: NaN; out-of-range values; swapped TT and UT1 at a modern epoch; a distance inside the Earth, in LEO, or above 10¹⁵ km; RA = 360; a provider topocentric place with NaN RA or zero distance |

The 2024 rows treat UT1 − UTC as zero, because the fixtures do not record it.
The consistent ~0.29″ azimuth offset for Dallas is within what the neglected
polar motion, that UT1 assumption, and the nutation truncation allow. It was
not attributed further.

**Mutation checks**, each run once against the revision under review and then
reverted:
- Removing the equation of the equinoxes failed 2 tests.
- Disabling diurnal aberration failed the parallax test.
- Replacing the ellipsoid with a sphere failed 1 test.

**What this establishes:** agreement with Horizons, to the stated tolerances,
for these sites, epochs (1952 and 2024) and targets. It does not establish
accuracy at other dates in the supported range. It also does not establish
agreement with a real atmosphere or with any ephemeris OIA may adopt under
D06. Horizons is an authoritative reference, not ground truth for refraction.

The `topo-alert-sun-solstice` case gives only refracted Az/El, with no
apparent place, so it can't be used without an ephemeris. It is not covered.

## Integration still required

- Move the kernel into `oia-time-geo`, or depend on it, once #12 implements
  its time primitives. Then take UT1, TT and their uncertainties from those
  primitives instead of raw Julian dates.
- Choose the ephemeris under D06 and record its versions with each result,
  next to `MODEL_ID`.
- Propagate observer position and time uncertainty, which the kernel does not
  do.
- Add a fuller nutation series or IAU 2006/2000A if sub-arcsecond accuracy is
  needed. Update this budget and the tests in the same change.
- Light time and planetary positions, the sky and solar-system views (#23 and
  SPACE-04), and full A04/A05 acceptance.
