# UTF-8 extraction kernel (temporary standalone package)

This package implements a bounded, side-effect-free UTF-8 passage extractor and
a read-only local file inspection example for the DOC-02 slice of issue #17.
It is not a new accepted workspace boundary or an integrated import pipeline.
The kernel can move into the module chosen by the P0 workspace/ingestion owner;
this PR does not modify their architecture map, root manifests, or crates.

See [contracts, commands, verification, and limitations](../../docs/implementation/document-text.md).

The local Cargo.lock exists only to reproduce standalone checks. Discard it
when integrating into P0 and regenerate the root workspace lockfile with Cargo;
do not treat this file as the reviewed shared workspace lockfile.

PDF parsing, OCR, providers, source-version persistence, citation reassessment,
and complete A03 acceptance remain open. The merged A03 synthetic originals are
read in place for exact-byte/citation checks; they are not observed evidence.
