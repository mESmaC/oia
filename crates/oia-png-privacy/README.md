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

This package is a workspace validation harness for the PNG container privacy
kernel (synthetic example: `clean-png-fixture`), not an additional accepted
service boundary. Integrate the module into the agreed `oia-media` owner after
`#30`/`#22` prerequisites are available; do not copy unmerged P0 code.
Dependency locking is managed by the root workspace `Cargo.lock`.
