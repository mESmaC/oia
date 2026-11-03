# Bounded UTF-16 document extraction

Implemented as a standalone, standard-library-only kernel in the workspace's
`oia-documents` crate. Work package: [#17](https://github.com/mESmaC/oia/issues/17).
Requirement: [DOC-02](../spec.md#41-public-document-library), passage extraction
only. Acceptance scope: the source preservation and partial-coverage subset of
[A03](../delivery.md#a03-cited-document-research), not the complete scenario.

## API and evidence contract

`oia_documents::utf16::extract_utf16(input, byte_order, limits, check_control)`
accepts caller-supplied bytes and an explicit `ByteOrder::LittleEndian` or
`ByteOrder::BigEndian`. It does not guess an encoding or switch endianness based
on content. Callers must establish the encoding from source metadata or a
separate explicit detection step. A BOM decoded as U+FEFF remains part of the
text. Other valid scalar values, including noncharacters, are not normalized.

Each owned `Utf16Passage` contains its exact decoded text, one-based line number,
and a zero-based, half-open `source_byte_range()` into the original **UTF-16**
buffer. These are not UTF-8 offsets. LF delimits passages; CRLF, blank lines,
controls, supplementary characters, BOMs, and unterminated final lines remain
unchanged in meaning. A final LF does not add an empty passage. Re-encoding each
passage with the recorded byte order reproduces its source-byte slice exactly.
The source slice is never mutated; decoded strings are separate derivatives.

`coverage().input_bytes()`, `extracted_bytes()` and `unprocessed_range()` all
refer to original encoded bytes, not decoded string lengths. `Complete` means
every source byte is represented by returned passages; `Empty` means the caller
supplied zero bytes, not that acquisition found no records. `PassageLimitReached`
is explicit partial extraction with an unprocessed source-byte suffix.

Do not feed the encoded offsets to the existing UTF-8 search/citation APIs or
pretend the decoded strings are the retained original. Persisting this derivative
will require a versioned processing record with source identity/version, encoding,
limits, coverage, and mappings. This slice adds no persistence or citation bridge.

## Bounds and failures

`Utf16Limits::new(max_input_bytes, max_decoded_bytes, max_passages)` requires
positive independent limits. The entire supplied buffer must fit the input
budget. The decoded-byte budget counts the UTF-8 length of **all** supplied text,
including the suffix beyond a passage-limited result; UTF-16 can expand when
decoded to UTF-8. No byte or character is silently replaced or truncated.

Odd byte lengths and unpaired high/low surrogates are typed errors. Surrogate
errors retain the offending source-byte offset and code unit. Validation continues
through the complete input after the passage limit, without retaining further
passages, so malformed suffixes cannot produce apparently valid partial text.
An output-budget error or stop request also fails the whole call rather than
publishing an earlier prefix as success.

Input-size checks precede allocation. String and passage-vector growth use
fallible reservations. Memory and work are bounded by the caller's input,
decoded-byte and passage limits; allocator capacity may exceed logical length.
Cancellation/deadline callbacks run at entry, before each decoded scalar, and
immediately before success. This is cooperative control, not a file-I/O timeout;
callbacks are responsible for their own execution time. Source text remains data,
never instructions or tool calls.

## Local verification and remaining scope

The three targeted tests in `tests/utf16_contract.rs` cover exact original-byte
round trips in both byte orders (including a supplementary scalar), partial
coverage, malformed suffixes, output expansion limits, and cancellation at the
return boundary. They address evidence preservation and bounded failure behavior,
not empirical OCR accuracy or formal implementation correspondence.

Run from the repository root with the pinned Rust 1.99.0 toolchain:

```text
cargo test -p oia-documents --test utf16_contract --locked --offline
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo test --workspace --locked --offline
```

This is not an acquisition command, application import path, encoding detector,
PDF parser, OCR engine, or search integration. Existing UTF-8 behavior is unchanged.
P0 storage/ingestion integration (#11/#13), source/version identities, PDF/OCR,
provider decisions D03/D04, and full DOC-02/A03 acceptance remain open. No new
dependency, provider, architecture decision, or Lean proof claim is introduced.
