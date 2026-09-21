"""右键菜单稳定性测试：连点 5 次，每次都要真的弹出菜单窗口(#32768)。

只验证"稳定"，所以故意绕开"一次通过就收工"的侥幸。
"""
import ctypes
import ctypes.wintypes as wt
import sys
import time

u = ctypes.WinDLL("user32", use_last_error=True)
u.SetProcessDpiAwarenessContext.argtypes = [ctypes.c_void_p]
u.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))

MOVE, ABS = 0x0001, 0x8000
RDOWN, RUP = 0x0008, 0x0010
LDOWN, LUP = 0x0002, 0x0004
KEYUP = 0x0002
VK_ESC = 0x1B
SW, SH = u.GetSystemMetrics(0), u.GetSystemMetrics(1)


def pt2abs(x, y):
    return int(x * 65535 / SW), int(y * 65535 / SH)


def rclick(x, y):
    ax, ay = pt2abs(x, y)
    u.mouse_event(MOVE | ABS, ax, ay, 0, 0)
    time.sleep(0.2)
    u.mouse_event(RDOWN | ABS, ax, ay, 0, 0)
    time.sleep(0.1)
    u.mouse_event(RUP | ABS, ax, ay, 0, 0)


def esc():
    u.keybd_event(VK_ESC, 0, 0, 0)
    time.sleep(0.05)
    u.keybd_event(VK_ESC, 0, KEYUP, 0)


def find_pet():
    P = ctypes.WINFUNCTYPE(wt.BOOL, wt.HWND, wt.LPARAM)
    out = []

    def cb(h, l):
        b = ctypes.create_unicode_buffer(256)
        u.GetClassNameW(h, b, 256)
        if b.value == "WorkBuddyPetNative":
            out.append(h)
        return True

    u.EnumWindows(P(cb), 0)
    return out[0] if out else None


def menu_count():
    P = ctypes.WINFUNCTYPE(wt.BOOL, wt.HWND, wt.LPARAM)
    out = []

    def cb(h, l):
        b = ctypes.create_unicode_buffer(256)
        u.GetClassNameW(h, b, 256)
        if b.value == "#32768" and u.IsWindowVisible(h):
            out.append(h)
        return True

    u.EnumWindows(P(cb), 0)
    return len(out)


WM_NCHITTEST = 0x0084


def hit(hwnd, sx, sy):
    """问窗口：这点算不算"打在我身上"。2=HTCAPTION(可拖)，-1=HTTRANSPARENT(穿透)"""
    lp = ((sy & 0xFFFF) << 16) | (sx & 0xFFFF)
    return u.SendMessageW(hwnd, WM_NCHITTEST, 0, lp)


def pick_opaque_point(hwnd):
    """不同姿态的立绘透明区域不一样，几何中心可能是空的 —— 必须现探测。
    命中判定按像素 alpha 走，所以这里直接用 WM_NCHITTEST 扫描出一个不透明点。"""
    r = wt.RECT()
    u.GetWindowRect(hwnd, ctypes.byref(r))
    w, h = r.right - r.left, r.bottom - r.top
    stage = h - round(95 * u.GetDpiForWindow(hwnd) / 96)
    cands = []
    # 立绘区域网格 + HUD 兜底（HUD 永远不透明）
    for fy in (0.30, 0.45, 0.60, 0.75):
        for fx in (0.30, 0.50, 0.70):
            cands.append((r.left + int(w * fx), r.top + int(stage * fy)))
    for fy in (0.90, 0.97):
        cands.append((r.left + int(w * 0.5), r.top + int(h * fy)))
    for (x, y) in cands:
        if hit(hwnd, x, y) == 2:
            return x, y
    return None


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 5
    hwnd = find_pet()
    if not hwnd:
        print("✗ 桌宠没在跑")
        return 1
    r = wt.RECT()
    u.GetWindowRect(hwnd, ctypes.byref(r))
    print(f"桌宠 ({r.left},{r.top}) {r.right-r.left}x{r.bottom-r.top}；连续右键 {n} 次\n")

    ok = 0
    for i in range(1, n + 1):
        spot = pick_opaque_point(hwnd)
        if not spot:
            print(f"  第 {i} 次：找不到不透明点，跳过")
            continue
        x, y = spot
        # 谁在最上面？命中测试说"打得到"不代表点击真的会送过来
        top = u.WindowFromPoint(wt.POINT(x, y))
        tb = ctypes.create_unicode_buffer(256)
        u.GetClassNameW(top, tb, 256)
        mine = "桌宠" if top == hwnd else f"被 {tb.value}(0x{top:X}) 盖住"
        print(f"  第 {i} 次准备：点 ({x},{y}) 命中={hit(hwnd, x, y)} 顶层={mine}")
        rclick(x, y)
        time.sleep(1.0)
        c = menu_count()
        if c:
            ok += 1
        print(
            f"  第 {i} 次：点 ({x},{y}) 命中={hit(hwnd, x, y)} "
            f"菜单窗口数={c}  → {'✓' if c else '✗'}"
        )
        if c:
            esc()
            time.sleep(0.8)
            left = menu_count()
            if left:
                print(f"           ⚠ ESC 后仍有 {left} 个菜单未关")
        time.sleep(0.4)

    print(f"\n结果：{ok}/{n} 次成功弹出菜单  → {'✓ 稳定' if ok == n else '✗ 不稳定'}")
    return 0 if ok == n else 1


if __name__ == "__main__":
    sys.exit(main())
