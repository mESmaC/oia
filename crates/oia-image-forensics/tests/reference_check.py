"""Independent synthetic raster/CLI checks using the pinned fixture Pillow.

Build with cargo build -p oia-image-forensics --locked first. No network,
user images, retained fixture files, or detector accuracy claims.
"""

import hashlib
import json
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import zlib

import PIL
from PIL import Image, PngImagePlugin

ROOT = Path(__file__).resolve().parents[3]


def png_chunk(kind, payload):
    return (struct.pack(">I", len(payload)) + kind + payload
            + struct.pack(">I", zlib.crc32(kind + payload)))


def png_chunks(encoded):
    position = 8
    while position < len(encoded):
        length, = struct.unpack_from(">I", encoded, position)
        yield encoded[position + 4:position + 8], encoded[position + 8:position + 8 + length]
        position += length + 12


def invoke(binary, source, *options, expected=0):
    process = subprocess.run(
        [str(binary), str(source), *map(str, options)],
        capture_output=True, text=True, check=False, timeout=30,
    )
    assert process.returncode == expected, (process.returncode, process.stderr)
    if expected == 1:
        assert not process.stdout, process.stdout
        assert process.stderr
        return None
    assert not process.stderr, process.stderr
    return json.loads(process.stdout)


def check(binary):
    assert PIL.__version__ == "12.3.0", PIL.__version__
    with tempfile.TemporaryDirectory(prefix="oia-forensics-") as directory:
        temporary = Path(directory)
        raster = Image.new("RGB", (32, 16))
        raster.putdata([
            ((column % 16) * 15, row * 15, ((column % 16) + row) * 7)
            for row in range(16) for column in range(32)
        ])
        source = temporary / "declared.png"
        metadata = PngImagePlugin.PngInfo()
        metadata.add_text("parameters", "private prompt; Steps: 20, Sampler: Euler, Seed: 42")
        raster.save(source, pnginfo=metadata)
        original = source.read_bytes()
        report = invoke(binary, source)
        assert report["source"]["sha256"] == hashlib.sha256(original).hexdigest()
        assert source.read_bytes() == original
        assert report["assessment"] == "inconclusive"
        assert report["learned_ai_detection"]["coverage"] == "not_implemented"
        assert "private prompt" not in json.dumps(report)
        assert report["schema_version"] == 2
        assert all("JPEG decoder" not in limitation
                   for limitation in report["raster_validation"]["limitations"])
        assert report["rgb_histograms"]["coverage"] == "complete"
        assert report["rgb_histograms"]["result"] == [
            raster.histogram()[start:start + 256] for start in (0, 256, 512)
        ]
        cue, = report["metadata"]["result"]["cues"]
        payload = original[cue["byte_offset"]:cue["byte_offset"] + cue["byte_length"]]
        assert cue["payload_sha256"] == hashlib.sha256(payload).hexdigest()
        assert report["repeated_blocks"]["result"]["candidates"] == [
            {"first": {"x": 0, "y": 0}, "repeated": {"x": 16, "y": 0}}
        ]
        partial = invoke(binary, source, "--max-blocks", 1, expected=2)
        assert partial["repeated_blocks"]["coverage"] == "partial"
        invoke(binary, source, "--max-pixels", 10, expected=1)
        invoke(binary, source, "--max-input-bytes", 10, expected=1)

        raw = b"".join(b"\0" + raster.crop((0, row, 32, row + 1)).tobytes() for row in range(16))
        compressed = zlib.compress(raw)
        header = png_chunk(b"IHDR", struct.pack(">IIBBBBB", 32, 16, 8, 2, 0, 0, 0))
        bad_adler = compressed[:-1] + bytes([compressed[-1] ^ 1])
        unfinished = b"\x78\x01\x00" + struct.pack("<HH", len(raw), 65535 - len(raw)) + raw
        for name, stream, expected in [
            ("split-idat", compressed, 0),
            ("bad-adler", bad_adler, 1),
            ("missing-adler", compressed[:-4], 1),
            ("no-final-deflate", unfinished, 1),
            ("excess-inflated", zlib.compress(raw * 2), 1),
            ("trailing-zlib", compressed + b"\0", 1),
        ]:
            malformed = temporary / (name + ".png")
            malformed.write_bytes(
                b"\x89PNG\r\n\x1a\n" + header
                + png_chunk(b"IDAT", stream[:3]) + png_chunk(b"IDAT", stream[3:])
                + png_chunk(b"IEND", b"")
            )
            invoke(binary, malformed, expected=expected)

        ancillary = [
            (b"pHYs", struct.pack(">IIB", 3000, 3000, 1)),
            (b"sRGB", b"\0"),
            (b"gAMA", struct.pack(">I", 45455)),
            (b"tIME", struct.pack(">HBBBBB", 2026, 11, 2, 12, 0, 0)),
            (b"bKGD", struct.pack(">HHH", 1, 2, 3)),
        ]
        auxiliary = temporary / "ancillary.png"
        auxiliary.write_bytes(b"\x89PNG\r\n\x1a\n" + header
                              + b"".join(png_chunk(kind, payload) for kind, payload in ancillary)
                              + png_chunk(b"IDAT", compressed) + png_chunk(b"IEND", b""))
        omitted = invoke(binary, auxiliary, expected=2)["metadata"]["result"]["uninspected"]
        assert {field["container"] for field in omitted} == {"pHYs", "sRGB", "gAMA", "tIME", "bKGD"}
        assert all("fixed-layout ancillary metadata" in field["reason"] for field in omitted)
        time_reason = next(field["reason"] for field in omitted if field["container"] == "tIME")
        assert "tIME modification time" in time_reason
        assert "not a generation/edit text field" not in time_reason
        multi_buffer = Image.frombytes(
            "RGBA", (65, 65), hashlib.shake_256(b"oia synthetic stream-boundary fixture").digest(65 * 65 * 4)
        )
        streamed = temporary / "stream-boundaries.png"
        multi_buffer.save(streamed)
        assert sum(len(payload) for kind, payload in png_chunks(streamed.read_bytes())
                   if kind == b"IDAT") > 8192
        counts = invoke(binary, streamed)["rgb_histograms"]["result"]
        assert counts == [multi_buffer.histogram()[start:start + 256] for start in (0, 256, 512)]

        for name, image, progressive in [
            ("baseline", raster, False),
            ("progressive", raster, True),
            ("grayscale", raster.convert("L"), False),
        ]:
            jpeg = temporary / (name + ".jpg")
            image.save(jpeg, quality=87, progressive=progressive)
            retained = jpeg.read_bytes()
            residual = temporary / (name + "-residual.png")
            jpeg_report = invoke(binary, jpeg, "--residual-map", residual, expected=2)
            assert jpeg_report["raster_validation"]["coverage"] == "partial"
            reason = jpeg_report["raster_validation"]["result"]
            for method in ("rgb_histograms", "repeated_blocks", "jpeg_recompression"):
                assert jpeg_report[method]["coverage"] == "partial"
                assert reason in jpeg_report[method]["limitations"]
            assert jpeg.read_bytes() == retained
            summary = jpeg_report["jpeg_recompression"]["result"]
            assert sum(summary["channel_difference_histogram"]) == 32 * 16 * 3
            with Image.open(residual) as derivative:
                assert len([key for key in derivative.info if key.startswith("oia:")]) == 5
                derivative.load()
                assert derivative.size == (32, 16)
                assert derivative.mode == "RGB"
                assert derivative.info["oia:source_sha256"] == hashlib.sha256(retained).hexdigest()
                assert derivative.info["oia:profile"] == jpeg_report["profile"]
                assert "not an observation" in derivative.info["oia:role"]
                assert "decoder recovery possible" in derivative.info["oia:input_validation"]
            assert summary["artifact"]["sha256"] == hashlib.sha256(residual.read_bytes()).hexdigest()
            saved_map = residual.read_bytes()
            early_labels = set()
            for kind, payload in png_chunks(saved_map):
                if kind == b"IDAT":
                    break
                if kind == b"tEXt":
                    early_labels.add(payload.split(b"\0", 1)[0])
            assert early_labels == {b"oia:role", b"oia:source_sha256", b"oia:profile",
                                    b"oia:settings", b"oia:input_validation"}
            invoke(binary, jpeg, "--residual-map", residual, expected=1)
            assert residual.read_bytes() == saved_map
            invoke(binary, jpeg, "--residual-map", jpeg, expected=1)
            assert jpeg.read_bytes() == retained
            if name == "baseline":
                jfxx_payload = b"JFXX\0\x13\x01\x01\x10\x20\x30"
                jfxx = temporary / "thumbnail.jpg"
                jfxx.write_bytes(retained[:2] + b"\xff\xe0"
                                 + struct.pack(">H", len(jfxx_payload) + 2)
                                 + jfxx_payload + retained[2:])
                fields = invoke(binary, jfxx, expected=2)["metadata"]["result"]["uninspected"]
                assert any(field["container"] == "JPEG marker FFE0"
                           and field["byte_length"] == len(jfxx_payload)
                           and "thumbnails" in field["reason"] for field in fields)
                exaggerated = bytearray(retained)
                frame = exaggerated.index(b"\xff\xc0")
                struct.pack_into(">HH", exaggerated, frame + 5, 2000, 2000)
                grossly_truncated = temporary / "exaggerated-frame.jpg"
                grossly_truncated.write_bytes(exaggerated)
                invoke(binary, grossly_truncated, expected=1)
                extended = bytearray(exaggerated)
                extended[frame + 1] = 0xc1
                exaggerated_extended = temporary / "exaggerated-extended-frame.jpg"
                exaggerated_extended.write_bytes(extended)
                invoke(binary, exaggerated_extended, expected=1)
                scan = retained.index(b"\xff\xda")
                length, = struct.unpack_from(">H", retained, scan + 2)
                entropy_start = scan + 2 + length
                truncated = temporary / "truncated-entropy.jpg"
                truncated.write_bytes(retained[:entropy_start + 2] + b"\xff\xd9")
                recovered = invoke(binary, truncated, expected=2)
                assert recovered["raster_validation"]["coverage"] == "partial"
                assert "decoder recovery possible" in recovered["raster_validation"]["result"]
                for method in ("rgb_histograms", "repeated_blocks", "jpeg_recompression"):
                    assert recovered[method]["coverage"] == "partial"
                    assert recovered["raster_validation"]["result"] in recovered[method]["limitations"]
    print("PASS: independent Pillow samples; PNG zlib integrity/refusals and format-specific limitations; JPEG coverage propagation and SOF0/SOF1 gross truncation refusal; APP0/ancillary omissions; pre-IDAT labels; source/overwrite preservation")


if __name__ == "__main__":
    binary = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else (
        ROOT / "target" / "debug" / ("oia-image-forensics.exe" if sys.platform == "win32" else "oia-image-forensics")
    )
    check(binary)
