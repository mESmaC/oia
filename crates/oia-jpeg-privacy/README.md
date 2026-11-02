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

This package is a workspace validation harness for the JPEG container privacy
kernel (synthetic example: `clean-jpeg-fixture`), not an additional accepted
service boundary. Integrate it into the agreed `oia-media` owner, alongside
[`oia-png-privacy`](../oia-png-privacy/README.md), once the `#30`/`#22`
prerequisites are available. Do not copy unmerged P0 code. Dependency locking
is managed by the root workspace `Cargo.lock`.
