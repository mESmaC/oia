# PNG container privacy kernel

**Implemented:** the dependency-independent byte/container slice of issue #46,
PRIVACY-01 and the file portion of A18. **Not implemented:** the application file
privacy workflow. The [controlling privacy contract](../contracts/file-privacy.md)
remains the target for integration; this kernel does not replace domain, media,
store, job or export ownership. No formal proof or full A18 pass is claimed.

## Boundary and profile

The temporary [standalone Rust package](../../crates/oia-png-privacy/Cargo.toml)
has no third-party dependencies. Its public entry points are inspect and clean.
Both borrow immutable caller-supplied bytes, take explicit finite Limits and a
shared cancellation/deadline/supersession callback, and return typed errors.
Neither reads/writes files, opens URLs, invokes tools/models nor publishes output.
Intended final module owner: oia-media. Integrating this package is not a
workspace, dependency or UI decision, and must not copy unmerged prerequisites.

The exact profile identifier is **oia-png-container-rgba8-v2**. It recognizes PNG
signature and chunk containers using the pinned W3C
[PNG Third Edition, 2025-06-24](https://www.w3.org/TR/2025/REC-png-3-20250624/),
sections 5.3–5.6 and 11. It is deliberately a container profile, not a conforming
full-image decoder or a promise that the compressed image is valid.

Profile history: **v1** (PR #67, e72064d) classified every eXIf chunk as
Unsupported. **v2** adds the eXIf handling below. A result or receipt labelled v1
keeps its v1 meaning; v2 does not enlarge what an earlier v1 cleanup removed.

| Region | Implemented behavior |
| --- | --- |
| IHDR | First, exactly once, 13 bytes; nonzero PNG-range dimensions within budgets; validate depth/color combinations, compression/filter and interlace method; complete profile only for 8-bit color type 6 (RGBA), non-interlaced |
| IDAT | One or more consecutive chunks with a nonempty combined payload; preserve every chunk byte, boundary and checksum; no decompression, filtering or pixel interpretation |
| IEND | Exactly zero payload bytes after IDAT, last in the input; reject trailing bytes |
| tEXt | Enumerate as uncompressed textual-property containers, not decoded identities; validate keyword/separator/null constraints; remove all instances only when text is explicitly selected |
| tIME | Enumerate embedded modification time, not filesystem/source/event time; require 7 bytes, valid calendar day and hour/minute/second ranges (including second 60); remove only when modification_time is selected |
| gAMA, sRGB, pHYs | Optional singletons before IDAT; require valid lengths and scalar ranges; retain exact bytes, including pixel-density/unit information that may itself be identifying |
| eXIf | Optional singleton before IDAT (section 11.3.4.5, Table 7); a duplicate or post-IDAT eXIf is Malformed. Enumerate as an opaque Exif container; only the Orientation tag (0x0112) in IFD0 is read (see below). Absent or default orientation: remove the whole chunk only when exif is selected. Non-default orientation: RequiredForRendering; retained, and an exif selection is refused |
| Any other chunk | Record Unsupported handling and Partial coverage; refuse cleanup, regardless of safe-to-copy flag |
| Other valid image parameters | Set unsupported_image_parameters and Partial coverage; refuse cleanup |

### eXIf and orientation

The contract (section 3) requires that removing an Exif container must not drop
orientation silently. The kernel therefore reads only enough of the eXIf payload
to decide whether orientation is present:

1. The payload must begin with the TIFF header `II*\0` or `MM\0*`, as section
   11.3.4.5.2 specifies. PNG eXIf omits the JPEG APP1 marker and the `Exif`
   identifier code.
2. The IFD0 offset must be at least 8, and the entry count plus all 12-byte
   entries must lie inside the payload. Lengths are checked before any read. The
   cancellation callback runs every 341 entries.
3. Each IFD0 entry tag is compared with 0x0112. A matching entry must be SHORT
   (type 3) with count 1. Its inline value is classified as follows:
   absent or 1 means AbsentOrDefault, which is Removable(Exif); 2 to 8 means
   RequiredForRendering(Exif); anything else is Unsupported.
4. A bad header, out-of-range IFD, duplicate Orientation entry, or unexpected
   type/count is Unsupported, with Partial coverage, and cleanup is refused. The
   PNG container itself is still valid, so this is not reported as Malformed.

No other tag values are decoded, returned or followed: not offsets to the Exif
or GPS sub-IFDs, IFD1 thumbnails, or MakerNotes. Selecting exif removes the
whole container, including any GPS, device/serial, author, timestamp or
thumbnail data it holds, but the kernel does not report which were present. Tags such as
ColorSpace, Gamma or X/YResolution are not treated as rendering-relevant here.
This profile does not establish whether a particular viewer consults them.

The kernel does not rewrite eXIf (for example, keeping only Orientation). A
non-default orientation can be cleaned only by a separately specified,
explicitly selected derivative profile. Pillow 12.3.0 accepts an `Exif`-prefixed
eXIf payload and reads its tags, so such chunks are real exposures for at least
one reader. The kernel reports them as Unsupported rather than as clean.

In particular, iCCP, cHRM, cICP, other color/HDR chunks, PLTE, tRNS,
compressed/international text (zTXt/iTXt), APNG chunks (acTL/fcTL/fdAT), unknown
critical chunks and private extensions are **outside** this profile. They are not
silently discarded, flattened, recompressed or reinterpreted. JPEG, PDF, archives
and other formats return UnsupportedFormat.
Malformed recognized containers return Malformed; invalid or truncated input is
not an empty-finding result. Unsupported payload semantics are not validated.

## Transformation and verification

Selection uses three explicit booleans: text, modification_time and exif. False
retains the corresponding chunks, including their private values. If exif is
selected and the eXIf chunk is RequiredForRendering, clean returns
RequiredForRendering(Exif) before allocating output. It does not drop the chunk,
and it does not perform a partial cleanup of the other selected targets. An
all-false selection returns a byte-identical candidate with zero removals, not a
privacy improvement.

clean inspects the entire input, refuses Partial coverage, computes the exact
bounded output size, and allocates a separate result buffer. It copies the
signature and every non-selected chunk without altering them. It independently
runs inspect over the completed candidate bytes, requires the selected chunk
classes to be absent, and compares every retained wire region in order. Original
bytes remain borrowed and unchanged. Even when input/output share the same parser,
verification is a fresh parse, not the writer's removal list.

The guarantee is structural: selected tEXt/tIME/eXIf containers absent and all retained
wire bytes identical, including IHDR, IDAT and supported rendering metadata. This
is stronger than comparing only concatenated IDAT payloads, but **does not**
validate the zlib stream, rendered appearance or content-level privacy. Coverage
CompleteContainerProfile means the named container profile only. Do not translate
it into complete file-wide metadata coverage, an anonymous image, an authentic
observation or a verified managed export.

Findings expose chunk types, handling and byte ranges, not raw metadata values or
source paths. Cleaned exposes bytes only through an explicit immutable accessor;
its Debug implementation omits bytes. The caller must treat the output as private
until the authorized store/export workflow commits it. Selected categories, the
retained pHYs/rendering fields and unsupported/content-level limits must be visible
in that workflow's preview/receipt.

## Resource and failure contract

Limits requires nonzero input/output bytes, individual chunk payload bytes, chunk
count, cumulative non-IHDR/IDAT/IEND payload bytes, width, height and pixel count.
No silent budget fallback is available. Length arithmetic and available slices
are checked before access; chunk/count/metadata budgets precede CRC work and
allocation. Input is not copied during inspection, and allocations use fallible
reservation. Metadata values are never decompressed or sent to another process.

The synchronous callback is checked on entry, at each chunk, at most every 4096
bytes during CRC/text scanning/copy/retained-byte comparison, every 341 eXIf
IFD0 entries, across output-size
planning, and immediately before success. Cancelled, DeadlineExceeded and
Superseded remain distinct; none returns a candidate. Callers must enforce wall
time through this callback and bound job concurrency, source snapshot acquisition
and total staging storage through the shared services. The kernel cannot enforce
those external controls itself. Numeric limits bound memory/work, not a measured
performance target; no decoded-pixel buffers or recursive payload parsing exist.

## Local verification evidence

On 2026-11-01 (America/New_York), Windows x86_64, Rust/Cargo 1.99.0:

```text
cargo +1.99.0 fmt --manifest-path crates/oia-png-privacy/Cargo.toml -- --check
cargo +1.99.0 clippy --manifest-path crates/oia-png-privacy/Cargo.toml --all-targets --locked -- -D warnings
cargo +1.99.0 test --manifest-path crates/oia-png-privacy/Cargo.toml --locked
python crates/oia-png-privacy/tests/reference_check.py
```

The six focused integration tests cover:

- original and retained-byte preservation, and explicit selection;
- Exif orientation handling: default orientation removed, orientation 6 retained
  or refused, an `Exif`-prefixed payload Unsupported, and a post-IDAT eXIf
  Malformed;
- refusing opaque and APNG regions;
- truncation, checksum, order and trailing-byte failures;
- resource limits;
- cancellation at every callback reached by the synthetic transformation of both
  fixtures, including the final success barrier.

The Exif test was checked by mutation: classifying orientation 2 to 8 as
removable makes it fail. They do not
establish filesystem isolation, recovery, live-provider behavior or full A18.

The [synthetic hex fixture](../../crates/oia-png-privacy/tests/fixtures/rgba.hex)
is a 1x1 RGBA8 PNG with a split zlib stream, gAMA/sRGB/pHYs, two invented textual
properties and one embedded timestamp. Its scanline is the filter byte 0 followed
by RGBA (12, 34, 56, 255). It is generated fixture content, not an observation.
The image stream and chunk CRCs were generated with Python 3.12's standard zlib
and struct tooling, independently of the Rust implementation. Python is solely
a development/reference tool, not a production dependency.

The [Exif fixture](../../crates/oia-png-privacy/tests/fixtures/rgba-exif.hex) is
the same file with one eXIf chunk inserted after pHYs. Its payload is a 52-byte
big-endian TIFF structure whose IFD0 has two entries: Orientation = 1 and an
invented Artist string, `Synthetic Bob`. The test-only wires in
`container_contract.rs` are built the same way: a little-endian Orientation = 6
IFD0, and the default payload with an `Exif` identifier-code prefix. All of
these were generated with Python struct/zlib, independently of the Rust code.
Pillow 12.3.0 reads them as Orientation 1 plus Artist, as Orientation 6, and as
Orientation 1 plus Artist, respectively.

Independent local validation uses Python struct/zlib plus Pillow 12.3.0 to parse
the original and actual Rust-produced candidate: compare original SHA-256 before
and after, every chunk CRC and retained wire byte, target absence and decoded
pixel equality. This is evidence for this synthetic fixture only, not complete
independent parser coverage for all admitted inputs or a perceptual proof.
The checker requires Python 3.12 and Pillow 12.3.0 in the development environment;
it executes the synthetic-only clean-fixture example and does not export user
files. For rgba.hex the candidate is 132 bytes (original: 232), with the three
selected containers absent and both decoded pixels and all retained chunk bytes
identical. For rgba-exif.hex (296 bytes) the four selected containers are absent,
Pillow reads the original's Orientation as 1 and finds no Exif in the candidate,
and the candidate is byte-identical to the rgba.hex candidate (SHA-256
`f0501c7b…8678d4764`). A 1x1 image cannot show an orientation effect visually.
The orientation evidence is the tag classification, which Pillow independently
agrees with, not a rendering comparison.

## Integration still required

Keep #46 open. #30/#22 and shared P0 services must supply immutable artifact IDs,
source/output hashes, authorization/workspace isolation, pinned plans, durable
cancellable jobs, alias/hardlink/symlink protection, private staging, atomic
provenance/output publication, recovery, export omission previews and privacy-safe
receipts. Add the format to an actual user-visible support list only when that
end-to-end path exists. No existing privacy contract or acceptance obligation is
waived by this standalone slice.
