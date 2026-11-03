# Sky geometry and motion kernels

This package contains provider-independent numerical slices of issue #25.
It has no dependencies and does not choose a runtime ephemeris (D06 remains
open).

## Observer geometry

Given a geocentric apparent place, the crate computes Earth rotation (GMST and
GAST), topocentric parallax and diurnal aberration, airless azimuth/elevation,
and optional refraction under caller-stated conditions. See the
[observer contract and accuracy budget](../../docs/implementation/sky-observer-geometry.md).

## Two-body motion and light time

The `motion` module propagates caller-supplied, versioned elliptical elements
in TDB and solves one-way light time against geometric target/observer
samplers. Positions use AU, velocities AU/day, and the J2000 ecliptic frame.
Propagated results retain element provenance and age; numerical results carry
iteration, residual, and tolerance reports. See the
[motion contract and verification record](../../docs/implementation/sky-motion.md).

These are modelled geometric calculations, not observations, apparent places,
or a long-range planetary/lunar ephemeris. The package does not convert UTC to
UT1, TT, or TDB (#12), provide N-body or satellite propagation, select an
ephemeris provider, or integrate with the investigation clock and views (#23).
It is a root workspace member but not yet an accepted architecture boundary;
wire it to shared time primitives when #12 implements them.

The tests read the retained JPL Horizons responses in `fixtures/sky/originals`
in place. Those responses are independent external references, not observed
evidence or OIA's selected ephemeris.
