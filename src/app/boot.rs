#![allow(unused_imports)]

use super::*;

pub(crate) fn run() {
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

/* ============================ 开场 / 跨日 ============================ */

pub(crate) fn on_startup(app: &mut App, now: i64) {
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
