#![allow(unused_imports)]

use super::*;

/* ============================ 窗口过程 ============================ */

pub(crate) unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
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

pub(crate) unsafe fn wnd_proc_dispatch(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let now = now_ms();
    let guard = match APP.get() {
        Some(g) => g,
        None => return DefWindowProcW(hwnd, msg, wparam, lparam),
    };
    let app = guard.lock().unwrap();
    wnd_proc_locked(app, hwnd, msg, wparam, lparam, now)
}

pub(crate) unsafe fn wnd_proc_locked(mut app: std::sync::MutexGuard<App>, hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, now: i64) -> LRESULT {
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
