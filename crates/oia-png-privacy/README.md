# PNG container privacy kernel (temporary standalone package)

Implements only the byte/container slice of PRIVACY-01 / issue #46: bounded
inspection and explicitly selected removal of uncompressed text, embedded
modification-time and Exif chunks from a conservative static RGBA8 PNG profile.
An Exif chunk with a non-default orientation is retained, and selecting it for
removal is refused, because removing it would change rendering.

See [the implemented profile, guarantees and limitations](../../docs/implementation/png-privacy.md).
The kernel has no filesystem, network, model, storage or publication authority.
A successful transformation is **not** a claim of anonymity, full PNG decoding,
complete metadata removal, or passing integrated A18 acceptance.

This package is a temporary standalone validation harness, not an additional
accepted workspace boundary. Integrate the module into the agreed oia-media
owner after #30/#22 prerequisites are available; do not copy unmerged P0 code.
The local Cargo.lock is for these standalone checks only. On integration, use
Cargo to update the root workspace lockfile instead of copying this lockfile.
