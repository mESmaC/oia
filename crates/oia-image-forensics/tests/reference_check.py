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

import PIL
from PIL import Image, PngImagePlugin

ROOT = Path(__file__).resolve().parents[3]


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
        assert report["rgb_histograms"] == [
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
            assert jpeg.read_bytes() == retained
            summary = jpeg_report["jpeg_recompression"]["result"]
            assert sum(summary["channel_difference_histogram"]) == 32 * 16 * 3
            with Image.open(residual) as derivative:
                derivative.load()
                assert derivative.size == (32, 16)
                assert derivative.mode == "RGB"
                assert derivative.info["oia:source_sha256"] == hashlib.sha256(retained).hexdigest()
                assert derivative.info["oia:profile"] == jpeg_report["profile"]
                assert "not an observation" in derivative.info["oia:role"]
                assert "decoder recovery possible" in derivative.info["oia:input_validation"]
            assert summary["artifact"]["sha256"] == hashlib.sha256(residual.read_bytes()).hexdigest()
            saved_map = residual.read_bytes()
            invoke(binary, jpeg, "--residual-map", residual, expected=1)
            assert residual.read_bytes() == saved_map
            invoke(binary, jpeg, "--residual-map", jpeg, expected=1)
            assert jpeg.read_bytes() == retained
            if name == "baseline":
                scan = retained.index(b"\xff\xda")
                length, = struct.unpack_from(">H", retained, scan + 2)
                entropy_start = scan + 2 + length
                truncated = temporary / "truncated-entropy.jpg"
                truncated.write_bytes(retained[:entropy_start + 2] + b"\xff\xd9")
                recovered = invoke(binary, truncated, expected=2)
                assert recovered["raster_validation"]["coverage"] == "partial"
                assert "decoder recovery possible" in recovered["raster_validation"]["result"]
    print("PASS: independent Pillow PNG samples; baseline/progressive/grayscale JPEG; labelled residuals; source/overwrite preservation; partial/error CLI; explicit unverified JPEG entropy")


if __name__ == "__main__":
    binary = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else (
        ROOT / "target" / "debug" / ("oia-image-forensics.exe" if sys.platform == "win32" else "oia-image-forensics")
    )
    check(binary)
