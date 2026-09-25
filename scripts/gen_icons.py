#!/usr/bin/env python3
"""Derive the launcher's icons from the reforgermods.net brand assets.

No image libraries, so this is reproducible from a clean checkout with nothing
but a Python interpreter. Run from the repo root:

    python3 scripts/gen_icons.py

Sources (from the reforgermods.net site assets, copied into brand/):

    reforger-mods-app-icon.png   -> application / taskbar icon, all sizes
    reforger-mods-icon-dark.png  -> transparent mark used in the app header

Downsampling is a box filter on premultiplied alpha. Premultiplying matters:
averaging straight RGBA makes fully transparent pixels drag their colour into
the edge, which shows up as a dark halo around the mark at 16px.
"""

import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BRAND = ROOT / "brand"
ICONS = ROOT / "src-tauri" / "icons"
UI_ASSETS = ROOT / "src" / "assets"


# ----------------------------------------------------------------- decoding

def read_png(path):
    """Decodes an 8-bit RGBA PNG into (width, height, rows of bytes)."""
    data = path.read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError(f"{path} is not a PNG")

    width = height = None
    idat = bytearray()
    pos = 8
    while pos < len(data):
        length, tag = struct.unpack_from(">I4s", data, pos)
        body = data[pos + 8 : pos + 8 + length]
        if tag == b"IHDR":
            width, height, depth, colour = struct.unpack(">IIBB", body[:10])
            if depth != 8 or colour != 6:
                raise ValueError(f"{path}: expected 8-bit RGBA, got depth={depth} colour={colour}")
        elif tag == b"IDAT":
            idat += body
        elif tag == b"IEND":
            break
        pos += 12 + length

    raw = zlib.decompress(bytes(idat))
    stride = width * 4
    rows = []
    previous = bytearray(stride)
    pos = 0
    for _ in range(height):
        filter_type = raw[pos]
        pos += 1
        line = bytearray(raw[pos : pos + stride])
        pos += stride
        unfilter(filter_type, line, previous, 4)
        rows.append(bytes(line))
        previous = line
    return width, height, rows


def unfilter(filter_type, line, previous, bpp):
    """Reverses one PNG scanline filter in place (RFC 2083 section 6)."""
    if filter_type == 0:
        return
    for i in range(len(line)):
        a = line[i - bpp] if i >= bpp else 0
        b = previous[i]
        if filter_type == 1:
            line[i] = (line[i] + a) & 0xFF
        elif filter_type == 2:
            line[i] = (line[i] + b) & 0xFF
        elif filter_type == 3:
            line[i] = (line[i] + ((a + b) >> 1)) & 0xFF
        elif filter_type == 4:
            c = previous[i - bpp] if i >= bpp else 0
            p = a + b - c
            pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
            pred = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
            line[i] = (line[i] + pred) & 0xFF
        else:
            raise ValueError(f"unknown filter {filter_type}")


# --------------------------------------------------------------- resampling

def resize(width, height, rows, size):
    """Box-filter downsample to size x size, averaging premultiplied alpha."""
    out = []
    for oy in range(size):
        y0, y1 = oy * height // size, max(oy * height // size + 1, (oy + 1) * height // size)
        row = bytearray()
        for ox in range(size):
            x0, x1 = ox * width // size, max(ox * width // size + 1, (ox + 1) * width // size)
            r = g = b = a = 0
            count = 0
            for y in range(y0, y1):
                line = rows[y]
                for x in range(x0, x1):
                    i = x * 4
                    alpha = line[i + 3]
                    # Premultiply so transparent pixels contribute no colour.
                    r += line[i] * alpha
                    g += line[i + 1] * alpha
                    b += line[i + 2] * alpha
                    a += alpha
                    count += 1
            if a == 0:
                row += bytes(4)
            else:
                row += bytes((r // a, g // a, b // a, a // count))
        out.append(bytes(row))
    return out


# ----------------------------------------------------------------- encoding

def write_png(rows, size):
    def chunk(tag, body):
        payload = tag + body
        return struct.pack(">I", len(body)) + payload + struct.pack(">I", zlib.crc32(payload))

    raw = b"".join(b"\x00" + r for r in rows)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def write_ico(entries):
    """Packs PNG-encoded images into an .ico container."""
    header = struct.pack("<HHH", 0, 1, len(entries))
    offset = 6 + 16 * len(entries)
    directory, blobs = b"", b""
    for size, blob in entries:
        directory += struct.pack(
            "<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(blob), offset
        )
        blobs += blob
        offset += len(blob)
    return header + directory + blobs


# --------------------------------------------------------------------- main

def main():
    ICONS.mkdir(parents=True, exist_ok=True)
    UI_ASSETS.mkdir(parents=True, exist_ok=True)

    app_w, app_h, app_rows = read_png(BRAND / "reforger-mods-app-icon.png")
    cache = {}

    def app_at(size):
        if size not in cache:
            cache[size] = write_png(resize(app_w, app_h, app_rows, size), size)
        return cache[size]

    for name, size in [
        ("32x32.png", 32),
        ("128x128.png", 128),
        ("128x128@2x.png", 256),
        ("icon.png", 512),
    ]:
        (ICONS / name).write_bytes(app_at(size))
        print(f"wrote src-tauri/icons/{name} ({size}x{size})")

    (ICONS / "icon.ico").write_bytes(write_ico([(s, app_at(s)) for s in (16, 32, 48, 256)]))
    print("wrote src-tauri/icons/icon.ico (16, 32, 48, 256)")

    # Transparent mark for the in-app header. 64px covers an 18px slot at 3x.
    mark_w, mark_h, mark_rows = read_png(BRAND / "reforger-mods-icon-dark.png")
    blob = write_png(resize(mark_w, mark_h, mark_rows, 64), 64)
    (UI_ASSETS / "brand-mark.png").write_bytes(blob)
    print(f"wrote src/assets/brand-mark.png (64x64, {len(blob)} bytes)")


if __name__ == "__main__":
    main()
