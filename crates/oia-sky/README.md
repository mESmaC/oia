# Observer sky geometry kernel (temporary standalone package)

This package is the provider-independent observer-geometry slice of issue #25.
It takes a geocentric apparent place from any ephemeris and computes:

- Earth rotation (GMST and GAST);
- the topocentric apparent place (parallax and diurnal aberration);
- airless azimuth and elevation;
- optionally, refraction under conditions the caller states.

It does not choose or evaluate an ephemeris (D06 is still open), and it does
not convert UTC to UT1 or TT, which belongs to #12. It is not an accepted
workspace boundary. Move the kernel into the P0 time and geometry module when
that module lands.

See [contract, accuracy budget, commands, and verification](../../docs/implementation/sky-observer-geometry.md).

The tests read the merged JPL Horizons responses in `fixtures/sky/originals`
in place. They are an independent reference, not observed evidence and not
OIA's ephemeris.

The local `Cargo.lock` exists only to reproduce standalone checks. When the
package joins the P0 workspace, discard it and regenerate the root workspace
lockfile with Cargo.
