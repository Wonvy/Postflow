#!/usr/bin/env python3
"""安装包的图片和图标：Windows 安装程序（NSIS、MSI）的侧边图、页眉图，以及安装程序图标。

不依赖任何图形库：用距离场画圆角矩形、圆头线段和圆，4×4 超采样抗锯齿，
直接写出 BMP（安装程序要求 24 位 BMP）、PNG 和 ICO。

    python3 scripts/installer-art.py

输出到 src-tauri/installer/。改了颜色或图形就重新跑一遍，把生成的文件一起提交。
"""

import math
import os
import struct
import zlib

OUT = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "installer")
SS = 4  # 超采样倍数

# 和应用图标（src-tauri/app-icon.svg）一样的紫色
TOP = (0xA2, 0x5B, 0xFF)
BOTTOM = (0x6A, 0x1F, 0xE0)
WHITE = (255, 255, 255)


def lerp(a, b, t):
    return a + (b - a) * t


def mix(c1, c2, t):
    return tuple(lerp(x, y, t) for x, y in zip(c1, c2))


class Canvas:
    """RGB 画布，坐标以最终像素为单位，内部按 SS 倍超采样。"""

    def __init__(self, w, h, fill=WHITE):
        self.w, self.h = w, h
        self.W, self.H = w * SS, h * SS
        self.px = [list(fill) for _ in range(self.W * self.H)]

    def gradient(self, c1, c2, x0=0.15, y0=0.0, x1=0.85, y1=1.0, glow=None):
        """斜向渐变铺满画布；glow=(cx, cy, r, alpha) 在左上角加一层柔光。"""
        dx, dy = (x1 - x0) * self.W, (y1 - y0) * self.H
        length = dx * dx + dy * dy
        for j in range(self.H):
            for i in range(self.W):
                t = ((i - x0 * self.W) * dx + (j - y0 * self.H) * dy) / length
                c = mix(c1, c2, min(1, max(0, t)))
                if glow:
                    gx, gy, gr, ga = glow
                    d = math.hypot(i / self.W - gx, j / self.H - gy) / gr
                    a = ga * max(0.0, 1 - d / 0.6) if d < 0.6 else 0
                    c = mix(c, WHITE, a)
                self.px[j * self.W + i] = list(c)

    def shape(self, sdf, bbox, color, alpha=1.0):
        """用距离函数画一个形状：sdf(x, y) < 0 在里面。bbox 是最终像素的 (x0, y0, x1, y1)。"""
        x0, y0, x1, y1 = bbox
        for j in range(max(0, int(y0 * SS) - 2), min(self.H, int(math.ceil(y1 * SS)) + 2)):
            for i in range(max(0, int(x0 * SS) - 2), min(self.W, int(math.ceil(x1 * SS)) + 2)):
                x, y = (i + 0.5) / SS, (j + 0.5) / SS
                d = sdf(x, y)
                # 边缘半个超采样像素的软过渡
                cover = min(1.0, max(0.0, 0.5 - d * SS))
                if cover <= 0:
                    continue
                p = self.px[j * self.W + i]
                a = cover * alpha
                for k in range(3):
                    p[k] = p[k] + (color[k] - p[k]) * a

    def rounded_rect(self, x, y, w, h, r, color, alpha=1.0):
        cx, cy, hx, hy = x + w / 2, y + h / 2, w / 2, h / 2

        def sdf(px, py):
            qx, qy = abs(px - cx) - (hx - r), abs(py - cy) - (hy - r)
            return math.hypot(max(qx, 0), max(qy, 0)) + min(max(qx, qy), 0) - r

        self.shape(sdf, (x, y, x + w, y + h), color, alpha)

    def gradient_rounded_rect(self, x, y, w, h, r, c1, c2):
        """圆角矩形里面填斜向渐变（应用图标的底）。"""
        steps = 24
        for s in range(steps):
            t0, t1 = s / steps, (s + 1) / steps
            # 按对角线方向切成很多条，每条一个颜色；条与条之间稍微重叠，避免出现缝
            col = mix(c1, c2, (t0 + t1) / 2)
            cx, cy, hx, hy = x + w / 2, y + h / 2, w / 2, h / 2

            def sdf(px, py, t0=t0, t1=t1):
                qx, qy = abs(px - cx) - (hx - r), abs(py - cy) - (hy - r)
                d = math.hypot(max(qx, 0), max(qy, 0)) + min(max(qx, qy), 0) - r
                t = ((px - x) * 0.7 + (py - y)) / (w * 0.7 + h)
                band = max(t0 - t, t - t1 - 0.01) * (w + h)
                return max(d, band)

            self.shape(sdf, (x, y, x + w, y + h), col)

    def line(self, ax, ay, bx, by, width, color, alpha=1.0):
        r = width / 2
        vx, vy = bx - ax, by - ay
        ll = vx * vx + vy * vy

        def sdf(px, py):
            t = 0 if ll == 0 else min(1, max(0, ((px - ax) * vx + (py - ay) * vy) / ll))
            return math.hypot(px - (ax + vx * t), py - (ay + vy * t)) - r

        self.shape(sdf, (min(ax, bx) - r, min(ay, by) - r, max(ax, bx) + r, max(ay, by) + r), color, alpha)

    def circle(self, cx, cy, r, color, alpha=1.0):
        self.shape(lambda px, py: math.hypot(px - cx, py - cy) - r, (cx - r, cy - r, cx + r, cy + r), color, alpha)

    def pixels(self):
        """缩小到最终尺寸（盒式滤波），返回一行行的 RGB 元组。"""
        rows = []
        n = SS * SS
        for y in range(self.h):
            row = []
            for x in range(self.w):
                r = g = b = 0.0
                for j in range(SS):
                    base = (y * SS + j) * self.W + x * SS
                    for i in range(SS):
                        p = self.px[base + i]
                        r += p[0]
                        g += p[1]
                        b += p[2]
                row.append((round(r / n), round(g / n), round(b / n)))
            rows.append(row)
        return rows


def logo(c, x, y, size, inverted=False):
    """应用图标：紫色圆角方块里三条白线。inverted 时是白底紫线（放在紫色背景上）。"""
    r = size * 186 / 824
    if inverted:
        c.rounded_rect(x, y, size, size, r, WHITE)
        line_color = BOTTOM
    else:
        c.gradient_rounded_rect(x, y, size, size, r, TOP, BOTTOM)
        line_color = WHITE
    s = size / 824
    w = 66 * s
    for (y0, x1, a) in ((388, 706, 1.0), (512, 602, 0.8), (636, 486, 0.55)):
        c.line(x + (318 - 100) * s, y + (y0 - 100) * s, x + (x1 - 100) * s, y + (y0 - 100) * s, w, line_color, a)


# ------------------------------------------------------------------ 文件格式


def write_bmp(path, rows):
    h, w = len(rows), len(rows[0])
    pad = (4 - (w * 3) % 4) % 4
    data = bytearray()
    for row in reversed(rows):  # BMP 从下往上存
        for (r, g, b) in row:
            data += bytes((b, g, r))
        data += b"\0" * pad
    header = struct.pack("<2sIHHI", b"BM", 54 + len(data), 0, 0, 54)
    info = struct.pack("<IiiHHIIiiII", 40, w, h, 1, 24, 0, len(data), 2835, 2835, 0, 0)
    with open(path, "wb") as f:
        f.write(header + info + data)


def png_bytes(rows, alpha_rows=None):
    h, w = len(rows), len(rows[0])
    raw = bytearray()
    for y, row in enumerate(rows):
        raw.append(0)
        for x, (r, g, b) in enumerate(row):
            raw += bytes((r, g, b, alpha_rows[y][x] if alpha_rows else 255))

    def chunk(tag, body):
        return struct.pack(">I", len(body)) + tag + body + struct.pack(">I", zlib.crc32(tag + body) & 0xFFFFFFFF)

    ihdr = struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(bytes(raw), 9)) + chunk(b"IEND", b"")


def write_ico(path, images):
    """images: [(size, png bytes)]，ICO 里直接放 PNG（Vista 以后都支持）。"""
    head = struct.pack("<HHH", 0, 1, len(images))
    entries, blobs = b"", b""
    offset = 6 + 16 * len(images)
    for size, data in images:
        dim = 0 if size >= 256 else size
        entries += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset)
        blobs += data
        offset += len(data)
    with open(path, "wb") as f:
        f.write(head + entries + blobs)


# ------------------------------------------------------------------ 图


def side_panel(c, w, h):
    """紫色渐变的侧边图：中间偏上一个白底图标，下面三条淡淡的线呼应图标。"""
    c.gradient(TOP, BOTTOM, glow=(0.2, 0.05, 1.0, 0.35))
    size = min(w * 0.46, 84)
    lx, ly = (w - size) / 2, h * 0.24
    # 图标下的柔和阴影
    for k in range(6):
        c.rounded_rect(lx - k + 1, ly + 4 - k + 1 + 2, size + 2 * k - 2, size + 2 * k - 2, size * 0.23 + k, BOTTOM, 0.06)
    logo(c, lx, ly, size, inverted=True)
    y = ly + size + h * 0.13
    for i, (frac, a) in enumerate(((0.62, 0.30), (0.46, 0.22), (0.30, 0.15))):
        lw = w * frac
        c.line((w - lw) / 2, y + i * 14, (w + lw) / 2, y + i * 14, 5, WHITE, a)


def header(c, w, h, logo_size):
    """白底页眉：图标靠右，左边是三条淡紫色的线。"""
    pad = (h - logo_size) / 2
    logo(c, w - logo_size - pad, pad, logo_size)
    y0 = h / 2 - 9
    for i, (frac, a) in enumerate(((0.42, 0.18), (0.30, 0.13), (0.18, 0.09))):
        lw = (w - logo_size - pad * 3) * frac / 0.42
        c.line(w - logo_size - pad * 2 - lw, y0 + i * 9, w - logo_size - pad * 2, y0 + i * 9, 3.2, BOTTOM, a)


def setup_icon(size):
    """安装程序图标：应用图标右下角加一个带向下箭头的白色圆形角标。透明背景。"""
    c = Canvas(size, size, fill=(0, 0, 0))
    mask = Canvas(size, size, fill=(0, 0, 0))
    m = size * 100 / 1024
    body = size - 2 * m
    logo(c, m, m, body)
    logo_mask(mask, m, m, body)
    # 角标
    br = size * 0.24
    bx, by = size - br - size * 0.02, size - br - size * 0.02
    c.circle(bx, by, br, WHITE)
    mask.circle(bx, by, br, WHITE)
    c.circle(bx, by, br * 0.8, mix(TOP, BOTTOM, 0.6))
    stroke = max(1.2, size * 0.045)
    c.line(bx, by - br * 0.42, bx, by + br * 0.36, stroke, WHITE)
    c.line(bx - br * 0.36, by + br * 0.02, bx, by + br * 0.38, stroke, WHITE)
    c.line(bx + br * 0.36, by + br * 0.02, bx, by + br * 0.38, stroke, WHITE)
    rows = c.pixels()
    alpha = [[p[0] for p in row] for row in mask.pixels()]
    # 半透明的边缘：把颜色从黑底里「解」出来，免得边上发黑
    fixed = []
    for row, arow in zip(rows, alpha):
        fixed.append([tuple(min(255, round(v * 255 / a)) if a else 0 for v in px) for px, a in zip(row, arow)])
    return png_bytes(fixed, alpha)


def logo_mask(c, x, y, size):
    c.rounded_rect(x, y, size, size, size * 186 / 824, WHITE)


def main():
    os.makedirs(OUT, exist_ok=True)

    # NSIS：欢迎页和完成页的侧边图 164×314，页眉 150×57
    c = Canvas(164, 314)
    side_panel(c, 164, 314)
    write_bmp(os.path.join(OUT, "nsis-sidebar.bmp"), c.pixels())

    c = Canvas(150, 57)
    header(c, 150, 57, 38)
    write_bmp(os.path.join(OUT, "nsis-header.bmp"), c.pixels())

    # MSI：横幅 493×58，对话框背景 493×312（左边 164 宽的紫色区域，其余白色留给文字）
    c = Canvas(493, 58)
    header(c, 493, 58, 40)
    write_bmp(os.path.join(OUT, "msi-banner.bmp"), c.pixels())

    c = Canvas(493, 312)
    side = Canvas(164, 312)
    side_panel(side, 164, 312)
    for j in range(c.H):
        for i in range(side.W):
            c.px[j * c.W + i] = side.px[j * side.W + i]
    write_bmp(os.path.join(OUT, "msi-dialog.bmp"), c.pixels())

    # 安装程序图标
    write_ico(os.path.join(OUT, "setup.ico"), [(s, setup_icon(s)) for s in (16, 24, 32, 48, 64, 128, 256)])
    print("written to", os.path.normpath(OUT))


if __name__ == "__main__":
    main()
