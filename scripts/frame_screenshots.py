#!/usr/bin/env python3
"""Frame PNG screenshots as macOS-style windows for README use.

Usage: python3 scripts/frame_screenshots.py <in_dir> <out_dir> [--width 1600]

Each PNG in <in_dir> is scaled to --width, given rounded corners, macOS traffic
light buttons, a 1 px hairline border and a soft drop shadow on a transparent
canvas. Output keeps the file name. The result is quantized to 256 colours when
it stays visually close to the RGBA version (mean abs error over white <= 3.0),
otherwise it is saved as full RGBA.
"""
import argparse
import math
import os
import sys

from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageStat

SS = 4  # supersampling factor for antialiased shapes
MAE_LIMIT = 3.0
LIGHTS = ["#FF5F57", "#FEBC2E", "#28C840"]


def rr_mask(size, box, radius):
    """Antialiased rounded-rectangle alpha mask of `size`, filling `box` = (x0, y0, x1, y1)."""
    w, h = size
    x0, y0, x1, y1 = box
    m = Image.new("L", (w * SS, h * SS), 0)
    ImageDraw.Draw(m).rounded_rectangle(
        [x0 * SS, y0 * SS, x1 * SS - 1, y1 * SS - 1], radius=round(radius * SS), fill=255)
    return m.resize((w, h), Image.Resampling.BOX)


def mae(a, b):
    """Mean absolute RGB error after compositing both RGBA images over white."""
    bg = Image.new("RGBA", a.size, (255, 255, 255, 255))
    ra = Image.alpha_composite(bg, a).convert("RGB")
    rq = Image.alpha_composite(bg, b.convert("RGBA")).convert("RGB")
    return sum(ImageStat.Stat(ImageChops.difference(ra, rq)).mean) / 3


def traffic_lights(win, s):
    rb, ring, pad = 6 * s, 0.5 * s, 2
    for i, hx in enumerate(LIGHTS):
        cx, cy = (20 + 20 * i) * s, 20 * s
        base = tuple(int(hx[j:j + 2], 16) for j in (1, 3, 5))
        dark = tuple(round(c * 0.85) for c in base)
        x0, y0 = math.floor(cx - rb - pad), math.floor(cy - rb - pad)
        size = math.ceil(2 * rb + 2 * pad + 1)
        patch = Image.new("RGBA", (size * SS, size * SS), (0, 0, 0, 0))
        d = ImageDraw.Draw(patch)
        ox, oy = (cx - x0) * SS, (cy - y0) * SS
        for r, col in ((rb, dark), (rb - ring, base)):  # outer ring colour, then fill
            R = r * SS
            d.ellipse([ox - R, oy - R, ox + R, oy + R], fill=col + (255,))
        win.alpha_composite(patch.resize((size, size), Image.Resampling.BOX), (x0, y0))


def frame(path, width):
    s = width / 1440
    shot = Image.open(path).convert("RGBA")
    shot = Image.alpha_composite(Image.new("RGBA", shot.size, (255, 255, 255, 255)), shot).convert("RGB")
    height = round(shot.height * width / shot.width)
    win = shot.resize((width, height), Image.Resampling.LANCZOS).convert("RGBA")
    radius = 12 * s

    traffic_lights(win, s)

    outer = rr_mask((width, height), (0, 0, width, height), radius)
    inner = rr_mask((width, height), (1, 1, width - 1, height - 1), radius - 1)
    edge = Image.new("RGBA", (width, height), (0, 0, 0, 0))  # 1 px rgba(0,0,0,0.12) hairline
    edge.putalpha(ImageChops.subtract(outer, inner).point(lambda v: round(v * 0.12)))
    win = Image.alpha_composite(win, edge)
    win.putalpha(outer)

    m, dy = round(48 * s), round(14 * s)
    cw, ch = width + 2 * m, height + 2 * m
    shadow_a = Image.new("L", (cw, ch), 0)
    shadow_a.paste(outer, (m, m + dy))
    shadow_a = shadow_a.filter(ImageFilter.GaussianBlur(22 * s)).point(lambda v: round(v * 0.22))
    shadow_a = shadow_a.point(lambda v: v if v >= 4 else 0)  # invisible tails -> exact transparency
    canvas = Image.new("RGBA", (cw, ch), (0, 0, 0, 0))
    canvas.putalpha(shadow_a)
    canvas.alpha_composite(win, (m, m))
    return canvas


def to_output(canvas):
    """Quantize to 256 entries; the last entry is reserved as exact transparency."""
    q = canvas.quantize(colors=255, method=Image.Quantize.FASTOCTREE)
    clear = canvas.getchannel("A").point(lambda v: 255 if v == 0 else 0)
    q.paste(255, (0, 0), clear)
    pal = (list(q.getpalette("RGBA")) + [0] * 1024)[:1020] + [0, 0, 0, 0]
    q.putpalette(pal, "RGBA")
    err = mae(canvas, q)
    return (q, True, err) if err <= MAE_LIMIT else (canvas, False, err)


def main():
    ap = argparse.ArgumentParser(description="Frame screenshots in macOS-style windows.")
    ap.add_argument("in_dir")
    ap.add_argument("out_dir")
    ap.add_argument("--width", type=int, default=1600)
    args = ap.parse_args()
    if os.path.abspath(args.in_dir) == os.path.abspath(args.out_dir):
        sys.exit("out_dir must differ from in_dir")
    names = sorted(n for n in os.listdir(args.in_dir) if n.lower().endswith(".png"))
    if not names:
        sys.exit(f"no PNG files in {args.in_dir}")
    os.makedirs(args.out_dir, exist_ok=True)
    for name in names:
        img, quantized, err = to_output(frame(os.path.join(args.in_dir, name), args.width))
        out = os.path.join(args.out_dir, name)
        img.save(out, optimize=True)
        print(f"{name}  {os.path.getsize(out) / 1024:.0f} KB  quantized={quantized}  mae={err:.2f}")


if __name__ == "__main__":
    main()
