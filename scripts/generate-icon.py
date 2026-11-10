#!/usr/bin/env python3
"""Generate assets/icon-master.png — ikon 1024x1024 Jadwal Sholat.

Tema mengikuti panel: latar gelap #1B1E24, siluet putih #E8EAED,
aksen emas #E6C46A. Jalankan: python3 scripts/generate-icon.py
lalu: npm run tauri icon assets/icon-master.png
"""

from PIL import Image, ImageDraw

S = 1024
BG = (27, 30, 36, 255)
WHITE = (232, 234, 237, 255)
GOLD = (230, 196, 106, 255)

img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(img)

# Latar rounded square
d.rounded_rectangle([48, 48, 976, 976], radius=210, fill=BG)

# Dasar/building base
d.rectangle([268, 700, 756, 744], fill=WHITE)

# Kubah utama + badan kubah
d.pieslice([366, 404, 658, 696], 180, 360, fill=WHITE)
d.rectangle([436, 540, 588, 700], fill=WHITE)

# Finial kubah (emas)
d.rectangle([504, 330, 520, 410], fill=GOLD)
d.ellipse([492, 298, 532, 338], fill=GOLD)

# Menara kiri-kanan
for cx in (296, 728):
    d.rectangle([cx - 28, 416, cx + 28, 700], fill=WHITE)
    d.pieslice([cx - 40, 376, cx + 40, 456], 180, 360, fill=WHITE)
    d.rectangle([cx - 6, 340, cx + 6, 382], fill=GOLD)
    d.ellipse([cx - 14, 320, cx + 14, 348], fill=GOLD)

img.save("assets/icon-master.png")
print("assets/icon-master.png written")
