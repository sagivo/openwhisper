#!/usr/bin/env python3
"""Generate tray-icon PNGs for OpenWhisper.

Produces two kinds of icons under src-tauri/icons/, each at 1x (22px) and 2x
(44px). All icons are opaque white on a transparent background so they work
as macOS "template images" (the OS auto-tints them — white in dark menu
bars, black in light menu bars).

- tray-mic-template.png / @2x.png:
    Idle waveform: five rounded vertical bars in a symmetric "voice peak"
    silhouette. Used whenever OpenWhisper is not recording.

- tray-mic-rec-NN.png / @2x.png  (NN = 00..FRAMES-1):
    Recording animation frames. Same five-bar geometry as the idle icon,
    but the bar heights vary per frame so the bars appear to "dance" when
    the frames are cycled at the tray. The frames are derived from a
    moving sine wave so the animation looks continuous.

The Rust tray code (`update_tray` in `lib.rs`) keeps the idle icon static
and cycles the rec frames while recording. Filenames are stable so the
embedded `include_bytes!` calls don't churn each time we tweak artwork.

Run from repo root:  python3 scripts/gen-tray-icons.py
"""

from __future__ import annotations

import math
from pathlib import Path
from PIL import Image, ImageDraw

OUT_DIR = Path(__file__).resolve().parents[1] / "src-tauri" / "icons"

WHITE = (255, 255, 255, 255)

# Number of recording animation frames. Keep small: more frames = bigger
# binary, and the menu-bar timer ticks ~12 fps so 8 frames already gives a
# smooth-looking ~1.5s loop.
FRAMES = 8

# Static idle pattern: symmetric, slightly tall-in-the-middle.
IDLE_HEIGHTS = [6.0, 12.0, 16.0, 12.0, 6.0]

# Min / max bar heights (in design units) for animated recording frames.
# Heights below MIN look like dropouts; above MAX clip the canvas.
REC_MIN_H = 4.0
REC_MAX_H = 18.0


def _draw_bars(
    size: int,
    heights: list[float],
    color: tuple[int, int, int, int],
) -> Image.Image:
    """Render `heights` (one per bar) as rounded vertical capsules.

    Geometry constants (bar width, gap, etc.) are kept identical between the
    idle icon and the animation frames so the animation feels like the same
    waveform dancing rather than a different glyph swapping in.
    """
    scale = 4  # supersample then downscale for cheap antialiasing
    big = Image.new("RGBA", (size * scale, size * scale), (0, 0, 0, 0))
    d = ImageDraw.Draw(big)

    s = (size * scale) / 22.0  # design grid is 22 units
    cx = (size * scale) / 2
    cy = (size * scale) / 2

    bar_w = 2.6 * s
    gap = 1.6 * s
    radius = bar_w / 2

    n = len(heights)
    total_w = n * bar_w + (n - 1) * gap
    start_x = cx - total_w / 2

    for i, h in enumerate(heights):
        bar_left = start_x + i * (bar_w + gap)
        bar_right = bar_left + bar_w
        bar_top = cy - (h * s) / 2
        bar_bot = cy + (h * s) / 2
        d.rounded_rectangle(
            [bar_left, bar_top, bar_right, bar_bot],
            radius=radius,
            fill=color,
        )

    return big.resize((size, size), Image.LANCZOS)


def _rec_heights(frame: int, total: int, num_bars: int = 5) -> list[float]:
    """Compute bar heights for animation frame `frame` of `total`.

    Each bar follows a sine wave with a small per-bar phase offset; the
    overall wave advances by one full cycle as `frame` walks 0..total.
    Result: when frames are played in order the bars look like they are
    rising and falling in a traveling-wave pattern rather than randomly
    flickering.
    """
    heights: list[float] = []
    for i in range(num_bars):
        # Per-bar phase offset so adjacent bars lag each other slightly,
        # plus the global animation phase from `frame`.
        phase = 2 * math.pi * (frame / total) + i * (2 * math.pi / num_bars)
        # sin(...) ∈ [-1, 1] -> normalize to [0, 1] then map to [MIN, MAX].
        unit = (math.sin(phase) + 1.0) / 2.0
        h = REC_MIN_H + unit * (REC_MAX_H - REC_MIN_H)
        heights.append(h)
    return heights


def main() -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)

    # Idle icon (used when not recording).
    for size, suffix in [(22, ""), (44, "@2x")]:
        path = OUT_DIR / f"tray-mic-template{suffix}.png"
        img = _draw_bars(size, IDLE_HEIGHTS, WHITE)
        img.save(path, format="PNG")
        print(f"wrote {path} ({size}x{size})")

    # Recording animation frames.
    for f in range(FRAMES):
        heights = _rec_heights(f, FRAMES)
        for size, suffix in [(22, ""), (44, "@2x")]:
            path = OUT_DIR / f"tray-mic-rec-{f:02d}{suffix}.png"
            img = _draw_bars(size, heights, WHITE)
            img.save(path, format="PNG")
            print(f"wrote {path} ({size}x{size})")


if __name__ == "__main__":
    main()
