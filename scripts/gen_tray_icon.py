"""Generate a macOS template-style microphone tray icon.

Produces icons/tray-icon.png (22x22 pt logical) and icons/tray-icon@2x.png (44x44).
Template icons must be black on transparent; macOS recolors them automatically.
"""
from PIL import Image, ImageDraw
from pathlib import Path

OUT_DIR = Path(__file__).resolve().parent.parent / "src-tauri" / "icons"
OUT_DIR.mkdir(parents=True, exist_ok=True)


def draw_mic(size: int) -> Image.Image:
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    s = size / 22.0  # design unit

    cx = size / 2
    cap_w = 8 * s
    cap_h = 12 * s
    cap_top = 2 * s
    cap_left = cx - cap_w / 2
    cap_right = cx + cap_w / 2
    cap_bottom = cap_top + cap_h
    d.rounded_rectangle(
        [cap_left, cap_top, cap_right, cap_bottom],
        radius=cap_w / 2,
        fill=(0, 0, 0, 255),
    )

    arc_w = 14 * s
    arc_h = 14 * s
    arc_left = cx - arc_w / 2
    arc_top = cap_top + cap_h / 2 - arc_h / 2 + 1 * s
    arc_right = cx + arc_w / 2
    arc_bottom = arc_top + arc_h
    stroke = max(1, int(round(1.6 * s)))
    d.arc(
        [arc_left, arc_top, arc_right, arc_bottom],
        start=20,
        end=160,
        fill=(0, 0, 0, 255),
        width=stroke,
    )

    stem_top = arc_bottom - stroke / 2 - 1 * s
    stem_bottom = stem_top + 3 * s
    stem_half = max(1, int(round(0.9 * s)))
    d.rectangle(
        [cx - stem_half, stem_top, cx + stem_half, stem_bottom],
        fill=(0, 0, 0, 255),
    )

    base_w = 6 * s
    base_y = stem_bottom
    base_h = max(1, int(round(1.6 * s)))
    d.rounded_rectangle(
        [cx - base_w / 2, base_y, cx + base_w / 2, base_y + base_h],
        radius=base_h / 2,
        fill=(0, 0, 0, 255),
    )
    return img


def main() -> None:
    big = draw_mic(88)
    small = big.resize((44, 44), Image.LANCZOS)
    tiny = big.resize((22, 22), Image.LANCZOS)

    tiny.save(OUT_DIR / "tray-icon.png")
    small.save(OUT_DIR / "tray-icon@2x.png")
    print(f"wrote {OUT_DIR / 'tray-icon.png'} and tray-icon@2x.png")


if __name__ == "__main__":
    main()
