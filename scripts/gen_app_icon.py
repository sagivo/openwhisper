"""Generate a 1024x1024 macOS app icon source.

Produces src-tauri/icons/app-icon-source.png. Run `npx tauri icon
src-tauri/icons/app-icon-source.png` afterward to generate the full
.icns / .ico / mip-mapped png set.
"""
from PIL import Image, ImageDraw, ImageFilter
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "src-tauri" / "icons" / "app-icon-source.png"
OUT.parent.mkdir(parents=True, exist_ok=True)

SIZE = 1024


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(len(a)))


def make_background(size: int) -> Image.Image:
    bg = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    grad = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    top = (108, 99, 255)        # indigo-ish
    bot = (32, 24, 92)           # deep navy
    px = grad.load()
    for y in range(size):
        t = y / (size - 1)
        c = lerp(top, bot, t) + (255,)
        for x in range(size):
            px[x, y] = c

    mask = Image.new("L", (size, size), 0)
    md = ImageDraw.Draw(mask)
    radius = int(size * 0.225)
    md.rounded_rectangle([0, 0, size, size], radius=radius, fill=255)
    bg.paste(grad, (0, 0), mask)

    glow = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    gd = ImageDraw.Draw(glow)
    cx = size * 0.32
    cy = size * 0.28
    r = size * 0.55
    gd.ellipse([cx - r, cy - r, cx + r, cy + r], fill=(255, 255, 255, 70))
    glow = glow.filter(ImageFilter.GaussianBlur(size * 0.08))
    bg.alpha_composite(glow, (0, 0))

    bg.putalpha(mask)
    return bg


def draw_mic(size: int) -> Image.Image:
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    s = size / 22.0  # reuse the tray-icon design grid

    cx = size / 2
    cap_w = 8.6 * s
    cap_h = 12.4 * s
    cap_top = 3.2 * s
    cap_left = cx - cap_w / 2
    cap_right = cx + cap_w / 2
    cap_bottom = cap_top + cap_h
    d.rounded_rectangle(
        [cap_left, cap_top, cap_right, cap_bottom],
        radius=cap_w / 2,
        fill=(255, 255, 255, 255),
    )

    arc_w = 14.6 * s
    arc_h = 14.6 * s
    arc_left = cx - arc_w / 2
    arc_top = cap_top + cap_h / 2 - arc_h / 2 + 1 * s
    arc_right = cx + arc_w / 2
    arc_bottom = arc_top + arc_h
    stroke = max(2, int(round(1.7 * s)))
    d.arc(
        [arc_left, arc_top, arc_right, arc_bottom],
        start=20,
        end=160,
        fill=(255, 255, 255, 255),
        width=stroke,
    )

    stem_top = arc_bottom - stroke / 2 - 1 * s
    stem_bottom = stem_top + 3.2 * s
    stem_half = max(2, int(round(0.95 * s)))
    d.rectangle(
        [cx - stem_half, stem_top, cx + stem_half, stem_bottom],
        fill=(255, 255, 255, 255),
    )

    base_w = 6.8 * s
    base_y = stem_bottom
    base_h = max(2, int(round(1.7 * s)))
    d.rounded_rectangle(
        [cx - base_w / 2, base_y, cx + base_w / 2, base_y + base_h],
        radius=base_h / 2,
        fill=(255, 255, 255, 255),
    )

    return img


def main() -> None:
    canvas = make_background(SIZE)

    mic_layer = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    mic = draw_mic(int(SIZE * 0.78))
    offset = ((SIZE - mic.width) // 2, (SIZE - mic.height) // 2)
    mic_layer.alpha_composite(mic, offset)

    shadow = mic_layer.split()[-1]
    shadow_img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    shadow_img.paste((0, 0, 0, 110), (0, int(SIZE * 0.012)), shadow)
    shadow_img = shadow_img.filter(ImageFilter.GaussianBlur(SIZE * 0.012))

    canvas.alpha_composite(shadow_img)
    canvas.alpha_composite(mic_layer)

    canvas.save(OUT, format="PNG")
    print(f"wrote {OUT} ({SIZE}x{SIZE})")


if __name__ == "__main__":
    main()
