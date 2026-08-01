#!/usr/bin/env python3
"""Generate placeholder icons for the Tauri app.

Creates:
  icons/32x32.png
  icons/128x128.png
  icons/128x128@2x.png  (256x256)
  icons/icon.icns        (macOS — minimal, may need real tooling later)
  icons/icon.ico         (Windows — multi-size)

The icon is a simple "K" monogram on a dark background. Replace with real
branding before release.
"""

from PIL import Image, ImageDraw, ImageFont
from pathlib import Path

ICONS_DIR = Path(__file__).parent.parent / "app" / "src-tauri" / "icons"
ICONS_DIR.mkdir(parents=True, exist_ok=True)

# Colors
BG = (30, 30, 46)       # dark slate
FG = (220, 220, 240)    # off-white
ACCENT = (126, 156, 216)  # soft blue


def draw_icon(size: int) -> Image.Image:
    """Draw the Knot 'K' monogram at the given size."""
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    # Rounded-rect background
    margin = max(1, size // 16)
    radius = max(2, size // 8)
    draw.rounded_rectangle(
        [margin, margin, size - margin, size - margin],
        radius=radius,
        fill=BG,
    )

    # Try to load a font; fall back to default if none available
    font = None
    font_paths = [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
        "/usr/share/fonts/truetype/freefont/FreeSansBold.ttf",
    ]
    for fp in font_paths:
        try:
            font = ImageFont.truetype(fp, size=int(size * 0.6))
            break
        except (OSError, IOError):
            continue
    if font is None:
        font = ImageFont.load_default()

    # Draw "K" centered
    text = "K"
    bbox = draw.textbbox((0, 0), text, font=font)
    text_w = bbox[2] - bbox[0]
    text_h = bbox[3] - bbox[1]
    x = (size - text_w) // 2 - bbox[0]
    y = (size - text_h) // 2 - bbox[1]
    draw.text((x, y), text, fill=FG, font=font)

    return img


def main() -> None:
    # PNG icons
    for name, size in [
        ("32x32.png", 32),
        ("128x128.png", 128),
        ("128x128@2x.png", 256),
    ]:
        img = draw_icon(size)
        img.save(ICONS_DIR / name, "PNG")
        print(f"  created {name} ({size}x{size})")

    # ICO (Windows) — multi-size
    ico_sizes = [16, 32, 48, 64, 128, 256]
    ico_images = [draw_icon(s) for s in ico_sizes]
    ico_images[0].save(
        ICONS_DIR / "icon.ico",
        format="ICO",
        sizes=[(s, s) for s in ico_sizes],
        append_images=ico_images[1:],
    )
    print(f"  created icon.ico (sizes: {ico_sizes})")

    # ICNS (macOS) — Pillow's ICNS support
    try:
        icns_img = draw_icon(512)
        icns_img.save(ICONS_DIR / "icon.icns", "ICNS")
        print("  created icon.icns (512x512)")
    except Exception as e:
        # ICNS requires special handling; create a placeholder PNG renamed
        # (Tauri will use png2icns or similar at bundle time on macOS)
        draw_icon(512).save(ICONS_DIR / "icon.icns", "PNG")
        print(f"  created icon.icns (placeholder PNG, real ICNS needs macOS tooling: {e})")

    print(f"\nAll icons generated in {ICONS_DIR}")


if __name__ == "__main__":
    main()
