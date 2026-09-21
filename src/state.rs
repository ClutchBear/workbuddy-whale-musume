//! 持久化与设置
//!
//! 上游把状态放在浏览器 localStorage（键名 `whale-moe:*`）。
//! 原生版没有浏览器，这里改用 exe 同目录的 `whale-state.json`，
//! 语义一一对应：同样的默认值、同样的开关含义、同样「读不到就用默认」。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::core::{CatchState, GameState, Growth, Quests, WeekSignin};

fn state_path() -> PathBuf {
    // 允许用环境变量覆盖，便于自测隔离
    if let Ok(p) = std::env::var("WORKBUDDY_PET_STATE") {
        return PathBuf::from(p);
    }
    let mut exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    exe.pop();
    exe.push("whale-state.json");
    exe
}

/* ============================ 设置 ============================ */

#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    /// 看板娘总开关
    pub pet: bool,
    /// 台词气泡
    pub bubble: bool,
    /// 粒子效果
    pub particles: bool,
    /// 小游戏入口
    pub game: bool,
    /// 关键词感知（涉及读取聊天内容，默认关）
    pub keywords: bool,
    /// 摸鱼提醒
    pub slack: bool,
    /// 深夜模式（23:00–5:59 不主动打扰）
    pub night: bool,
    /// 天气视觉特效
    pub weather_fx: bool,
    /// 工具细分姿态
    pub tool_pose: bool,
    /// 拖拽惯性
    pub drag_physics: bool,
    /// 主动关怀
    pub proactive: bool,
    /// 无障碍模式（键盘可达 + 状态播报）
    pub a11y: bool,
    /// 他怎么称呼我（用于台词替换「主人」）
    pub title: String,
    /// 她的自称（留空 = 鲸鱼娘）
    pub self_name: String,
    /// 天气城市，留空 = 零联网
    pub weather_city: String,
    /// Open-Meteo 选填 API Key
    pub weather_key: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            pet: true,
            bubble: true,
            particles: true,
            game: true,
            keywords: false,
            slack: true,
            night: true,
            weather_fx: true,
            tool_pose: true,
            drag_physics: true,
            proactive: true,
            a11y: false,
            title: "主人".to_string(),
            self_name: String::new(),
            weather_city: String::new(),
            weather_key: String::new(),
        }
    }
}

/* ============================ 用量统计 ============================ */

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Usage {
    pub tools: i32,
    pub code: i32,
    pub successes: i32,
    pub failures: i32,
    pub messages: i32,
    pub keyword_hits: i32,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct GameStats {
    pub plays: i32,
    pub wins: i32,
    pub combo_max: i32,
    pub highscore: i32,
    pub plays_today: i32,
    pub today: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub at: i64,
    pub kind: String,
    pub text: String,
}

/* ============================ 总体状态 ============================ */

#[derive(Clone, Serialize, Deserialize)]
pub struct AppState {
    pub settings: Settings,
    pub growth: Growth,
    pub quests: Option<Quests>,
    pub week: Option<WeekSignin>,
    pub usage: Usage,
    pub game_stats: GameStats,
    pub journal: Vec<JournalEntry>,
    /// 悬浮位置（None = 用默认右下角）
    pub float_x: Option<i32>,
    pub float_y: Option<i32>,
    /// 累计摸头次数
    pub pats: i64,
    /// 已选称号 id
    pub badge: String,
    /// 首次陪伴时间戳
    pub companion_since: i64,
    /// 最近一次交互（afk 判定用）
    pub last_interaction: i64,
    /// 上次显示节日立绘的日期
    pub festival_shown: String,
    /// 空闲闲聊的下一时间点
    pub next_chat_at: i64,
    /// 上次分时问候时间点
    pub last_greet_at: i64,
    /// 上次问候桶
    pub last_greet_bucket: String,
    /// 最近说过的台词（避免复读）
    pub recent_lines: Vec<String>,

    #[serde(skip)]
    pub game: Option<GameState>,
    #[serde(skip)]
    pub catch: Option<CatchState>,
    #[serde(skip)]
    pub dirty: bool,
}

impl Default for AppState {
    fn default() -> Self {
        AppState {
            settings: Settings::default(),
            growth: Growth::default(),
            quests: None,
            week: None,
            usage: Usage::default(),
            game_stats: GameStats::default(),
            journal: Vec::new(),
            float_x: None,
            float_y: None,
            pats: 0,
            badge: String::new(),
            companion_since: 0,
            last_interaction: 0,
            festival_shown: String::new(),
            next_chat_at: 0,
            last_greet_at: 0,
            last_greet_bucket: String::new(),
            recent_lines: Vec::new(),
            game: None,
            catch: None,
            dirty: false,
        }
    }
}

impl AppState {
    pub fn load() -> AppState {
        let p = state_path();
        let mut st = match std::fs::read_to_string(&p) {
            Ok(s) => serde_json::from_str::<AppState>(&s).unwrap_or_default(),
            Err(_) => AppState::default(),
        };
        // 兼容旧档：补全缺省字段
        if st.settings.title.is_empty() {
            st.settings.title = "主人".to_string();
        }
        st
    }

    pub fn save(&mut self) {
        let p = state_path();
        let tmp = p.with_extension("json.tmp");
        if let Ok(s) = serde_json::to_string_pretty(self) {
            // 先写临时文件再替换，避免写到一半崩溃留下坏档
            if std::fs::write(&tmp, s).is_ok() {
                let _ = std::fs::rename(&tmp, &p);
            }
        }
        self.dirty = false;
    }

    /// 标记脏，由主循环在空闲时落盘（避免每次交互都写盘）
    pub fn touch(&mut self) {
        self.dirty = true;
    }

    pub fn push_recent(&mut self, line: &str) {
        if line.is_empty() {
            return;
        }
        self.recent_lines.retain(|r| r != line);
        self.recent_lines.push(line.to_string());
        if self.recent_lines.len() > 8 {
            self.recent_lines.remove(0);
        }
    }

    /// 成长日记：同一天同类事件只记一条，最多保留 80 条
    pub fn journal_add(&mut self, kind: &str, text: &str, now: i64) {
        if text.is_empty() {
            return;
        }
        let day = crate::core::day_key(now);
        for e in self.journal.iter().rev() {
            if e.kind == kind && e.text == text && crate::core::day_key(e.at) == day {
                return;
            }
        }
        self.journal.push(JournalEntry { at: now, kind: kind.to_string(), text: text.to_string() });
        if self.journal.len() > 80 {
            let n = self.journal.len() - 80;
            self.journal.drain(0..n);
        }
        self.touch();
    }
}

/// 相对时间描述（成长日记用）
pub fn rel_time(at: i64, now: i64) -> String {
    let d = now - at;
    if d < 60_000 {
        "刚刚".to_string()
    } else if d < 3_600_000 {
        format!("{} 分钟前", d / 60_000)
    } else if d < 86_400_000 {
        format!("{} 小时前", d / 3_600_000)
    } else if d < 30 * 86_400_000 {
        format!("{} 天前", d / 86_400_000)
    } else {
        format!("{} 个月前", d / (30 * 86_400_000))
    }
}
