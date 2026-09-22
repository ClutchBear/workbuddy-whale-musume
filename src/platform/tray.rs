//! 托盘图标：添加 / 更新 / 移除，以及由 `assets/emoji/*.png` 生成的 HICON。
//!
//! 只吃「姿势名」等纯参数，不碰应用状态，因此能待在平台层。

#![allow(unused_imports)]

use super::*;

pub(crate) const WM_TRAY: u32 = 0x8000 + 2;

/* ============================ 托盘 ============================ */

pub(crate) static TRAY_ADDED: OnceLock<Mutex<bool>> = OnceLock::new();

pub(crate) fn add_tray(hwnd: HWND) {
    let _ = TRAY_ADDED.set(Mutex::new(false));
    update_tray(hwnd, "idle-cute", "鲸鱼娘 · 待机");
}

pub(crate) fn tray_label(state: &str) -> &'static str {
    match state {
        "tool" => "鲸鱼娘 · 工作中",
        "afk" => "鲸鱼娘 · 打盹中",
        "failure" => "鲸鱼娘 · 读库失败",
        _ => "鲸鱼娘 · 待机",
    }
}

pub(crate) fn update_tray(hwnd: HWND, pose: &str, tip: &str) {
    unsafe {
        let mut data: NOTIFYICONDATAW = std::mem::zeroed();
        data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = hwnd;
        data.uID = 1;
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        data.uCallbackMessage = WM_TRAY;
        if let Ok(mut added) = TRAY_ADDED.get().unwrap().lock() {
            let icon = make_icon(pose);
            data.hIcon = icon;
            let tips: Vec<u16> = tip.encode_utf16().take(127).chain(std::iter::once(0)).collect();
            data.szTip[..tips.len()].copy_from_slice(&tips);
            let ok = Shell_NotifyIconW(if *added { NIM_MODIFY } else { NIM_ADD }, &data);
            if ok.as_bool() {
                *added = true;
            }
        }
    }
}

pub(crate) fn make_icon(pose: &str) -> HICON {
    unsafe {
        let s = 32i32;
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() {
            return HICON::default();
        }
        let mut bmi = BITMAPINFO::default();
        bmi.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: s,
            biHeight: -s,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0, // BI_RGB
            ..Default::default()
        };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let color = CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
            .unwrap_or_default();
        let mut mask_bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let mask_bm = CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut mask_bits, None, 0)
            .unwrap_or_default();
        if color.is_invalid() || bits.is_null() {
            let _ = DeleteDC(dc);
            return HICON::default();
        }
        if let Some(f) = assets::get(pose) {
            for y in 0..s {
                for x in 0..s {
                    let fx = (x * f.w / s) as i32;
                    let fy = (y * f.h / s) as i32;
                    let i = ((fy * f.w + fx) * 4) as usize;
                    let a = f.rgba[i + 3];
                    let o = ((y * s + x) * 4) as usize;
                    let p = (bits as usize + o) as *mut u8;
                    let af = a as u32;
                    // DIB 字节序 BGR：byte0=蓝，需放 rgba[i+2]
                    *p = ((f.rgba[i + 2] as u32 * af) / 255) as u8;
                    *p.add(1) = ((f.rgba[i + 1] as u32 * af) / 255) as u8;
                    *p.add(2) = ((f.rgba[i] as u32 * af) / 255) as u8;
                    *p.add(3) = a;
                }
            }
        }
        let _ = DeleteDC(dc);
        let mut ii = ICONINFO::default();
        ii.fIcon = true.into();
        ii.hbmColor = color;
        ii.hbmMask = mask_bm;
        let ic = CreateIconIndirect(&ii);
        let _ = DeleteObject(HGDIOBJ(color.0));
        if !mask_bm.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(mask_bm.0));
        }
        ic.unwrap_or_default()
    }
}

pub(crate) fn remove_tray(hwnd: HWND) {
    unsafe {
        let mut data: NOTIFYICONDATAW = std::mem::zeroed();
        data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = hwnd;
        data.uID = 1;
        let _ = Shell_NotifyIconW(NIM_DELETE, &data);
    }
}
