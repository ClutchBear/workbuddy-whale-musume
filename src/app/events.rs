#![allow(unused_imports)]

use super::*;

/* ============================ 养成事件 ============================ */

pub(crate) fn apply_event(app: &mut App, ev: Event, now: i64, delta_min: Option<f32>) {
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

pub(crate) fn push_quest(app: &mut App, metric: &str, amount: i32, now: i64) {
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
pub(crate) fn check_usage_achievements(app: &mut App, now: i64) {
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

pub(crate) fn maybe_proactive(app: &mut App, now: i64, busy: bool) {
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
