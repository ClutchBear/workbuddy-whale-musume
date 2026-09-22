//! 鲸鱼娘桌宠 · 原生版（Windows）
//!
//! 把 dsh-whale-musume（浏览器插件）的全套功能搬到原生分层窗口：
//! 状态机在 `core.rs`，静态数据在 `data.rs`，绘制在 `render.rs`，
//! 窗口 / 输入 / 调度在 `app/`，平台原语在 `platform/`。
//!
//! 为什么不用 WebView / 浏览器内核：这台机器上 WebView2 渲染进程首屏之后
//! 会冻结定时器、事件与 rAF，桌宠这类「一直动」的东西根本跑不起来。
//! 原生分层窗口（UpdateLayeredWindow）反而更小更稳，也没有运行时依赖。
//!
//! 依赖方向（箭头指向被依赖方，无环）：
//!
//! ```text
//! main ─→ app/* ─┬─→ platform/*   Win32 原语（时间 / 显示器 / 菜单 / 托盘）
//!                ├─→ log          日志
//!                └─→ core / data / state / render / assets / weather
//! ```
//!
//! `App` 定义在 `app/mod.rs`：它 71 个字段保持扁平，因为各子模块都是它的
//! 后代模块，本来就能访问私有字段；拆字段留到下一档（见 docs）。

#![windows_subsystem = "windows"]

mod app;
mod assets;
mod core;
mod data;
mod log;
mod platform;
mod render;
mod state;
mod weather;

fn main() {
    app::run();
}
