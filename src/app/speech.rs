#![allow(unused_imports)]

use super::*;

/* ============================ 台词 / 立绘 ============================ */

/// 换装动画状态：phase 0 = 旧图下压（140ms），phase 1 = 新图弹起（480ms）
pub(crate) struct Swap {
    pub(crate) pending: String,
    pub(crate) phase: u8,
    pub(crate) t0: i64,
}

/// 原版打字机逐字延迟：默认 64ms；「，。！？～…」260ms；空格 90ms；每第 5 字 130ms
pub(crate) fn type_delay(ch: char, index_after: usize) -> i64 {
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

pub(crate) fn start_typing(app: &mut App, line: String, now: i64) {
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
pub(crate) fn set_bubble(app: &mut App, line: &str, _now: i64) {
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

pub(crate) fn say(app: &mut App, bank: &str, event: &str, now: i64) {
    let l = core::pick_dialogue_avoid_recent(bank, event, 0, &mut app.rng, &app.st.recent_lines);
    set_bubble(app, l, now);
}

/// 切换基础姿势。实际换图动画由 tick 里的派生姿势比对驱动
/// （motion-hide swap：旧图下压 140ms → 换图 → 新图弹起 480ms），
/// 这里只改基础姿势本体。
pub(crate) fn set_pose(app: &mut App, pose: &str) {
    if app.pose != pose {
        app.pose = pose.to_string();
    }
}

/// 临时情绪立绘（点击反应、庆祝等），到期自动回落。
/// 对齐原版 showMood(kind, duration, animate)：animate=false 时切姿势
/// 不播换装动画（feed/poke/praise/star 等菜单动作原版就不传 animate）。
pub(crate) fn set_mood(app: &mut App, pose: &str, until: i64, animate: bool) {
    app.mood = Some((pose.to_string(), until, animate));
}
