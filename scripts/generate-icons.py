#!/usr/bin/env python3
"""Draws the Safe Invest icon at every size the build needs.

Written with nothing but the standard library so the icon can be regenerated on
any machine — no image toolchain to install, and the icon is reviewable as code
rather than as an opaque binary.

    python3 scripts/generate-icons.py
"""

from __future__ import annotations

import struct
import zlib
from io import BytesIO
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "crates" / "app" / "icons"

# Deep indigo to violet, with a mint line: colourful without being childish,
# and it still reads at 16 px in a task bar.
TOP = (30, 27, 75)
BOTTOM = (76, 29, 149)
LINE = (52, 211, 153)
GLOW = (167, 243, 208)


def lerp(a: tuple[int, int, int], b: tuple[int, int, int], t: float) -> tuple[int, int, int]:
    return tuple(round(x + (y - x) * t) for x, y in zip(a, b))


def blend(dst: list[int], src: tuple[int, int, int], alpha: float) -> None:
    """Alpha-composites `src` over the RGBA pixel `dst` in place."""
    if alpha <= 0:
        return
    alpha = min(alpha, 1.0)
    existing = dst[3] / 255
    out_a = alpha + existing * (1 - alpha)
    if out_a <= 0:
        dst[:] = [0, 0, 0, 0]
        return
    for i in range(3):
        dst[i] = round((src[i] * alpha + dst[i] * existing * (1 - alpha)) / out_a)
    dst[3] = round(out_a * 255)


def coverage(distance: float, feather: float) -> float:
    """Anti-aliasing ramp: 1 well inside a shape, 0 well outside."""
    return max(0.0, min(1.0, 0.5 - distance / feather))


def rounded_square_distance(x: float, y: float, size: float, radius: float) -> float:
    """Signed distance to a rounded square covering the whole canvas."""
    half = size / 2
    dx = abs(x - half) - (half - radius)
    dy = abs(y - half) - (half - radius)
    if dx <= 0 and dy <= 0:
        return max(dx, dy)
    dx = max(dx, 0.0)
    dy = max(dy, 0.0)
    return (dx * dx + dy * dy) ** 0.5 - radius


def segment_distance(px: float, py: float, ax: float, ay: float, bx: float, by: float) -> float:
    vx, vy = bx - ax, by - ay
    wx, wy = px - ax, py - ay
    length = vx * vx + vy * vy
    t = 0.0 if length == 0 else max(0.0, min(1.0, (wx * vx + wy * vy) / length))
    cx, cy = ax + t * vx, ay + t * vy
    return ((px - cx) ** 2 + (py - cy) ** 2) ** 0.5


def draw(size: int) -> bytes:
    """Renders one square icon and returns it as PNG bytes."""
    scale = size / 256
    feather = max(1.0, 1.2 * scale)
    radius = 56 * scale
    stroke = max(1.6, 18 * scale)

    # The rising line: four points, climbing left to right with one dip, so the
    # shape reads as "a portfolio growing" rather than a generic zigzag.
    path = [(46, 176), (100, 128), (146, 158), (212, 66)]
    points = [(x * scale, y * scale) for x, y in path]

    pixels = [[0, 0, 0, 0] for _ in range(size * size)]

    for y in range(size):
        for x in range(size):
            px, py = x + 0.5, y + 0.5
            pixel = pixels[y * size + x]

            # Background plate.
            plate = coverage(rounded_square_distance(px, py, size, radius), feather)
            if plate > 0:
                blend(pixel, lerp(TOP, BOTTOM, py / size), plate)

            if plate <= 0:
                continue

            # The line, drawn as a union of capsules.
            nearest = min(
                segment_distance(px, py, *points[i], *points[i + 1])
                for i in range(len(points) - 1)
            )
            blend(pixel, LINE, coverage(nearest - stroke / 2, feather) * plate)

            # A soft halo so the line still separates from the plate at 16 px.
            halo = coverage(nearest - stroke, feather * 3) * 0.28
            blend(pixel, GLOW, halo * plate)

            # The end-point marker: where the portfolio got to.
            tip = ((px - points[-1][0]) ** 2 + (py - points[-1][1]) ** 2) ** 0.5
            blend(pixel, GLOW, coverage(tip - 15 * scale, feather) * plate)

    raw = bytearray()
    for y in range(size):
        raw.append(0)  # PNG filter: none
        for x in range(size):
            raw.extend(pixels[y * size + x])

    return encode_png(size, bytes(raw))


def encode_png(size: int, raw: bytes) -> bytes:
    def chunk(tag: bytes, data: bytes) -> bytes:
        body = tag + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    header = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)  # 8-bit RGBA
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def encode_ico(frames: dict[int, bytes]) -> bytes:
    """Packs PNG frames into an .ico, the form Windows wants for a resource."""
    sizes = sorted(frames)
    out = BytesIO()
    out.write(struct.pack("<HHH", 0, 1, len(sizes)))

    offset = 6 + 16 * len(sizes)
    for size in sizes:
        data = frames[size]
        out.write(
            struct.pack(
                "<BBBBHHII",
                0 if size >= 256 else size,  # 0 means 256
                0 if size >= 256 else size,
                0,
                0,
                1,
                32,
                len(data),
                offset,
            )
        )
        offset += len(data)

    for size in sizes:
        out.write(frames[size])
    return out.getvalue()


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    frames = {size: draw(size) for size in (16, 32, 48, 64, 128, 256)}

    (OUT / "icon.png").write_bytes(frames[256])
    (OUT / "128x128.png").write_bytes(frames[128])
    (OUT / "32x32.png").write_bytes(frames[32])
    (OUT / "icon.ico").write_bytes(encode_ico({s: frames[s] for s in (16, 32, 48, 64, 128, 256)}))

    for name in ("icon.png", "128x128.png", "32x32.png", "icon.ico"):
        print(f"  {name:14} {(OUT / name).stat().st_size:>7} octets")


if __name__ == "__main__":
    main()
