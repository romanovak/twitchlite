"""Regenera los iconos de TwitchLite. Requiere Python y Pillow; no se ejecuta en la app."""
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / "assets"
ASSETS.mkdir(exist_ok=True)
PURPLE = "#9146FF"
NEON = "#39FF14"
INK = "#101014"
SCALE = 16

# Geometría del Glitch de Twitch, en su cuadrícula original de 24 unidades.
OUTER = [(6, 0), (1.714, 4.286), (1.714, 19.714), (6.857, 19.714),
         (6.857, 24), (11.143, 19.714), (14.571, 19.714), (22.286, 12), (22.286, 0)]
FACE = [(6.857, 1.714), (20.571, 1.714), (20.571, 11.143), (17.143, 14.571),
        (13.714, 14.571), (10.714, 17.571), (10.714, 14.571), (6.857, 14.571)]

def font(size):
    for path in [Path("C:/Windows/Fonts/seguisb.ttf"), Path("C:/Windows/Fonts/segoeuib.ttf")]:
        if path.exists():
            return ImageFont.truetype(str(path), round(size))
    raise RuntimeError("Se necesita Segoe UI Semibold o Bold para regenerar la marca.")

def point(p):
    return tuple((v * (56 / 24) + 2) * SCALE for v in p)

def box(rect):
    return tuple(round(v * SCALE) for v in rect)

master = Image.new("RGBA", (64 * SCALE, 64 * SCALE))
draw = ImageDraw.Draw(master)
draw.polygon([point(p) for p in OUTER], fill=PURPLE)
draw.polygon([point(p) for p in FACE], fill="white")
for left in [11.571, 16.286]:
    draw.rectangle((*point((left, 4.714)), *point((left + 1.714, 9.857))), fill=PURPLE)
# El distintivo ocupa la esquina libre del Glitch y conserva visibles sus ojos y silueta.
draw.rounded_rectangle(box((27, 44, 63, 63)), radius=4 * SCALE, fill=INK,
                       outline="#244329", width=SCALE)
draw.text((45 * SCALE, 53.5 * SCALE), "lite", font=font(15 * SCALE), fill=NEON, anchor="mm")

for size in [32, 64, 256]:
    master.resize((size, size), Image.Resampling.LANCZOS).save(ASSETS / f"twitchlite-{size}.png", optimize=True)
master.resize((256, 256), Image.Resampling.LANCZOS).save(
    ASSETS / "twitchlite.ico", sizes=[(s, s) for s in [16, 20, 24, 32, 40, 48, 64, 128, 256]])

def polygon(points):
    return " ".join(f"{x:g},{y:g}" for x, y in points)

svg = f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" role="img" aria-label="TwitchLite">
  <title>TwitchLite</title>
  <g transform="translate(2 2) scale({56/24:g})">
    <polygon points="{polygon(OUTER)}" fill="{PURPLE}"/>
    <polygon points="{polygon(FACE)}" fill="#fff"/>
    <path d="M11.571 4.714h1.714v5.143h-1.714zM16.286 4.714H18v5.143h-1.714z" fill="{PURPLE}"/>
  </g>
  <rect x="27" y="44" width="36" height="19" rx="4" fill="{INK}" stroke="#244329"/>
  <text x="45" y="59" text-anchor="middle" font-family="Segoe UI, sans-serif" font-size="15" font-weight="600" fill="{NEON}">lite</text>
</svg>
'''
(ASSETS / "twitchlite.svg").write_text(svg, encoding="utf-8")

# Lámina de revisión local; los archivos que consume la app están en assets/.
preview_dir = ROOT / "release" / "v0.1.3"
preview_dir.mkdir(parents=True, exist_ok=True)
preview = Image.new("RGB", (960, 440), "#0e0e10")
p = ImageDraw.Draw(preview)
p.text((36, 24), "TwitchLite", font=font(30), fill="white")
p.text((36, 66), "Twitch + lite en verde neón", font=font(16), fill="#adadb8")
preview.paste(master.resize((256, 256), Image.Resampling.LANCZOS), (32, 120), master.resize((256, 256), Image.Resampling.LANCZOS))
for row, background in enumerate(["#18181b", "#f3f3f7"]):
    y = 120 + row * 140
    p.rounded_rectangle((340, y, 924, y + 116), radius=12, fill=background)
    x = 366
    for size in [64, 48, 32, 24, 16]:
        icon = master.resize((size, size), Image.Resampling.LANCZOS)
        preview.paste(icon, (x, y + (76 - size) // 2 + 4), icon)
        p.text((x, y + 84), f"{size} px", font=font(12), fill="#adadb8" if row == 0 else "#55555d")
        x += 112
preview.save(preview_dir / "branding-preview.png")
print("Generados SVG, PNG, ICO con nueve tamaños y vista previa de TwitchLite.")
