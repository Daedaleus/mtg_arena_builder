"""Erzeugt das App-Logo (assets/icon.png, assets/icon.ico) ohne Fremdbibliotheken.

Aufruf:  python tools/make_icon.py
Motiv: zwei gefächerte Spielkarten mit Haken auf dunklem Grund.
"""

import math
import os
import struct
import zlib

SIZE = 256
SS = 4  # Supersampling je Achse


def rounded_rect(px, py, cx, cy, w, h, r, angle=0.0):
    """True, wenn (px,py) im (um angle gedrehten) abgerundeten Rechteck liegt."""
    dx, dy = px - cx, py - cy
    if angle:
        c, s = math.cos(-angle), math.sin(-angle)
        dx, dy = dx * c - dy * s, dx * s + dy * c
    qx, qy = abs(dx) - (w / 2 - r), abs(dy) - (h / 2 - r)
    if qx <= 0 or qy <= 0:
        return qx <= r and qy <= r
    return qx * qx + qy * qy <= r * r


def near_segment(px, py, ax, ay, bx, by, half_width):
    vx, vy = bx - ax, by - ay
    t = max(0.0, min(1.0, ((px - ax) * vx + (py - ay) * vy) / (vx * vx + vy * vy)))
    dx, dy = px - (ax + t * vx), py - (ay + t * vy)
    return dx * dx + dy * dy <= half_width * half_width


def mix(a, b, t):
    return tuple(a[i] + (b[i] - a[i]) * t for i in range(3))


def sample(x, y):
    """Farbe (r,g,b,a) an der Stelle x,y im 256er-Raster."""
    if not rounded_rect(x, y, 128, 128, 240, 240, 52):
        return (0, 0, 0, 0)
    color = mix((38, 52, 92), (16, 20, 38), y / SIZE)  # Hintergrundverlauf

    back = math.radians(-14)
    if rounded_rect(x, y, 100, 132, 104, 148, 12, back):
        color = (92, 124, 190)
        if rounded_rect(x, y, 100, 132, 88, 132, 7, back):
            color = (64, 92, 156)

    front = math.radians(9)
    if rounded_rect(x + 3, y - 4, 150, 130, 108, 152, 13, front):
        color = mix(color, (0, 0, 0), 0.35)  # Schatten
    if rounded_rect(x, y, 150, 130, 108, 152, 13, front):
        color = (244, 236, 214)
        if rounded_rect(x, y, 150, 130, 90, 134, 7, front):
            color = (252, 247, 232)
        # Haken
        if near_segment(x, y, 122, 134, 142, 156, 10) or near_segment(x, y, 142, 156, 182, 106, 10):
            color = (46, 160, 96)
    return (*color, 255)


def render():
    n = SIZE * SS
    rows = []
    for py in range(SIZE):
        row = []
        for px in range(SIZE):
            acc = [0.0, 0.0, 0.0, 0.0]
            for sy in range(SS):
                for sx in range(SS):
                    r, g, b, a = sample(px + (sx + 0.5) / SS, py + (sy + 0.5) / SS)
                    acc[0] += r * a
                    acc[1] += g * a
                    acc[2] += b * a
                    acc[3] += a
            if acc[3] == 0:
                row.append((0, 0, 0, 0))
            else:
                row.append((round(acc[0] / acc[3]), round(acc[1] / acc[3]), round(acc[2] / acc[3]), round(acc[3] / (SS * SS))))
        rows.append(row)
    assert n
    return rows


def downscale(rows, size):
    f = SIZE // size
    out = []
    for y in range(size):
        row = []
        for x in range(size):
            acc = [0, 0, 0, 0]
            for dy in range(f):
                for dx in range(f):
                    r, g, b, a = rows[y * f + dy][x * f + dx]
                    acc[0] += r * a
                    acc[1] += g * a
                    acc[2] += b * a
                    acc[3] += a
            if acc[3] == 0:
                row.append((0, 0, 0, 0))
            else:
                row.append((round(acc[0] / acc[3]), round(acc[1] / acc[3]), round(acc[2] / acc[3]), round(acc[3] / (f * f))))
        out.append(row)
    return out


def png_bytes(rows):
    size = len(rows)
    raw = b"".join(b"\x00" + bytes(v for px in row for v in px) for row in rows)

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def dib_bytes(rows):
    """32-Bit-BMP-Eintrag für .ico (kleine Größen, maximale Kompatibilität)."""
    size = len(rows)
    header = struct.pack("<IiiHHIIiiII", 40, size, size * 2, 1, 32, 0, 0, 0, 0, 0, 0)
    pixels = b"".join(bytes((b, g, r, a)) for row in reversed(rows) for (r, g, b, a) in row)
    mask_row = bytes(((size + 31) // 32) * 4)
    return header + pixels + mask_row * size


def main():
    root = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets")
    os.makedirs(root, exist_ok=True)
    full = render()
    with open(os.path.join(root, "icon.png"), "wb") as f:
        f.write(png_bytes(full))

    entries = [(256, png_bytes(full))] + [(s, dib_bytes(downscale(full, s))) for s in (64, 32, 16)]
    # 48 px ist kein Teiler von 256 -> aus 768er-Zwischenschritt nicht nötig; Windows skaliert selbst.
    offset = 6 + 16 * len(entries)
    ico = struct.pack("<HHH", 0, 1, len(entries))
    for size, data in entries:
        ico += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    with open(os.path.join(root, "icon.ico"), "wb") as f:
        f.write(ico + b"".join(d for _, d in entries))
    print("assets/icon.png und assets/icon.ico geschrieben")


if __name__ == "__main__":
    main()
