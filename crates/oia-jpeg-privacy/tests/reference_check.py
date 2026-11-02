"""Independent check of the Rust JPEG cleaner on synthetic fixtures only.

Development tool: requires Python 3.12, Pillow 12.3.0 and the pinned Rust
1.99.0 toolchain. It runs the clean-fixture example (which embeds the
synthetic fixtures; no user file is read or exported), walks original and
candidate with a separately written marker parser, and decodes both with
Pillow's libjpeg.
"""

import hashlib
import io
from pathlib import Path
import struct
import subprocess

import PIL
from PIL import Image

PACKAGE = Path(__file__).resolve().parents[1]
FIXTURES = PACKAGE / "tests" / "fixtures"
EXIF_ORIENTATION = 0x0112
EXIF_ARTIST = 0x013B
TARGET_PREFIXES = {
    0xE1: (b"Exif\x00\x00", b"http://ns.adobe.com/xap/1.0/\x00"),
    0xED: (b"Photoshop 3.0\x00",),
}
SYNTHETIC_IDENTIFIERS = [b"Synthetic Bob", b"Synthetic Carol", b"Synthetic Dave",
                         b"Fixture, not an observation"]


def segments(data):
    """Return (marker, wire bytes) for each segment; a scan includes its data."""
    assert data[:2] == b"\xff\xd8"
    result = [(0xD8, data[:2])]
    offset = 2
    while True:
        start = offset
        assert data[offset] == 0xFF
        while data[offset] == 0xFF:
            offset += 1
        marker = data[offset]
        offset += 1
        if marker == 0xD9:
            result.append((marker, data[start:offset]))
            assert offset == len(data), "bytes after EOI"
            return result
        length, = struct.unpack_from(">H", data, offset)
        offset += length
        assert offset <= len(data)
        if marker == 0xDA:
            while True:
                byte = data[offset]
                if byte != 0xFF:
                    offset += 1
                    continue
                following = data[offset + 1]
                if following == 0x00 or 0xD0 <= following <= 0xD7:
                    offset += 2
                    continue
                break
        result.append((marker, data[start:offset]))


def is_target(marker, wire):
    if marker == 0xFE:
        return True
    payload = wire[wire.index(bytes([0xFF, marker])) + 4:]
    return any(payload.startswith(prefix) for prefix in TARGET_PREFIXES.get(marker, ()))


def decode(data):
    with Image.open(io.BytesIO(data)) as image:
        image.load()
        return image.size, image.mode, image.tobytes(), dict(image.getexif()), dict(image.info)


def check(name, example_args, target_count):
    fixture = FIXTURES / name
    original = bytes.fromhex(fixture.read_text(encoding="utf-8"))
    original_hash = hashlib.sha256(original).hexdigest()
    candidate = subprocess.run(
        ["cargo", "+1.99.0", "run", "--quiet", "--locked", "--offline", "--manifest-path",
         str(PACKAGE / "Cargo.toml"), "--example", "clean-fixture", "--", *example_args],
        check=True, stdout=subprocess.PIPE, timeout=120,
    ).stdout
    before = segments(original)
    after = segments(candidate)
    assert sum(is_target(m, w) for m, w in before) == target_count
    assert not any(is_target(m, w) for m, w in after)
    # Every retained segment, including entropy-coded data, is byte-identical and in order.
    assert after == [(m, w) for m, w in before if not is_target(m, w)]
    for identifier in SYNTHETIC_IDENTIFIERS:
        assert identifier not in candidate, identifier
    size, mode, pixels, exif, info = decode(original)
    c_size, c_mode, c_pixels, c_exif, c_info = decode(candidate)
    if name == "baseline.hex":
        # Pillow independently reads the default orientation the kernel relied on.
        assert exif.get(EXIF_ORIENTATION) == 1 and EXIF_ARTIST in exif
        assert "xmp" in info and "photoshop" in info
    assert "comment" in info
    assert c_exif == {}
    assert not {"comment", "xmp", "photoshop", "exif"} & set(c_info)
    assert (size, mode, pixels) == (c_size, c_mode, c_pixels) and size == (16, 16)
    reread = bytes.fromhex(fixture.read_text(encoding="utf-8"))
    assert hashlib.sha256(reread).hexdigest() == original_hash
    print(f"PASS synthetic {name}: {len(original)} -> {len(candidate)} bytes; "
          f"{target_count} target segments absent; {len(after)} retained segments "
          "and decoded pixels identical")
    print(f"Original SHA-256: {original_hash}")
    print(f"Candidate SHA-256: {hashlib.sha256(candidate).hexdigest()}")


def main():
    assert PIL.__version__ == "12.3.0", PIL.__version__
    check("baseline.hex", [], 4)
    check("progressive.hex", ["progressive"], 1)


if __name__ == "__main__":
    main()
