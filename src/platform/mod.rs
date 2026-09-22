//! 平台原语（Win32）：时间、显示器工作区、默认落点。
//!
//! 这一层**不依赖 `App`**；凡需要应用状态的一律留在 `app/` 下。
//! 注意：业务代码里仍散落着直接调用 Win32 的 `unsafe` 块——把它们全部下沉需要
//! 先定义 trait 端口，属于下一档重构；本轮只做「平台原语集中」。

#![allow(unused_imports)]

mod menu;
mod tray;

pub(crate) use menu::*;
pub(crate) use tray::*;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use std::sync::{Mutex, OnceLock};

use crate::assets;
use crate::log::{logf, trace};

/* ============================ 基础工具 ============================ */

pub fn now_ms() -> i64 {
    let ft = unsafe { windows::Win32::System::SystemInformation::GetSystemTimeAsFileTime() };
    let v = ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64;
    (v / 10_000) as i64 - 11_644_473_600_000
}

pub(crate) fn default_pos(w: i32, h: i32) -> (i32, i32) {
    unsafe {
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetMonitorInfoW(mon, &mut mi).as_bool() {
            let r = mi.rcWork;
            return (r.right - w - 24, r.bottom - h - 24);
        }
        (GetSystemMetrics(SM_CXSCREEN) - w - 24, GetSystemMetrics(SM_CYSCREEN) - h - 24)
    }
}

pub(crate) fn work_area(hwnd: HWND) -> (i32, i32, i32, i32) {
    unsafe {
        // 用窗口所在显示器：多显示器下光标可能与窗口不同屏，
        // 按光标取工作区会把窗口钳到别的屏幕边缘（实测窗口被拽到顶边）
        let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetMonitorInfoW(mon, &mut mi).as_bool() {
            let r = mi.rcWork;
            return (r.left, r.top, r.right - r.left, r.bottom - r.top);
        }
        (0, 0, GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN))
    }
}
