# Observer sky geometry kernel

This package is the provider-independent observer-geometry slice of issue #25.
It takes a geocentric apparent place from any ephemeris and computes:

- Earth rotation (GMST and GAST);
- the topocentric apparent place (parallax and diurnal aberration);
- airless azimuth and elevation;
- optionally, refraction under conditions the caller states.

It does not choose or evaluate an ephemeris (D06 is still open), and it does
not convert UTC to UT1 or TT, which belongs to #12. The package is a
member of the root Cargo workspace and has no dependencies. It is not yet an
accepted architecture boundary. Move the kernel into `oia-time-geo`, or wire it
to that crate's time primitives, when #12 implements them.

See [contract, accuracy budget, commands, and verification](../../docs/implementation/sky-observer-geometry.md).

The tests read the merged JPL Horizons responses in `fixtures/sky/originals`
in place. They are an independent reference, not observed evidence and not
OIA's ephemeris.
