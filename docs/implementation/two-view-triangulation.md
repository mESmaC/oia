# Two-view triangulation kernel

This implements the dependency-independent numerical slice of [#37](https://github.com/mESmaC/oia/issues/37)
in [`oia_analysis::triangulation`](../../crates/oia-analysis/src/triangulation.rs).
It contributes to `FIELD-03`, `GEO-04`, and `MEDIA-02`, and only the geometric
part of [A13](../delivery.md#a13-fieldwork-and-multi-observer-research).
It is not an integrated camera workbench or completed acceptance scenario.

## Implemented workflow

Supply two already calibrated sightlines, their exact observation, calibration,
pose and Cartesian-frame version references, a common target-epoch reference,
and clock-alignment, feature-correspondence and acquisition-independence review
references. Supply absolute camera-position, angular and timing-displacement
error bounds for each view, plus explicit range, crossing-angle and output-radius
limits. `reconstruct` returns a nominal closest approach, residual separation,
and a conditional outer position sphere, retaining the exact input and method ID
`oia-two-view-closest-approach-v2`. Version 2 adds the resolved-baseline gate
described below; it preserves version 1's closest-approach and radius formulas.

These are caller-supplied references and assertions. The kernel does not resolve
records, inspect images, verify review conclusions, establish clock alignment,
fit a calibration or determine whether a camera actually moved. It refuses
missing reviews, unknown/expired calibration status, or a capture pose version
that differs from the calibrated pose. Reusing an observation or source is also
refused; distinct IDs alone do not prove independent acquisition.

Directions are normalized internally; originals are retained unchanged.
Both views must use the same frame version, whose definition must specify metre
units, axes, origin and any epoch needed to interpret the frame. This module
does not convert geographic coordinates or scientific time scales. These
transient analysis arguments do not replace the shared contracts in
[`oia-time-geo`](../../crates/oia-time-geo/src/lib.rs) or the future persisted
evidence envelope. No new dependencies or production subprocesses are used.

The result is always derived. If either input is simulated, the result retains
`Simulation`, including mixed inputs. Otherwise it is
`DerivedFromObservations`, never an observation or a verified physical location.
Source truth, feature correspondence and the supplied numerical bounds require
independent assessment at integration.

## Numerical method and uncertainty

For nominal camera positions `p`, `q` and unit directions `u`, `v`, let
`w = q - p` and `n = u × v`. The line parameters are:

```text
t = ((w × v) · n) / |n|²
s = ((w × u) · n) / |n|²
a = p + t u
b = q + s v
center = (a + b) / 2
separation = |a - b|
```

The nominal solution must be in front of both cameras and within the requested
range limit. The acute crossing angle uses `atan2(|n|, |u·v|)`, so parallel,
antiparallel and poorly conditioned views fail before division. Co-located
origins are refused. A refusal means this method cannot return a candidate under
the specified conditions; it is not evidence that no real target exists.

For each view, the caller supplies:

- `e`: an absolute Euclidean camera-position error bound in metres.
- `alpha`: an absolute angular direction-error bound in radians, incorporating
  calibration, feature localization and relevant lens effects.
- `d`: an absolute target-displacement bound from capture to the common target
  epoch. It must account for clock uncertainty and target motion. An alignment
  reference alone does not establish this bound; for example, a justified speed
  bound times a justified time-error bound can contribute to it.

With a caller-assumed maximum **true** camera-to-target distance `R` at capture,
any feasible position at the common epoch lies inside each infinite line tube
of radius `e + R sin(alpha) + d`. Unknown bounds are rejected; `Some(0.0)` is an
explicit caller assertion of zero error, not a default. Standard deviations and
classifier scores must not be supplied as absolute bounds.

The baseline must strictly exceed `e1 + e2 + 2 * 0.001 m`, so the two origin
error balls remain disjoint after the per-view numerical allowance. A smaller
nonzero baseline is refused as `BaselineNotResolved`, even if the nominal rays
intersect. Thus a tiny nominal baseline with zero stated errors cannot produce
an apparently resolved range. The gate uses worst-case absolute bounds; it does
not model correlated camera-position errors that could cancel in a relative
baseline measurement.

The implementation adds a declared `0.001 m` numerical allowance to each tube
radius, producing `h1` and `h2`. If the closest-line separation exceeds
`h1 + h2`, it rejects the pair as inconsistent with those bounds.

For the line-normal projectors `P1 = I - u uᵀ`, `P2 = I - v vᵀ`, let
`g = (b - a) / 2` be the half-gap between the closest points and
`y = x - center`. The closest-approach construction makes `g` perpendicular to
both lines, so `P1 g = P2 g = g`. Since `center - a = g` and
`center - b = -g`, the two squared line distances expand as:

```text
|P1(x - p)|² + |P2(x - q)|²
    = |P1 y + g|² + |P2 y - g|²
    = |P1 y|² + |P2 y|² + 2|g|²
```

The cross terms cancel because `g·P1 y = g·y = g·P2 y`. A feasible point has
line distances at most `h1` and `h2`, so the sum of the two projected `y`
norms squared is at most `h1² + h2² - 2|g|²`. Dropping the nonnegative gap
term gives the deliberately looser bound used by the implementation:

```text
lambda_min = 1 - |u·v| = |u×v|² / (1 + |u·v|)
lambda_min * |x - center|²
    <= |P1(x - center)|² + |P2(x - center)|²
    <= h1² + h2²
radius = hypot(h1, h2) / sqrt(lambda_min)
```

The second form of `lambda_min` avoids subtracting nearly equal numbers. The
radius encloses the feasible region of the stated error model in ideal
arithmetic; it does not describe its exact shape. Infinite tubes intentionally
relax the finite rays and angular cones, so a returned sphere **does not prove
that a feasible point exists**. A small residual alone is not an accuracy
estimate. The requested maximum radius is enforced without lowering input
uncertainty or extending the range limit.

This is a derivation, not a checked Lean theorem or a floating-point interval
proof. The millimetre allowance is a numerical margin tested on the listed
fixtures, not a formally established roundoff bound for every supported input.
Neither the sphere nor its radius is a calibrated probability/confidence region
or a claim about real-camera accuracy.

## Domain and failures

All values must be finite. Each Cartesian coordinate is within
`[-10,000,000, 10,000,000] m`; positive range/radius limits and nonnegative
position/displacement bounds are at most `10,000,000 m`. Direction vectors can
have any finite, nonzero magnitude and are scaled before normalization to avoid
overflow and underflow. Angular errors are in `[0, pi/2]` radians. The requested
minimum crossing angle is in `[1 degree, 90 degrees]`; narrower crossings are
outside this implementation's numerical domain.

Typed errors distinguish missing prerequisites, stale or moved calibration,
frame/source/version conflicts, unknown error bounds, invalid numbers, unresolved
baselines, degenerate geometry, nominal ranges outside limits, inconsistent sightlines and excessive
output uncertainty. Nothing is clamped into a plausible range or replaced with
an invented observation. A changed calibration requires a new call with new
references; already returned results retain their original inputs. Automatic
dependency reassessment awaits application integration.

The call processes exactly two views with fixed-size arithmetic and bounded
identifier types. It performs no IO, variable-size search, iterative solver,
retention, background jobs or publication. Scheduling/cancellation and atomic
publication belong to the application service when integrated.

## Local execution and verification

Run from the repository root with the pinned Rust 1.99.0 toolchain:

```text
cargo run -p oia-analysis --example triangulate-views --locked --offline
cargo test -p oia-analysis --test triangulation_contract --locked --offline
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo test --workspace --locked --offline
```

The example uses **synthetic** cameras at `(0, 0, 0)` and `(10, 0, 0)` metres,
looking along `(1, 2, 0)` and `(-1, 2, 0)`. Their analytic intersection is
`(5, 10, 0)`. For an assumed maximum range of `100 m`, position errors of
`0.01 m`, angular errors of `0.001 rad` and timing displacement of `0.02 m`, the
computed outer radius is approximately `0.292925 m`. This is a modelled fixture
result. The example also prints moved-camera and parallel-view refusals.

Three targeted contract tests address the specific failure risks of false
unique range, unjustified precision and evidence-class/version loss:

- Independently derived symmetric intersection and skew-line results (metre
  tolerance `1e-12`), exact input retention, swapped views, direction rescaling,
  and containment of a known feasible target under declared perturbations.
  A large-coordinate, eight-million-metre, approximately one-degree crossing
  fixture is checked against its constructed target within `0.001 m`.
- Parallel, antiparallel, nearly parallel, co-located, unresolved-baseline, behind-camera,
  out-of-range, inconsistent, overly uncertain, non-finite and zero-direction
  inputs must not return a candidate.
- Moved/expired calibration, missing timing and review prerequisites, different
  frames and reused inputs are rejected; mixed simulation and observation inputs
  stay simulated and successful calls retain all supplied references.

Executed on 2026-11-02 on Windows x86_64 MSVC with Rust/Cargo 1.99.0: the example,
all three focused tests, workspace formatting, Clippy and workspace tests passed.
The delivery PR records the checked revision and any rebase validation.
No hosted CI, live-camera tests,
hardware/physical-accuracy measurements or formal proofs were run.

The follow-up to [PR #89's review](https://github.com/mESmaC/oia/pull/89#pullrequestreview-5745012691)
adds a `1e-300 m` nonzero-baseline refusal and checks the origin-uncertainty gate
at its threshold and just above it within the existing geometry test. Error
messages use readable view/prerequisite names; typed error variants remain
available for callers.

## Remaining scope

Issue #37 stays open for reference-based camera/lens/time calibration and
residuals, pixel-to-ray and coordinate conversions, verified multi-view
correspondence, non-static trajectories, observation planning, integrated
uncertainty visualization, persistence/jobs/UI and full A13/A04/A10 acceptance.
Those still require the issue's #27/#30/#25/#24 prerequisites and shared P0
contracts. This slice selects no provider, camera, renderer, timing service or
new architecture dependency.

When #14 exposes this kernel through a service, the response and inspection
view must retain the caller-asserted status of calibration validity, pose-version
equality, metre-frame interpretation, source independence, timing/displacement
bounds and maximum true range. Resolving references does not itself verify these
claims. This is an integration requirement, not an implemented service feature.
