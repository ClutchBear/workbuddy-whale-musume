//! 应用层：窗口、输入、调度与绘制入口。
//!
//! 分工：
//!
//! | 文件 | 职责 |
//! |---|---|
//! | `boot.rs`     | 进程启动（单实例 / 建窗 / 起线程）与开场、跨日 |
//! | `sample.rs`   | 只读采样 WorkBuddy 的 SQLite 会话库 |
//! | `speech.rs`   | 气泡打字机、台词库挑选、姿势与情绪切换 |
//! | `events.rs`   | 业务事件 → 成长值 / 任务 / 成就 / 主动行为 |
//! | `fx.rs`       | 粒子与头顶大表情 |
//! | `interact.rs` | 点击吉祥物、菜单命令、键盘与文本编辑 |
//! | `pointer.rs`  | 拖动、甩动惯性、右键点击分发、窗口显隐 |
//! | `tick.rs`     | 主循环：状态机推进与各项定时 |
//! | `paint.rs`    | 重绘、贴屏（UpdateLayeredWindow）、列表页滚动上限 |
//! | `physics.rs`  | 速度采样、缓动、松手回弹 |
//! | `wndproc.rs`  | 窗口过程与消息分发（含同线程重入守卫） |
//!
//! `App` 的字段保持扁平：各子模块都是它的**后代模块**，天然可访问私有字段，
//! 不必给 71 个字段逐个加 `pub`。子模块统一 `use super::*;` 即可拿到全部名字。

#![allow(unused_imports)]

mod boot;
mod events;
mod fx;
mod game;
mod interact;
mod paint;
mod physics;
mod pointer;
mod sample;
mod speech;
mod tick;
mod wndproc;

pub(crate) use boot::*;
pub(crate) use events::*;
pub(crate) use fx::*;
pub(crate) use game::*;
pub(crate) use interact::*;
pub(crate) use paint::*;
pub(crate) use physics::*;
pub(crate) use pointer::*;
pub(crate) use sample::*;
pub(crate) use speech::*;
pub(crate) use tick::*;
pub(crate) use wndproc::*;

use std::sync::{Mutex, OnceLock};

use crate::assets;
use crate::core;
use crate::core::{Event, Rng, Signals};
use crate::data;
use crate::log::*;
use crate::platform::*;
use crate::render;
use crate::render::{Canvas, HitRect, Page, Particle, Theme, WIN_H, WIN_W};
use crate::state;
use crate::state::AppState;
use crate::weather;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub(crate) const CLASS: PCWSTR = w!("WorkBuddyPetNative");
pub(crate) const WM_STATUS_UPDATE: u32 = 0x8000 + 1; // WM_APP + 1
const WM_TEST_DONE: u32 = 0x8000 + 2; // 测试连接后台线程完成
const WM_WEATHER_DONE: u32 = 0x8000 + 3; // 定时天气刷新后台线程完成

/// 「测试连接」后台线程的结果槽（UI 线程收 WM_TEST_DONE 后取走）
static TEST_OUT: std::sync::Mutex<Option<Result<String, String>>> = std::sync::Mutex::new(None);
/// 定时天气刷新后台线程的结果槽
pub(crate) static WX_OUT: std::sync::Mutex<Option<crate::weather::Weather>> = std::sync::Mutex::new(None);

pub(crate) const POLL_MS: u64 = 1500;
/// 心跳超过这个时长没更新就判为悬挂（崩溃残留的 working 记录）
pub(crate) const STALE_MS: i64 = 10 * 60_000;

/* ============================ 应用状态 ============================ */

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Editing {
    None,
    City,
    Key,
}

pub(crate) struct App {
    hwnd: HWND,
    canvas: Canvas,
    hits: Vec<HitRect>,
    scale: f32,
    st: AppState,
    rng: Rng,

    status: Status,
    prev_working: i32,
    prev: core::PrevState,
    state_name: &'static str,

    pose: String,
    /// 当前实际画出来的姿势（换装动画的目标态，对齐原版 layerState）
    shown_pose: String,
    /// 换装两段动画（原版 motion-hide swap：下压 140ms → 换图 → 弹起 480ms）
    swap: Option<Swap>,
    /// 临时情绪立绘 (姿势, 到期, animate)——animate=false 时切姿势不播动画
    /// （对齐原版 showMood(kind,duration,animate)，feed/poke/praise/star 不传 animate）
    mood: Option<(String, i64, bool)>,
    /// 待机微动作 (0=hop 1=squint, 开始时刻)，对齐原版 18-28s 随机
    micro: Option<(u8, i64)>,
    next_micro_at: i64,
    /// 点击 react 弹跳（原版 dsh-whale-react，每次点击必播）
    react_at: i64,
    /// 三连击一次性旋转（原版 dsh-whale-spin 850ms，非持续自转）
    spin_at: i64,
    /// 头顶大表情 burst（原版 burst("🍰") 等，wm-burst 900ms）
    bursts: Vec<(u32, i64)>,
    boot_at: i64,
    bubble: (String, i64),
    // 对齐原版打字机气泡：.0 全文 / .1 气泡到期（开播 +4500ms）
    bubble_shown: usize,        // 已逐字显示的字符数（按 char 计，中文安全）
    bubble_next_char_at: i64,   // 下一字揭示时刻
    bubble_pending: String,     // 打字中来新台词：先补全当前句，160ms 后播这条
    bubble_pending_at: i64,
    particles: Vec<Particle>,

    page: Page,
    scroll: i32,
    focus_row: i32,
    editing: Editing,
    edit_buf: String,

    dragging: bool,
    moved: bool,
    drag_origin: POINT,
    win_origin: POINT,
    drag_angle: f32,
    pos_samples: Vec<(i32, i32, i64)>,
    inertia: (f32, f32, f32),

    weather: Option<weather::Weather>,
    fx_t: f32,
    cursor_cell: usize,

    busy_since: i64,
    stuck_since: i64,
    away_at: i64,
    proactive_last: i64,
    next_chat_at: i64,
    next_action_at: i64,
    next_tick_at: i64,
    next_weather_at: i64,
    /// 定时刷新已在后台线程跑（防重复 spawn）
    weather_busy: bool,
    next_save_at: i64,
    last_redraw_at: i64,
    tray_sig: String,
    last_click_at: i64,
    // 对齐原版 patMascot 的连击/节流状态
    pat_history: Vec<i64>,
    last_triple_at: i64,
    celebrate_until: i64,
    last_pat_at: i64,
    last_pat_speech_at: i64,
    // 对齐原版 applyReleasePhysics：拖动最后事件的位移(px) 与松手动画
    drag_last: (i32, i32),
    drag_last_delta: (i32, i32),
    released: bool,
    move_dbg_n: u32,
    release_squash: Option<(i64, i64)>, // 轻放 squash 回弹 (开始时刻, 时长ms=300)
    angle_anim: Option<(f32, i64, i64)>, // (起始角度, 开始时刻, 时长ms)
    // 气泡 pop（出现 220ms）与 out（消失前 200ms 淡出），对齐原版 wm-pop / dsh-whale-out
    bubble_pop_at: i64,
    bubble_out_at: i64,
    click_count: i32,
    timer_count: u64,
    visible: bool,
    last_day: String,
}

pub(crate) static APP: OnceLock<Mutex<App>> = OnceLock::new();

/// App 里全是本窗口的句柄，只在创建窗口的那个线程上用；
/// 静态量要求 Send，这里明确声明（HWND / HDC 是裸指针，编译器推不出来）。
unsafe impl Send for App {}

thread_local! {
    static WND_PROC_IN: std::cell::Cell<bool> = std::cell::Cell::new(false);
}
