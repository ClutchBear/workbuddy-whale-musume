#![allow(unused_imports)]

use super::*;

/* ============================ 交互 ============================ */

/// 忠实移植原版 patMascot()（dsh-whale-moe.js:703）：
/// 庆祝保护 → 忙态直落拍拍 → 分区反应（tail/belly，不进连击计数）→
/// 2s 滑动窗三连（2600ms 冷却 + 2200ms 庆祝期）→ 普通拍拍（450ms 内
/// rapid-fire 只播反馈，跳过成长与台词；pat 台词节流 2500ms）。
pub(crate) fn on_click_mascot(app: &mut App, nx: f32, ny: f32, now: i64) {
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

pub(crate) fn feed(app: &mut App, now: i64) {
    app.st.last_interaction = now;
    apply_event(app, Event::Feed, now, None);
    burst(app, 0x1f370, now); // 原版 burst("🍰")
    spawn_particles(app, 8, 2, 1);
    set_mood(app, "eat", now + 3_000, false); // 原版 showMood("eat",3000) 无 animate
    say(app, "interact", "feed", now);
    push_quest(app, "feed", 1, now);
}

pub(crate) fn poke(app: &mut App, now: i64) {
    app.st.last_interaction = now;
    apply_event(app, Event::Poke, now, None);
    burst(app, 0x1f4a2, now); // 原版 burst("💢")
    spawn_particles(app, 4, 0, 0);
    set_mood(app, "angry", now + 3_000, false);
    say(app, "interact", "poke", now);
}

pub(crate) fn praise(app: &mut App, now: i64) {
    app.st.last_interaction = now;
    apply_event(app, Event::Praise, now, None);
    burst(app, 0x2728, now); // 原版 burst("✨")
    spawn_particles(app, 10, 1, 1);
    // 原版 showMood("tail-swing", 3000)：摇尾巴姿势而非星星，无 animate
    set_mood(app, "tail-swing", now + 3_000, false);
    say(app, "interact", "praise", now);
}

/// 进入文本编辑时把键盘焦点拿到自己窗口（同线程 SetFocus，安全不挂）。
pub(crate) fn grab_focus(app: &mut App) {
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
        let _ = SetFocus(Some(app.hwnd));
    }
}

pub(crate) fn commit_edit(app: &mut App, now: i64) {
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

pub(crate) fn on_key(app: &mut App, vk: usize, now: i64) {
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

pub(crate) fn on_command(app: &mut App, id: usize, now: i64) {
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
