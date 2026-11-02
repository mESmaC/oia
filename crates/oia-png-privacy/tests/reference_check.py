import hashlib
import io
from pathlib import Path
import struct
import subprocess
import zlib

from PIL import Image


PACKAGE = Path(__file__).resolve().parents[1]
FIXTURES = PACKAGE / "tests" / "fixtures"
EXIF_ORIENTATION = 0x0112
EXIF_ARTIST = 0x013B


def chunks(data):
    assert data[:8] == b"\x89PNG\r\n\x1a\n"
    offset = 8
    result = []
    while offset < len(data):
        length, kind = struct.unpack_from(">I4s", data, offset)
        end = offset + 12 + length
        assert end <= len(data)
        payload = data[offset + 8:end - 4]
        checksum, = struct.unpack_from(">I", data, end - 4)
        assert checksum == zlib.crc32(kind + payload)
        result.append((kind, payload, data[offset:end]))
        offset = end
    assert result[0][0] == b"IHDR" and result[-1][0] == b"IEND"
    assert result[-1][1] == b""
    return result


def pixels(data):
    with Image.open(io.BytesIO(data)) as image:
        image.verify()
    with Image.open(io.BytesIO(data)) as image:
        assert image.size == (1, 1) and image.mode == "RGBA"
        return image.tobytes()


def exif(data):
    with Image.open(io.BytesIO(data)) as image:
        return dict(image.getexif())


def check(name, example_args, target_count):
    fixture = FIXTURES / name
    original = bytes.fromhex(fixture.read_text(encoding="utf-8"))
    original_hash = hashlib.sha256(original).hexdigest()
    candidate = subprocess.run(
        ["cargo", "+1.99.0", "run", "--quiet", "--locked", "--manifest-path",
         str(PACKAGE / "Cargo.toml"), "--example", "clean-fixture", "--",
         *example_args],
        check=True, stdout=subprocess.PIPE, timeout=120,
    ).stdout
    before = chunks(original)
    after = chunks(candidate)
    targets = {b"tEXt", b"tIME", b"eXIf"}
    assert sum(kind in targets for kind, _, _ in before) == target_count
    if b"eXIf" in {kind for kind, _, _ in before}:
        # Pillow independently reads the default orientation the kernel relied on.
        tags = exif(original)
        assert tags.get(EXIF_ORIENTATION) == 1 and EXIF_ARTIST in tags
    assert exif(candidate) == {}
    assert not any(kind in targets for kind, _, _ in after)
    assert after == [chunk for chunk in before if chunk[0] not in targets]
    compressed = b"".join(payload for kind, payload, _ in after if kind == b"IDAT")
    assert zlib.decompress(compressed) == bytes([0, 12, 34, 56, 255])
    assert pixels(original) == pixels(candidate) == bytes([12, 34, 56, 255])
    reread = bytes.fromhex(fixture.read_text(encoding="utf-8"))
    assert hashlib.sha256(reread).hexdigest() == original_hash
    print(f"PASS synthetic {name}: {len(original)} -> {len(candidate)} bytes; "
          f"{target_count} target chunks absent; retained wire bytes, CRCs "
          "and pixels identical")
    print(f"Original SHA-256: {original_hash}")
    print(f"Candidate SHA-256: {hashlib.sha256(candidate).hexdigest()}")
    return candidate


def main():
    plain = check("rgba.hex", [], 3)
    with_exif = check("rgba-exif.hex", ["exif"], 4)
    assert plain == with_exif


if __name__ == "__main__":
    main()
