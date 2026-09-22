#![allow(unused_imports)]

use super::*;

/* ============================ 每帧调度 ============================ */

pub(crate) fn tick(app: &mut App, now: i64) {
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
