# -*- coding: utf-8 -*-
"""纯标准库生成小鲸鱼娘 icon（无 Pillow 依赖）。
512px 超采样绘制 → 逐级盒滤波缩到 256/64/48/32/16 → 写 ICO
（256 用 PNG 帧，其余用 32bpp BMP DIB 帧，兼容性最好）。
"""
import struct, zlib, os

SS = 512          # 超采样画布
W = SS

def in_ellipse(x, y, cx, cy, rx, ry):
    dx = (x - cx) / rx
    dy = (y - cy) / ry
    return dx * dx + dy * dy <= 1.0

def in_tri(px, py, ax, ay, bx, by, cx, cy):
    def sign(x1, y1, x2, y2, x3, y3):
        return (x1 - x3) * (y2 - y3) - (x2 - x3) * (y1 - y3)
    d1 = sign(px, py, ax, ay, bx, by)
    d2 = sign(px, py, bx, by, cx, cy)
    d3 = sign(px, py, cx, cy, ax, ay)
    neg = (d1 < 0) or (d2 < 0) or (d3 < 0)
    pos = (d1 > 0) or (d2 > 0) or (d3 > 0)
    return not (neg and pos)

def draw():
    """返回 (W*W*4) RGBA bytearray，预画 512px"""
    buf = bytearray(W * W * 4)

    def put(x, y, r, g, b, a):
        if a <= 0:
            return
        i = (y * W + x) * 4
        da = buf[i + 3] / 255.0
        sa = a / 255.0
        ia = 1.0 - sa
        out_a = sa + da * ia
        if out_a <= 0:
            return
        for k, v in enumerate((r, g, b)):  # 统一存 RGBA
            buf[i + k] = int((v * sa + buf[i + k] * da * ia) / out_a)
        buf[i + 3] = int(out_a * 255)

    def fill(shape, r, g, b, a=255):
        for y in range(W):
            for x in range(W):
                if shape(x, y):
                    put(x, y, r, g, b, a)

    # 尾鳍（右侧两片三角圆角）+ 尾柄
    fill(lambda x, y: in_tri(x, y, 235, 90, 300, 40, 300, 140) or
                       in_tri(x, y, 235, 210, 300, 160, 300, 260) or
                       in_ellipse(x, y, 235, 150, 42, 70),
        52, 140, 205)
    # 身体（大椭圆，微左倾的鲸）
    fill(lambda x, y: in_ellipse(x, y, 220, 300, 185, 140), 78, 168, 232)
    # 头部圆（让轮廓更像鲸：前圆后尖）
    fill(lambda x, y: in_ellipse(x, y, 195, 280, 150, 132), 78, 168, 232)
    # 背鳍
    fill(lambda x, y: in_tri(x, y, 268, 165, 322, 95, 330, 180), 52, 140, 205)
    # 肚皮
    fill(lambda x, y: in_ellipse(x, y, 185, 345, 118, 82), 205, 236, 250)
    # 嘴（微笑弧：两个小圆点拼的弧线用短棒近似）
    fill(lambda x, y: in_ellipse(x, y, 118, 322, 26, 7), 40, 90, 140, 200)
    fill(lambda x, y: in_ellipse(x, y, 140, 318, 26, 8), 205, 236, 250)  # 挖回嘴线，留弧
    # 眼睛 + 高光
    fill(lambda x, y: in_ellipse(x, y, 150, 272, 22, 26), 25, 30, 45)
    fill(lambda x, y: in_ellipse(x, y, 143, 262, 8, 9), 255, 255, 255)
    # 腮红
    fill(lambda x, y: in_ellipse(x, y, 108, 305, 17, 10), 255, 150, 170, 190)
    # 头顶蝴蝶结（娘味）
    fill(lambda x, y: in_tri(x, y, 232, 128, 180, 100, 182, 160), 255, 110, 155)
    fill(lambda x, y: in_tri(x, y, 232, 128, 284, 100, 282, 160), 255, 110, 155)
    fill(lambda x, y: in_ellipse(x, y, 232, 129, 20, 20), 255, 140, 178)
    # 喷水（头顶小水花）
    fill(lambda x, y: in_ellipse(x, y, 210, 66, 10, 10), 150, 220, 255, 230)
    fill(lambda x, y: in_ellipse(x, y, 246, 46, 12, 12), 150, 220, 255, 230)
    fill(lambda x, y: in_ellipse(x, y, 282, 66, 9, 9), 150, 220, 255, 230)
    fill(lambda x, y: in_ellipse(x, y, 246, 92, 7, 7), 170, 230, 255, 200)
    return buf

def box_resize(buf, sw, dw):
    """盒滤波缩放（任意比例），返回 dw*dw*4"""
    out = bytearray(dw * dw * 4)
    ratio = sw / dw
    for dy in range(dw):
        y0 = int(dy * ratio); y1 = max(y0 + 1, int((dy + 1) * ratio))
        for dx in range(dw):
            x0 = int(dx * ratio); x1 = max(x0 + 1, int((dx + 1) * ratio))
            sr = sg = sb = sa = n = 0
            for y in range(y0, min(y1, sw)):
                for x in range(x0, min(x1, sw)):
                    i = (y * sw + x) * 4
                    a = buf[i + 3]
                    sr += buf[i] * a; sg += buf[i + 1] * a; sb += buf[i + 2] * a
                    sa += a; n += 1
            j = (dy * dw + dx) * 4
            if sa:
                out[j] = sr // sa; out[j + 1] = sg // sa; out[j + 2] = sb // sa
                out[j + 3] = sa // n
    return out

def png_frame(buf, w):
    """RGBA → PNG 帧（ICO 内嵌）"""
    raw = b"".join(b"\x00" + bytes(buf[y * w * 4:(y + 1) * w * 4]) for y in range(w))
    def chunk(t, d):
        c = t + d
        return struct.pack(">I", len(d)) + c + struct.pack(">I", zlib.crc32(c))
    ihdr = struct.pack(">IIBBBBB", w, w, 8, 6, 0, 0, 0)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr)
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))

def bmp_frame(buf, w):
    """RGBA → 32bpp BMP DIB 帧（顶行在前，行序需 bottom-up）"""
    hdr = struct.pack("<IiiHHIIiiII", 40, w, w * 2, 1, 32, 0, w * w * 4, 0, 0, 0, 0)
    rows = b""
    for y in range(w - 1, -1, -1):
        row = bytearray()
        for x in range(w):
            i = (y * w + x) * 4
            row += bytes((buf[i + 2], buf[i + 1], buf[i], buf[i + 3]))  # BGRA
        rows += bytes(row)
    and_stride = ((w + 31) // 32) * 4
    rows += b"\x00" * (and_stride * w)  # AND 掩码全 0（用 alpha 通道）
    return hdr + rows

def write_ico(path, frames):
    """frames: [(size, data, is_png)]"""
    out = struct.pack("<HHH", 0, 1, len(frames))
    offset = 6 + 16 * len(frames)
    entries = b""
    body = b""
    for size, data, is_png in frames:
        entries += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0,
                               1, 32 if not is_png else 0, len(data), offset)
        body += data
        offset += len(data)
    with open(path, "wb") as f:
        f.write(out + entries + body)

def main():
    big = draw()
    # 去掉四周空隙：内容大致在 90..470 之间，裁到中心 400 区域再缩放
    crop, cw = bytearray(400 * 400 * 4), 400
    ox, oy = 60, 60
    for y in range(cw):
        for x in range(cw):
            si = ((y + oy) * W + (x + ox)) * 4
            di = (y * cw + x) * 4
            crop[di:di + 4] = big[si:si + 4]
    frames = [(256, png_frame(box_resize(crop, cw, 256), 256), True)]
    for s in (64, 48, 32, 16):
        frames.append((s, bmp_frame(box_resize(crop, cw, s), s), False))
    out = os.path.join(os.path.dirname(__file__), "pet.ico")
    write_ico(out, frames)
    print("OK", out, os.path.getsize(out))

if __name__ == "__main__":
    main()
