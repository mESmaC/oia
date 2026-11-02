# JPEG container privacy kernel

**Implemented:** the dependency-independent JPEG byte/container slice of issue
#46, which serves PRIVACY-01 and the file portion of A18.

**Not implemented:** the application file-privacy workflow. The
[controlling privacy contract](../contracts/file-privacy.md) remains the target
for integration. This kernel does not replace domain, media, store, job or
export ownership. No formal proof is claimed, and no full A18 pass is claimed.

This slice follows the shape of the [PNG container kernel](png-privacy.md). It
does not change the PNG profile.

## Boundary and profile

The temporary [standalone Rust package](../../crates/oia-jpeg-privacy/Cargo.toml)
has no third-party dependencies. Its public entry points are `inspect` and
`clean`. Both:

- borrow immutable bytes supplied by the caller;
- take explicit finite `Limits`;
- take a shared cancellation/deadline/supersession callback;
- return typed errors.

Neither reads or writes files, opens URLs, invokes tools or models, nor
publishes output. The intended final module owner is oia-media. Integrating
this package is not a workspace, dependency or UI decision, and must not copy
unmerged prerequisites.

The exact profile identifier is **oia-jpeg-container-huffman-v1**. It walks
the interchange-format marker/segment syntax of ITU-T Recommendation T.81
(09/92) | ISO/IEC 10918-1, Annex B. Application identifiers follow:

- JFIF 1.02 (ITU-T T.871);
- Exif 2.3x APP1 (CIPA DC-008);
- XMP Specification Part 3 (APP1, and Extended XMP);
- the Adobe Photoshop File Formats Specification (APP13 image resources, APP14).

It is a container profile. It is not a conforming decoder, and it does not
promise that the entropy-coded data is valid.

### Container walk

1. The input must begin with SOI (`FF D8`). Anything else returns
   UnsupportedFormat.
2. Any number of `FF` fill bytes may precede a marker (T.81 B.1.1.2). They
   belong to the following segment's wire range.
3. Segments with a length field must declare a length of 2 or more, and must
   fit inside the input.
4. A second SOI, a stuffed `00` or RSTn outside entropy-coded data, a reserved
   marker (`02`-`BF`), a scan before the frame, a second frame, a missing EOI,
   and EOI before any scan are each Malformed.
5. After each scan header, entropy-coded data runs until the first marker that
   is neither a stuffed `FF 00` nor RST0-RST7 (optionally preceded by fill
   bytes). That data is recorded in `entropy_coded_range` and preserved byte
   for byte. It is never Huffman-decoded.
6. The walk ends at EOI. Bytes after EOI are reported in `trailing_range`,
   which makes coverage Partial. Examples are appended gain maps, motion-photo
   video and MPF secondary images.

### Segment handling

| Segment | Implemented behavior |
| --- | --- |
| SOF0, SOF1, SOF2 | Exactly one frame. Validate header length, precision (8 for SOF0; 8 or 12 for SOF1/SOF2), nonzero width, component count (1 to 4 for progressive), unique component identifiers, sampling factors 1 to 4, and quantization selectors 0 to 3. Width, height and pixel budgets apply. Retained. A zero height (lines deferred to DNL) sets `unsupported_frame` and makes coverage Partial |
| Other SOFn (lossless, arithmetic, hierarchical) | Header length and dimension budgets are validated. `unsupported_frame` is set, handling is Unsupported, coverage is Partial |
| DQT, DHT, DRI | Table structure is validated: precision/destination nibbles, 64- or 128-byte tables, 16 Huffman counts totalling 1 to 256 codes, a 2-byte restart interval. Retained exactly. Tables are not cross-checked against frames or scans |
| SOS | Header length, 1 to 4 components that exist in the frame without duplicates, and table selectors are validated. Spectral and approximation parameters are checked: Ss=0, Se=63, Ah=Al=0 for sequential frames; T.81 G.1.1.1.1 constraints for progressive frames. Retained together with its entropy-coded data |
| COM | Removable(Comment) |
| APP0 `JFIF\0` | Retained exactly only when the payload is exactly 14 bytes, units are 0 to 2, and the thumbnail is 0x0. A thumbnail, or a JFIF header that is short, padded or otherwise nonconforming, is Unsupported. Density values are retained, and may themselves be identifying |
| APP1 `Exif\0\0` | Read only the IFD0 Orientation tag (see below). Absent or 1: Removable(Exif). 2 to 8: RequiredForRendering(Exif). Uninterpretable: Unsupported |
| APP1 XMP (`http://ns.adobe.com/xap/1.0/\0`) | Removable(Xmp), unless the packet contains the byte strings `Orientation` or `hdr-gain-map`. In that case it is Unsupported (see below) |
| APP1 Extended XMP | Unsupported. Portions can split a property name across segments, so no single segment can be checked |
| APP13 `Photoshop 3.0\0` | Walk the `8BIM` image-resource blocks inside this segment. Removable(PhotoshopIrb) only when every block parses within the segment and none is 0x0422/0x0423 (embedded Exif copies) or 0x0424 (embedded XMP). Otherwise Unsupported |
| APP14 `Adobe` | Retained exactly only in the standard 12-byte form with a color transform of 0 to 2. Longer variants, such as Photoshop's 31-byte form, carry uninspected bytes and are Unsupported |
| APP2 ICC_PROFILE, MPF, FPXR; all other APPn | Unsupported, giving Partial coverage. ICC profiles follow the contract row for embedded color profiles |
| DNL, DAC, DHP, EXP, JPG, JPGn, TEM | Unsupported, giving Partial coverage |

Any Unsupported segment, an unsupported frame, or trailing bytes after EOI
makes coverage Partial, and `clean` refuses Partial input with
UnsupportedProfile. Malformed recognized containers return Malformed. Invalid
or truncated input is never reported as an empty-finding result.

### Exif and orientation

File-privacy contract section 3 says that removing an Exif container must not
silently drop orientation. The rule is the same as the PNG v2 profile's, applied
to the TIFF structure after APP1's six-byte `Exif\0\0` identifier code:

1. The TIFF header must be `II*\0` or `MM\0*`. An APP1 that begins with `Exif`
   followed by anything else does not match the identifier, and falls through to
   Unsupported.
2. The IFD0 offset must be at least 8. The entry count and all 12-byte entries
   must lie inside the payload, and lengths are checked before any read. The
   cancellation callback runs every 341 entries.
3. A matching Orientation entry (0x0112) must be SHORT with count 1. A value
   that is absent or 1 is AbsentOrDefault; 2 to 8 is RequiredForRendering;
   anything else is Unsupported.
4. A duplicate Orientation entry, an unexpected type or count, or an
   out-of-range IFD is Unsupported.

Each Exif APP1 is classified independently. If any one is RequiredForRendering,
an exif selection is refused for the whole file.

No other tag values are decoded, returned or followed. The kernel does not
follow sub-IFDs or GPS IFDs, read the IFD1 thumbnail, or read MakerNotes.
Selecting exif removes the whole segment, including any GPS, device, serial,
author, timestamp or thumbnail data it holds. The kernel does not report which
of these were present.

As in the PNG profile, ColorSpace, Gamma and resolution tags are not treated as
rendering-relevant. This profile does not establish whether a particular color
manager uses Exif ColorSpace or an interoperability index to pick a color space.

libjpeg-family decoders choose the color transform from JFIF/Adobe markers and
component identifiers, not from Exif. Removing Exif therefore does not change
that choice. Removing the only APP0/APP1 segment can leave a file that is
neither JFIF- nor Exif-conforming, while remaining valid T.81 interchange
format.

The kernel does not rewrite Exif (for example, keeping only Orientation). A
non-default orientation can be cleaned only by a separately specified,
explicitly selected derivative profile.

### XMP and Photoshop guards

XMP is UTF-8 RDF/XML whose property names cannot be expressed through character
references. A packet that contains `Orientation` (for example
`tiff:Orientation`) or the Adobe gain-map namespace (`hdr-gain-map`) could carry
rendering semantics. Some applications honor them, and the kernel does not
interpret RDF. Such packets are therefore Unsupported, not Removable. The check
is a conservative byte-string guard, not an XMP parser:

- It can refuse packets that merely mention the word.
- It does not establish which viewers consult other XMP properties, such as
  crop or panorama metadata.

Photoshop resources 0x0422 to 0x0424 hold embedded copies of Exif or XMP, which
could duplicate orientation, so their presence keeps APP13 Unsupported. Other
resources are removed with the segment without being decoded. Examples are
IPTC-NAA, thumbnails, resolution and caption digests.

## Transformation and verification

Selection uses four explicit booleans: `comment`, `exif`, `xmp` and
`photoshop_irb`. A false value retains the corresponding segments, including
their private values. If a RequiredForRendering segment is selected, `clean`
returns RequiredForRendering before allocating output. It does not drop the
segment, and it does not partly clean the other selected targets. An all-false
selection returns a byte-identical candidate with zero removals. That is not a
privacy improvement.

`clean` takes these steps:

1. Inspect the entire input, and refuse Partial coverage.
2. Compute the exact bounded output size, and allocate a separate result buffer.
3. Copy every non-selected segment's wire bytes, including fill bytes and
   entropy-coded data, without altering them.
4. Run `inspect` independently over the completed candidate.
5. Require complete coverage, an identical frame, the selected targets absent,
   and each retained segment unchanged in kind, handling, length and every byte,
   in order.

Original bytes remain borrowed and unchanged. Even though input and output share
a parser, verification is a fresh parse, not the writer's removal list.

The guarantee is structural: the selected COM/APP1/APP13 segments are absent,
and every retained wire byte is identical. That includes all frame, table and
scan headers and all entropy-coded data. The guarantee **does not** validate the
Huffman stream or the rendered appearance, and it does not address content-level
privacy.

CompleteContainerProfile means complete coverage for the named container profile
only. Do not translate it into complete file-wide metadata coverage, an
anonymous image, an authentic observation or a verified managed export.

Findings expose markers, structural kinds, handling and byte ranges. They do not
expose raw metadata values or source paths. `Cleaned` exposes bytes only through
an explicit immutable accessor, and its Debug implementation omits bytes.

The caller must treat the output as private until the authorized store/export
workflow commits it. That workflow's preview and receipt must show:

- the selected categories;
- the retained JFIF density and Adobe fields;
- the unsupported and content-level limits.

## Resource and failure contract

`Limits` requires nonzero values for:

- input bytes and output bytes;
- segment count;
- cumulative payload bytes of APPn, COM and other non-table segments;
- frame width, height and pixel count.

There is no silent budget fallback. Individual segment payloads are bounded by
JPEG's 16-bit length field. Length arithmetic and slice bounds are checked
before access. Input is not copied during inspection, and allocations use
fallible reservation. Metadata values are never decompressed, decoded or sent
to another process.

The synchronous callback is checked at these points:

- on entry;
- at each segment;
- at least every 4096 bytes of fill-byte and entropy-coded scanning, and of
  copying and retained-byte comparison;
- at each Photoshop resource block;
- every 341 Exif IFD0 entries;
- before each XMP guard scan;
- across output-size planning;
- immediately before success.

Cancelled, DeadlineExceeded and Superseded remain distinct, and none of them
returns a candidate. Callers must enforce wall time through this callback. Job
concurrency, source snapshot acquisition and total staging storage must be
bounded through the shared services; the kernel cannot enforce those external
controls itself.

Numeric limits bound memory and work. They are not a measured performance
target. The kernel allocates no decoded-pixel buffers and parses no payloads
recursively.

## Local verification evidence

On 2026-11-01 (America/New_York), on Windows x86_64 with Rust/Cargo 1.99.0, at
base revision `772ee4e`:

```text
cargo +1.99.0 fmt --manifest-path crates/oia-jpeg-privacy/Cargo.toml -- --check
cargo +1.99.0 clippy --manifest-path crates/oia-jpeg-privacy/Cargo.toml --all-targets --locked -- -D warnings
cargo +1.99.0 test --manifest-path crates/oia-jpeg-privacy/Cargo.toml --locked
python -I crates/oia-jpeg-privacy/tests/reference_check.py
```

The six focused integration tests cover:

- **Retained bytes:** original and retained-byte preservation, explicit and
  partial selection, an all-false no-op, Debug output without metadata, and a
  ten-scan progressive frame.
- **Exif orientation:** default orientation is removed. Orientation 6 is
  retained when exif is not selected, and refused when it is. An out-of-range
  value and a non-TIFF payload are Unsupported.
- **Opaque regions are refused:** ICC, XMP naming Orientation, Extended XMP,
  APP13 with embedded Exif, an unknown APPn, JFXX, a JFIF thumbnail, bytes
  after EOI, and an arithmetic SOF9 frame.
- **Fail-closed structure:** every truncated prefix, a restart marker outside
  scan data, a scan before the frame, a corrupt length, and non-JPEG input.
- **Resource limits:** every budget dimension, including a zero limit.
- **Cancellation:** cancellation at every callback reached while cleaning both
  fixtures, plus deadline and supersession.

Two mutation checks confirmed that the tests catch the defects they target:

- classifying orientation 2 to 8 as removable makes the orientation test fail;
- disabling the XMP `Orientation` guard makes the opaque-region test fail.

The tests do not establish filesystem isolation, recovery, live-provider
behavior or full A18.

### Fixtures

The fixtures were generated by [make_fixtures.py](../../crates/oia-jpeg-privacy/tests/make_fixtures.py)
with Python 3.12 and Pillow 12.3.0. Pillow's bundled libjpeg encodes a 16x16
RGB image of four flat 8x8 color blocks, at quality 90 with 4:4:4 sampling.
Python struct then inserts invented segments after JFIF APP0. They are generated
fixture content, not observations. The committed hex files are canonical,
because another libjpeg build could produce different entropy-coded bytes.

- [baseline.hex](../../crates/oia-jpeg-privacy/tests/fixtures/baseline.hex)
  (SOF0, 1069 bytes) carries:
  - a big-endian Exif APP1 with Orientation = 1 and the Artist `Synthetic Bob`;
  - an XMP APP1 with `dc:creator` `Synthetic Carol`;
  - a Photoshop APP13 holding one IPTC-NAA resource with the By-line
    `Synthetic Dave`;
  - one COM segment.
- [progressive.hex](../../crates/oia-jpeg-privacy/tests/fixtures/progressive.hex)
  (SOF2 with ten scans, 566 bytes) carries the same COM segment only.

The test-only segments in `container_contract.rs` are also invented. They
include a little-endian Exif IFD0 with Orientation 6 or 9, and an ICC header
with an opaque body.

### Independent reference check

[reference_check.py](../../crates/oia-jpeg-privacy/tests/reference_check.py)
runs the synthetic-only `clean-fixture` example. It reads no user file and
exports nothing. It then:

1. walks the original and the Rust-produced candidate with a separately written
   Python marker parser;
2. requires the target segments to be absent, and every retained segment's wire
   bytes, including entropy-coded data, to be identical and in order;
3. checks that none of the invented identifier strings remain in the candidate;
4. decodes both with Pillow 12.3.0, and requires equal size, mode and pixels;
5. confirms that Pillow reads the original's Orientation as 1 and finds no Exif,
   XMP, Photoshop or comment data in the candidate;
6. confirms that the fixture's SHA-256 is unchanged.

Results:

| Fixture | Original | Candidate | Segments | Candidate SHA-256 |
| --- | --- | --- | --- | --- |
| baseline.hex | 1069 bytes | 651 bytes | 4 removed, 11 retained | `b466723c…8090d500aa01a6386614dbff` |
| progressive.hex | 566 bytes | 535 bytes | 1 removed, 26 retained | `24f749ad…9d9642ed0bfec7a5f62f4b1c4` |

Pixel equality follows from the identical entropy-coded data, given one decoder.
This is evidence for these synthetic fixtures only. It is not complete
independent parser coverage for all admitted inputs, and it is not a perceptual
proof. A 16x16 image cannot show an orientation effect; the orientation evidence
is the tag classification, which Pillow independently agrees with.

### Additional local probe

A scratch program outside the repository ran read-only inspection, then `clean`
where coverage was complete, over 20 JPEG files shipped with Windows 11 under
`C:\Windows\Web`. It walked every file without error:

- 11 files were CompleteContainerProfile: baseline frames of up to 6400x4000
  with restart intervals and Adobe or JFIF headers, and no removable targets.
  Each `clean` returned byte-identical output with zero removals.
- 9 files were Partial. All 9 carry an ICC profile; the 8 progressive
  seven-scan files also carry a 31-byte Adobe APP14. Their Exif, XMP and
  Photoshop segments were classified Removable, but cleanup is refused while
  coverage is Partial.

This is a robustness observation, not a compatibility list or a fixture. The
files were not retained or committed.

## Integration still required

Keep #46 open. #30/#22 and the shared P0 services must supply:

- immutable artifact IDs and source/output hashes;
- authorization and workspace isolation;
- pinned plans and durable, cancellable jobs;
- alias, hardlink and symlink protection;
- private staging;
- atomic publication of provenance and output, and recovery;
- export omission previews and privacy-safe receipts.

The IFD0 orientation reader duplicates the PNG kernel's. The #30 forensic slice
also reads JPEG. Both are to merge into one oia-media JPEG reader at
integration. Add JPEG to an actual user-visible support list only when that
end-to-end path exists. This standalone slice waives no existing privacy
contract or acceptance obligation.
