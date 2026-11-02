# JPEG container privacy kernel (temporary standalone package)

Implements only the JPEG byte/container slice of PRIVACY-01 / issue #46. It
inspects a JPEG file within bounds, and removes comment, Exif, XMP and
Photoshop IRB/IPTC segments only when each is explicitly selected. The JPEG
must use one of the Huffman-coded frame types SOF0, SOF1 or SOF2. An Exif
segment with a non-default orientation is retained, and selecting it for
removal is refused, because removing it would change rendering.

See [the implemented profile, guarantees and limitations](../../docs/implementation/jpeg-privacy.md).
The kernel has no filesystem, network, model, storage or publication authority.
It walks the container and never decodes entropy-coded image data. A successful
transformation is **not** a claim of anonymity, JPEG decoding or validity,
complete metadata removal, or passing integrated A18 acceptance.

This package is a temporary standalone validation harness, not an accepted
workspace boundary. Integrate it into the agreed oia-media owner, alongside
[`oia-png-privacy`](../oia-png-privacy/README.md), once the #30/#22
prerequisites are available. Do not copy unmerged P0 code. The local
Cargo.lock is for these standalone checks only. On integration, have Cargo
update the root workspace lockfile instead of copying this one.
