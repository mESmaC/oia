# Synthetic document corpus for A03

This is development input for [issue #17](https://github.com/mESmaC/oia/issues/17),
[DOC-01/DOC-02](../../../docs/spec.md#41-public-document-library), and the
[A03 acceptance scenario](../../../docs/delivery.md#a03-cited-document-research).
Every account, identity, date omission, and image is synthetic. None is an
observation, a real disclosure, or evidence for an event. The `.invalid` URLs are
identifiers only; no fetch is required or permitted by the fixture.

The corpus contains eight original artifacts referenced by nine source records.
Its manifest is a **fixture format**, not OIA's production record or bundle schema.
The P0 records, storage, and ingestion contracts remain owned by their existing
sessions. Integrate this corpus through those contracts after they land; do not
introduce another production evidence model from this manifest.

## Contents and expected behavior

| Input | Contract exercised |
| --- | --- |
| Original report and republication | Two source records reference identical retained bytes. Deduplication must preserve both identities and the republication relationship. |
| Corrected edition | Distinct original bytes and provider version; previous citations remain resolvable and dependent results need reassessment. |
| Releases A and B | Different redactions remain separate releases. No hidden text is retained behind the redactions. |
| Conflicting account | Preserve disagreement; a fictional assertion is not an established fact. |
| Untrusted passage | Source instructions remain quoted data. Expected network/tool/publication actions: none. |
| Raster page 1 | PNG with no text layer; human-authored expected transcription is separate in the manifest. |
| Raster page 2 | Readable fixture heading and deliberately unreadable content region. Extracting the heading alone must not imply complete page processing. |
| Missing page 3 | Explicit omission with no artifact or substitute image. Missing is distinct from unreadable. |
| Reference-only record | No retained payload; the omission reason models retention being prohibited. |

`manifest.json` records SHA-256, byte length, media type, simulation labels,
source identities, provider versions, and explicit unknown event/publication
times. Conditions for access, automated processing, retention, and redistribution
are separate fixture values; they do not select the project's distribution license.

Citation ranges are zero-based, half-open **UTF-8 byte offsets** into the exact
artifact, qualified by source record version and page. The non-ASCII text before
the original report's citation intentionally distinguishes byte offsets from
character offsets. The old and corrected reports have separate citation entries.
The raster transcript is an expectation, not an OCR result or an additional source.

The directory's `.gitattributes` disables Git text conversion for `originals/**`.
Those bytes must survive checkout unchanged even with `core.autocrlf=true`;
normalizing line endings would invalidate both hashes and byte-offset citations.
The generated manifest uses LF explicitly. These attributes apply only to this
corpus, without changing repository-wide or user Git configuration. Keep the
attributes with the corpus when moving it to another repository or directory.

## Local use

From the repository root, with Python 3.12:

```text
python scripts/fixtures/verify_a03.py
```

The validator uses only the Python standard library. It verifies retained hashes
and lengths, source/version references, exact text anchors, declared coverage,
required relationships, and bounded PNG structure/decompression. It does not call
providers, execute document text, run OCR, or write to the corpus. It is a local
fixture checker, not a production parser for arbitrary case bundles.

To reproduce the fixtures into a **new** directory:

```text
python -m pip install -r scripts/fixtures/requirements.txt
python scripts/fixtures/build_a03.py --output <new-directory>
python scripts/fixtures/verify_a03.py <new-directory>
```

The generator requires Pillow 12.3.0 and its bundled font; no system font is used.
It refuses an existing destination, including this checked-in corpus. Compare
the new manifest and artifacts byte-for-byte before intentionally revising a
fixture. A changed fixture needs reviewed expectations and a new fixture identity
when its contract changes; do not update hashes merely to make a check pass.

## Scope and handoff

This slice prepares the critical original-preservation and exact-citation inputs
required by A03. It does not make a test suite a prerequisite for ordinary feature
development. D03 (real seed corpus/provider) and D04 (OCR/model choices) remain
open. This corpus provides no real provider, scanned PDF, OCR accuracy dataset,
production extraction path, persistence integration, or formal proof.

After [#11](https://github.com/mESmaC/oia/issues/11) and
[#13](https://github.com/mESmaC/oia/issues/13) land, #17's runtime implementer should
map these inputs into the shared source/evidence contracts, retain originals,
exercise extraction and coverage reporting, and add the real document/PDF and
provider fixtures appropriate to the selected adapters. Later A03 integration
must also exercise correction propagation and ensure source text cannot initiate
actions. Expected states in this manifest are not evidence that OIA implements them.

## Validation evidence

Validated on Windows x86-64 with Python 3.12.10 and Pillow 12.3.0, against local
changes based on `edb4958` (2026-11-02 UTC):

- Local validator: eight artifacts, nine source records, four exact citations.
- Fresh-directory regeneration: manifest and all originals byte-identical.
- Original-byte corruption, altered citation range, and a fabricated replacement
  for the missing page: each rejected in disposable copies.
- Both PNG pages visually inspected: labels and page 1 text readable, page 2
  content region intentionally unreadable; no page 3 placeholder.
- Generator refuses an existing destination without changing its files.
- Git index/checkout round trip with `core.autocrlf=true`: all eight originals
  and the manifest remained byte-identical with the local attributes, and the
  checked-out corpus passed validation. A disposable control without the
  attributes reproduced CRLF conversion and failed integrity validation.

These checks establish fixture consistency only. Runtime ingestion, OCR,
citation invalidation, and the complete A03 scenario have not been executed.
No Cargo/Lake workspace exists in this checkout, so Rust/Lean checks do not apply
to this development-only slice.
