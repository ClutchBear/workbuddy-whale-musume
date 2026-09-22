//! 日志：写到 exe 同目录的 `workbuddy-pet.log`。
//!
//! 默认只在 `WORKBUDDY_PET_TRACE=1` 时输出；`log_always` / `logf_force` 不受开关控制
//! ——双击启动、panic、单实例退出这类必须留痕。

#![allow(unused_imports)]

use crate::core;
use crate::platform::now_ms;

pub(crate) fn trace() -> bool {
    std::env::var("WORKBUDDY_PET_TRACE").map(|v| v == "1").unwrap_or(false)
}

pub(crate) fn logf(s: &str) {
    if !trace() {
        return;
    }
    logf_force(s);
}

/// 不受 WORKBUDDY_PET_TRACE 开关控制的强制日志（双击启动也能排查天气测试）。
pub(crate) fn logf_force(s: &str) {
    let mut p = std::env::current_exe().unwrap_or_default();
    p.pop();
    p.push("workbuddy-pet.log");
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
        let t = core::local_time(now_ms());
        let _ = writeln!(f, "[{:02}:{:02}:{:02}] {}", t.hour, t.minute, (now_ms() / 1000) % 60, s);
    }
}

pub(crate) fn log_always(s: &str) {
    let mut p = std::env::current_exe().unwrap_or_default();
    p.pop();
    p.push("workbuddy-pet.log");
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
        let _ = writeln!(f, "{}", s);
    }
}
