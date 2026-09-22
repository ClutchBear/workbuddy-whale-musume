#![allow(unused_imports)]

use super::*;

pub(crate) fn save_pos(app: &mut App) {
    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(app.hwnd, &mut r);
    }
    app.st.float_x = Some(r.left);
    app.st.float_y = Some(r.top);
    app.st.touch();
}

pub(crate) fn toggle_visible(app: &mut App) {
    app.visible = !app.visible;
    unsafe {
        let _ = ShowWindow(app.hwnd, if app.visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
    }
}

/* ============================ 点击分发 ============================ */

pub(crate) fn on_click_point(app: &mut App, x: i32, y: i32, now: i64) {
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
