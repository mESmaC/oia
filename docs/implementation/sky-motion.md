# Two-body sky motion and light-time kernel

Status: implemented in the dependency-free `oia-sky` workspace package. This is
the provider-independent numerical motion slice of
[#25](https://github.com/mESmaC/oia/issues/25), supporting part of
[SPACE-05](../spec.md#43-space-imagery-and-solar-system) and the numerical
subset of [A05](../delivery.md#a05-space-observations-and-planetary-context).
It is not integrated with application services, an investigation clock, a
renderer, or an ephemeris provider. D06 remains open.

## Boundary and evidence semantics

The `oia_sky::motion` module provides two mathematical operations:

1. Propagate caller-supplied elliptical orbital elements with two-body motion.
2. Solve one-way light time for geometric target and observer position
   samplers in one declared center and frame.

All positions are AU, velocities AU/day, gravitational parameters AU³/day²,
and epochs TDB Julian dates. `TdbEpoch` is **provisional pending #12** and
accepts only 1900-01-01 through 2100-01-01 (`MIN_JD_TDB`/`MAX_JD_TDB`). The only
frame is `EclipticOfJ2000`. A `CenterId` is an identity label, not a built-in
body or gravitational-parameter catalog. No provider is selected, no network or
production Horizons parser is used, and no planetary coefficient table is
embedded.

Propagated states are **derived/modelled**, not observations. `OrbitalState`
retains the exact `ElementProvenance` source/version, element epoch, signed
element age, and a `KeplerReport`. Callers must still validate whether supplied
elements are appropriate for their investigation and time span.

`LightTimePosition` is an observer-to-target **position** at the retarded
emission epoch. It is not a light-time-corrected velocity, an apparent place,
or evidence of a detection. It does not include annual/diurnal aberration,
gravitational light bending, atmospheric refraction, or observer-specific
parallax. Do not feed these J2000 ecliptic geometric vectors directly into the
existing true-equator/equinox-of-date `ApparentPlace` interface.

## Two-body contract

`KeplerOrbit::state_at` evaluates:

- `n = sqrt(mu / a) / a`;
- `M = M0 + n * (t_tdb - element_epoch_tdb)`;
- Kepler's equation `E - e sin(E) = M`;
- perifocal position and velocity;
- rotation `Rz(Omega) Rx(i) Rz(omega)` into the J2000 ecliptic.

`Omega` is the ascending node from ecliptic +X toward +Y, `i` the inclination
about the line of nodes, and `omega` the argument of periapsis from that node
in the orbital plane. `omega` is not the longitude of periapsis. The initial
mean anomaly is explicitly at the element epoch.

The constructor requires finite values, `a > 0`, `mu > 0`, `0 <= e < 1`, and
`0 <= i <= 180 degrees`. Node, argument-of-periapsis, and initial-mean-anomaly
inputs must be within [-360, 360] degrees and are normalized to [0, 360).
Propagation rejects an absolute element age above 36,525 days (100 Julian
years) and an absolute unwrapped mean anomaly above 1e6 radians. At the phase
bound, one `f64` ulp is about 1.2e-10 radians (17 m at 1 AU), so an already
rounded Kepler residual cannot disguise unbounded phase loss. Hyperbolic,
parabolic, N-body, relativistic, collision, and TLE/SGP4 propagation are not
implemented. In particular, fitting osculating elements at one instant does
not make the result a long-range planetary or lunar ephemeris.

The Kepler solver uses Newton steps constrained to a bracket, with bisection
fallback. `PropagationLimits` requires 1–10,000 iterations and a finite
positive angular tolerance. Exhaustion returns `KeplerDidNotConverge`; a
successful result records iterations, absolute residual, and tolerance.
`MOTION_MODEL_ID` identifies the model separately from the observer `MODEL_ID`.

## Light-time contract

`light_time_corrected_position` solves the fixed-point equation

```text
tau = |target_position(t_reception - tau) - observer_position(t_reception)| / c
```

with `c = 299792.458 km/s`, `1 AU = 149597870.700 km`, and 86,400 seconds/day.
The observer is evaluated once at reception. Target and observer samplers must
return geometric states with the requested exact epoch, center, and frame;
sampler failures remain distinguishable as `Target` and `Observer` errors.
Zero range, non-finite derived values, epoch/frame/center mismatches, and
iteration exhaustion are typed errors rather than fallback measurements.

`LightTimeLimits` requires 1–10,000 target evaluations and a finite positive
tolerance in days. A limit of one is accepted for budget-failure probing but
cannot succeed. Range is limited to 1e9 AU. Every successful solve performs at
least one update, so a coarse tolerance cannot return the initial `tau = 0`
guess as a correction. A result contains reception/emission epochs, the
corrected relative position, light time, range, iterations, absolute residual,
and the caller-selected tolerance. A loose tolerance can therefore be inspected
with the result; it is not silently hidden.

The common center must be inertial. A moving, non-inertial origin can introduce
an error of approximately `v_center * tau`; the original heliocentric Mars
reference exhibited about 10.1 km from this effect. Tight reference checks use
the solar-system barycenter instead.

Within the 1900–2100 JD window, the emission-epoch bound actually limits a
successful range to about 1.26e7 AU before the 1e9 AU outer range check binds.
A reception at `MIN_JD_TDB` cannot accept any positive light time because its
emission would precede the supported window.

Absolute TDB epochs are `f64` Julian dates. Near 2024 their spacing is about
40.2 microseconds, so subtracting absolute epochs cannot reproduce a much smaller
light-time tolerance exactly. Retain the returned duration for fine timing.
Higher-precision time-scale and uncertainty primitives belong to #12.

## Local verification

Environment: Windows x86_64, Rust 1.99.0, offline. The initial change was
rebased onto `main` at `28473f1`; review-follow-up measurements are identified
by the exact review-fix revision below. Existing retained fixtures are read in
place and never changed. They are JPL Horizons API 1.2 responses retrieved on
2026-11-02; the two added SSB Mars responses were retrieved on 2026-11-03 and
carry their own exact citations. Sources include DE441, mar099, and
jup365_merged as recorded in the [fixture manifest](../../fixtures/sky/manifest.json).
They are independent external references, not OIA observations or a D06
provider selection.

Commands:

```text
cargo +1.99.0 test -p oia-sky --test horizons_motion --locked --offline -- --nocapture --test-threads 1
cargo +1.99.0 test -p oia-sky --locked --offline -- --nocapture --test-threads 1
cargo +1.99.0 clippy -p oia-sky --all-targets --locked --offline -- -D warnings
cargo +1.99.0 fmt --all -- --check
cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings
cargo +1.99.0 test --workspace --locked --offline
python -B -I scripts/sky/verify_sky.py
```

Review-fix code validation revision: `9e58ce8f11d09c94e4a394bacff0faaa57d183dc`,
on `main` at `28473f1`. This documentation-only follow-up records that exact ID
without changing the validated code or fixtures; the final PR head is recorded
in the PR handoff.

`tests/horizons_motion.rs` contains five targeted tests for numerical
correctness and concrete failure risks. The fixture reader asserts correction
type, AU/day units, J2000 ecliptic frame, column layout, and two retained rows
per file. The analytic checks cover circular position/velocity orientation,
provenance and element age, a stationary light-time solution, and solver
budget exhaustion. Invalid domains and non-finite inputs are rejected.

### Short-horizon two-body references

For Earth, Mars, Jupiter, and the Moon, test code fits osculating elements to
the 2024-04-08 18:00 TDB geometric Horizons state using stated test-only GM
values (central body plus target) from the
[DE441 technical comments](https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/planets/de441_tech-comments.txt).
The Jupiter GM5 value is the Jupiter-system/barycenter value, not the isolated
planet value. The test then compares propagation with the **separate 19:00 TDB
row**. The first row is therefore an input, not independent validation. The
50 km assertion is a deliberately coarse numerical-wiring bound, not a physical
accuracy claim.

| Reference case | Checked propagation | Test bound | Measured position residual |
| --- | ---: | ---: | ---: |
| Earth, heliocentric | 1 hour | 50 km | 0.245089 km |
| Mars, heliocentric | 1 hour | 50 km | 0.000956 km |
| Jupiter, heliocentric | 1 hour | 50 km | 0.295529 km |
| Moon, geocentric | 1 hour | 50 km | 0.184668 km |

### Mars light-time reference

The initial heliocentric comparison was not a valid inertial-center oracle. Its
10.099 km residual is consistent with the Sun's barycentric motion during light
time (`v_sun * tau`), not a hidden kernel error. The replacement uses newly
retained SSB-centered geometric and `LT CORRECTED` Mars responses. A two-body
SSB target sampler and the fixed SSB origin satisfy the solver's own inertial
precondition; no residual is subtracted or fitted away. The correction itself
is about 18,231.3 km, and both cases converged in three target evaluations.

| Reception epoch (TDB JD) | Test position/range bound | Measured position residual | Measured light-time residual |
| --- | ---: | ---: | ---: |
| 2460409.250000000 | 0.25 km | 0.005544 km | 1.910e-13 day |
| 2460409.291666667 | 0.25 km | 0.092063 km | 2.281e-12 day |

The independent SSB check is tight enough to catch sub-kilometre regressions.
The 92 m position residual comes from the two-body SSB sampler: the SSB is
inertial but is not Mars's force center. Thus the position/range bound mostly
checks that sampler, while the light-time-duration bound checks the kernel's
retarded-epoch wiring. An independent cubic-Hermite sampler reviewed against
the same rows gives sub-metre agreement, but that sampler is not part of this
PR. These two close epochs still do not establish a general light-time accuracy
bound, absolute planetary accuracy, or accuracy outside the explicitly
supported numerical domains.

**What the tests establish:** the implemented formulas, frame rotation,
retarded-epoch wiring, provenance retention, and bounded failure behavior agree
with analytic cases and the cited retained rows to the stated limits. They do
not establish long-range accuracy, N-body perturbations, time-scale conversion,
apparent-place correctness, observational accuracy, or full A05 acceptance.

## Integration still required

- Select and version runtime ephemerides/orientation sources under D06; do not
  treat the test-only fitted elements as a built-in catalog.
- Connect #12's higher-precision time, time-scale conversion, and uncertainty
  primitives; propagate element and epoch uncertainty through results.
- Add independently sourced multi-date elements and references before claiming
  accuracy beyond these one-hour fixture checks.
- Add N-body/provider integration, lunar perturbations, illumination/phases,
  planetary rotation, satellites, and explicitly defined corrected velocities
  as separately scoped work.
- Integrate the investigation clock and #23/SPACE-04 views, including explicit
  geometric/apparent and true/illustrative scale labels, then run full A04/A05
  acceptance. No Lean proof or Rust-to-Lean correspondence is claimed here.
