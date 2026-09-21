"""用合成鼠标/键盘事件真机验证桌宠交互。

注意：这里特意**不用** SendMessage 直接给窗口发 WM_RBUTTONUP —— 那是绕过系统的"走后门"，
证明不了真实点击路径。必须用 mouse_event/keybd_event 走系统输入队列。

验证项：
  1. 右键点击立绘 -> 弹出菜单（检测 #32768 弹出菜单窗口 + 日志）
  2. 左键点击（不拖动）-> 立即刷新
  3. ESC -> 退出
"""
import ctypes
import ctypes.wintypes as wt
import subprocess
import sys
import time

u = ctypes.WinDLL("user32", use_last_error=True)
u.SetProcessDpiAwarenessContext.argtypes = [ctypes.c_void_p]
u.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))

MOUSE_MOVE, MOUSE_ABS = 0x0001, 0x8000
LDOWN, LUP = 0x0002, 0x0004
RDOWN, RUP = 0x0008, 0x0010
KEYUP = 0x0002
VK_ESC = 0x1B

SW, SH = u.GetSystemMetrics(0), u.GetSystemMetrics(1)


def move(x, y):
    u.mouse_event(MOUSE_MOVE | MOUSE_ABS,
                  int(x * 65535 / SW), int(y * 65535 / SH), 0, 0)


def rclick(x, y):
    move(x, y)
    time.sleep(0.15)
    u.mouse_event(RDOWN | MOUSE_ABS, int(x * 65535 / SW), int(y * 65535 / SH), 0, 0)
    time.sleep(0.08)
    u.mouse_event(RUP | MOUSE_ABS, int(x * 65535 / SW), int(y * 65535 / SH), 0, 0)


def lclick(x, y):
    move(x, y)
    time.sleep(0.15)
    u.mouse_event(LDOWN | MOUSE_ABS, int(x * 65535 / SW), int(y * 65535 / SH), 0, 0)
    time.sleep(0.08)
    u.mouse_event(LUP | MOUSE_ABS, int(x * 65535 / SW), int(y * 65535 / SH), 0, 0)


def drag(x, y, dx, dy):
    """按下 -> 移动 -> 松开。用来验证"拖动不会被误判成单击" """
    move(x, y)
    time.sleep(0.15)
    u.mouse_event(LDOWN | MOUSE_ABS, int(x * 65535 / SW), int(y * 65535 / SH), 0, 0)
    time.sleep(0.1)
    for i in range(1, 6):
        nx, ny = x + dx * i // 5, y + dy * i // 5
        move(nx, ny)
        time.sleep(0.05)
    time.sleep(0.1)
    u.mouse_event(LUP | MOUSE_ABS,
                  int((x + dx) * 65535 / SW), int((y + dy) * 65535 / SH), 0, 0)


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


def popup_menu_open():
    """弹出菜单的类名固定是 #32768"""
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


def log_tail(path, n=40):
    try:
        return open(path, encoding="utf-8", errors="replace").read().splitlines()[-n:]
    except FileNotFoundError:
        return ["(日志不存在)"]


def grep(lines, kw):
    return [l for l in lines if kw in l]


def main():
    log = sys.argv[1] if len(sys.argv) > 1 else r"D:\work\demo\workbuddy-pet-native\dist\workbuddy-pet.log"
    hwnd = find_pet()
    if not hwnd:
        print("✗ 没找到桌宠窗口，先把它跑起来")
        return 1
    r = wt.RECT()
    u.GetWindowRect(hwnd, ctypes.byref(r))
    w, h = r.right - r.left, r.bottom - r.top
    stage_h = h - round(95 * u.GetDpiForWindow(hwnd) / 96)
    cx, cy = r.left + w // 2, r.top + stage_h // 2
    print(f"桌宠 hwnd=0x{hwnd:X} 位置=({r.left},{r.top}) 尺寸={w}x{h}")
    print(f"右键/左键目标点 = ({cx},{cy})（立绘中心）")
    print()

    # ---- 1. 右键 -> 应弹菜单 ----
    print("① 右键点击立绘…")
    rclick(cx, cy)
    time.sleep(1.2)
    n = popup_menu_open()
    print(f"   弹出菜单窗口数 = {n}  → {'✓ 菜单已弹出' if n else '✗ 没有菜单'}")
    for l in grep(log_tail(log), "收到 WM_"):
        print("   日志:", l.strip())
    esc()  # 关掉菜单
    time.sleep(0.8)
    print(f"   ESC 后菜单窗口数 = {popup_menu_open()}  → {'✓ 已关闭' if popup_menu_open() == 0 else '✗ 仍在'}")
    print()

    # ---- 2a. 拖动 -> 不应被判成单击 ----
    print("②a 左键拖动 200px…")
    before = len(grep(log_tail(log), "单击"))
    drag(cx, cy, -180, -120)
    time.sleep(1.0)
    after = grep(log_tail(log), "单击")
    print(f"   '单击' 日志条数 {before} -> {len(after)}  → {'✓ 拖动未被误判' if len(after) == before else '✗ 被误判成单击'}")
    for l in grep(log_tail(log), "按下")[-2:]:
        print("   日志:", l.strip())
    print()

    # ---- 2b. 左键单击（不拖动）-> 立即刷新 ----
    print("②b 左键单击立绘（不移动）…")
    hwnd2 = find_pet()
    r2 = wt.RECT()
    u.GetWindowRect(hwnd2, ctypes.byref(r2))
    w2, h2 = r2.right - r2.left, r2.bottom - r2.top
    stage2 = h2 - round(95 * u.GetDpiForWindow(hwnd2) / 96)
    cx2, cy2 = r2.left + w2 // 2, r2.top + stage2 // 2
    before = len(grep(log_tail(log), "单击"))
    lclick(cx2, cy2)
    time.sleep(1.0)
    after = grep(log_tail(log), "单击")
    print(f"   '单击' 日志条数 {before} -> {len(after)}  → {'✓ 触发立即刷新' if len(after) > before else '✗ 没触发'}")
    for l in after[-2:]:
        print("   日志:", l.strip())
    print()

    # ---- 3. ESC -> 退出 ----
    print("③ 按 ESC…")
    esc()
    time.sleep(1.5)
    still = find_pet()
    print(f"   窗口还在吗: {'在' if still else '不在'}  → {'✗ 没退出' if still else '✓ 已退出'}")

    print()
    print("===== 完整日志 =====")
    for l in log_tail(log, 60):
        print(" ", l)
    return 0


if __name__ == "__main__":
    sys.exit(main())
