"""Regenerate the synthetic JPEG fixtures (development tool, not production).

Requires Python 3.12 and Pillow 12.3.0. Pillow's bundled libjpeg encodes a
16x16 image of four flat 8x8 colour blocks; Python struct then inserts
invented metadata segments after JFIF APP0. All names and values are
invented fixture content, not observations. The committed .hex files are the
canonical fixtures; re-running this with another libjpeg build may produce
different entropy-coded bytes.
"""

import io
from pathlib import Path
import struct

import PIL
from PIL import Image

FIXTURES = Path(__file__).resolve().parent / "fixtures"
COLOURS = [(12, 34, 56), (200, 40, 40), (40, 200, 40), (240, 240, 240)]


def segment(marker, payload):
    assert len(payload) + 2 <= 0xFFFF
    return bytes([0xFF, marker]) + struct.pack(">H", len(payload) + 2) + payload


def exif_payload():
    # Big-endian TIFF; IFD0: Orientation (SHORT, 1) = 1, Artist (ASCII, 14).
    artist = b"Synthetic Bob\x00"
    ifd_offset = 8
    entries = 2
    value_offset = ifd_offset + 2 + entries * 12 + 4
    tiff = b"MM\x00\x2a" + struct.pack(">I", ifd_offset)
    tiff += struct.pack(">H", entries)
    tiff += struct.pack(">HHIHH", 0x0112, 3, 1, 1, 0)
    tiff += struct.pack(">HHII", 0x013B, 2, len(artist), value_offset)
    tiff += struct.pack(">I", 0) + artist
    return b"Exif\x00\x00" + tiff


def xmp_payload():
    packet = (
        '<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF '
        'xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">'
        '<rdf:Description xmlns:dc="http://purl.org/dc/elements/1.1/">'
        "<dc:creator>Synthetic Carol</dc:creator>"
        "</rdf:Description></rdf:RDF></x:xmpmeta>"
    ).encode("utf-8")
    return b"http://ns.adobe.com/xap/1.0/\x00" + packet


def photoshop_payload():
    # One IPTC-NAA resource (0x0404) holding dataset 2:80 By-line.
    iptc = b"\x1c\x02\x50" + struct.pack(">H", 14) + b"Synthetic Dave"
    block = b"8BIM" + struct.pack(">H", 0x0404) + b"\x00\x00"
    block += struct.pack(">I", len(iptc)) + iptc
    if len(iptc) % 2:
        block += b"\x00"
    return b"Photoshop 3.0\x00" + block


def encode(progressive):
    image = Image.new("RGB", (16, 16))
    for index, colour in enumerate(COLOURS):
        x, y = (index % 2) * 8, (index // 2) * 8
        image.paste(colour, (x, y, x + 8, y + 8))
    out = io.BytesIO()
    image.save(out, "JPEG", quality=90, subsampling=0, progressive=progressive)
    return out.getvalue()


def after_jfif(data, inserted):
    assert data[:4] == b"\xff\xd8\xff\xe0"
    end = 4 + struct.unpack_from(">H", data, 4)[0]
    return data[:end] + inserted + data[end:]


def main():
    assert PIL.__version__ == "12.3.0", PIL.__version__
    comment = segment(0xFE, b"Fixture, not an observation")
    baseline = after_jfif(
        encode(False),
        segment(0xE1, exif_payload())
        + segment(0xE1, xmp_payload())
        + segment(0xED, photoshop_payload())
        + comment,
    )
    progressive = after_jfif(encode(True), comment)
    for name, data in [("baseline.hex", baseline), ("progressive.hex", progressive)]:
        (FIXTURES / name).write_text(data.hex() + "\n", encoding="utf-8", newline="\n")
        print(f"{name}: {len(data)} bytes")


if __name__ == "__main__":
    main()
