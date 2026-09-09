#!/usr/bin/env python3
"""NetStorm _shapes.shp decoder (memory-safe).

File layout (reverse-engineered):
  * A run of top-level containers: magic '1.10', u32 count, count x (u32 offset, u32 zero).
    Offsets are RELATIVE TO THE CONTAINER'S OWN POSITION (container 0 sits at 0,
    which is why its offsets look absolute).
  * Each frame record: 24-byte header  <2H 2h 2i I I> =
        A = shape box height, B = shape box width - 1,
        c = hotspot row, d = hotspot column (inside the shape box),
        e = x offset of the stored block's left edge from the hotspot,
        f = y offset of the stored block's top edge from the hotspot,
        n, flag = unknown small ints.
    then RLE rows: op 0 = end of row; op 1,s = skip s pixels; odd op = literal (op>>1)
    bytes; even op = run of (op>>1) pixels of one colour byte.  Rows continue until a
    36-byte trailer, which starts on a 4-byte boundary (0-3 zero pad bytes before it).
    The stored block is the frame's bounding box (cropped), so its size comes from the
    RLE itself, never from the header.
  * Trailer: 6 shorts (a box, x values /16 and y values /11 also stored as 6 floats).

Safety: no allocation is ever sized from header fields.  Every raster is capped at
MAX_PIXELS / MAX_DIM, every offset is validated, all pixel buffers are bytearrays.
"""
import struct, zlib, sys, os
from collections import Counter

SHP = '/home/petro/work/NetStorm/Netstorm/d/_shapes.shp'
COL = '/home/petro/work/NetStorm/Netstorm/d/SUNCANNON.COL'
OUT = os.getcwd()
MAX_PIXELS = 4_000_000
MAX_DIM = 2048
HDR, TRL = 24, 36

data = open(SHP, 'rb').read()
N = len(data)
pal = open(COL, 'rb').read()[8:8 + 768]


class BadFrame(Exception):
    pass


# ------------------------------------------------------------------ containers
def parse_container(o):
    if data[o:o + 4] != b'1.10': return None
    cnt = struct.unpack_from('<I', data, o + 4)[0]
    if cnt > 100_000 or o + 8 + 8 * cnt > N: return None
    return [o + struct.unpack_from('<I', data, o + 8 + 8 * i)[0] for i in range(cnt)]


conts = []
o = 0
while o < N:
    ents = parse_container(o)
    if ents is None: break
    conts.append(ents); o += 8 + 8 * len(ents)
TABLE_END = o
uniq = sorted(set(a for ents in conts for a in ents))
NEXT = {a: (uniq[i + 1] if i + 1 < len(uniq) else N) for i, a in enumerate(uniq)}


def check_offset(a):
    if a not in NEXT:
        raise BadFrame(f"{a:#x} is not a table target")
    if not (TABLE_END <= a and a + HDR + TRL <= NEXT[a] <= N):
        raise BadFrame(f"{a:#x} record outside data region or too small")


# ---------------------------------------------------------------------- decode
def is_trailer(t, A=None, B=None):
    """36-byte trailer signature: 6 floats that equal the 6 shorts behind them
    divided by 16 (x) / 11 (y) - a tile-unit box.  Header independent."""
    if t + TRL > N: return False
    fl = struct.unpack_from('<6f', data, t)
    sh = struct.unpack_from('<6h', data, t + 24)
    return all(v == v and abs(v - sh[i] / (16 if i % 2 == 0 else 11)) < 0.15 for i, v in enumerate(fl))


def decode_spans(a):
    """Return (hdr, rows) where rows = [[(x, bytes), ...], ...] for the stored block.
    The block ends at the record's own trailer (records may be followed by
    unreferenced records, so the next table offset is only a hard bound)."""
    check_offset(a)
    hdr = struct.unpack_from('<2H2h2iII', data, a)
    A, B = hdr[0], hdr[1]
    bound = NEXT[a]
    p = a + HDR
    rows = []
    if A == 0 or B == 0 or abs(hdr[4]) >= 1 << 30 or abs(hdr[5]) >= 1 << 30:
        return hdr, rows                        # empty placeholder frame (sentinel offsets)
    while True:
        t = (p + 3) & ~3
        if t + TRL <= bound and not any(data[p:t]) and is_trailer(t, A, B):
            return hdr, rows                    # pad + trailer reached
        if p >= bound - TRL:
            raise BadFrame(f"{a:#x}: no trailer found before {bound:#x}")
        x = 0; spans = []
        while True:
            if p >= bound - TRL:
                raise BadFrame(f"{a:#x}: row {len(rows)} runs into next record")
            op = data[p]; p += 1
            if op == 0: break
            if op == 1:
                x += data[p]; p += 1
            elif op & 1:
                k = op >> 1
                if p + k > bound - TRL: raise BadFrame(f"{a:#x}: literal run past record")
                spans.append((x, data[p:p + k])); x += k; p += k
            else:
                k = op >> 1
                spans.append((x, bytes((data[p],)) * k)); x += k; p += 1
            if x > MAX_DIM: raise BadFrame(f"{a:#x}: row wider than {MAX_DIM}")
        rows.append(spans)
        if len(rows) > MAX_DIM: raise BadFrame(f"{a:#x}: more than {MAX_DIM} rows")


class Raster:
    """Indexed image + 1-byte alpha mask, both flat bytearrays."""
    __slots__ = ('w', 'h', 'idx', 'mask')

    def __init__(self, w, h):
        if w <= 0 or h <= 0 or w > MAX_DIM or h > MAX_DIM or w * h > MAX_PIXELS:
            raise BadFrame(f"refusing raster {w}x{h}")
        self.w, self.h = w, h
        self.idx = bytearray(w * h); self.mask = bytearray(w * h)

    def blit_spans(self, rows, x0, y0):
        for y, spans in enumerate(rows):
            yy = y0 + y
            if not 0 <= yy < self.h: raise BadFrame(f"row {yy} outside {self.w}x{self.h}")
            base = yy * self.w
            for x, px in spans:
                xs = x0 + x
                if xs < 0 or xs + len(px) > self.w:
                    raise BadFrame(f"span x={xs}+{len(px)} outside width {self.w}")
                self.idx[base + xs:base + xs + len(px)] = px
                self.mask[base + xs:base + xs + len(px)] = b'\x01' * len(px)

    def blit(self, src, x0, y0):
        for y in range(src.h):
            s = y * src.w; d = (y0 + y) * self.w + x0
            self.idx[d:d + src.w] = src.idx[s:s + src.w]
            self.mask[d:d + src.w] = src.mask[s:s + src.w]

    def scaled(self, k):
        r = Raster(self.w * k, self.h * k)
        for y in range(self.h):
            s = y * self.w
            ri = b''.join(bytes((v,)) * k for v in self.idx[s:s + self.w])
            rm = b''.join(bytes((v,)) * k for v in self.mask[s:s + self.w])
            for j in range(k):
                d = (y * k + j) * r.w
                r.idx[d:d + r.w] = ri; r.mask[d:d + r.w] = rm
        return r

    def png(self, path, bg=(255, 0, 255, 0)):
        lut = [bytes(pal[3 * i:3 * i + 3]) + b'\xff' for i in range(256)]
        bgb = bytes(bg)
        raw = bytearray()
        for y in range(self.h):
            raw.append(0); s = y * self.w
            raw += b''.join(lut[self.idx[s + x]] if self.mask[s + x] else bgb for x in range(self.w))
        def chunk(t, d): return struct.pack('>I', len(d)) + t + d + struct.pack('>I', zlib.crc32(t + d) & 0xffffffff)
        with open(path, 'wb') as fh:
            fh.write(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', self.w, self.h, 8, 6, 0, 0, 0))
                     + chunk(b'IDAT', zlib.compress(bytes(raw))) + chunk(b'IEND', b''))


def decode_frame(a):
    """Frame on its shape-box canvas ((A+1) x (B+1)), block placed via hotspot + offsets."""
    hdr, rows = decode_spans(a)
    A, B, c, d, e, f, n, flag = hdr
    if not rows:
        return Raster(1, 1), hdr, (0, 0, True)   # placeholder / empty frame
    bh = len(rows)
    bw = max((x + len(px) for spans in rows for x, px in spans), default=0)
    x0, y0 = d + e, c + f
    W, H = B + 1, A + 1
    fits = 0 <= x0 and 0 <= y0 and x0 + bw <= W and y0 + bh <= H
    if not fits:                           # grow canvas rather than trust the header
        W = max(W, x0 + bw) - min(0, x0); H = max(H, y0 + bh) - min(0, y0)
        x0 -= min(0, d + e); y0 -= min(0, c + f)
    r = Raster(max(W, 1), max(H, 1))
    if rows: r.blit_spans(rows, x0, y0)
    return r, hdr, (bw, bh, fits)


# ------------------------------------------------------------------------ main
def stats():
    st = Counter(); fitfail = []
    for ci, ents in enumerate(conts):
        for a in ents:
            try:
                r, hdr, (bw, bh, fits) = decode_frame(a)
            except BadFrame as ex:
                st['bad'] += 1; continue
            st['ok'] += 1
            st['fits' if fits else 'nofit'] += 1
            if not fits and len(fitfail) < 8: fitfail.append((ci, a, hdr, bw, bh))
            st['full' if bh == hdr[0] else 'cropped'] += 1
    print("frame stats:", dict(st))
    for ci, a, hdr, bw, bh in fitfail:
        print(f"  no-fit c{ci} {a:#x} hdr={hdr} block={bw}x{bh}")


def sheet(ci, scale=2, maxframes=40, gap=2):
    frames = []
    for a in conts[ci][:maxframes]:
        try:
            r, hdr, info = decode_frame(a)
            frames.append(r)
        except BadFrame as ex:
            print(f"  c{ci} {a:#x}: {ex}")
    if not frames: return None
    W = sum(fr.w + gap for fr in frames); H = max(fr.h for fr in frames)
    cols = len(frames)
    while (W * H * scale * scale > MAX_PIXELS or W * scale > MAX_DIM or H * scale > MAX_DIM) and cols > 1:
        cols = (cols + 1) // 2
        W = max(sum(fr.w + gap for fr in frames[i:i + cols]) for i in range(0, len(frames), cols))
        H = sum(max(fr.h for fr in frames[i:i + cols]) + gap for i in range(0, len(frames), cols))
    sh = Raster(W, H)
    x = y = 0; rowh = 0
    for i, fr in enumerate(frames):
        if i and i % cols == 0: x = 0; y += rowh + gap; rowh = 0
        sh.blit(fr, x, y); x += fr.w + gap; rowh = max(rowh, fr.h)
    big = sh.scaled(scale) if scale > 1 else sh
    path = os.path.join(OUT, f'c{ci}.png')
    big.png(path, bg=(40, 40, 40, 255))
    print(f"  wrote {path}: {len(frames)} frames, {big.w}x{big.h}")
    return path


if __name__ == '__main__':
    print(f"{len(conts)} containers, {len(uniq)} unique frames")
    stats()
    for ci in [int(x) for x in sys.argv[1:]] or (0, 109, 2, 23, 37):
        sheet(ci)
