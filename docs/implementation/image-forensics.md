# Still-image forensic cues

Implemented offline library/CLI slice of
[#30](https://github.com/mESmaC/oia/issues/30), supporting part of
[MEDIA-01](../spec.md#52-forensic-media-and-recreation) and
[A10](../delivery.md#a10-media-provenance-and-reconstruction-prompts).
Entry point: [oia_image_forensics::analyze](../../crates/oia-image-forensics/src/lib.rs).
This workspace package is not the full forensic workbench. D09 method/model,
corpus, threshold, hardware and error-presentation selection remains open.
The separate codec-worker crate keeps the image dependency tree out of
`oia-media`; that crate retains ownership of the media application boundary and
will integrate this worker rather than duplicate its diagnostics.

## Evidence contract

The caller supplies retained bytes, Config and a cooperative control callback.
Analysis never changes those bytes. JSON records original-byte SHA-256/length,
dimensions, schema/profile, implementation version, optional OIA_BUILD_REVISION,
analysis start/duration, settings, method coverage and limitations. Start time
is analysis time, not capture/publication time; it is absent when the system
clock cannot represent a Unix timestamp. Hashes establish integrity, not truth.

Assessment is always inconclusive and review state unreviewed. No cue does not
prove camera origin or absence of AI/editing. Declarations are never promoted
to observations or verified history. No calibrated probability, likelihood,
false-positive rate, detector accuracy or formal proof is claimed.

Decoded samples are derivatives, not observations: encoded raster coordinates,
8-bit RGBA, no EXIF rotation or ICC/gamma color management. Histograms include
RGB beneath transparent PNG pixels, not a color-managed composited display.
Report schema version 2 wraps `rgb_histograms` in the same method envelope as
other pixel diagnostics (`coverage`, `scope`, `result`, `limitations`), replacing
the version 1 bare array so input uncertainty is visible without reading a
separate report field.

## Input and decoder boundary

Reuse [oia-png-privacy](../../crates/oia-png-privacy/README.md) and
[oia-jpeg-privacy](../../crates/oia-jpeg-privacy/README.md) inspect results and
exact chunk/segment ranges. No second container walker is introduced. Inspectors
check structure, length/order, PNG CRCs and configured budgets, not compressed
sample validity. Validated ranges form an internal decode view, not an exported
cleaned copy.

- PNG: retain IHDR/PLTE/tRNS/IDAT/IEND; inspect metadata separately without
  inflating text/profiles. Refuse APNG rather than choosing its first frame,
  depths above 8 bits, unknown critical chunks, duplicate/late palette or
  transparency, container errors and budget overflow. Grayscale, indexed and
  alpha inputs within this profile use the pinned PNG decoder.
  Before raster decoding, a separate bounded zlib integrity pass over IDAT
  payloads from the shared inspector requires the final deflate block, Adler-32
  and stream end with no trailing compressed bytes. It discards inflated data
  in 8 KiB work units with control checks and a five-bytes-per-pixel cap (at
  most four sample bytes plus a filter byte per pixel, also covering Adam7).
- JPEG: retain codec segments, APP0 and APP14; inspect COM/application metadata
  separately. Accept supported sequential/progressive Huffman frames with one
  or three components; cap scans before decoding. Refuse unsupported processes,
  CMYK/YCCK, trailing bytes, structural errors, codec errors and budget overflow.
  Baseline SOF0 entropy shorter than a conservative two-bit-per-block floor is
  refused before decoding: each component needs at least one coded block per
  32-by-32 source region (maximum sampling factor four). Stuffed/restart bytes
  are counted generously, so this only rejects grossly impossible inputs and
  does not certify remaining entropy. Progressive frames do not use this floor.
- Other formats are unsupported. No fallback resizing or format conversion.

### JPEG entropy completeness is unverified

The zune-jpeg strict option is enabled, but it does not exclude recovery. An
independent synthetic check found that a scan truncated after two entropy bytes
and reattached EOI still decoded. The pinned decoder zero-fills after a marker
and exposes no public recovery receipt. A trial with jpeg-decoder 0.3.2 also
accepted that fixture; it is not shipped or used as a silent fallback.

Every JPEG report therefore has raster_validation.coverage = partial and the
result "JPEG entropy completeness unverified; decoder recovery possible".
profile_is_partial() includes that state; the CLI exits 2 even for valid JPEG
fixtures. Histograms, applicable block-repeat scans and residuals also remain
partial with the same raster-validation reason, even if their computations
finish. A non-applicable method stays non-applicable. These are calculations
on decoder output, not verified source samples. Residual PNG metadata repeats
the input validation limitation. No per-image claim of whether recovery
actually occurred is made.
An audited fail-closed entropy contract or observable recovery status is needed
before claiming stronger JPEG validation.

## Diagnostic profile

Profile: oia-still-image-cues-v1. These are exploratory diagnostics, not evaluated
classifiers or a resolution of D09.

### Text declarations

Inspect PNG tEXt/uncompressed iTXt and JPEG COM/standard APP1 XMP for:

- Diffusion-style parameters/comment text containing Steps, Sampler and Seed.
- A prompt JSON object declaring a recognized ComfyUI sampler node class.
- Selected software/XMP/comment mentions of Photoshop, GIMP, Lightroom or
  Affinity Photo; ordinary editing or a textual mention can produce the cue.
- IPTC trained-algorithmic-media source-type URI strings in XMP. XML structure,
  signatures, trust and binding to pixels are not verified.

Each cue contains its container, original payload offset/length, payload SHA-256
and interpretation, not raw text, prompts or input paths. Declarations are
forgeable/removable. Compressed text, Exif, extended XMP, opaque application
metadata and credential/profile regions remain explicit uninspected ranges,
making metadata coverage partial. No compressed metadata inflation, C2PA or
watermark verification is implemented.
Fixed-layout ancillary PNG metadata (including pHYs, sRGB, gAMA, tIME and bKGD)
also remains uninspected/partial, with a reason specific to that scope rather
than calling it compressed text. APP0 records, including possible JFIF/JFXX
embedded thumbnails, and APP14 declarations are listed as uninspected ranges;
retaining them for codec interpretation does not imply metadata inspection.

### Histograms and repeated blocks

RGB histograms count decoded samples in bins 0 through 255. Repeated-block
candidates use nonoverlapping block_size squares in row-major order. Blocks
must be opaque and have sample range at least minimum_channel_range within at
least one RGB channel. Excluded nonopaque/low-range block counts and uncovered
right/bottom strips remain visible.

SHA-256 selects potential repeats; exact RGBA comparison confirms equality.
Each repeat references the first matching block. Block/candidate caps return
partial coverage and a stop reason. A raster smaller than a block is
not_applicable. Genuine textures/layouts can match; shifted, rotated, scaled,
blended or recompressed copies are generally missed. This is not a full
copy-move/compositing detector; the range threshold is an unevaluated heuristic.

### JPEG recompression residual

For decoder RGB output I, encode with the pinned image JPEG encoder at recorded
quality and decode to J. Per-channel D = abs(I - J). Report the unamplified
256-bin difference histogram, maximum and arithmetic mean of all RGB samples.
No resampling occurs. PNG is not_applicable; no lossy history is invented.

Optional visualization channels are min(255, D * residual_gain), not probabilities
or raw measurements. The separately hashed PNG contains oia:role,
oia:source_sha256, oia:profile, oia:settings and oia:input_validation labels
immediately after IHDR, before IDAT, so readers encounter them before image data;
source metadata is not copied. The CLI uses create-new and refuses existing
destinations, including the input. On write failure it attempts to remove only
the newly created incomplete derivative. Brightness depends on quality, encoder,
texture and save history, not solely on editing or AI generation.

## Bounds and failure behavior

Reports include full configuration. Defaults:

| Bound/setting | Default |
| --- | --- |
| Input bytes / encoded artifact bytes | 32 MiB each |
| Decoded pixels / maximum dimension | 4,000,000 / 8,192 |
| Decoder allocation budget | 128 MiB, best effort |
| Container items / JPEG scans / metadata bytes | 8,192 / 64 / 1 MiB |
| Examined grid blocks / candidate pairs | 65,536 / 100 |
| Cooperative deadline | 30,000 ms |
| Block size / minimum within-channel range | 16 pixels / 16 |
| JPEG quality / visualization gain | 90 / 8 |
| Residual-map emission | Disabled |

Input, dimensions/pixels, container/metadata/scan and artifact overflow fail the
analysis instead of truncating. Block/candidate limits are explicitly partial.
Zero/invalid configurations fail. PNG uses decoder limits; JPEG checks dimensions
and seven bytes/pixel for output/conversion plus two bytes per component per
pixel on a conservatively 32-pixel-padded raster for progressive coefficients
before decoding (reserved for all JPEG frames, roughly six additional bytes
per pixel for three components). These are not hard
peak-RSS guarantees: codec scratch, working buffers, hash tables and report
allocations consume additional memory. Do not raise budgets from untrusted input
or assume this library creates a sandbox.

Control checks occur between bounded work units and after report construction;
failed cancellation/deadline/supersession checkpoints return no report. Codecs
cannot be interrupted mid-call and allocation failure is not guaranteed
recoverable. Hard isolation, kill deadlines, durable leases and transactional
publication belong to worker integration. The CLI reads a regular local file
with a max-plus-one sentinel, with no network or credential access.

## Dependencies and choices

Production analysis is Rust; Python/Pillow is development-only. Direct external
versions are exact pins; transitive versions/checksums live in root Cargo.lock.
No root manifest, toolchain, existing parser code or production subprocess
integration is changed.

| Dependency | Role | Declared license |
| --- | --- | --- |
| image 0.25.10, PNG/JPEG only | PNG decoding, PNG/JPEG encoding | MIT OR Apache-2.0 |
| flate2 1.1.10, zlib-rs backend | Bounded complete PNG IDAT zlib/Adler-32 validation before raster decoding | MIT OR Apache-2.0 |
| zune-jpeg 0.5.15 / zune-core 0.5.3, std only | Direct JPEG decoder/options, avoiding image's non-strict wrapper | MIT OR Apache-2.0 OR Zlib |
| crc32fast 1.5.4 | CRCs for derived PNG labels | MIT OR Apache-2.0 |
| sha2 0.10.9 | Integrity identities and repeated-block lookup | MIT OR Apache-2.0 |
| serde 1.0.229 / serde_json 1.0.154 | Reports and bounded workflow JSON parsing | MIT OR Apache-2.0 |
| oia-png-privacy / oia-jpeg-privacy | Shared container inspectors and source ranges | Workspace MIT OR Apache-2.0 |

Cargo metadata/lock review found 40 external packages in this resolution, with
permissive declared licenses (MIT, Apache-2.0, Zlib, BSD, Unicode-3.0 and alternative
Unlicense/0BSD expressions). This records roles/declared licensing, not a complete
security or legal audit. No God's Eye View source/data/model was ported, and no
model weights are bundled.
The PNG decoder identity explicitly names transitive png 0.18.1. The zlib
integrity pass directly uses already-locked flate2/zlib-rs packages; it adds
no packages or alternate production runtime.

## Verification and remaining work

Verified locally on 2026-11-02, Windows x86_64 MSVC, rustc 1.99.0 (b940084d7),
Python 3.12.10, Pillow 12.3.0. Source: this implementation PR head, with its base
revision recorded in the PR. Focused and workspace checks passed:

```text
cargo build -p oia-image-forensics --locked --offline
cargo test -p oia-image-forensics --locked --offline
python crates/oia-image-forensics/tests/reference_check.py
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo test --workspace --locked --offline
```

Four Rust contract tests cover retained bytes/anchors and no assertion promotion,
residual arithmetic versus visualization, partial budgets/uninspected metadata,
and malformed/oversized/animated/stale-result refusal. The numerical reference
uses the pinned codec in a separately calculated formula, not independent codec
validation. The Pillow check independently verifies PNG histograms,
baseline/progressive/grayscale JPEG acceptance, readable labelled derivatives,
hashes, overwrite refusal, partial/error exits and explicit unverified coverage
of the recoverable truncated-entropy fixture. Review-follow-up assertions also
cover pixel-method uncertainty propagation, the grossly inflated JPEG header
refusal, the coefficient allocation allowance, APP0/ancillary omissions,
pre-IDAT labels and PNG zlib checksum/end/trailing/excess-output failures.
Fixtures are synthetic patterns.
No corpus/operational accuracy evaluation, non-Windows execution, hardware
measurement or Lean proof is claimed.

Full A10 remains incomplete: no UI comparison/zoom/channel/frequency views,
evaluated copy-move/compositing methods, learned AI detector, credential/watermark
trust verification, video or restoration. Application integration must supply
SourceRecord/Artifact/version/workspace IDs, authorization, job orchestration,
persistence and exports; hashes/timestamps cannot stand in for those contracts.
Issue #30 stays open; D09 remains a prerequisite for an evaluated detector release.
