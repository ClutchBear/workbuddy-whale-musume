#![allow(unused_imports)]

use super::*;

pub(crate) fn finish_game(app: &mut App, now: i64) {
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
