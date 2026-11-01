#!/usr/bin/env python3
"""Create disposable metadata fixtures (requires Pillow); never overwrite."""

import argparse
from pathlib import Path

from PIL import Image, ImageCms, PngImagePlugin


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    destination: Path = args.destination
    destination.mkdir(exist_ok=False)
    image = Image.new("RGB", (640, 480))
    image.putdata(
        [
            ((x * 7 + y) % 256, (y * 3) % 256, (x + y * 2) % 256)
            for y in range(480)
            for x in range(640)
        ]
    )
    # ICC creation timestamps can differ; retain originals and hashes.
    profile = ImageCms.ImageCmsProfile(ImageCms.createProfile("sRGB")).tobytes()
    exif = Image.Exif()
    exif[0x112] = 6
    exif[0x10F] = "Private camera manufacturer"
    exif[0x110] = "Private device model"
    exif[0x132] = "2025:01:02 03:04:05"
    exif[0x8825] = {1: "N", 2: (51.0, 30.0, 0.0), 3: "E", 4: (0.0, 7.0, 0.0)}
    for progressive in (False, True):
        filename = "progressive-private.jpg" if progressive else "private.jpg"
        image.save(
            destination / filename,
            quality=92,
            progressive=progressive,
            exif=exif,
            icc_profile=profile,
            comment=b"Private comment " * 100,
        )
    info = PngImagePlugin.PngInfo()
    info.add_text("Author", "Private author " * 100)
    info.add_text("Creation Time", "2025-01-02")
    info.add(b"gAMA", (45455).to_bytes(4, "big"))
    image.save(
        destination / "private.png",
        compress_level=1,
        pnginfo=info,
        exif=exif,
        icc_profile=profile,
    )
    alpha = image.convert("RGBA")
    alpha_row = bytes(x % 256 for x in range(image.width))
    alpha.putalpha(Image.frombytes("L", image.size, alpha_row * image.height))
    alpha.save(
        destination / "alpha-private.png",
        compress_level=1,
        pnginfo=info,
        icc_profile=profile,
    )


if __name__ == "__main__":
    main()
