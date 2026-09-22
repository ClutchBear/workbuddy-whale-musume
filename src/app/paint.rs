#![allow(unused_imports)]

use super::*;

/* ============================ 绘制 ============================ */

pub(crate) fn redraw(app: &mut App) {
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
pub(crate) fn is_light() -> bool {
    match std::env::var("WORKBUDDY_PET_THEME").unwrap_or_default().as_str() {
        "light" => true,
        "dark" => false,
        _ => false,
    }
}

pub(crate) fn present(app: &mut App) {
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

/// 各列表页的内容高度封顶（scroll 单位 = 物理像素）。
/// 关键：布局经 m() 缩放，滚动量也必须是物理像素——旧版 `scroll * m(1)` 在
/// DPI 1.25/1.5 下 m(1) 截断成 1，封顶远小于内容高度 → 永远滚不到底（实测踩坑）。
pub(crate) fn max_scroll(page: Page, s: f32) -> i32 {
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
