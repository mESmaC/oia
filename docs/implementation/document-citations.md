# Structural UTF-8 citation checks

`oia_documents::citations::validate_citations` implements the byte-anchor subset
of DOC-03/DOC-04 and passive input handling from DOC-05, issue [#18](https://github.com/mESmaC/oia/issues/18).
It checks a caller-supplied batch against one immutable [UTF-8 extraction](document-text.md).
It follows the structural validation boundary in the
[search and QA contract](../contracts/document-search-and-cited-qa.md) and
[D04 ADR](../adr/0007-d04-ai-execution-and-grounding.md).

Each citation supplies a zero-based half-open original byte range and an exact
quote. Quotes may cross extracted passages, including their original CRLF/LF.
The validator rejects empty/reversed ranges, ranges outside the original input,
unextracted suffixes, invalid UTF-8 character boundaries, wrong quote lengths
and mismatched bytes. It does not normalize text, search for replacement anchors,
interpret instructions, copy the input or change any extraction/review state.
The first invalid citation is identified by its zero-based batch index; errors
do not include the quote or source text.

Both citation count and cumulative quote bytes have explicit positive limits.
Overlapping/repeated quotes count separately. Overflow is refused. Extraction
limits bound the number of passages scanned per citation. Control is checked
at entry, per preflight citation, per validation citation/passage, per at most
4,096-byte comparison and at the final success boundary. This is cooperative
control, with no measured latency guarantee. The batch returns only success or
a typed error, with no partial validation output. An empty batch succeeds but
does not establish that a draft contains required citations. A valid quote in
a partial extraction does not make that extraction complete.

## Read-only local example

```text
cargo run -p oia-documents --example check-citation --locked --offline -- fixtures/documents/a03/originals/report-v1.txt 121 128 lights.
```

Arguments are `PATH START_BYTE END_BYTE QUOTE`. Exit 0 means exact structural
match in this supplied extraction; 1 means an invalid citation, budget, stop,
I/O or argument error. Input is bounded to 1,048,576 bytes, extraction to 10,000
passages, and one quote to 4,096 bytes. A five-second cooperative deadline spans
extraction/validation after file reading. Blocking I/O is not interruptible.
The example reads a local UTF-8 file and never writes it or fetches URLs.

## Integration and validation scope

The caller must authorize and pin the correct source/artifact/extraction versions
before supplying bytes. Success says nothing about version identity, acquisition
completeness, authenticity, entailment, truth, or human review. This helper has
no network, tool, model, publication, or persistence authority. It is not an
integrated QA or citation service. Application integration remains dependent on
P0 services and #16/#17; source correction/reassessment, models, page/region
citations and full A03 remain open. No Lean proof is claimed.

Two focused assertion groups cover exact anchors (including the existing
synthetic A03 fixture), cross-passage Unicode/CRLF ranges, unavailable text,
fabricated quotes, finite budgets and rejection at every control boundary.
These protect citation integrity and stopped-batch publication rather than
model quality or semantic support. Local checks and environment are recorded
in the delivering PR.
