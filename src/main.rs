//! 鲸鱼娘桌宠 · 原生版（Windows）
//!
//! 把 dsh-whale-musume（浏览器插件）的全套功能搬到原生分层窗口：
//! 状态机在 `core.rs`，静态数据在 `data.rs`，绘制在 `render.rs`，这里负责
//! 窗口、输入、调度与 WorkBuddy 状态采样。
//!
//! 为什么不用 WebView / 浏览器内核：这台机器上 WebView2 渲染进程首屏之后
//! 会冻结定时器、事件与 rAF，桌宠这类「一直动」的东西根本跑不起来。
//! 原生分层窗口（UpdateLayeredWindow）反而更小更稳，也没有运行时依赖。

#![windows_subsystem = "windows"]

mod assets;
mod core;
mod data;
mod render;
mod state;
mod weather;

use std::sync::{Mutex, OnceLock};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use core::{Event, Rng, Signals};
use render::{Canvas, HitRect, Page, Particle, Theme, WIN_H, WIN_W};
use state::AppState;

const CLASS: PCWSTR = w!("WorkBuddyPetNative");
const WM_STATUS_UPDATE: u32 = 0x8000 + 1; // WM_APP + 1
const WM_TEST_DONE: u32 = 0x8000 + 2; // 测试连接后台线程完成
const WM_WEATHER_DONE: u32 = 0x8000 + 3; // 定时天气刷新后台线程完成

/// 「测试连接」后台线程的结果槽（UI 线程收 WM_TEST_DONE 后取走）
static TEST_OUT: std::sync::Mutex<Option<Result<String, String>>> = std::sync::Mutex::new(None);
/// 定时天气刷新后台线程的结果槽
static WX_OUT: std::sync::Mutex<Option<crate::weather::Weather>> = std::sync::Mutex::new(None);
const WM_TRAY: u32 = 0x8000 + 2;
const POLL_MS: u64 = 1500;
/// 心跳超过这个时长没更新就判为悬挂（崩溃残留的 working 记录）
const STALE_MS: i64 = 10 * 60_000;

/* 菜单命令 id */
const M_FEED: usize = 1;
const M_POKE: usize = 2;
const M_PRAISE: usize = 3;
const M_GAME: usize = 4;
const M_CATCH: usize = 5;
const M_GROW: usize = 6;
const M_SETTINGS: usize = 7;
const M_HOME: usize = 8;
const M_QUIT: usize = 9;
const M_SHOW: usize = 10;

/* ============================ 基础工具 ============================ */

pub fn now_ms() -> i64 {
    let ft = unsafe { windows::Win32::System::SystemInformation::GetSystemTimeAsFileTime() };
    let v = ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64;
    (v / 10_000) as i64 - 11_644_473_600_000
}

fn trace() -> bool {
    std::env::var("WORKBUDDY_PET_TRACE").map(|v| v == "1").unwrap_or(false)
}

fn logf(s: &str) {
    if !trace() {
        return;
    }
    logf_force(s);
}

/// 不受 WORKBUDDY_PET_TRACE 开关控制的强制日志（双击启动也能排查天气测试）。
fn logf_force(s: &str) {
    let mut p = std::env::current_exe().unwrap_or_default();
    p.pop();
    p.push("workbuddy-pet.log");
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
        let t = core::local_time(now_ms());
        let _ = writeln!(f, "[{:02}:{:02}:{:02}] {}", t.hour, t.minute, (now_ms() / 1000) % 60, s);
    }
}

/* ============================ WorkBuddy 采样 ============================ */

#[derive(Clone, Default)]
struct Status {
    working: i32,
    working_active: i32,
    total: i32,
    title: String,
    db_ok: bool,
    sampled_at: i64,
    latest: i64,
    stale: bool,
}

fn db_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("WORKBUDDY_DB") {
        return std::path::PathBuf::from(p);
    }
    let base = std::env::var("USERPROFILE").unwrap_or_default();
    std::path::PathBuf::from(base).join(".workbuddy").join("workbuddy.db")
}

fn sample() -> Status {
    let now = now_ms();
    let mut st = Status { sampled_at: now, db_ok: false, ..Default::default() };
    let p = db_path();
    let s = p.to_string_lossy().replace('\\', "/");
    // 只读打开；WAL 模式下再退到 immutable，尽量不干扰宿主进程
    let con = match rusqlite::Connection::open_with_flags(
        format!("file:{}?mode=ro", s),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    ) {
        Ok(c) => c,
        Err(_) => match rusqlite::Connection::open_with_flags(
            format!("file:{}?immutable=1", s),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
        ) {
            Ok(c) => c,
            Err(_) => return st,
        },
    };
    st.db_ok = true;
    if let Ok(mut q) = con.prepare(
        "SELECT COUNT(*), MAX(updated_at) FROM sessions WHERE deleted_at IS NULL AND status='working'",
    ) {
        if let Ok(mut rows) = q.query([]) {
            if let Ok(Some(r)) = rows.next() {
                st.working = r.get::<_, i32>(0).unwrap_or(0);
                st.latest = r.get::<_, i64>(1).unwrap_or(0);
            }
        }
    }
    if let Ok(mut q) = con.prepare("SELECT COUNT(*) FROM sessions WHERE deleted_at IS NULL") {
        if let Ok(mut rows) = q.query([]) {
            if let Ok(Some(r)) = rows.next() {
                st.total = r.get::<_, i32>(0).unwrap_or(0);
            }
        }
    }
    // 活跃会话标题：既用于信息条，也当作「任务内容 / 关键词」的输入
    if let Ok(mut q) = con.prepare(
        "SELECT COALESCE(title,''), COALESCE(cwd,'') FROM sessions WHERE deleted_at IS NULL AND status='working' ORDER BY updated_at DESC LIMIT 1",
    ) {
        if let Ok(mut rows) = q.query([]) {
            if let Ok(Some(r)) = rows.next() {
                let t: String = r.get::<_, String>(0).unwrap_or_default();
                let c: String = r.get::<_, String>(1).unwrap_or_default();
                st.title = if t.is_empty() {
                    c.rsplit(['\\', '/']).next().unwrap_or("").to_string()
                } else {
                    t
                };
            }
        }
    }
    st.stale = st.working > 0 && st.latest > 0 && (now - st.latest) > STALE_MS;
    st.working_active = if st.stale { 0 } else { st.working };
    st
}

/* ============================ 应用状态 ============================ */

#[derive(Clone, Copy, PartialEq, Eq)]
enum Editing {
    None,
    City,
    Key,
}

struct App {
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

static APP: OnceLock<Mutex<App>> = OnceLock::new();

/// App 里全是本窗口的句柄，只在创建窗口的那个线程上用；
/// 静态量要求 Send，这里明确声明（HWND / HDC 是裸指针，编译器推不出来）。
unsafe impl Send for App {}

/* ============================ 主流程 ============================ */

fn log_always(s: &str) {
    let mut p = std::env::current_exe().unwrap_or_default();
    p.pop();
    p.push("workbuddy-pet.log");
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
        let _ = writeln!(f, "{}", s);
    }
}

fn main() {
    std::panic::set_hook(Box::new(|info| {
        let loc = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "未知位置".into());
        let msg = info.payload().downcast_ref::<&str>().map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "非字符串 panic".into());
        log_always(&format!("!!! PANIC: {msg} @ {loc}"));
    }));
    unsafe {
        // ★ 单实例：命名互斥体，已有实例在跑就直接退出（双击第二次无效）
        let mtx = CreateMutexW(None, true, w!("Local\\wb-pet-single-instance"));
        if mtx.is_ok() && GetLastError() == ERROR_ALREADY_EXISTS {
            log_always("已有 wb-pet 实例运行，本次启动退出");
            return;
        }
        // 互斥体句柄刻意不关闭：进程活着 = 持有锁，进程退出系统自动释放
        let _ = mtx;
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let mut dir = std::env::current_exe().unwrap_or_default();
        dir.pop();
        dir.push("assets");
        assets::init(dir.clone());
        let all = assets::scan();
        assets::start_prefetch(all.clone());
        logf(&format!("立绘 {} 张，目录 {:?}", all.len(), dir));

        let hmodule = GetModuleHandleW(None).unwrap_or_default();
        let hinstance = HINSTANCE(hmodule.0);
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance,
            lpszClassName: CLASS,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);

        let scale = (GetDpiForSystem() as f32 / 96.0).max(1.0);
        let w = (WIN_W as f32 * scale) as i32;
        let h = (WIN_H as f32 * scale) as i32;

        let hwnd = match CreateWindowExW(
            // ⚠ 不加 WS_EX_NOACTIVATE：本机实测对 NOACTIVATE 窗口按下鼠标时，
            // 系统激活流程会死锁（AppHangB1，卡在 WM_ACTIVATE 后无下文）。
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            CLASS,
            w!("鲸鱼娘"),
            WS_POPUP,
            CW_USEDEFAULT,
            0,
            w,
            h,
            None,
            None,
            Some(hinstance),
            None,
        ) {
            Ok(h) => h,
            Err(_) => {
                logf("创建窗口失败");
                return;
            }
        };

        let canvas = match Canvas::new(w, h, scale) {
            Some(c) => c,
            None => {
                logf("创建画布失败");
                return;
            }
        };

        let mut st = AppState::load();
        let now = now_ms();
        if st.companion_since == 0 {
            st.companion_since = now;
        }
        st.last_interaction = now;

        let mut app = App {
            hwnd,
            canvas,
            hits: Vec::new(),
            scale,
            st,
            rng: Rng::new(now as u64 ^ 0x5f3759df),
            status: Status::default(),
            prev_working: 0,
            prev: core::PrevState {
                state: "boot".into(),
                since: now,
                last_speech_at: 0,
                streak: 0,
                line_count: 0,
            },
            state_name: "idle",
            pose: "idle-cute".into(),
            shown_pose: "idle-cute".into(),
            swap: None,
            mood: None,
            micro: None,
            next_micro_at: 0,
            react_at: 0,
            spin_at: 0,
            bursts: Vec::new(),
            boot_at: now,
            bubble: (String::new(), 0),
            bubble_shown: 0,
            bubble_next_char_at: 0,
            bubble_pending: String::new(),
            bubble_pending_at: 0,
            particles: Vec::new(),
            page: Page::Pet,
            scroll: 0,
            focus_row: -1,
            editing: Editing::None,
            edit_buf: String::new(),
            dragging: false,
            moved: false,
            drag_origin: POINT::default(),
            win_origin: POINT::default(),
            drag_angle: 0.0,
            pos_samples: Vec::new(),
            inertia: (0.0, 0.0, 0.0),
            weather: None,
            fx_t: 0.0,
            cursor_cell: 0,
            busy_since: 0,
            stuck_since: now,
            away_at: 0,
            proactive_last: 0,
            next_chat_at: now + 60_000,
            next_action_at: now + 35_000, // 原版大动作间隔 35-60s
            next_tick_at: now + 60_000,
            next_weather_at: now,
            weather_busy: false,
            next_save_at: now + 30_000,
            last_redraw_at: 0,
            tray_sig: String::new(),
            last_click_at: 0,
            pat_history: Vec::new(),
            last_triple_at: 0,
            celebrate_until: 0,
            last_pat_at: 0,
            last_pat_speech_at: 0,
            drag_last: (0, 0),
            drag_last_delta: (0, 0),
            released: false,
            move_dbg_n: 0,
            release_squash: None,
            angle_anim: None,
            bubble_pop_at: 0,
            bubble_out_at: 0,
            click_count: 0,
            timer_count: 0,
            visible: true,
            last_day: core::day_key(now),
        };

        let (x, y) = match (app.st.float_x, app.st.float_y) {
            (Some(fx), Some(fy)) => (fx, fy),
            _ => default_pos(w, h),
        };
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);

        let _ = APP.set(Mutex::new(app));
        add_tray(hwnd);

        // 采样线程
        let hwnd_val = hwnd.0 as isize;
        std::thread::spawn(move || loop {
            let s = sample();
            if let Ok(mut g) = APP.get().unwrap().lock() {
                g.status = s;
            }
            let _ = PostMessageW(Some(HWND(hwnd_val as *mut _)), WM_STATUS_UPDATE, WPARAM(0), LPARAM(0));
            std::thread::sleep(std::time::Duration::from_millis(POLL_MS));
        });

        // 动画帧 ~33ms
        let _ = SetTimer(Some(hwnd), 1, 33, None);

        {
            let mut g = APP.get().unwrap().lock().unwrap();
            on_startup(&mut g, now);
            redraw(&mut g);
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }
    }
}

fn default_pos(w: i32, h: i32) -> (i32, i32) {
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

/* ============================ 开场 / 跨日 ============================ */

fn on_startup(app: &mut App, now: i64) {
    let today = core::day_key(now);
    // 每日签到
    if app.st.growth.last_signin != today {
        apply_event(app, Event::Signin, now, None);
        push_quest(app, "signin", 1, now);
        let r = core::compute_week_signin(app.st.week.as_ref(), &today, now);
        if r.milestone.is_some() {
            apply_event(app, Event::Weekly, now, None);
            app.st.journal_add("weekly", &format!("本周签到达成 {} 格", r.week.days.len()), now);
        }
        app.st.week = Some(r.week);
        say(app, "daily", "signin", now);
    }
    app.st.quests = Some(core::refresh_quests(app.st.quests.as_ref(), now, &mut app.rng));

    // 节日换装
    let fest = core::festival_key(now);
    if !fest.is_empty() && app.st.festival_shown != today {
        app.st.festival_shown = today;
        set_mood(app, fest, now + 8_000, false);
        say(app, "daily", "holiday", now);
    }

    // 分时问候（深夜不主动打扰）
    let hour = core::local_time(now).hour;
    if !core::is_late_night(hour) && app.st.settings.pet {
        let bucket = core::greet_bucket(hour);
        app.st.last_greet_at = now;
        app.st.last_greet_bucket = bucket.to_string();
        let mut line = core::pick_dialogue("greet", bucket, 0, &mut app.rng).to_string();
        if let Some(w) = &app.weather {
            line = format!("{} · 现在 {}", line, w.summary());
        }
        set_bubble(app, &line, now);
    }
    app.next_chat_at = now + core::IDLE_CHAT_MIN_MS;
}

/* ============================ 台词 / 立绘 ============================ */

/// 换装动画状态：phase 0 = 旧图下压（140ms），phase 1 = 新图弹起（480ms）
struct Swap {
    pending: String,
    phase: u8,
    t0: i64,
}

/// 原版打字机逐字延迟：默认 64ms；「，。！？～…」260ms；空格 90ms；每第 5 字 130ms
fn type_delay(ch: char, index_after: usize) -> i64 {
    if "，。！？～…".contains(ch) {
        260
    } else if ch == ' ' {
        90
    } else if index_after % 5 == 0 {
        130
    } else {
        64
    }
}

fn start_typing(app: &mut App, line: String, now: i64) {
    // 原版 showLineNow：只有气泡本来隐藏时才播 wm-pop 出场动画
    let was_hidden = app.bubble.0.is_empty() || app.bubble.1 <= now;
    app.bubble = (line, now + 4_500); // 原版 showLineNow：bubbleHideAt = now + 4500
    app.bubble_shown = 0;
    app.bubble_next_char_at = now; // 首字立即出
    app.bubble_pending_at = 0;
    app.bubble_out_at = 0;
    if was_hidden {
        app.bubble_pop_at = now;
    }
}

/// 对齐原版 showLine()：气泡正在打字时，不重启打字机——先把当前句瞬间补全，
/// 新台词进单槽队列，160ms 后开播（scheduleNext）。
fn set_bubble(app: &mut App, line: &str, _now: i64) {
    if line.is_empty() || !app.st.settings.bubble {
        return;
    }
    let out = core::apply_names(line, &app.st.settings.title, &app.st.settings.self_name);
    app.st.push_recent(&out);
    let now = now_ms();
    let typing = app.bubble.1 > now && app.bubble_shown < app.bubble.0.chars().count();
    if typing {
        app.bubble_shown = app.bubble.0.chars().count(); // 补全当前句
        app.bubble_pending = out;
        app.bubble_pending_at = now + 160;
    } else {
        start_typing(app, out, now);
    }
}

fn say(app: &mut App, bank: &str, event: &str, now: i64) {
    let l = core::pick_dialogue_avoid_recent(bank, event, 0, &mut app.rng, &app.st.recent_lines);
    set_bubble(app, l, now);
}

/// 切换基础姿势。实际换图动画由 tick 里的派生姿势比对驱动
/// （motion-hide swap：旧图下压 140ms → 换图 → 新图弹起 480ms），
/// 这里只改基础姿势本体。
fn set_pose(app: &mut App, pose: &str) {
    if app.pose != pose {
        app.pose = pose.to_string();
    }
}

/// 临时情绪立绘（点击反应、庆祝等），到期自动回落。
/// 对齐原版 showMood(kind, duration, animate)：animate=false 时切姿势
/// 不播换装动画（feed/poke/praise/star 等菜单动作原版就不传 animate）。
fn set_mood(app: &mut App, pose: &str, until: i64, animate: bool) {
    app.mood = Some((pose.to_string(), until, animate));
}

/// 头顶大表情（原版 burst(symbol)，wm-burst 900ms 关键帧）
fn burst(app: &mut App, code: u32, now: i64) {
    app.bursts.push((code, now));
    if app.bursts.len() > 6 {
        app.bursts.remove(0);
    }
}

fn spawn_particles(app: &mut App, n: i32, kind: u8, hue: u8) {
    if !app.st.settings.particles {
        return;
    }
    for _ in 0..n {
        let a = app.rng.f64() * std::f64::consts::TAU;
        let sp = 0.6 + app.rng.f64() * 1.4;
        app.particles.push(Particle {
            x: (WIN_W / 2) as f32 + (app.rng.f64() as f32 - 0.5) * 90.0,
            y: 170.0 + (app.rng.f64() as f32 - 0.5) * 40.0,
            vx: (a.cos() * sp) as f32,
            vy: (-1.2 - app.rng.f64() * 0.9) as f32,
            life: 1.0,
            max: 1.0,
            kind,
            hue,
        });
    }
    if app.particles.len() > 90 {
        let n = app.particles.len() - 90;
        app.particles.drain(0..n);
    }
}

/* ============================ 养成事件 ============================ */

fn apply_event(app: &mut App, ev: Event, now: i64, delta_min: Option<f32>) {
    let pats = app.st.pats;
    let out = core::compute_growth(&app.st.growth, ev, now, pats, delta_min.unwrap_or(0.0));
    app.st.growth = out.growth;
    if ev == Event::Pat {
        app.st.pats += 1;
    }
    if out.leveled_up {
        set_mood(app, "levelup", now + 4_200, true); // 原版 showMood("levelup",4200,true)
        say(app, "daily", "levelup", now);
        app.st.journal_add("level", &format!("羁绊升到 Lv{}", app.st.growth.level), now);
    }
    for a in out.unlocks.iter() {
        set_mood(app, "achievement", now + 4_500, false);
        say(app, "interact", "achievement", now);
        app.st.journal_add("achv", &format!("解锁成就「{}」", core::achievement_name(a)), now);
    }
    // 羁绊里程碑
    let u = core::bond_unlocks(app.st.growth.level);
    if u.badge && app.st.badge.is_empty() {
        app.st.badge = "鲸汐守护者".to_string();
        say(app, "bond", "l5", now);
        app.st.journal_add("bond", "称号「鲸汐守护者」解锁", now);
    } else if u.action {
        app.st.journal_add("bond3", "羁绊 Lv3：解锁新的待机动作", now);
    } else if u.egg {
        app.st.journal_add("bond7", "羁绊 Lv7：隐藏彩蛋已解锁", now);
    }
    app.st.touch();
}

fn push_quest(app: &mut App, metric: &str, amount: i32, now: i64) {
    let r = core::compute_quests(app.st.quests.as_ref(), metric, amount, now, &mut app.rng);
    app.st.quests = Some(r.quests);
    for id in r.completed.iter() {
        if let Some((desc, _, _, _, _, _)) = core::quest_def(id) {
            set_bubble(app, &format!("任务可以领取啦：{}", desc), now);
        }
    }
    app.st.touch();
}

/// 用量类成就：上游靠 DOM 统计，这里用我们能观测到的等价信号
fn check_usage_achievements(app: &mut App, now: i64) {
    let mut got: Vec<String> = Vec::new();
    let u = app.st.usage.clone();
    let have = |id: &str| app.st.growth.achievements.iter().any(|a| a == id);
    let pairs: [(&str, bool); 12] = [
        ("first-tool", u.tools >= 1),
        ("tools-10", u.tools >= 10),
        ("tools-50", u.tools >= 50),
        ("tools-100", u.tools >= 100),
        ("first-code", u.code >= 1),
        ("code-20", u.code >= 20),
        ("first-success", u.successes >= 1),
        ("success-10", u.successes >= 10),
        ("first-failure", u.failures >= 1),
        ("fail-10", u.failures >= 10),
        ("keyword-master", u.keyword_hits >= 10),
        ("messages-100", u.messages >= 100),
    ];
    for (id, hit) in pairs {
        if hit && !have(id) {
            got.push(id.to_string());
        }
    }
    if u.messages >= 500 && !have("messages-500") {
        got.push("messages-500".into());
    }
    let days = (now - app.st.companion_since) / 86_400_000;
    for (id, d) in [("day1", 1i64), ("day7", 7), ("day30", 30)] {
        if days >= d && !have(id) {
            got.push(id.to_string());
        }
    }
    let hour = core::local_time(now).hour;
    if (hour >= 22 || hour < 6) && !have("night-owl") {
        got.push("night-owl".into());
    }
    if app.status.working_active > 0 && (hour >= 22 || hour < 6) && !have("night-work") {
        got.push("night-work".into());
    }
    for id in got {
        app.st.growth.achievements.push(id.clone());
        set_mood(app, "achievement", now + 3_500, true);
        burst(app, 0x1f3c5, now);
        say(app, "interact", "achievement", now);
        app.st.journal_add("achv", &format!("解锁成就「{}」", core::achievement_name(&id)), now);
    }
}

/* ============================ 主动关怀 ============================ */

fn maybe_proactive(app: &mut App, now: i64, busy: bool) {
    if !app.st.settings.proactive || now - app.proactive_last < core::Proactive::MIN_GAP_MS {
        return;
    }
    let mut fired: Option<&'static str> = None;
    if busy {
        if app.busy_since == 0 {
            app.busy_since = now;
        } else if now - app.busy_since > core::Proactive::LONG_WORK_MS {
            app.busy_since = now;
            fired = Some("long-work");
        }
        let hour = core::local_time(now).hour;
        if fired.is_none()
            && core::is_late_night(hour)
            && app.st.settings.night
            && app.busy_since > 0
            && now - app.busy_since > core::Proactive::NIGHT_WORK_MS
        {
            app.busy_since = now;
            fired = Some("late-night");
        }
    } else {
        app.busy_since = 0;
        if now - app.stuck_since > core::Proactive::STUCK_MS && app.state_name != "idle" {
            app.stuck_since = now;
            fired = Some("stuck");
        }
        if app.away_at > 0 && now - app.away_at > core::Proactive::AWAY_MS {
            let long = now - app.away_at > 2 * 3_600_000;
            app.away_at = 0;
            fired = Some("welcome-back");
            if long && !app.st.growth.achievements.iter().any(|a| a == "comeback") {
                app.st.growth.achievements.push("comeback".into());
                app.st.journal_add("achv", "解锁成就「欢迎回来」", now);
            }
        }
    }
    if let Some(kind) = fired {
        app.proactive_last = now;
        say(app, "proactive", kind, now);
        logf(&format!("主动关怀：{}", kind));
    }
}

/* ============================ 交互 ============================ */

/// 忠实移植原版 patMascot()（dsh-whale-moe.js:703）：
/// 庆祝保护 → 忙态直落拍拍 → 分区反应（tail/belly，不进连击计数）→
/// 2s 滑动窗三连（2600ms 冷却 + 2200ms 庆祝期）→ 普通拍拍（450ms 内
/// rapid-fire 只播反馈，跳过成长与台词；pat 台词节流 2500ms）。
fn on_click_mascot(app: &mut App, nx: f32, ny: f32, now: i64) {
    app.st.last_interaction = now;
    let zone = core::hit_zone(nx, ny);
    let busy = app.state_name == "tool";

    // 三连庆祝期内忽略一切点击
    if now < app.celebrate_until {
        return;
    }

    // 非忙态分区反应：肚皮 / 尾巴（原版 readPref("zones") 原生版恒开）
    if !busy && zone == "tail" {
        apply_event(app, Event::Tail, now, None);
        spawn_particles(app, 6, 1, 2);
        set_mood(app, "react-tail", now + 2_600, true); // 原版 showMood(...,2600,true)
        say(app, "interact", "tail", now);
        return;
    }
    if !busy && zone == "belly" {
        apply_event(app, Event::Belly, now, None);
        spawn_particles(app, 6, 2, 1);
        set_mood(app, "react-belly", now + 2_600, true);
        say(app, "interact", "belly", now);
        return;
    }

    // 拍拍计数：2s 滑动窗口（原版 patHistory filter）
    app.pat_history.retain(|t| now - t < 2_000);
    app.pat_history.push(now);

    // 三连：2s 内 >=3 次且距上次三连 >=2600ms
    if app.pat_history.len() >= 3 && now - app.last_triple_at >= 2_600 {
        app.pat_history.clear();
        app.last_triple_at = now;
        app.celebrate_until = now + 2_200;
        spawn_particles(app, 12, 1, 1);
        set_mood(app, "star", now + 2_200, false); // 原版 showMood("star",2200) 无 animate
        app.spin_at = now; // 原版 dsh-whale-spin 一次性 850ms 旋转
        apply_event(app, Event::Triple, now, None);
        say(app, "interact", "triple", now);
        return;
    }

    // 普通拍拍：rapid-fire（<450ms）只播姿势/粒子反馈，跳过成长与台词
    let rapid = now - app.last_pat_at < 450;
    app.last_pat_at = now;
    let pose = if busy {
        if app.rng.f64() < 0.5 { "work-pat" } else { "work-ram" }
    } else if zone == "head" {
        "react-head"
    } else {
        "blush"
    };
    let dur: i64 = if busy { 2_400 } else { 2_600 };
    set_mood(app, pose, now + dur, true); // 原版 showMood(...,busy?2400:2600,true)
    if busy {
        spawn_particles(app, 6, 2, 0);
    } else {
        spawn_particles(app, 8, 0, 0);
    }
    if rapid {
        return;
    }
    apply_event(app, Event::Pat, now, None);
    push_quest(app, "pat", 1, now);
    if now - app.last_pat_speech_at >= 2_500 {
        app.last_pat_speech_at = now;
        say(app, "interact", "pat", now);
    }
    if app.st.pats == 1 {
        app.st.journal_add("first", "第一次摸了摸她的头", now);
    }
}

fn feed(app: &mut App, now: i64) {
    app.st.last_interaction = now;
    apply_event(app, Event::Feed, now, None);
    burst(app, 0x1f370, now); // 原版 burst("🍰")
    spawn_particles(app, 8, 2, 1);
    set_mood(app, "eat", now + 3_000, false); // 原版 showMood("eat",3000) 无 animate
    say(app, "interact", "feed", now);
    push_quest(app, "feed", 1, now);
}

fn poke(app: &mut App, now: i64) {
    app.st.last_interaction = now;
    apply_event(app, Event::Poke, now, None);
    burst(app, 0x1f4a2, now); // 原版 burst("💢")
    spawn_particles(app, 4, 0, 0);
    set_mood(app, "angry", now + 3_000, false);
    say(app, "interact", "poke", now);
}

fn praise(app: &mut App, now: i64) {
    app.st.last_interaction = now;
    apply_event(app, Event::Praise, now, None);
    burst(app, 0x2728, now); // 原版 burst("✨")
    spawn_particles(app, 10, 1, 1);
    // 原版 showMood("tail-swing", 3000)：摇尾巴姿势而非星星，无 animate
    set_mood(app, "tail-swing", now + 3_000, false);
    say(app, "interact", "praise", now);
}

/* ============================ 每帧调度 ============================ */

fn tick(app: &mut App, now: i64) {
    let dt = 33.0f32 / 1000.0;
    app.fx_t += dt;

    for p in app.particles.iter_mut() {
        p.x += p.vx;
        p.y += p.vy;
        p.vy += 22.0 * dt;
        p.life -= dt * 0.55;
    }
    app.particles.retain(|p| p.life > 0.0);

    // 头顶大表情 900ms 到期清理（原版 wm-burst）
    app.bursts.retain(|(_, t0)| now - *t0 < 900);
    // 点击 react 弹跳 650ms 后移除类（原版 wm-react-soft 620ms）
    if app.react_at > 0 && now - app.react_at >= 650 {
        app.react_at = 0;
    }
    // 三连旋转 850ms 后移除类（原版 dsh-whale-spin）
    if app.spin_at > 0 && now - app.spin_at >= 850 {
        app.spin_at = 0;
    }
    // 待机微动作到期（原版 900ms 后移除 hop/squint 类）
    if let Some((_, t0)) = app.micro {
        if now - t0 >= 900 {
            app.micro = None;
        }
    }

    if let Some((_, until, _)) = app.mood {
        if now > until {
            // mood 是派生覆盖层，过期即失效，基础姿势 app.pose 原样生效
            // （回退到基础姿势会走下面的换装动画，对齐原版 setPose 行为）
            app.mood = None;
        }
    }
    // 排队台词到点开播（原版 scheduleNext 160ms）
    if app.bubble_pending_at > 0 && now >= app.bubble_pending_at {
        let line = std::mem::take(&mut app.bubble_pending);
        app.bubble_pending_at = 0;
        if !line.is_empty() {
            start_typing(app, line, now);
        }
    }
    // 打字机逐字揭示
    let total = app.bubble.0.chars().count();
    if app.bubble_shown < total && now >= app.bubble_next_char_at {
        let ch = app.bubble.0.chars().nth(app.bubble_shown).unwrap();
        app.bubble_shown += 1;
        app.bubble_next_char_at = now + type_delay(ch, app.bubble_shown);
    }
    // 到期隐藏：原版条件 = 超时 且 打字已结束（未完则等打完）。
    // 消失前先走 200ms 淡出下移动画（原版 dsh-whale-out）
    if app.bubble.1 > 0 && now > app.bubble.1 && app.bubble_shown >= total {
        if app.bubble_out_at == 0 {
            app.bubble_out_at = now;
        } else if now - app.bubble_out_at >= 200 {
            app.bubble = (String::new(), 0);
            app.bubble_shown = 0;
            app.bubble_pending_at = 0;
            app.bubble_pending.clear();
            app.bubble_out_at = 0;
            app.bubble_pop_at = 0;
        }
    }

    /* ---- 状态机 ---- */
    let sig = Signals {
        pet_disabled: !app.st.settings.pet,
        error: !app.status.db_ok,
        tool: app.status.working_active > 0,
        thinking: false,
        waiting: false,
        success_at: -1,
        curious_at: -1,
        last_interaction: app.st.last_interaction,
        dense_code: false,
    };
    let c = core::compute_state(&app.prev, &sig, now);
    app.state_name = c.state;
    if c.state != app.prev.state {
        app.prev.state = c.state.to_string();
        app.prev.since = c.since;
        app.stuck_since = now;
        app.mood = None;
        if !c.line.is_empty() {
            set_bubble(app, &c.line, now);
        }
        let mut pose = c.pose.unwrap_or("idle-cute").to_string();
        if c.state == "tool" && app.st.settings.tool_pose {
            if let Some(tp) = core::tool_pose(&app.status.title) {
                pose = tp.to_string();
            }
        }
        set_pose(app, &pose);
        if c.state == "tool" {
            say(app, "work", "tool", now);
        }
    }
    app.prev.last_speech_at = if c.speak { now } else { app.prev.last_speech_at };
    app.prev.line_count = c.line_count;
    app.prev.streak = c.streak;

    /* ---- 工作完成 / 开始 ---- */
    if app.prev_working > 0 && app.status.working_active == 0 && app.status.db_ok {
        app.st.usage.successes += 1;
        apply_event(app, Event::Success, now, None);
        push_quest(app, "success", 1, now);
        // 原版 success 状态变化：burst("★") + spawnParticles(12)
        burst(app, 0x2b50, now);
        spawn_particles(app, 12, 1, 1);
        set_mood(app, "success", now + 3_000, true); // 原版 STATE_HOLD success 3000
        say(app, "work", "success", now);
        app.st.touch();
    }
    if app.status.working_active > 0 && app.prev_working == 0 {
        app.st.usage.tools += 1;
        push_quest(app, "tool", 1, now);
        burst(app, 0x1f4ac, now); // 原版 tool/thinking 状态变化 burst("…")
        set_mood(app, "running", now + 2_000, true);
        say(app, "work", "start", now);
        app.st.touch();
    }
    app.prev_working = app.status.working_active;

    /* ---- 关键词感知：拿活跃会话标题当输入 ---- */
    if app.st.settings.keywords
        && !app.status.title.is_empty()
        && !app.st.recent_lines.iter().any(|l| *l == app.status.title)
    {
        if let Some(k) = core::match_keyword(&app.status.title) {
            app.st.usage.keyword_hits += 1;
            if let Some(p) = core::keyword_pose(k) {
                set_mood(app, p, now + 3_000, true); // 原版 showMood(KEYWORD_POSES[id],3000,true)
            }
            let l = core::pick_dialogue("keyword", k, 0, &mut app.rng).to_string();
            set_bubble(app, &l, now);
            app.st.push_recent(&app.status.title);
            app.st.touch();
        }
    }

    /* ---- 定时任务 ---- */
    if now > app.next_tick_at {
        let out = core::compute_growth(&app.st.growth, Event::Tick, now, app.st.pats, 1.0);
        app.st.growth = out.growth;
        app.next_tick_at = now + 60_000;
    }

    if now > app.next_action_at {
        // 原版大动作：随机 35-60s 一次，每张停留 4.2-5.8s（showMood 临时姿势）
        let idle = app.state_name == "idle";
        app.next_action_at = now + 35_000 + app.rng.idx(25_000) as i64;
        if idle {
            // 原版 teasing 复活：非工作台视图按 TEASE_CHANCE*4 (≈2.4%) 概率
            if app.rng.f64() < 0.024 {
                set_mood(app, "teasing", now + 3_200, true);
                say(app, "interact", "tease", now);
            } else {
                // 天气立绘优先露一小会儿（原版 idleChatTick 天气分支 6500ms）
                let mut used_weather = false;
                if let Some(w) = app.weather.clone() {
                    if app.rng.f64() < 0.35 {
                        if let Some(p) = core::weather_pose(w.kind(), w.temp) {
                            set_mood(app, p, now + 6_500, true);
                            used_weather = true;
                        }
                    }
                }
                if !used_weather {
                    let mut pool: Vec<&'static str> = data::IDLE_ACTION_POOL.to_vec();
                    if core::bond_unlocks(app.st.growth.level).action {
                        pool.push("wink");
                    }
                    let p = pool[app.rng.idx(pool.len())];
                    let dwell = 4_200 + app.rng.idx(1_600) as i64; // 原版 4200+rand(1600)
                    set_mood(app, p, now + dwell, true);
                }
            }
        }
    }

    // 待机微动作：随机 18-28s 一次，hop（720ms）或 squint（560ms），幅度轻
    if app.state_name == "idle" && !app.dragging {
        if app.next_micro_at == 0 {
            app.next_micro_at = now + 18_000 + app.rng.idx(10_000) as i64;
        } else if now >= app.next_micro_at && app.micro.is_none() {
            app.next_micro_at = now + 18_000 + app.rng.idx(10_000) as i64;
            app.micro = Some((if app.rng.f64() < 0.5 { 0 } else { 1 }, now));
        }
    }

    if now > app.next_weather_at {
        app.next_weather_at = now + weather::REFRESH_MS;
        let city = app.st.settings.weather_city.clone();
        let key = app.st.settings.weather_key.clone();
        // 定时刷新必须走后台线程：同步 fetch 会把 UI 线程卡住几十秒，
        // 期间所有 posted 消息（包括测试结果 WM_TEST_DONE）全部堆积不出。
        if !city.is_empty() && !app.weather_busy {
            app.weather_busy = true;
            let hwnd_val = app.hwnd.0 as isize;
            std::thread::spawn(move || {
                let w = weather::current(&city, &key, now);
                logf(&format!("weather: refresh done ok={}", w.is_some()));
                if let Ok(mut slot) = WX_OUT.lock() {
                    *slot = w;
                }
                unsafe {
                    let _ = PostMessageW(Some(HWND(hwnd_val as *mut _)), WM_WEATHER_DONE, WPARAM(0), LPARAM(0));
                }
            });
        }
    }

    // 结果槽轮询兜底：PostMessage 万一延迟/丢失，下一帧（33ms）也会把结果画出来
    if let Ok(mut slot) = TEST_OUT.lock() {
        if let Some(r) = slot.take() {
            match r {
                Ok(s) => set_bubble(app, &s, now),
                Err(e) => set_bubble(app, &e, now),
            }
        }
    }
    if let Ok(mut slot) = WX_OUT.lock() {
        if slot.is_some() {
            app.weather = slot.take();
            app.weather_busy = false;
        }
    }

    if now > app.next_chat_at {
        app.next_chat_at = now
            + core::IDLE_CHAT_MIN_MS
            + app.rng.idx((core::IDLE_CHAT_MAX_MS - core::IDLE_CHAT_MIN_MS) as usize) as i64;
        let hour = core::local_time(now).hour;
        let quiet = app.st.settings.night && core::is_late_night(hour);
        if !quiet && app.state_name != "tool" && app.st.settings.bubble {
            let topic = core::classify_task(&app.status.title);
            let l = if topic != "general" {
                core::pick_dialogue_avoid_recent("context", topic, 0, &mut app.rng, &app.st.recent_lines)
                    .to_string()
            } else {
                match core::mood_tier(app.st.growth.mood) {
                    "high" => core::pick_dialogue("bond", "high-mood", 0, &mut app.rng).to_string(),
                    "low" => core::pick_dialogue("bond", "low-mood", 0, &mut app.rng).to_string(),
                    _ => core::pick_dialogue("daily", "idle", 0, &mut app.rng).to_string(),
                }
            };
            set_bubble(app, &l, now);
        }
    }

    maybe_proactive(app, now, app.status.working_active > 0);

    /* ---- 派生姿势与换装动画（对齐原版 setPose 的 motion-hide swap）---- */
    let busy = app.state_name == "tool";
    let eff = if app.dragging && app.moved {
        "pick-up".to_string()
    } else if let Some((p, until, _)) = &app.mood {
        if *until > now && (!busy || p == "work-pat" || p == "work-ram") {
            (*p).clone()
        } else {
            app.pose.clone()
        }
    } else {
        app.pose.clone()
    };
    if eff != app.shown_pose {
        // animate=false 的 mood 生效时（feed/poke/praise/star 等菜单动作）直接换图
        let instant = matches!(&app.mood, Some((p, u, a)) if *u > now && *p == eff && !*a);
        if instant {
            app.swap = None;
            app.shown_pose = eff;
        } else {
            match &mut app.swap {
                None => {
                    app.swap = Some(Swap { pending: eff, phase: 0, t0: now });
                }
                Some(sw) => {
                    if sw.pending != eff {
                        if sw.phase == 1 {
                            sw.phase = 0;
                            sw.t0 = now;
                        }
                        sw.pending = eff;
                    }
                }
            }
        }
    }
    // 推进两段换装：旧图下压 140ms（cubic-bezier(0.55,0,1,0.45)）→ 换图 →
    // 新图从 translateY(18px) scale(0.88,0.94) 弹起 480ms（cubic-bezier(0.22,1,0.36,1)）
    let mut swap_done = false;
    if let Some(sw) = &mut app.swap {
        if sw.phase == 0 && now - sw.t0 >= 140 {
            app.shown_pose = std::mem::take(&mut sw.pending);
            sw.phase = 1;
            sw.t0 = now;
        } else if sw.phase == 1 && now - sw.t0 >= 480 {
            swap_done = true;
        }
    }
    if swap_done {
        app.swap = None;
    }

    let today = core::day_key(now);
    if today != app.last_day {
        app.last_day = today;
        on_startup(app, now);
    }

    check_usage_achievements(app, now);

    if now > app.next_save_at {
        app.next_save_at = now + 30_000;
        if app.st.dirty {
            app.st.save();
        }
    }

    // 小游戏推进
    if app.page == Page::Game {
        if let Some(g) = app.st.game.clone() {
            if g.status == "playing" {
                let mut gg = g.clone();
                core::game_tick(&mut gg, now, &mut app.rng);
                app.st.game = Some(gg);
            } else {
                finish_game(app, now);
            }
        } else if let Some(c) = app.st.catch.clone() {
            if c.status == "playing" {
                let mut cc = c.clone();
                core::catch_tick(&mut cc, now, &mut app.rng);
                app.st.catch = Some(cc);
            } else {
                let grade = core::game_grade(c.score);
                apply_event(app, match grade { "win" => Event::GameWin, "draw" => Event::GameDraw, _ => Event::GameLose }, now, None);
                set_bubble(app, &format!("接零食本局 {} 分", c.score), now);
                app.st.catch = None;
            }
        }
    }
}

fn finish_game(app: &mut App, now: i64) {
    let g = match app.st.game.clone() {
        Some(g) => g,
        None => return,
    };
    let grade = core::game_grade(g.score);
    let today = core::day_key(now);
    {
        let gs = &mut app.st.game_stats;
        if gs.today != today {
            gs.today = today;
            gs.plays_today = 0;
        }
        gs.plays += 1;
        gs.plays_today += 1;
        if grade == "win" {
            gs.wins += 1;
        }
        gs.combo_max = gs.combo_max.max(g.combo_max);
    }
    if g.score > app.st.game_stats.highscore {
        app.st.game_stats.highscore = g.score;
        apply_event(app, Event::HighScore, now, None);
        set_mood(app, "celebrate", now + 4_000, false);
        set_bubble(app, "刷新纪录啦！", now);
        if !app.st.growth.achievements.iter().any(|a| a == "game-highscore") {
            app.st.growth.achievements.push("game-highscore".into());
            app.st.journal_add("achv", "解锁成就「纪录刷新」", now);
        }
    } else {
        apply_event(app, match grade { "win" => Event::GameWin, "draw" => Event::GameDraw, _ => Event::GameLose }, now, None);
        set_mood(
            app,
            if grade == "win" { "game-win" } else if grade == "draw" { "game-happy" } else { "game-lose" },
            now + 3_400,
            true, // 原版 settleGame showMood(...,3400,true)
        );
    }
    // 每日养成奖励上限 3 局，多玩只计分
    let rewarded = app.st.game_stats.plays_today <= core::Game1::REWARDS_PER_DAY;
    if rewarded {
        if !app.st.growth.achievements.iter().any(|a| a == "game-first") {
            app.st.growth.achievements.push("game-first".into());
            app.st.journal_add("achv", "解锁成就「初次开玩」", now);
        }
        if grade == "win" && !app.st.growth.achievements.iter().any(|a| a == "game-win") {
            app.st.growth.achievements.push("game-win".into());
            app.st.journal_add("achv", "解锁成就「泡泡之王」", now);
        }
        if g.combo_max >= 10 && !app.st.growth.achievements.iter().any(|a| a == "game-combo10") {
            app.st.growth.achievements.push("game-combo10".into());
            app.st.journal_add("achv", "解锁成就「连击达人」", now);
        }
    }
    app.st.game = None;
    app.st.touch();
}

/* ============================ 绘制 ============================ */

fn redraw(app: &mut App) {
    let now = now_ms();
    let theme = Theme { dark: !is_light() };
    // 列表页（设置/成就/日记/任务）固定浅色「白底黑字」，对齐系统右键菜单风格；
    // 宠物页仍跟随系统深浅色。
    let list_theme = Theme { dark: false };
    app.hits.clear();
    app.canvas.clear();

    if !app.st.settings.pet {
        render::draw_recall(&mut app.canvas, theme, &mut app.hits);
        app.canvas.finish();
        present(app);
        return;
    }

    match app.page {
        Page::Pet => {
            // 姿势派生与换装推进都在 tick 里完成，这里只画 shown_pose
            let pose = app.shown_pose.clone();

            /* ---- 变换合成（全部对齐原版 CSS keyframes 数值）---- */
            let t = (now - app.boot_at) as f32;
            let mut sx = 1.0f32;
            let mut sy = 1.0f32;
            let mut mdy = 0.0f32;
            let mut ang = 0.0f32;
            // 呼吸 wm-breathe-soft：3.4s ease-in-out，0 → -4px
            let bt = (t % 3400.0) / 3400.0;
            mdy += -4.0 * 0.5 * (1.0 - (bt * std::f32::consts::TAU).cos());
            // 摇摆 wm-sway-soft：6s ease-in-out，0 → +1.2°
            let wt = (t % 6000.0) / 6000.0;
            ang += 1.2 * 0.5 * (1.0 - (wt * std::f32::consts::TAU).cos());
            if app.dragging && app.moved {
                // 原版 dragging transform：translateY(3px) rotate(--wm-drag-angle)
                mdy += 3.0;
                ang += app.drag_angle;
            } else {
                // 松手倾斜回正（angle_anim 在 WM_TIMER 里推进 drag_angle）
                ang += app.drag_angle;
                // 轻放 squash 回弹（原版 scale(1.05,0.96)→1，300ms easeOutBack）
                if let Some((t0, dur)) = app.release_squash {
                    let tt = ((now - t0) as f32 / dur as f32).min(1.0);
                    let k = ease_out_back(tt);
                    sx *= 1.05 - 0.05 * k;
                    sy *= 0.96 + 0.04 * k;
                }
            }
            // 换装两段动画
            if let Some(sw) = &app.swap {
                if sw.phase == 0 {
                    // 下压：translateY 0→12px，scale→(0.86,0.92)，140ms ease-in
                    let tt = ((now - sw.t0) as f32 / 140.0).min(1.0);
                    let k = tt * tt;
                    sx *= 1.0 - 0.14 * k;
                    sy *= 1.0 - 0.08 * k;
                    mdy += 12.0 * k;
                } else {
                    // 弹起：translateY 18px→0，scale (0.88,0.94)→1，480ms ease-out
                    let tt = ((now - sw.t0) as f32 / 480.0).min(1.0);
                    let k = ease_out_cubic(tt);
                    sx *= 0.88 + 0.12 * k;
                    sy *= 0.94 + 0.06 * k;
                    mdy += 18.0 * (1.0 - k);
                }
            }
            // 待机微动作：hop 720ms（-14px scale(1.04,0.96) rot-3°@45%）
            //            squint 560ms（-6px scale(1.04,0.94)@40%）
            if let Some((kind, t0)) = app.micro {
                if kind == 0 {
                    let tt = ((now - t0) as f32 / 720.0).min(1.0);
                    let f = if tt < 0.45 { tt / 0.45 } else { 1.0 - (tt - 0.45) / 0.55 };
                    mdy += -14.0 * f;
                    sx *= 1.0 + 0.04 * f;
                    sy *= 1.0 - 0.04 * f;
                    ang += -3.0 * f;
                } else {
                    let tt = ((now - t0) as f32 / 560.0).min(1.0);
                    let f = if tt < 0.4 { tt / 0.4 } else { 1.0 - (tt - 0.4) / 0.6 };
                    mdy += -6.0 * f;
                    sx *= 1.0 + 0.04 * f;
                    sy *= 1.0 - 0.06 * f;
                }
            }
            // 点击 react 弹跳：wm-react-soft 620ms（-12px scale(1.1,0.9) rot-4°@35%）
            if app.react_at > 0 {
                let tt = ((now - app.react_at) as f32 / 620.0).min(1.0);
                let f = if tt < 0.35 { tt / 0.35 } else { 1.0 - (tt - 0.35) / 0.65 };
                mdy += -12.0 * f;
                sx *= 1.0 + 0.10 * f;
                sy *= 1.0 - 0.10 * f;
                ang += -4.0 * f;
            }
            // 三连旋转：wm-spin-soft 850ms（50% 处 -12px rot180 scale(1.04,0.96)）
            if app.spin_at > 0 {
                let tt = ((now - app.spin_at) as f32 / 850.0).min(1.0);
                ang += 360.0 * tt;
                let b = (tt * std::f32::consts::PI).sin();
                mdy += -12.0 * b;
                sx *= 1.0 + 0.04 * b;
                sy *= 1.0 - 0.04 * b;
            }

            // 原版无底部状态面板（跑任务时也只有气泡台词），badge 文字已随面板一起删除；
            // kind 仅用于天气特效降档判定
            let kind: u8 = if !app.status.db_ok {
                3
            } else {
                match app.state_name {
                    "tool" => 1,
                    "success" => 2,
                    "failure" => 3,
                    _ => 0,
                }
            };

            // 打字机：只显示已揭示字符，未完带 ▍ 光标（原版 caret 800ms steps(1) 闪烁）
            let mut bubble = app.bubble.0.chars().take(app.bubble_shown).collect::<String>();
            if app.bubble_shown < app.bubble.0.chars().count() && now % 800 < 400 {
                bubble.push('\u{258D}');
            }
            // 气泡出场 wm-pop（220ms，近似为淡入+上浮）与消失前 dsh-whale-out（200ms 淡出+4px）
            let mut bubble_alpha = 1.0f32;
            let mut bubble_dy = 0.0f32;
            if app.bubble_pop_at > 0 {
                let tt = (now - app.bubble_pop_at) as f32 / 220.0;
                if tt < 1.0 {
                    bubble_alpha = (tt / 0.36).min(1.0);
                    bubble_dy = -3.0 * (1.0 - tt);
                } else {
                    app.bubble_pop_at = 0;
                }
            }
            if app.bubble_out_at > 0 {
                let tt = (now - app.bubble_out_at) as f32 / 200.0;
                if tt < 1.0 {
                    bubble_alpha *= 1.0 - tt;
                    bubble_dy += 4.0 * tt;
                }
            }
            let fx = if app.st.settings.weather_fx {
                app.weather.as_ref().and_then(|w| w.fx())
            } else {
                None
            };

            let v = render::PetView {
                st: &app.st,
                theme,
                pose: &pose,
                angle: ang,
                sx,
                sy,
                mdy,
                bubble: &bubble,
                bubble_alpha,
                bubble_dy,
                badge_kind: kind,
                particles: &app.particles,
                bursts: &app.bursts,
                now,
                fx,
                fx_t: app.fx_t,
                focus: app.st.settings.a11y && app.focus_row >= 0,
                focus_row: app.focus_row,
            };
            render::draw_pet(&mut app.canvas, &v, &mut app.hits);
        }
        Page::Settings => {
            render::draw_settings(&mut app.canvas, &app.st, list_theme, app.scroll, app.focus_row, &mut app.hits)
        }
        Page::Achievements => {
            render::draw_achievements(&mut app.canvas, &app.st, list_theme, app.scroll, &mut app.hits)
        }
        Page::Journal => render::draw_journal(&mut app.canvas, &app.st, list_theme, app.scroll, &mut app.hits, now),
        Page::Quests => render::draw_quests(&mut app.canvas, &app.st, list_theme, &mut app.hits),
        Page::Game => {
            if let Some(c) = app.st.catch.clone() {
                render::draw_catch(&mut app.canvas, &c, theme, &mut app.hits);
            } else if let Some(g) = app.st.game.clone() {
                let gv = render::GameView { g: &g, th: theme, cursor: app.cursor_cell };
                render::draw_game(&mut app.canvas, &gv, &mut app.hits);
            }
        }
    }

    // 浮层之前先把页面文字的 alpha 重建掉：px() 预乘混合下，GDI 字的
    // alpha=0 像素会被半透明遮罩整体丢弃 RGB（字被洗白/毁色的根因）。
    app.canvas.flush_text();

    // 列表页顶部反馈气泡：台词只画在宠物页立绘头顶的话，
    // 设置页里点「测试连接」等按钮毫无可见反馈（用户以为点不动）。
    if app.page != Page::Pet && !app.bubble.0.is_empty() {
        let shown: String = app.bubble.0.chars().take(app.bubble_shown).collect();
        if !shown.is_empty() {
            render::draw_bubble_overlay(&mut app.canvas, theme, &shown);
        }
    }

    // 输入框浮层（城市 / API Key），盖在页面之上
    if app.editing != Editing::None {
        let label = match app.editing {
            Editing::City => "输入城市名（如：北京）",
            Editing::Key => "输入 Open-Meteo API Key（可留空）",
            Editing::None => "",
        };
        render::draw_edit_box(&mut app.canvas, theme, label, &app.edit_buf, now);
    }

    app.canvas.finish();
    present(app);
}

/// 主题：跟随系统深浅色（读不到就按深色，可用 WORKBUDDY_PET_THEME 覆盖）
fn is_light() -> bool {
    match std::env::var("WORKBUDDY_PET_THEME").unwrap_or_default().as_str() {
        "light" => true,
        "dark" => false,
        _ => false,
    }
}

fn present(app: &mut App) {
    unsafe {
        let mut pt = POINT::default();
        let mut sz = SIZE { cx: app.canvas.w, cy: app.canvas.h };
        let blend = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: 1 };
        // pptDst 传 None：保持窗口当前位置（传 Some(0,0) 会每帧把窗口拽回原点）
        let _ = UpdateLayeredWindow(
            app.hwnd,
            None,
            None,
            Some(&mut sz),
            Some(app.canvas.hdc()),
            Some(&mut pt),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        );
    }
}

/* ============================ 托盘 ============================ */

static TRAY_ADDED: OnceLock<Mutex<bool>> = OnceLock::new();

fn add_tray(hwnd: HWND) {
    let _ = TRAY_ADDED.set(Mutex::new(false));
    update_tray(hwnd, "idle-cute", "鲸鱼娘 · 待机");
}

fn tray_label(state: &str) -> &'static str {
    match state {
        "tool" => "鲸鱼娘 · 工作中",
        "afk" => "鲸鱼娘 · 打盹中",
        "failure" => "鲸鱼娘 · 读库失败",
        _ => "鲸鱼娘 · 待机",
    }
}

fn update_tray(hwnd: HWND, pose: &str, tip: &str) {
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

fn make_icon(pose: &str) -> HICON {
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

fn remove_tray(hwnd: HWND) {
    unsafe {
        let mut data: NOTIFYICONDATAW = std::mem::zeroed();
        data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = hwnd;
        data.uID = 1;
        let _ = Shell_NotifyIconW(NIM_DELETE, &data);
    }
}

/* ============================ 右键菜单 ============================ */

/// 各列表页的内容高度封顶（scroll 单位 = 物理像素）。
/// 关键：布局经 m() 缩放，滚动量也必须是物理像素——旧版 `scroll * m(1)` 在
/// DPI 1.25/1.5 下 m(1) 截断成 1，封顶远小于内容高度 → 永远滚不到底（实测踩坑）。
fn max_scroll(page: Page, s: f32) -> i32 {
    let m = |x: i32| -> i32 { (x as f32 * s) as i32 };
    match page {
        // 设置页内容底 ≈ 712（20 行×30 + 3 组名×20 + 3 缝×6 + 头部 34）
        Page::Settings => m(712) - m(WIN_H) + m(8),
        // 成就墙 39 项 / 3 列 = 13 行 × 54 ≈ 736
        Page::Achievements => m(730) - m(WIN_H) + m(8),
        // 日记最近 12 条 × 34 ≈ 442
        Page::Journal => m(450) - m(WIN_H) + m(8),
        _ => 0,
    }
}

fn show_menu(hwnd: HWND, x: i32, y: i32) -> u32 {
    unsafe {
        let menu = match CreatePopupMenu() {
            Ok(m) => m,
            Err(_) => return 0,
        };
        let _ = AppendMenuW(menu, MF_STRING, M_FEED, w!("投喂小点心"));
        let _ = AppendMenuW(menu, MF_STRING, M_POKE, w!("戳一下"));
        let _ = AppendMenuW(menu, MF_STRING, M_PRAISE, w!("夸夸她"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_STRING, M_GAME, w!("小游戏：戳泡泡"));
        let _ = AppendMenuW(menu, MF_STRING, M_CATCH, w!("小游戏：接零食"));
        let _ = AppendMenuW(menu, MF_STRING, M_GROW, w!("成长面板"));
        let _ = AppendMenuW(menu, MF_STRING, M_SETTINGS, w!("设置"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_STRING, M_HOME, w!("回到原位"));
        let _ = AppendMenuW(menu, MF_STRING, M_SHOW, w!("隐藏 / 显示"));
        let _ = AppendMenuW(menu, MF_STRING, M_QUIT, w!("退出"));

        // 不调 SetForegroundWindow：对 NOACTIVATE/TOPMOST 工具窗激活流程可能死锁；
        // 菜单选择走鼠标 + TPM_RETURNCMD，无需前台。
        logf("menu: enter TrackPopupMenu (no foreground)");
        let r = TrackPopupMenu(menu, TPM_LEFTALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD, x, y, Some(0), hwnd, None);
        let r = r.0 as u32;
        logf(&format!("menu: TrackPopupMenu returned {r}"));
        let _ = DestroyMenu(menu);
        r
    }
}

/* ============================ 窗口过程 ============================ */

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // 同线程重入守卫：std Mutex 不可重入，持锁期间被同步重入（如 ReleaseCapture
    // 同步发 WM_CAPTURECHANGED）时，二次 lock 会同线程自死锁（本机实测）。
    // 重入消息放行默认处理；跨线程竞争走阻塞 lock，不丢消息。
    if WND_PROC_IN.with(|f| f.get()) {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    WND_PROC_IN.with(|f| f.set(true));
    let r = wnd_proc_dispatch(hwnd, msg, wparam, lparam);
    WND_PROC_IN.with(|f| f.set(false));
    r
}

thread_local! {
    static WND_PROC_IN: std::cell::Cell<bool> = std::cell::Cell::new(false);
}

unsafe fn wnd_proc_dispatch(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let now = now_ms();
    let guard = match APP.get() {
        Some(g) => g,
        None => return DefWindowProcW(hwnd, msg, wparam, lparam),
    };
    let app = guard.lock().unwrap();
    wnd_proc_locked(app, hwnd, msg, wparam, lparam, now)
}

unsafe fn wnd_proc_locked(mut app: std::sync::MutexGuard<App>, hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, now: i64) -> LRESULT {
    let a: &mut App = &mut app;

    if trace() && !matches!(msg, 0x0020 | 0x0084 | 0x0200 | 0x0113) {
        logf(&format!("msg 0x{msg:04X} wp=0x{:X} lp=0x{:X}", wparam.0, lparam.0 as usize));
    }
    match msg {
        // 按下鼠标时系统会走激活舞步，本机实测该流程对 TOPMOST 工具窗死锁
        // （AppHangB1，卡在 WM_ACTIVATE 后无下文，右键消息永远收不到）。
        // 返回 MA_NOACTIVATE 跳过激活：不抢焦点 + 不死锁。编辑态单独放行。
        WM_MOUSEACTIVATE => {
            let want = a.editing != Editing::None;
            return LRESULT(if want { 2 } else { 3 }); // 2=MA_ACTIVATE 3=MA_NOACTIVATE
        }
        WM_NCHITTEST => {
            let x = ((lparam.0 as i32) & 0xFFFF) as i16 as i32;
            let y = (((lparam.0 as i32) >> 16) & 0xFFFF) as i16 as i32;
            let mut pt = POINT { x, y };
            if ScreenToClient(hwnd, &mut pt).as_bool() {
                // ScreenToClient 后已是物理像素，与 canvas 同空间，勿再乘 scale
                let px = pt.x;
                let py = pt.y;
                if px >= 0 && py >= 0 && px < a.canvas.w && py < a.canvas.h {
                    let idx = ((py * a.canvas.w + px) * 4 + 3) as usize;
                    let al = *((a.canvas.bits_ptr() + idx) as *const u8);
                    if al < 10 {
                        return LRESULT(-1); // HTTRANSPARENT：透明处穿透给下层
                    }
                }
            }
            // ⚠ 不用 HTCAPTION：系统模态拖拽(SC_MOVE)依赖激活流程，本机实测死锁
            // （AppHangB1）。改 HTCLIENT + 手动拖拽（SetCapture + WM_MOUSEMOVE）。
            return LRESULT(1); // HTCLIENT
        }
        WM_LBUTTONDOWN => {
            // 手动拖拽：捕获鼠标，后续 WM_MOUSEMOVE 都归我们
            use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
            a.dragging = true;
            a.moved = false;
            a.pos_samples.clear();
            a.released = false;
            a.move_dbg_n = 0;
            let _ = GetCursorPos(&mut a.drag_origin);
            a.drag_last = (a.drag_origin.x, a.drag_origin.y);
            a.drag_last_delta = (0, 0);
            if trace() {
                logf(&format!("lbdown: origin=({},{})", a.drag_origin.x, a.drag_origin.y));
            }
            let mut r0 = RECT::default();
            let _ = GetWindowRect(hwnd, &mut r0);
            a.win_origin = POINT { x: r0.left, y: r0.top };
            // 不调 set_pose("pick-up")：那是持久姿势，写入后松手回不来（卡死在
            // 拖起姿势）。pick-up 由 redraw 按 dragging && moved 派生。
            let _ = SetCapture(hwnd);
            return LRESULT(0);
        }
        WM_MOUSEMOVE => {
            if a.dragging {
                let mut pt = POINT::default();
                let _ = GetCursorPos(&mut pt);
                let dx = pt.x - a.drag_origin.x;
                let dy = pt.y - a.drag_origin.y;
                if dx.abs() > 3 || dy.abs() > 3 {
                    a.moved = true;
                }
                // 原版 onDrag：记录瞬时位移（松手惯性口径）+ 倾斜跟随 vx*1.1 clamp ±16
                let ddx = pt.x - a.drag_last.0;
                let ddy = pt.y - a.drag_last.1;
                a.drag_last = (pt.x, pt.y);
                // 只记非零位移：松手瞬间系统会补一条同位置 move，若让它覆盖
                // 会把真实甩出速度清成 (0,0)，惯性步进永远触发不了（实测踩坑）
                if ddx != 0 || ddy != 0 {
                    a.drag_last_delta = (ddx, ddy);
                }
                {
                    a.move_dbg_n += 1;
                    if trace() && a.move_dbg_n % 3 == 0 {
                        logf(&format!("move: pt=({},{}) ddx={}", pt.x, pt.y, ddx));
                    }
                }
                if a.moved && a.page == Page::Pet {
                    a.drag_angle = (ddx as f32 * 1.1).clamp(-16.0, 16.0);
                    a.angle_anim = None;
                }
                if a.page != Page::Pet {
                    // 列表页（设置/成就/日记）按住拖动 = 滚动。
                    // 本窗 NOACTIVATE 无键盘焦点，滚轮消息不保证送达（实测设置页
                    // 城市行在陪伴表现组下方，用户滚不到 = 找不到城市入口）。
                    if a.moved {
                        let cap = max_scroll(a.page, a.canvas.scale);
                        a.scroll = (a.scroll - ddy).clamp(0, cap);
                        redraw(a);
                    }
                    return LRESULT(0);
                }
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    a.win_origin.x + dx,
                    a.win_origin.y + dy,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
            // 不拦截：让 hover 分区互动等逻辑继续走
        }
        WM_LBUTTONUP => {
            use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
            if a.dragging {
                a.dragging = false;
                let _ = ReleaseCapture();
                if a.moved {
                    if a.page != Page::Pet {
                        // 列表页拖动=滚动，无松手物理
                        return LRESULT(0);
                    }
                    if trace() {
                        logf(&format!("lbup: moved={} delta={:?} released={}", a.moved, a.drag_last_delta, a.released));
                    }
                    if !a.released {
                        a.released = true;
                        apply_release(a, now);
                    }
                } else {
                    let x = ((lparam.0 as i32) & 0xFFFF) as i16 as i32;
                    let y = (((lparam.0 as i32) >> 16) & 0xFFFF) as i16 as i32;
                    on_click_point(a, x, y, now);
                }
            }
            return LRESULT(0);
        }
        WM_CAPTURECHANGED => {
            a.dragging = false;
            return LRESULT(0);
        }
        WM_RBUTTONUP | WM_NCRBUTTONUP | WM_CONTEXTMENU => {
            // 坐标系：WM_RBUTTONUP=客户区；WM_NCRBUTTONUP 与 WM_CONTEXTMENU=屏幕坐标
            let mut pt = POINT::default();
            if msg == WM_RBUTTONUP {
                let x = ((lparam.0 as i32) & 0xFFFF) as i16 as i32;
                let y = (((lparam.0 as i32) >> 16) & 0xFFFF) as i16 as i32;
                pt = POINT { x, y };
                let _ = ClientToScreen(hwnd, &mut pt);
            } else if msg == WM_NCRBUTTONUP {
                let x = ((lparam.0 as i32) & 0xFFFF) as i16 as i32;
                let y = (((lparam.0 as i32) >> 16) & 0xFFFF) as i16 as i32;
                pt = POINT { x, y };
            } else {
                // WM_CONTEXTMENU：lparam 可能是 -1（键盘菜单），用光标位置兜底
                let _ = GetCursorPos(&mut pt);
            }
            // TrackPopupMenu 是模态循环：期间 WM_TIMER 会重入 wnd_proc，
            // 必须先释放 APP 锁，否则同线程二次 lock() 直接死锁
            drop(a);
            drop(app);
            // 模态循环期间锁已释放，重置重入守卫让 WM_TIMER 正常处理
            WND_PROC_IN.with(|f| f.set(false));
            let cmd = show_menu(hwnd, pt.x, pt.y);
            if cmd != 0 {
                // on_command 里可能有 ShowWindow/SetWindowPos 等同步发消息的调用，
                // 会重入 wnd_proc → APP.lock()。守卫必须恢复 true 让重入走默认处理，
                // 否则同线程二次 lock 死锁（隐藏/显示、回到原位实测挂死）。
                WND_PROC_IN.with(|f| f.set(true));
                let g = APP.get().unwrap();
                let mut app2 = g.lock().unwrap();
                on_command(&mut app2, cmd as usize & 0xFFFF, now_ms());
            }
            return LRESULT(0);
        }
        WM_COMMAND => {
            on_command(a, wparam.0 & 0xFFFF, now);
            return LRESULT(0);
        }
        WM_TRAY => {
            match lparam.0 as u32 {
                e if e == WM_LBUTTONUP || e == WM_LBUTTONDBLCLK => toggle_visible(a),
                e if e == WM_RBUTTONUP || e == WM_CONTEXTMENU => {
                    let mut pt = POINT::default();
                    let _ = GetCursorPos(&mut pt);
                    drop(a);
                    drop(app); // 同上：模态菜单期间放锁
                    show_menu(hwnd, pt.x, pt.y);
                }
                _ => {}
            }
            return LRESULT(0);
        }
        WM_WINDOWPOSCHANGED => {
            let p = lparam.0 as *const WINDOWPOS;
            if !p.is_null() && (((*p).flags & SWP_NOMOVE).0) == 0 {
                let wp = *p;
                a.pos_samples.push((wp.x, wp.y, now));
                if a.pos_samples.len() > 8 {
                    a.pos_samples.remove(0);
                }
                if a.pos_samples.len() >= 2 {
                    a.moved = true;
                }
            }
        }
        WM_ENTERSIZEMOVE => {
            a.moved = false;
        }
        WM_EXITSIZEMOVE => {
            if trace() {
                logf(&format!("exitmove: moved={} delta={:?} released={}", a.moved, a.drag_last_delta, a.released));
            }
            a.dragging = false;
            if a.moved && !a.released {
                a.released = true;
                apply_release(a, now);
                // 姿势已派生化：mood/pick-up 过期自动回落，无需在此补写
            }
        }
        WM_MOUSEMOVE => {
            if a.page == Page::Game {
                if let Some(c) = a.st.catch.as_mut() {
                    let x = ((lparam.0 as i32) & 0xFFFF) as i16 as i32;
                    core::catch_move(c, x as f32 / WIN_W as f32);
                }
            }
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) & 0xFFFF) as i16 as i32;
            // 每页封顶：滚过头会出现整页空白；滚轮一格 120 → 约 3 行
            let cap = max_scroll(a.page, a.canvas.scale);
            a.scroll = (a.scroll - delta).clamp(0, cap);
            redraw(a);
            return LRESULT(0);
        }
        WM_KEYDOWN => {
            on_key(a, wparam.0 & 0xFFFF, now);
            return LRESULT(0);
        }
        WM_CHAR => {
            if a.editing != Editing::None {
                match (wparam.0 as u32, char::from_u32(wparam.0 as u32)) {
                    (8, _) => {
                        a.edit_buf.pop();
                    }
                    (13, _) => commit_edit(a, now),
                    (27, _) => a.editing = Editing::None,
                    (_, Some(ch)) if !ch.is_control() => a.edit_buf.push(ch),
                    _ => {}
                }
                redraw(a);
                return LRESULT(0);
            }
        }
        WM_TIMER => {
            a.timer_count += 1;
            if trace() && a.timer_count % 60 == 0 {
                let mood = a.mood.as_ref().map(|(p, u, _)| format!("{}@{}ms", p, u - now)).unwrap_or_else(|| "-".into());
                logf(&format!("tick alive, base={}, mood={}, dragging={}, bubble={}/{}", a.pose, mood, a.dragging, a.bubble_shown, a.bubble.0.chars().count()));
            }
            // 松手动画推进（squash 回弹 / 倾斜回正）
            if let Some((t0, dur)) = a.release_squash {
                if now - t0 >= dur {
                    a.release_squash = None;
                }
            }
            if let Some((a0, t0, dur)) = a.angle_anim {
                let t = ((now - t0) as f32 / dur as f32).min(1.0);
                a.drag_angle = a0 * (1.0 - ease_out_cubic(t));
                if t >= 1.0 {
                    a.drag_angle = 0.0;
                    a.angle_anim = None;
                }
            }
            tick(a, now);
            // 托盘只在立绘 / 状态文案变化时才刷新（否则 30fps 空转刷 Shell_NotifyIcon）
            let sig = format!("{}|{}", a.shown_pose, a.state_name);
            if sig != a.tray_sig {
                a.tray_sig = sig;
                update_tray(a.hwnd, &a.shown_pose.clone(), tray_label(a.state_name));
            }
            // 有动画才按帧重绘，静止时半秒一次，别白烧 CPU
            let typing = a.bubble_shown < a.bubble.0.chars().count();
            let animating = !a.particles.is_empty()
                || !a.bursts.is_empty()
                || a.swap.is_some()
                || a.micro.is_some()
                || a.react_at > 0
                || a.spin_at > 0
                || a.bubble_pop_at > 0
                || a.bubble_out_at > 0
                || typing
                || a.page == Page::Game
                || a.dragging
                || a.release_squash.is_some()
                || a.angle_anim.is_some()
                || (a.st.settings.weather_fx && a.weather.is_some());
            if animating || now - a.last_redraw_at >= 500 {
                a.last_redraw_at = now;
                redraw(a);
            }
            return LRESULT(0);
        }
        WM_STATUS_UPDATE => {
            redraw(a);
            return LRESULT(0);
        }
        WM_TEST_DONE => {
            logf_force("weather_test: WM_TEST_DONE received");
            if let Ok(mut slot) = TEST_OUT.lock() {
                if let Some(r) = slot.take() {
                    match r {
                        Ok(s) => set_bubble(a, &s, now),
                        Err(e) => set_bubble(a, &e, now),
                    }
                }
            }
            redraw(a);
            return LRESULT(0);
        }
        WM_WEATHER_DONE => {
            if let Ok(mut slot) = WX_OUT.lock() {
                if slot.is_some() {
                    a.weather = slot.take();
                    a.weather_busy = false;
                }
            }
            redraw(a);
            return LRESULT(0);
        }
        WM_DISPLAYCHANGE => {
            let mut r = RECT::default();
            let _ = GetWindowRect(hwnd, &mut r);
            let (x, y) = default_pos(r.right - r.left, r.bottom - r.top);
            let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), x, y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
        }
        WM_DESTROY => {
            let _ = KillTimer(Some(hwnd), 1);
            remove_tray(hwnd);
            a.st.save();
            PostQuitMessage(0);
            return LRESULT(0);
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn velocity(app: &App) -> (f32, f32) {
    let n = app.pos_samples.len();
    if n < 2 {
        return (0.0, 0.0);
    }
    let (x1, y1, t1) = app.pos_samples[n - 2];
    let (x2, y2, t2) = app.pos_samples[n - 1];
    let dt = (t2 - t1).max(1) as f32;
    ((x2 - x1) as f32 / dt, (y2 - y1) as f32 / dt)
}

fn step_inertia(app: &mut App) {
    let (vx, vy, k) = app.inertia;
    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(app.hwnd, &mut r);
    }
    let w = r.right - r.left;
    let h = r.bottom - r.top;
    let nx = (r.left as f32 + vx * 16.0) as i32;
    let ny = (r.top as f32 + vy * 16.0) as i32;
    let (sx, sy, sw, sh) = work_area(app.hwnd);
    let cx = nx.clamp(sx, (sx + sw - w).max(sx));
    let cy = ny.clamp(sy, (sy + sh - h).max(sy));
    let hit_edge = cx != nx || cy != ny;
    unsafe {
        let _ = SetWindowPos(app.hwnd, Some(HWND_TOPMOST), cx, cy, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
    }
    let k2 = k * 0.86;
    app.drag_angle *= 0.82;
    if hit_edge || k2 < 0.08 || (vx.abs() < 0.01 && vy.abs() < 0.01) {
        app.inertia = (0.0, 0.0, 0.0);
        app.drag_angle = 0.0;
        save_pos(app);
    } else {
        app.inertia = (vx * 0.86, vy * 0.86, k2);
    }
}

/// 忠实移植原版 applyReleasePhysics()（dsh-whale-moe.js v1.7.0）：
/// - speed < 6（轻放）：位置不动，squash 回弹 scale(1.05,0.96)→1，300ms cubic-bezier(.34,1.3,.64,1)
/// - 否则一次位移 clamp(v*2.4, ±70)（×scale 转物理像素），撞工作区边缘则停
/// - 倾斜：hit_edge ? 0 : clamp(vx*1.25, ±20)，420ms cubic-bezier(.2,.9,.3,1) 平滑回 0
fn apply_release(app: &mut App, now: i64) {
    let (vx, vy) = app.drag_last_delta;
    let speed = ((vx * vx + vy * vy) as f32).sqrt();
    let sc = app.scale;
    if trace() {
        logf(&format!("release: delta=({vx},{vy}) speed={speed:.1}"));
    }

    if speed < 6.0 {
        // 轻放：原地 squash 弹性回正（原版 scale(1.05,0.96)→1，300ms easeOutBack）
        app.release_squash = Some((now, 300));
        app.drag_angle = 0.0;
        app.angle_anim = None;
        app.inertia = (0.0, 0.0, 0.0);
        save_pos(app);
        return;
    }

    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(app.hwnd, &mut r);
    }
    let w = r.right - r.left;
    let h = r.bottom - r.top;
    let (sx, sy, sw, sh) = work_area(app.hwnd);
    let margin = (8.0 * sc) as i32;
    let max_left = (sx + sw - w - margin).max(sx + margin);
    let max_top = (sy + sh - h - margin).max(sy + margin);

    let step_x = ((vx as f32 * 2.4).clamp(-70.0, 70.0) * sc) as i32;
    let step_y = ((vy as f32 * 2.4).clamp(-70.0, 70.0) * sc) as i32;
    let nx = (r.left + step_x).clamp(sx + margin, max_left);
    let ny = (r.top + step_y).clamp(sy + margin, max_top);
    let hit_edge = (nx == sx + margin && step_x < 0)
        || (nx == max_left && step_x > 0)
        || (ny == sy + margin && step_y < 0)
        || (ny == max_top && step_y > 0);
    unsafe {
        let _ = SetWindowPos(app.hwnd, Some(HWND_TOPMOST), nx, ny, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
    }
    save_pos(app);

    let angle = if hit_edge { 0.0 } else { (vx as f32 * 1.25).clamp(-20.0, 20.0) };
    app.drag_angle = angle;
    app.angle_anim = Some((angle, now, 480));
    app.inertia = (0.0, 0.0, 0.0);
    app.release_squash = None;
}

/// easeOutBack（cubic-bezier(.34,1.3,.64,1) 近似，带轻微 overshoot）
fn ease_out_back(t: f32) -> f32 {
    let c1 = 1.70158f32;
    let c3 = c1 + 1.0;
    let u = t - 1.0;
    1.0 + c3 * u * u * u + c1 * u * u
}

/// easeOutCubic（cubic-bezier(.2,.9,.3,1) 近似）
fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t) * (1.0 - t) * (1.0 - t)
}

fn work_area(hwnd: HWND) -> (i32, i32, i32, i32) {
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

fn save_pos(app: &mut App) {
    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(app.hwnd, &mut r);
    }
    app.st.float_x = Some(r.left);
    app.st.float_y = Some(r.top);
    app.st.touch();
}

fn toggle_visible(app: &mut App) {
    app.visible = !app.visible;
    unsafe {
        let _ = ShowWindow(app.hwnd, if app.visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
    }
}

/* ============================ 点击分发 ============================ */

fn on_click_point(app: &mut App, x: i32, y: i32, now: i64) {
    // 输入框打开时：点框内 = 不动（光标继续）；点框外 = 先保存（同回车）
    // 再照常处理这次点击——比如直接点「测试连接」就是保存+测试一步到位。
    if app.editing != Editing::None {
        let s = app.scale;
        let m = |v: i32| -> i32 { (v as f32 * s) as i32 };
        // 框几何与 draw_edit_box 保持一致：bw 276 / bh 96 居中
        let (bx, by, bw, bh) = (m((WIN_W - 276) / 2), m((WIN_H - 96) / 2), m(276), m(96));
        if x >= bx && x < bx + bw && y >= by && y < by + bh {
            return;
        }
        commit_edit(app, now);
        redraw(app);
    }
    // lparam 客户区坐标是物理像素；命中矩形（draw_* 内经 m() 缩放）也是物理像素。
    // 直接用，勿再乘 scale（双重换算会导致永远落不进任何分区 → 拍拍失效）。
    let sx = x;
    let sy = y;
    app.st.last_interaction = now;

    // 「返回」最高优先级：列表页标题栏最后画（sticky 置顶），back 命中区排在
    // hits 尾部；滚动中的条目命中区可能与它重叠，必须先判 back 防误触条目。
    if let Some(h) = app.hits.iter().find(|h| h.id == "back") {
        if h.contains(sx, sy) {
            app.page = match app.page {
                Page::Achievements | Page::Journal => Page::Settings,
                _ => Page::Pet,
            };
            app.scroll = 0;
            return;
        }
    }

    for h in app.hits.clone().iter() {
        if !h.contains(sx, sy) {
            continue;
        }
        match h.id.as_str() {
            "mascot" => {
                if !app.st.settings.pet {
                    return;
                }
                // 原版 frame click：先播 dsh-whale-react 弹跳（650ms），再进 patMascot
                app.react_at = now;
                let mx = (WIN_W - 200) / 2;
                // 立绘分区表按逻辑像素（WIN_W=300），先把物理坐标除回 scale
                let lx = (x as f32 / app.scale) as i32;
                let ly = (y as f32 / app.scale) as i32;
                let nx = (lx - mx) as f32 / 200.0;
                let ny = (ly - 76) as f32 / 200.0;
                on_click_mascot(app, nx, ny, now);
                return;
            }
            "back" => {
                // 返回 = 回上一级：成就墙/日记从设置页进入，返回设置页；设置页返回宠物主页
                app.page = match app.page {
                    Page::Achievements | Page::Journal | Page::Quests => Page::Settings,
                    _ => Page::Pet,
                };
                app.scroll = 0;
                return;
            }
            "again" => {
                app.st.game = Some(core::game_new_state(now));
                return;
            }
            _ => {}
        }
        if let Some(rest) = h.id.strip_prefix("cell:") {
            if let Ok(i) = rest.parse::<usize>() {
                if let Some(g) = app.st.game.as_mut() {
                    let r = core::game_pop(g, i, now);
                    if r.hit {
                        match r.kind {
                            Some(core::Bubble::Bomb) => spawn_particles(app, 6, 2, 2),
                            Some(core::Bubble::Star) => spawn_particles(app, 8, 1, 1),
                            _ => spawn_particles(app, 4, 2, 2),
                        }
                    }
                }
            }
            return;
        }
        if let Some(rest) = h.id.strip_prefix("claim:") {
            let r = core::claim_quest(app.st.quests.as_ref(), rest, now, &mut app.rng);
            if r.claimed {
                apply_event(app, Event::Quest, now, None);
                app.st.growth.affinity = (app.st.growth.affinity + r.affinity as f32).min(10_000.0);
                app.st.growth.mood = (app.st.growth.mood + r.mood as f32).min(100.0);
                burst(app, 0x1f3af, now); // 原版 claimQuestById burst("🎯")
                set_bubble(app, "谢谢你的奖励～", now);
                if app.state_name != "tool" {
                    set_mood(app, "daily-done", now + 3_200, true); // 原版 showMood("daily-done",3200,true)
                }
                if r.newly_all {
                    apply_event(app, Event::QuestAll, now, None);
                    set_mood(app, "celebrate", now + 3_400, true);
                }
                app.st.quests = Some(r.quests);
                app.st.touch();
            }
            return;
        }
        match h.id.as_str() {
            "pet" => {
                app.st.settings.pet = !app.st.settings.pet;
                if !app.st.settings.pet {
                    app.away_at = now;
                }
            }
            "bubble" => app.st.settings.bubble = !app.st.settings.bubble,
            "particles" => app.st.settings.particles = !app.st.settings.particles,
            "game" => app.st.settings.game = !app.st.settings.game,
            "keywords" => app.st.settings.keywords = !app.st.settings.keywords,
            "slack" => app.st.settings.slack = !app.st.settings.slack,
            "night" => app.st.settings.night = !app.st.settings.night,
            "weather_fx" => app.st.settings.weather_fx = !app.st.settings.weather_fx,
            "tool_pose" => app.st.settings.tool_pose = !app.st.settings.tool_pose,
            "drag_physics" => app.st.settings.drag_physics = !app.st.settings.drag_physics,
            "proactive" => app.st.settings.proactive = !app.st.settings.proactive,
            "a11y" => app.st.settings.a11y = !app.st.settings.a11y,
            "weather_city" => {
                app.editing = Editing::City;
                app.edit_buf = app.st.settings.weather_city.clone();
                grab_focus(app);
            }
            "weather_key" => {
                app.editing = Editing::Key;
                app.edit_buf = app.st.settings.weather_key.clone();
                grab_focus(app);
            }
            "weather_test" => {
                let city = app.st.settings.weather_city.clone();
                let key = app.st.settings.weather_key.clone();
                // 同步 fetch 会卡 UI 线程十几秒（geocode+forecast 两个 8s 超时），
                // 改后台线程跑，窗口先气泡告知，完成后 WM_TEST_DONE 回填
                set_bubble(app, "正在测天气，等一下哈～", now);
                redraw(app);
                let hwnd_val = app.hwnd.0 as isize;
                logf_force(&format!("weather_test: start city={city}"));
                std::thread::spawn(move || {
                    let t0 = now_ms();
                    let r = match crate::weather::test_connection(&city, &key) {
                        Ok(w) => Ok(format!("连上了：{} {}", w.place, w.summary())),
                        Err(e) => Err(e),
                    };
                    logf_force(&format!(
                        "weather_test: done in {}ms -> {}",
                        now_ms() - t0,
                        match &r { Ok(s) => s.as_str(), Err(e) => e.as_str() }
                    ));
                    if let Ok(mut slot) = TEST_OUT.lock() {
                        *slot = Some(r);
                    }
                    unsafe {
                        let _ = PostMessageW(Some(HWND(hwnd_val as *mut _)), WM_TEST_DONE, WPARAM(0), LPARAM(0));
                    }
                });
            }
            "page_quests" => app.page = Page::Quests,
            "page_achv" => app.page = Page::Achievements,
            "page_journal" => app.page = Page::Journal,
            "reset_pos" => {
                app.st.float_x = None;
                app.st.float_y = None;
                let (px, py) = default_pos(app.canvas.w, app.canvas.h);
                unsafe {
                    let _ = SetWindowPos(app.hwnd, Some(HWND_TOPMOST), px, py, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
                }
                set_bubble(app, "回到原位啦～", now);
            }
            "reset_growth" => {
                app.st.growth = core::Growth::default();
                app.st.growth.companion_since = now;
                app.st.journal.clear();
                app.st.pats = 0;
                app.st.badge.clear();
                app.st.quests = None;
                app.st.week = None;
                say(app, "interact", "reset", now);
                app.st.journal_add("reset", "养成数据已重置", now);
            }
            _ => {}
        }
        app.st.touch();
        return;
    }
}

/// 进入文本编辑时把键盘焦点拿到自己窗口（同线程 SetFocus，安全不挂）。
fn grab_focus(app: &mut App) {
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
        let _ = SetFocus(Some(app.hwnd));
    }
}

fn commit_edit(app: &mut App, now: i64) {
    let buf = app.edit_buf.trim().to_string();
    match app.editing {
        Editing::City => {
            if buf.is_empty() {
                set_bubble(app, "城市已清空，不联网啦", now);
            } else {
                set_bubble(app, &format!("城市设为「{}」，测一下？", buf), now);
            }
            app.st.settings.weather_city = buf;
            app.next_weather_at = 0;
        }
        Editing::Key => {
            set_bubble(app, if buf.is_empty() { "API Key 已清空" } else { "API Key 已保存" }, now);
            app.st.settings.weather_key = buf;
        }
        Editing::None => {}
    }
    app.editing = Editing::None;
    app.st.touch();
}

fn on_key(app: &mut App, vk: usize, now: i64) {
    const VK_ESCAPE: usize = 0x1B;
    const VK_RETURN: usize = 0x0D;
    const VK_SPACE: usize = 0x20;
    const VK_TAB: usize = 0x09;
    const VK_LEFT: usize = 0x25;
    const VK_UP: usize = 0x26;
    const VK_RIGHT: usize = 0x27;
    const VK_DOWN: usize = 0x28;

    if app.editing != Editing::None {
        return;
    }
    match vk {
        VK_ESCAPE => {
            if app.page == Page::Game {
                app.st.game = None;
                app.st.catch = None;
                app.page = Page::Pet;
            } else if app.page != Page::Pet {
                app.page = Page::Pet;
                app.scroll = 0;
            } else {
                unsafe { PostQuitMessage(0) };
            }
        }
        VK_TAB => {
            if app.st.settings.a11y {
                app.focus_row += 1;
                set_bubble(app, &format!("状态：{}", app.state_name), now);
            }
        }
        VK_RETURN | VK_SPACE => {
            if app.st.settings.a11y {
                on_click_mascot(app, 0.5, 0.2, now);
            }
        }
        VK_LEFT | VK_RIGHT | VK_UP | VK_DOWN if app.st.settings.a11y => {
            let (dx, dy) = match vk {
                VK_LEFT => (-8, 0),
                VK_RIGHT => (8, 0),
                VK_UP => (0, -8),
                _ => (0, 8),
            };
            let mut r = RECT::default();
            unsafe {
                let _ = GetWindowRect(app.hwnd, &mut r);
                let _ = SetWindowPos(app.hwnd, Some(HWND_TOPMOST), r.left + dx, r.top + dy, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
            }
            save_pos(app);
        }
        _ => {}
    }
}

fn on_command(app: &mut App, id: usize, now: i64) {
    match id {
        M_FEED => feed(app, now),
        M_POKE => poke(app, now),
        M_PRAISE => praise(app, now),
        M_GAME => {
            if app.st.settings.game {
                app.st.game = Some(core::game_new_state(now));
                app.page = Page::Game;
                set_bubble(app, "来戳泡泡吧！30 秒一局～", now);
            }
        }
        M_CATCH => {
            if app.st.settings.game {
                app.st.catch = Some(core::catch_new_state(now));
                app.page = Page::Game;
                set_bubble(app, "接零食！左右移动篮子～", now);
            }
        }
        M_GROW => app.page = Page::Quests,
        M_SETTINGS => {
            app.page = Page::Settings;
            app.scroll = 0;
        }
        M_HOME => {
            app.st.float_x = None;
            app.st.float_y = None;
            let (x, y) = default_pos(app.canvas.w, app.canvas.h);
            unsafe {
                let _ = SetWindowPos(app.hwnd, Some(HWND_TOPMOST), x, y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
            }
            set_bubble(app, "回到原位啦～", now);
        }
        M_SHOW => toggle_visible(app),
        M_QUIT => unsafe { PostQuitMessage(0) },
        _ => {}
    }
}
