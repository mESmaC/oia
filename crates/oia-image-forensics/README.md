# Offline still-image forensic cues

Rust library and CLI for a bounded MEDIA-01 / A10 slice of
[#30](https://github.com/mESmaC/oia/issues/30). These are investigative
diagnostics, **not verdicts that an image is real, fake, edited or AI-generated**.
Learned AI detection and credential/watermark verification are explicitly
`not_implemented`; D09 remains open.

## Run

From the repository root with Rust 1.99.0:

```text
cargo build -p oia-image-forensics --locked
cargo run -p oia-image-forensics --locked --offline -- image.png
cargo run -p oia-image-forensics --locked --offline -- image.jpg --residual-map residual.png --jpeg-quality 90
cargo run -p oia-image-forensics --locked --offline -- --help
```

The initial build may download locked dependencies. Analysis is local and
offline. JSON goes to stdout, errors to stderr. Exit `0` means the declared
diagnostic profile completed, `2` means partial coverage, and `1` means failure
or a stopped run; none means authentic. **All JPEG reports exit `2`: decoder
acceptance does not establish entropy completeness or exclude recovered pixels.**

Static PNG up to 8-bit sample depth and supported Huffman JPEG are accepted.
Animated PNG, other formats, high-depth PNG and CMYK/YCCK JPEG are refused.
Source files are read-only. Optional residual PNGs are labelled derivatives
and never overwrite existing files. No GUI or persisted case integration is
implemented in this package.

## Report

- Original-byte SHA-256, size, dimensions, profile/implementation, timestamps,
  configuration, review state, and optional build revision.
- Forgeable generation/edit metadata signatures with exact payload ranges
  and hashes; prompts and raw metadata are not exported.
- RGB histograms and exact, grid-aligned, opaque repeated-block candidates.
- JPEG residual statistics and an optional amplified visualization; PNG is
  not silently converted to JPEG for this operation.
- Method scope/coverage/limitations, including unverified JPEG raster
  completeness. Assessment remains `inconclusive` and review `unreviewed`.

Schema version `2` puts histograms in a method envelope, not a bare array. All
applicable JPEG pixel methods carry partial coverage and the same input-recovery
reason. PNG checks include bounded complete IDAT zlib/Adler-32 validation;
derivative labels appear before image data. Neither establishes authenticity.

Library: `oia_image_forensics::analyze(&bytes, &config, control)`. The callback
may return `Cancelled`, `DeadlineExceeded` or `Superseded`. A final checkpoint
prevents stopped calls returning reports. Codec calls are only cooperatively
cancellable between calls, not hard process deadlines.

See the [implementation contract](../../docs/implementation/image-forensics.md)
for defaults, formulas, unsupported cases, dependencies and verification.
