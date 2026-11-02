# ALPR and Flock camera directory kernel (temporary standalone package)

Implements the dependency-free catalog normalization, manufacturer classification,
viewport bounding, Web Mercator detail-zoom selection, coordinate precision cache,
and illustrative bearing-wedge geometry for the mapped ALPR / Flock camera
directory slice of [issue #28](https://github.com/mESmaC/oia/issues/28)
(`CAM-06`, `GEO-01`, `GEO-02`, `GEO-04`, `CASE-02`; distinct from `CAM-01`/`CAM-02`
viewable camera feeds).

See the [contract](../../docs/contracts/alpr-camera-map.md),
[implementation and verification notes](../../docs/implementation/alpr-camera-map.md),
[fictional New Meridian corridor fixture](../../fixtures/cameras/alpr/new-meridian-corridor-fixture.json),
and companion [interactive map prototype](../../docs/design/mockups/flock-camera-map.html).

The kernel has no filesystem, network, model, storage, or publication authority.
Mapped OpenStreetMap ALPR records represent community-mapped equipment assertions,
not verified hardware deployments, live video streams, license-plate read databases,
or exhaustive road coverage.

This package is a temporary standalone validation harness (`[workspace]` isolated,
zero third-party dependencies, `--locked --offline` on Rust 1.99.0), not an
additional accepted workspace crate boundary. Integrate the module into the
agreed `oia-ingest` / `oia-time-geo` / `oia-query` owners after P0 and #23/#28
prerequisites merge; discard this local `Cargo.lock` on integration.
