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

The exact profile identifier is **oia-png-container-rgba8-v1**. It recognizes PNG
signature and chunk containers using the pinned W3C
[PNG Third Edition, 2025-06-24](https://www.w3.org/TR/2025/REC-png-3-20250624/),
sections 5.3–5.6 and 11. It is deliberately a container profile, not a conforming
full-image decoder or a promise that the compressed image is valid.

| Region | Implemented behavior |
| --- | --- |
| IHDR | First, exactly once, 13 bytes; nonzero PNG-range dimensions within budgets; validate depth/color combinations, compression/filter and interlace method; complete profile only for 8-bit color type 6 (RGBA), non-interlaced |
| IDAT | One or more consecutive chunks with a nonempty combined payload; preserve every chunk byte, boundary and checksum; no decompression, filtering or pixel interpretation |
| IEND | Exactly zero payload bytes after IDAT, last in the input; reject trailing bytes |
| tEXt | Enumerate as uncompressed textual-property containers, not decoded identities; validate keyword/separator/null constraints; remove all instances only when text is explicitly selected |
| tIME | Enumerate embedded modification time, not filesystem/source/event time; require 7 bytes, valid calendar day and hour/minute/second ranges (including second 60); remove only when modification_time is selected |
| gAMA, sRGB, pHYs | Optional singletons before IDAT; require valid lengths and scalar ranges; retain exact bytes, including pixel-density/unit information that may itself be identifying |
| Any other chunk | Record Unsupported handling and Partial coverage; refuse cleanup, regardless of safe-to-copy flag |
| Other valid image parameters | Set unsupported_image_parameters and Partial coverage; refuse cleanup |

In particular, eXIf (including orientation), iCCP, cHRM, cICP, other color/HDR
chunks, PLTE, tRNS, compressed/international text (zTXt/iTXt), APNG chunks
(acTL/fcTL/fdAT), unknown critical chunks and private extensions are **outside**
this profile. They are not silently discarded, flattened, recompressed or
reinterpreted. JPEG, PDF, archives and other formats return UnsupportedFormat.
Malformed recognized containers return Malformed; invalid or truncated input is
not an empty-finding result. Unsupported payload semantics are not validated.

## Transformation and verification

Selection uses two explicit booleans: text and modification_time. False retains
the corresponding chunks, including their private values. An all-false selection
returns a byte-identical candidate with zero removals, not a privacy improvement.

clean inspects the entire input, refuses Partial coverage, computes the exact
bounded output size, and allocates a separate result buffer. It copies the
signature and every non-selected chunk without altering them. It independently
runs inspect over the completed candidate bytes, requires the selected chunk
classes to be absent, and compares every retained wire region in order. Original
bytes remain borrowed and unchanged. Even when input/output share the same parser,
verification is a fresh parse, not the writer's removal list.

The guarantee is structural: selected tEXt/tIME containers absent and all retained
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
bytes during CRC/text scanning/copy/retained-byte comparison, across output-size
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

The five focused integration tests address original/retained-byte preservation
and explicit selection; refusing opaque/APNG regions; truncation/checksum/order
and trailing-byte failure; resource limits; cancellation at every callback reached
by the synthetic transformation, including its final success barrier. They do not
establish filesystem isolation, recovery, live-provider behavior or full A18.

The [synthetic hex fixture](../../crates/oia-png-privacy/tests/fixtures/rgba.hex)
is a 1x1 RGBA8 PNG with a split zlib stream, gAMA/sRGB/pHYs, two invented textual
properties and one embedded timestamp. Its scanline is the filter byte 0 followed
by RGBA (12, 34, 56, 255). It is generated fixture content, not an observation.
The image stream and chunk CRCs were generated with Python 3.12's standard zlib
and struct tooling, independently of the Rust implementation. Python is solely
a development/reference tool, not a production dependency.

Independent local validation uses Python struct/zlib plus Pillow 12.3.0 to parse
the original and actual Rust-produced candidate: compare original SHA-256 before
and after, every chunk CRC and retained wire byte, target absence and decoded
pixel equality. This is evidence for this synthetic fixture only, not complete
independent parser coverage for all admitted inputs or a perceptual proof.
The checker requires Python 3.12 and Pillow 12.3.0 in the development environment;
it executes the synthetic-only clean-fixture example and does not export user
files. The actual candidate is 132 bytes (original: 232), with the three selected
containers absent and both decoded pixels and all retained chunk bytes identical.

## Integration still required

Keep #46 open. #30/#22 and shared P0 services must supply immutable artifact IDs,
source/output hashes, authorization/workspace isolation, pinned plans, durable
cancellable jobs, alias/hardlink/symlink protection, private staging, atomic
provenance/output publication, recovery, export omission previews and privacy-safe
receipts. Add the format to an actual user-visible support list only when that
end-to-end path exists. No existing privacy contract or acceptance obligation is
waived by this standalone slice.
