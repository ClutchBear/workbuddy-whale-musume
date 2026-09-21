"""一次性补丁：把上一轮并行 Edit 互相覆盖掉的修改补回来。
   本机已知坑：同一文件的多个 Edit 并行发会互相覆盖，所以合并成单脚本顺序替换。
"""
import io, sys, re

ROOT = r"D:\work\demo\workbuddy-pet-native"


def patch(path, pairs, must=True):
    p = ROOT + "\\" + path
    s = io.open(p, encoding="utf-8").read()
    for old, new in pairs:
        if old not in s:
            print("  !! 未命中 [%s]: %r" % (path, old[:70]))
            continue
        s = s.replace(old, new, 1)
    io.open(p, "w", encoding="utf-8", newline="").write(s)
    print("  已改 %s" % path)


# ---------------- render.rs ----------------
patch("src/render.rs", [
    ("                biCompression: DIB_RGB_COLORS,\n                biSizeImage: 0,",
     "                biCompression: 0, // BI_RGB\n                biSizeImage: 0,"),
    ("            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();\n"
     "            let dib = CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0);\n"
     "            if dib.is_invalid() || bits.is_null() {\n"
     "                let _ = DeleteDC(dc);\n"
     "                return None;\n"
     "            }\n"
     "            let _old = SelectObject(dc, dib);",
     "            let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();\n"
     "            let dib = match CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {\n"
     "                Ok(b) => b,\n"
     "                Err(_) => {\n"
     "                    let _ = DeleteDC(dc);\n"
     "                    return None;\n"
     "                }\n"
     "            };\n"
     "            if bits.is_null() {\n"
     "                let _ = DeleteDC(dc);\n"
     "                return None;\n"
     "            }\n"
     "            let _old = SelectObject(dc, HGDIOBJ(dib.0));"),
    ("            let old = SelectObject(self.dc, self.font(size_idx));\n"
     "            SetTextColor(self.dc, COLORREF(rgb(c.0, c.1, c.2)));",
     "            let old = SelectObject(self.dc, HGDIOBJ(self.font(size_idx).0));\n"
     "            SetTextColor(self.dc, COLORREF(rgb(c.0, c.1, c.2)));"),
    ("        unsafe {\n            for i in 0..(w * h) as usize {",
     "        let total = (w as usize) * (h as usize);\n        unsafe {\n            for i in 0..total {"),
    ("        draw_fx(cv, fx, v.fx_t, m(WIN_W), m(WIN_H), v.badge_kind == 1);",
     "        draw_fx(cv, &fx, v.fx_t, m(WIN_W), m(WIN_H), v.badge_kind == 1);"),
    ("    for i in 0..16 {\n        let r = i / 4;\n        let c = i % 4;\n"
     "        let x = gx + c * (cell + gap);\n        let y = grid_top + r * (cell + gap);",
     "    for i in 0..16usize {\n        let r = i / 4;\n        let c = i % 4;\n"
     "        let x = gx + (c as i32) * (cell + gap);\n        let y = grid_top + (r as i32) * (cell + gap);"),
])

# ---------------- core.rs ----------------
patch("src/core.rs", [
    ("use crate::data::*;\nuse windows::Win32::Foundation::{FILETIME, SYSTEMTIME};",
     "use serde::{Deserialize, Serialize};\n\nuse crate::data::*;\n"
     "use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};"),
    ("                Some((_, _, m, target, _, _)) if !s.claimed && m == metric => QuestSlot {",
     "                // quest_def → (desc, metric, target, affinity, mood, always)\n"
     "                Some((_, m, target, _, _, _)) if !s.claimed && m == metric => QuestSlot {"),
    ("        if let Some((_, _, _, target, _, _)) = quest_def(&s.id) {",
     "        if let Some((_, _, target, _, _, _)) = quest_def(&s.id) {"),
    ("                if let Some((_, _, _, target, _, _)) = quest_def(&s.id) {",
     "                if let Some((_, _, target, _, _, _)) = quest_def(&s.id) {"),
    ("    let (aff, md) = quest_def(id).map(|q| (q.3, q.4)).unwrap_or((0, 0));",
     "    let (aff, md) = quest_def(id).map(|q| (q.3, q.4)).unwrap_or((0, 0));"),
])

# ---------------- main.rs ----------------
patch("src/main.rs", [
    ("pub fn now_ms() -> i64 {\n"
     "    let mut ft = FILETIME::default();\n"
     "    unsafe { windows::Win32::System::SystemInformation::GetSystemTimeAsFileTime(&mut ft) };\n"
     "    let v = ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64;",
     "pub fn now_ms() -> i64 {\n"
     "    let ft = unsafe { windows::Win32::System::SystemInformation::GetSystemTimeAsFileTime() };\n"
     "    let v = ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64;"),
    ("static APP: OnceLock<Mutex<App>> = OnceLock::new();",
     "static APP: OnceLock<Mutex<App>> = OnceLock::new();\n\n"
     "/// App 里全是本窗口的句柄，只在创建窗口的那个线程上用；\n"
     "/// 静态量要求 Send，这里明确声明（HWND / HDC 是裸指针，编译器推不出来）。\n"
     "unsafe impl Send for App {}"),
    ("            let _ = PostMessageW(HWND(hwnd_val as *mut _), WM_STATUS_UPDATE, WPARAM(0), LPARAM(0));",
     "            let _ = PostMessageW(Some(HWND(hwnd_val as *mut _)), WM_STATUS_UPDATE, WPARAM(0), LPARAM(0));"),
    ("            if !p.is_null() && ((*p).flags & SWP_NOMOVE).0 == 0 {",
     "            if !p.is_null() && (((*p).flags & SWP_NOMOVE).0) == 0 {"),
    ("        let color = CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0);\n"
     "        let mut mask_bits: *mut core::ffi::c_void = std::ptr::null_mut();\n"
     "        let mask_bm = CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut mask_bits, None, 0);",
     "        let color = CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)\n"
     "            .unwrap_or_default();\n"
     "        let mut mask_bits: *mut std::ffi::c_void = std::ptr::null_mut();\n"
     "        let mask_bm = CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut mask_bits, None, 0)\n"
     "            .unwrap_or_default();"),
    ("        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();",
     "        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();"),
    ("        let _ = TrackPopupMenu(menu, TPM_LEFTALIGN | TPM_RIGHTBUTTON, x, y, 0, hwnd, None);",
     "        let _ = TrackPopupMenu(menu, TPM_LEFTALIGN | TPM_RIGHTBUTTON, x, y, Some(0), hwnd, None);"),
])
print("done")
