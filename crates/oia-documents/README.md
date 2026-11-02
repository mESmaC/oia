# UTF-8 extraction and search kernels

This package implements a bounded, side-effect-free UTF-8 passage extractor and
a read-only local file inspection example for the DOC-02 slice of issue #17.
It is a member of the Rust workspace, not an integrated import pipeline.
Final application placement remains coordinated with the P0 service owners.

See [contracts, commands, verification, and limitations](../../docs/implementation/document-text.md).

The independent [exact-text search kernel and local example](../../docs/implementation/document-search.md)
add bounded passage matching for the CASE-05/DOC-04 slice of #18. This remains a
validation package; application search and final module placement
under the proposed `oia-query` boundary await merged P0 contracts.

`search::find_token` and the example's `--token` option additionally match whole
Unicode-whitespace-delimited tokens. Case and punctuation remain literal;
results preserve original byte anchors and disclose the selected matching mode.

The root workspace Cargo.lock pins builds.

`search::SearchSession` resumes either matching mode across bounded pages while
preserving original anchors, cumulative coverage, and a retryable cursor after
cancellation. The `find-text-pages` example bounds total pages as well as page
size; see the search documentation for commands and limitations.

PDF parsing, OCR, providers, source-version persistence, citation reassessment,
and complete A03 acceptance remain open. The merged A03 synthetic originals are
read in place for exact-byte/citation checks; they are not observed evidence.
