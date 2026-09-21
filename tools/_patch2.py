"""第二轮补丁（同前：合并成单次顺序替换，避免并行 Edit 互相覆盖）"""
import io

ROOT = r"D:\work\demo\workbuddy-pet-native"


def patch(path, pairs):
    p = ROOT + "\\" + path
    s = io.open(p, encoding="utf-8").read()
    for old, new in pairs:
        if old not in s:
            print("  !! 未命中 [%s]: %r" % (path, old[:70]))
            continue
        s = s.replace(old, new, 1)
    io.open(p, "w", encoding="utf-8", newline="").write(s)
    print("  已改 %s" % path)


patch("src/render.rs", [
    ("            let old = SelectObject(self.dc, self.font(size_idx));\n"
     "            SetTextColor(self.dc, windows::Win32::Foundation::COLORREF(rgb(c.0, c.1, c.2)));",
     "            let old = SelectObject(self.dc, HGDIOBJ(self.font(size_idx).0));\n"
     "            SetTextColor(self.dc, COLORREF(rgb(c.0, c.1, c.2)));"),
    ("            cv.text(bx + m(8), by + m(7) + i * lh, l, FS_BODY, th.text());",
     "            cv.text(bx + m(8), by + m(7) + (i as i32) * lh, l, FS_BODY, th.text());"),
])

patch("src/core.rs", [
    ("#[derive(Clone, Debug)]\npub struct Growth {",
     "#[derive(Clone, Debug, Serialize, Deserialize)]\npub struct Growth {"),
    ("#[derive(Clone, Debug)]\npub struct QuestSlot {",
     "#[derive(Clone, Debug, Serialize, Deserialize)]\npub struct QuestSlot {"),
    ("#[derive(Clone, Debug)]\npub struct Quests {",
     "#[derive(Clone, Debug, Serialize, Deserialize)]\npub struct Quests {"),
    ("pub fn pick_dialogue_avoid_recent(\n    name: &str,",
     "pub fn pick_dialogue_avoid_recent<'a>(\n    name: &str,"),
    ("    recent: &[String],\n) -> &'static str {",
     "    recent: &'a [String],\n) -> &'static str {"),
])

patch("src/main.rs", [
    ("                biCompression: DIB_RGB_COLORS,\n                ..Default::default()",
     "                biCompression: 0, // BI_RGB\n                ..Default::default()"),
    ("                e if e == WM_LBUTTONUP.0 || e == WM_LBUTTONDBLCLK.0 => toggle_visible(a),",
     "                e if e == WM_LBUTTONUP || e == WM_LBUTTONDBLCLK => toggle_visible(a),"),
    ("                e if e == WM_RBUTTONUP.0 || e == WM_CONTEXTMENU.0 => {",
     "                e if e == WM_RBUTTONUP || e == WM_CONTEXTMENU => {"),
    ("            if !p.is_null() && ((*p).flags & SWP_NOMOVE).0 == 0 {",
     "            if !p.is_null() && (((*p).flags & SWP_NOMOVE).0) == 0 {"),
    # windows 0.61 没有导出 SetFocus；编辑城市/Key 时临时抢前台即可收到 WM_CHAR
    ('                app.editing = Editing::City;\n'
     '                app.edit_buf = app.st.settings.weather_city.clone();\n'
     '                unsafe {\n'
     '                    let _ = SetFocus(Some(app.hwnd));\n'
     '                }',
     '                app.editing = Editing::City;\n'
     '                app.edit_buf = app.st.settings.weather_city.clone();\n'
     '                grab_focus(app);'),
    ('                app.editing = Editing::Key;\n'
     '                app.edit_buf = app.st.settings.weather_key.clone();\n'
     '                unsafe {\n'
     '                    let _ = SetFocus(Some(app.hwnd));\n'
     '                }',
     '                app.editing = Editing::Key;\n'
     '                app.edit_buf = app.st.settings.weather_key.clone();\n'
     '                grab_focus(app);'),
    ("fn commit_edit(app: &mut App, now: i64) {",
     "/// 进入文本编辑时临时抢前台（窗口默认 WS_EX_NOACTIVATE，否则收不到 WM_CHAR）。\n"
     "/// windows 0.61 没导出 SetFocus，用 SetForegroundWindow 达到同样效果。\n"
     "fn grab_focus(app: &mut App) {\n"
     "    unsafe {\n"
     "        let _ = SetForegroundWindow(app.hwnd);\n"
     "    }\n"
     "}\n\n"
     "fn commit_edit(app: &mut App, now: i64) {"),
])
print("done")
