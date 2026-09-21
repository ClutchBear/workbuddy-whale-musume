//! 核心状态机 —— whale-moe-core.js 的 Rust 移植
//!
//! 上游是纯函数、无 DOM 的状态机，这里保持同样的契约：
//! 输入（上一状态 + 信号 + 时间）→ 输出（新状态），不碰存储、不碰窗口。
//! 台词与静态表在 `data.rs`（由 tools/gen-data.mjs 从上游原样导出）。

use serde::{Deserialize, Serialize};

use crate::data::*;
use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};

/* ============================ 常量 ============================ */

pub const AFK_MS: i64 = 180_000;
pub const SPEECH_GAP_MS: i64 = 6_000;
pub const SUCCESS_WINDOW_MS: i64 = 2_000;
pub const CURIOUS_WINDOW_MS: i64 = 6_000;

pub const IDLE_CHAT_MIN_MS: i64 = 5 * 60_000;
pub const IDLE_CHAT_MAX_MS: i64 = 8 * 60_000;

/// 主动关怀阈值（v1.8.0）。基调是「陪着，不是指挥」。
pub struct Proactive;
impl Proactive {
    pub const LONG_WORK_MS: i64 = 25 * 60_000;
    pub const NIGHT_WORK_MS: i64 = 10 * 60_000;
    pub const STUCK_MS: i64 = 8 * 60_000;
    pub const AWAY_MS: i64 = 3 * 60_000;
    pub const MIN_GAP_MS: i64 = 15 * 60_000;
}

/// 养成数值上限与衰减
pub struct Growth1;
impl Growth1 {
    pub const MOOD_MAX: f32 = 100.0;
    pub const AFFINITY_MAX: f32 = 10_000.0;
    pub const SATIETY_MAX: f32 = 100.0;
    pub const LEVEL_STEP: f32 = 500.0;
    pub const SATIETY_DECAY_PER_MIN: f32 = 0.15;
}

/// 戳泡泡 · 泡泡派对
pub struct Game1;
impl Game1 {
    pub const DURATION_MS: i64 = 30_000;
    pub const GRID: usize = 4;
    pub const SPAWN_INTERVAL_MS: i64 = 500;
    pub const BUBBLE_LIFE_MS: i64 = 1_600;
    pub const STAR_LIFE_MS: i64 = 1_200;
    pub const STAR_P: f64 = 0.15;
    pub const BOMB_P: f64 = 0.10;
    pub const COMBO_WINDOW_MS: i64 = 1_200;
    pub const WIN_SCORE: i32 = 300;
    pub const DRAW_SCORE: i32 = 150;
    pub const BASE: i32 = 10;
    pub const STAR_SCORE: i32 = 30;
    pub const BOMB_SCORE: i32 = -20;
    pub const COMBO_CAP: i32 = 10;
    pub const REWARDS_PER_DAY: i32 = 3;
}

/// 接零食（catch the snacks）
pub struct Catch1;
impl Catch1 {
    pub const DURATION_MS: i64 = 30_000;
    pub const SPAWN_INTERVAL_MS: i64 = 900;
    pub const BASKET_W: f32 = 0.18;
    pub const BASKET_Y: f32 = 0.92;
    pub const CATCH_BAND: f32 = 0.05;
    pub const FALL_BASE: f32 = 0.16;
    pub const FALL_MAX: f32 = 0.42;
    pub const CAKE_P: f64 = 0.75;
    pub const STAR_P: f64 = 0.15;
    pub const BOMB_P: f64 = 0.10;
    pub const CAKE_SCORE: i32 = 10;
    pub const STAR_SCORE: i32 = 30;
    pub const BOMB_SCORE: i32 = -20;
    pub const COMBO_WINDOW_MS: i64 = 1_500;
}

/// 羁绊解锁等级
pub const BOND_LV3_ACTION: i32 = 3;
pub const BOND_LV5_BADGE: i32 = 5;
pub const BOND_LV7_EGG: i32 = 7;

/* ============================ 随机数 ============================ */

/// 极简 xorshift64*。上游逻辑只需要「可复现的伪随机」，不值得为此引入 rand。
#[derive(Clone, Copy)]
pub struct Rng(pub u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(if seed == 0 { 0x9E3779B97F4A7C15 } else { seed })
    }
    /// [0,1)
    pub fn f64(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        ((x.wrapping_mul(0x2545F4914F6CDD1D)) >> 11) as f64 / (1u64 << 53) as f64
    }
    /// [0,n)
    pub fn idx(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        let v = (self.f64() * n as f64) as usize;
        if v >= n {
            n - 1
        } else {
            v
        }
    }
}

/* ============================ 本地时间 ============================ */

#[derive(Clone, Copy, Debug)]
pub struct LocalTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub weekday: u32, // 0=周日
}

/// epoch 毫秒 → 本地时间。走 Windows 时区转换，夏令时自动正确。
pub fn local_time(ms: i64) -> LocalTime {
    let ft_value = (ms + 11_644_473_600_000) * 10_000;
    let ft = FILETIME {
        dwLowDateTime: (ft_value & 0xFFFF_FFFF) as u32,
        dwHighDateTime: ((ft_value >> 32) & 0xFFFF_FFFF) as u32,
    };
    let mut utc = SYSTEMTIME::default();
    let mut local = SYSTEMTIME::default();
    unsafe {
        if FileTimeToSystemTime(&ft, &mut utc).is_ok()
            && SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).is_ok()
        {
            return LocalTime {
                year: local.wYear as i32,
                month: local.wMonth as u32,
                day: local.wDay as u32,
                hour: local.wHour as u32,
                minute: local.wMinute as u32,
                weekday: local.wDayOfWeek as u32,
            };
        }
    }
    LocalTime { year: 1970, month: 1, day: 1, hour: 0, minute: 0, weekday: 4 }
}

pub fn day_key(ms: i64) -> String {
    let t = local_time(ms);
    format!("{}-{}-{}", t.year, t.month, t.day)
}

/// 本周一（周一为一周之始）
pub fn week_key(ms: i64) -> String {
    let t = local_time(ms);
    // SYSTEMTIME 的 wDayOfWeek: 0=周日。换算成「距周一的天数」
    let since_monday = (t.weekday + 6) % 7;
    let d = day_index(t.year, t.month, t.day) - since_monday as i64;
    let (y, m, dd) = from_day_index(d);
    format!("{}-{}-{}", y, m, dd)
}

/// 蔡勒公式：年月日 → 自 1970-01-01 起的天数
fn day_index(y: i32, m: u32, d: u32) -> i64 {
    let (y, m) = if m <= 2 { (y - 1, m + 12) } else { (y, m) };
    let y = y as i64;
    let m = m as i64;
    let d = d as i64;
    365 * y + y / 4 - y / 100 + y / 400 + (153 * m + 8) / 5 + d - 719_561
}

fn from_day_index(z: i64) -> (i32, u32, u32) {
    // Howard Hinnant 的 civil_from_days
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as i64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    ((if m <= 2 { y + 1 } else { y }) as i32, m as u32, d as u32)
}

/// 分时问候桶。深夜 23:00–5:59 返回 night（调用方据此不主动打扰）
pub fn greet_bucket(hour: u32) -> &'static str {
    if hour >= 23 || hour < 6 {
        "night"
    } else if hour < 9 {
        "morning"
    } else if hour < 12 {
        "forenoon"
    } else if hour < 14 {
        "noon"
    } else if hour < 18 {
        "afternoon"
    } else {
        "evening"
    }
}

pub fn is_late_night(hour: u32) -> bool {
    hour >= 23 || hour < 6
}

/* ============================ 节日 ============================ */

/// 农历节日用一张小表，其余按公历固定日期。
pub fn festival_key(ms: i64) -> &'static str {
    let t = local_time(ms);
    let key = format!("{:04}-{:02}-{:02}", t.year, t.month, t.day);
    match key.as_str() {
        "2026-02-17" | "2027-02-06" => "festival-spring",
        "2026-09-25" | "2027-09-15" => "festival-mid-autumn",
        _ => match (t.month, t.day) {
            (10, 31) => "festival-halloween",
            (12, 25) => "festival-christmas",
            (2, 14) => "valentine",
            _ => "",
        },
    }
}

/* ============================ 状态机 ============================ */

pub const STATES: [&str; 10] = [
    "idle", "waiting", "thinking", "tool", "success", "failure", "curious", "teasing", "afk",
    "hidden",
];

pub fn pose_for_state(state: &str) -> Option<&'static str> {
    Some(match state {
        "idle" => "idle-cute",
        "waiting" => "waiting",
        "thinking" => "thinking",
        "tool" => "running",
        "success" => "success",
        "failure" => "failure",
        "curious" => "curious",
        "teasing" => "teasing",
        "afk" => "afk",
        "blush" => "blush",
        "angry" => "angry",
        "eat" => "eat",
        "star" => "star",
        "celebrate" => "celebrate",
        "sleep" => "sleep",
        "greet" => "greet",
        "night" => "night",
        "wink" => "wink",
        "bold" => "bold",
        "abstract" => "abstract",
        "sweep" => "sweep",
        _ => return None,
    })
}

/// 状态机输入信号
#[derive(Clone, Copy, Default)]
pub struct Signals {
    pub pet_disabled: bool,
    pub error: bool,
    pub tool: bool,
    pub thinking: bool,
    pub waiting: bool,
    pub success_at: i64, // -1 表示无
    pub curious_at: i64,
    pub last_interaction: i64,
    pub dense_code: bool,
}

#[derive(Clone, Debug)]
pub struct Computed {
    pub state: &'static str,
    pub pose: Option<&'static str>,
    pub line: String,
    pub speak: bool,
    pub at: i64,
    pub since: i64,
    pub streak: i32,
    pub line_count: i64,
}

#[derive(Clone, Default)]
pub struct PrevState {
    pub state: String,
    pub since: i64,
    pub last_speech_at: i64,
    pub streak: i32,
    pub line_count: i64,
}

/// 纯状态迁移。优先级：error > tool > thinking > success(窗口) > curious(窗口) > waiting > afk/idle。
/// afk 放在 error/tool 之后，保证真实工作不会被打盹状态盖住。
pub fn compute_state(prev: &PrevState, s: &Signals, now: i64) -> Computed {
    let line_count = prev.line_count;
    if s.pet_disabled {
        return Computed {
            state: "hidden",
            pose: None,
            line: String::new(),
            speak: false,
            at: now,
            since: now,
            streak: 0,
            line_count,
        };
    }
    let state: &'static str = if s.error {
        "failure"
    } else if s.tool {
        "tool"
    } else if s.thinking {
        "thinking"
    } else if s.success_at >= 0 && now - s.success_at >= 0 && now - s.success_at <= SUCCESS_WINDOW_MS {
        "success"
    } else if s.curious_at >= 0 && now - s.curious_at >= 0 && now - s.curious_at <= CURIOUS_WINDOW_MS {
        "curious"
    } else if s.waiting {
        "waiting"
    } else if now - s.last_interaction >= AFK_MS {
        "afk"
    } else {
        "idle"
    };

    let changed = state != prev.state;
    let gap_ok = now - prev.last_speech_at >= SPEECH_GAP_MS;
    let speak = (changed || gap_ok) && state != "hidden";
    let lc = if changed { prev.line_count + 1 } else { prev.line_count };

    Computed {
        state,
        pose: pose_for_state(state),
        line: if speak { pick_line(state, lc) } else { String::new() },
        speak,
        at: now,
        since: if changed { now } else { prev.since },
        streak: if state == "failure" && changed { prev.streak + 1 } else { 0 },
        line_count: lc,
    }
}

pub fn pick_line(state: &str, count: i64) -> String {
    for (k, arr) in LINES {
        if *k == state && !arr.is_empty() {
            let i = (count.unsigned_abs() as usize) % arr.len();
            return arr[i].to_string();
        }
    }
    String::new()
}

/// 称呼替换：主人 → 用户对她的称呼；鲸鱼娘 → 她的自称。
/// 台词库里有 363 处自称，集中在这里替换而不是逐条改写。
pub fn apply_names(line: &str, title: &str, self_name: &str) -> String {
    let t = if title.is_empty() { "主人" } else { title };
    let s = if self_name.is_empty() { "鲸鱼娘" } else { self_name };
    line.replace("主人", t).replace("鲸鱼娘", s)
}

/* ============================ 台词选取 ============================ */

pub fn bank(name: &str) -> &'static [(&'static str, &'static [&'static str])] {
    match name {
        "daily" => DIALOGUE_DAILY,
        "work" => DIALOGUE_WORK,
        "interact" => DIALOGUE_INTERACT,
        "keyword" => DIALOGUE_KEYWORD,
        "meme" => DIALOGUE_MEME,
        "context" => DIALOGUE_CONTEXT,
        "weather" => DIALOGUE_WEATHER,
        "greet" => DIALOGUE_GREET,
        "bond" => DIALOGUE_BOND,
        "proactive" => DIALOGUE_PROACTIVE,
        _ => &[],
    }
}

pub fn pick_dialogue(
    name: &str,
    event: &str,
    counter: i64,
    rng: &mut Rng,
) -> &'static str {
    pick_dialogue_avoid_recent(name, event, counter, rng, &[])
}

/// 尽量不重复最近说过的台词（避免连着两句一样）
pub fn pick_dialogue_avoid_recent<'a>(
    name: &str,
    event: &str,
    counter: i64,
    rng: &mut Rng,
    recent: &'a [String],
) -> &'static str {
    let arr = bank(name)
        .iter()
        .find(|(k, _)| *k == event)
        .map(|(_, v)| *v)
        .unwrap_or(&[]);
    if arr.is_empty() {
        return "";
    }
    let fresh: Vec<&&str> = arr.iter().filter(|s| !recent.iter().any(|r| r == *s)).collect();
    let pool: Vec<&&str> = if fresh.is_empty() {
        arr.iter().collect()
    } else {
        fresh
    };
    let i = (counter.unsigned_abs() as usize + (rng.f64() * 97.0) as usize) % pool.len();
    pool[i]
}

/* ============================ 分区互动 ============================ */

/// 归一化点击点 → 分区 id。顺序判定 tail > head > belly，未命中回落 head。
pub fn hit_zone(nx: f32, ny: f32) -> &'static str {
    let x = nx.clamp(0.0, 1.0);
    let y = ny.clamp(0.0, 1.0);
    for (id, x0, y0, x1, y1) in HIT_ZONES {
        if x >= *x0 && x <= *x1 && y >= *y0 && y <= *y1 {
            return id;
        }
    }
    "head"
}

/* ============================ 关键词 / 任务分类 ============================ */

pub fn match_keyword(text: &str) -> Option<&'static str> {
    if text.is_empty() {
        return None;
    }
    let lower = text.to_lowercase();
    for (id, words) in KEYWORDS {
        for w in *words {
            if lower.contains(&w.to_lowercase()) {
                return Some(id);
            }
        }
    }
    None
}

/// 关键词 → 表情包立绘（13 种梗表情 + 若干情绪）
pub fn keyword_pose(id: &str) -> Option<&'static str> {
    Some(match id {
        "kyun" => "meme-kyun",
        "omg" => "meme-omg",
        "doge" => "meme-doge",
        "sike" => "meme-sike",
        "worship" => "meme-worship",
        "peace" => "meme-peace",
        "doubt" => "meme-doubt",
        "wakuwaku" => "meme-wakuwaku",
        "smilepain" => "meme-smile-pain",
        "ojisan" => "meme-ojisan",
        "crazy" => "meme-shock",
        "thanks" => "meme-heart",
        "praise" => "meme-yes",
        "hug" => "meme-heart",
        "cute" => "meme-kyun",
        "cheer" => "meme-wakuwaku",
        "tired" | "slack" => "work-slack",
        "hungry" => "daily-eat",
        "goodnight" | "night" => "sleep",
        "ddl" => "work-deadline",
        "cake" => "meme-broke",
        "flag" => "meme-smug",
        "bugtalk" => "meme-cry",
        "deploy" => "work-deploy",
        "meeting" => "work-meeting",
        "review" => "work-review",
        "worker" => "work-boss",
        _ => return None,
    })
}

/// 任务文本 → 话题分类（空闲闲聊时贴题用）
pub fn classify_task(text: &str) -> &'static str {
    if text.is_empty() {
        return "general";
    }
    let lower = text.to_lowercase();
    for (id, words) in TASK_TOPICS {
        for w in *words {
            if lower.contains(&w.to_lowercase()) {
                return id;
            }
        }
    }
    "general"
}

/// 工具文本 → 细分姿态。识别不了返回 None（上游：回落通用 running，绝不乱猜）
pub fn tool_pose(text: &str) -> Option<&'static str> {
    if text.is_empty() || text.len() > 4000 {
        return None;
    }
    let lower = text.to_lowercase();
    for (_id, pose, words) in TOOL_POSES {
        for w in *words {
            if lower.contains(&w.to_lowercase()) {
                return Some(pose);
            }
        }
    }
    None
}

/* ============================ 养成 ============================ */

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Growth {
    pub mood: f32,
    pub affinity: f32,
    pub satiety: f32,
    pub last_signin: String,
    pub signin_streak: i32,
    pub achievements: Vec<String>,
    pub level: i32,
    /// 首次陪伴时间戳（陪伴天数用）
    pub companion_since: i64,
}

impl Default for Growth {
    fn default() -> Self {
        Growth {
            mood: 70.0,
            affinity: 0.0,
            satiety: 80.0,
            last_signin: String::new(),
            signin_streak: 0,
            achievements: Vec::new(),
            level: 1,
            companion_since: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Pat,
    Poke,
    Feed,
    Triple,
    Success,
    Failure,
    Thanks,
    Praise,
    Belly,
    Tail,
    Tick,
    Signin,
    GameWin,
    GameDraw,
    GameLose,
    HighScore,
    Quest,
    QuestAll,
    Weekly,
}

impl Event {
    pub fn as_str(&self) -> &'static str {
        match self {
            Event::Pat => "pat",
            Event::Poke => "poke",
            Event::Feed => "feed",
            Event::Triple => "triple",
            Event::Success => "success",
            Event::Failure => "failure",
            Event::Thanks => "thanks",
            Event::Praise => "praise",
            Event::Belly => "belly",
            Event::Tail => "tail",
            Event::Tick => "tick",
            Event::Signin => "signin",
            Event::GameWin => "game-win",
            Event::GameDraw => "game-draw",
            Event::GameLose => "game-lose",
            Event::HighScore => "high-score",
            Event::Quest => "quest",
            Event::QuestAll => "questAll",
            Event::Weekly => "weekly",
        }
    }
}

pub struct GrowthOutcome {
    pub growth: Growth,
    pub unlocks: Vec<String>,
    pub leveled_up: bool,
}

/// 养成数值推进。返回值里带上本次解锁的成就，供表现层弹庆祝。
pub fn compute_growth(prev: &Growth, ev: Event, now: i64, pats: i64, delta_min: f32) -> GrowthOutcome {
    let mut g = prev.clone();
    let mut dm = 0.0f32;
    let mut da = 0.0f32;
    let mut ds = 0.0f32;

    match ev {
        Event::Pat => {
            dm += 4.0;
            da += 2.0;
        }
        Event::Poke => dm -= 6.0,
        Event::Feed => {
            ds += 30.0;
            da += 5.0;
            dm += 3.0;
        }
        Event::Triple => {
            dm += 10.0;
            da += 10.0;
        }
        Event::Success => dm += 3.0,
        Event::Failure => dm -= 5.0,
        Event::Thanks => {
            dm += 6.0;
            da += 20.0;
        }
        Event::Praise => {
            dm += 5.0;
            da += 8.0;
        }
        Event::Belly => {
            dm += 3.0;
            da += 2.0;
        }
        Event::Tail => {
            dm += 2.0;
            da += 3.0;
        }
        Event::Tick => ds -= delta_min * Growth1::SATIETY_DECAY_PER_MIN,
        Event::Signin => {
            let today = day_key(now);
            if g.last_signin != today {
                let y = day_key(now - 86_400_000);
                g.signin_streak = if g.last_signin == y { g.signin_streak + 1 } else { 1 };
                g.last_signin = today;
                dm += 5.0;
            }
        }
        Event::GameWin => {
            dm += 8.0;
            da += 12.0;
        }
        Event::GameDraw => {
            dm += 2.0;
            da += 3.0;
        }
        Event::GameLose => dm -= 3.0,
        Event::HighScore => da += 5.0,
        Event::Quest => {
            da += 8.0;
            dm += 2.0;
        }
        Event::QuestAll => {
            da += 20.0;
            dm += 5.0;
        }
        Event::Weekly => {
            da += 30.0;
            dm += 5.0;
        }
    }

    g.mood = (g.mood + dm).clamp(0.0, Growth1::MOOD_MAX);
    g.affinity = (g.affinity + da).clamp(0.0, Growth1::AFFINITY_MAX);
    g.satiety = (g.satiety + ds).clamp(0.0, Growth1::SATIETY_MAX);

    let level = ((g.affinity / Growth1::LEVEL_STEP).floor() as i32 + 1).max(1);
    let leveled_up = level > g.level;
    if leveled_up {
        g.level = level;
    }

    let mut unlocks = evaluate_achievements(&g);
    // 摸头 / 投喂 / 三连击 / 道谢：按累计次数解锁
    let conds: [(&str, bool); 4] = [
        ("first-pat", ev == Event::Pat && pats >= 1),
        ("ten-pats", ev == Event::Pat && pats >= 10),
        ("hundred-pats", ev == Event::Pat && pats >= 100),
        ("first-feed", ev == Event::Feed),
    ];
    for (id, hit) in conds {
        if hit && !g.achievements.iter().any(|a| a == id) {
            g.achievements.push(id.to_string());
            unlocks.push(id.to_string());
        }
    }
    if ev == Event::Triple && !g.achievements.iter().any(|a| a == "first-triple") {
        g.achievements.push("first-triple".into());
        unlocks.push("first-triple".into());
    }
    if ev == Event::Thanks && !g.achievements.iter().any(|a| a == "thanks") {
        g.achievements.push("thanks".into());
        unlocks.push("thanks".into());
    }

    GrowthOutcome { growth: g, unlocks, leveled_up }
}

pub fn evaluate_achievements(g: &Growth) -> Vec<String> {
    let mut out = Vec::new();
    if g.level >= 5 && !g.achievements.iter().any(|a| a == "lv5") {
        out.push("lv5".into());
    }
    if g.level >= 10 && !g.achievements.iter().any(|a| a == "lv10") {
        out.push("lv10".into());
    }
    if g.signin_streak >= 3 && !g.achievements.iter().any(|a| a == "signin3") {
        out.push("signin3".into());
    }
    if g.signin_streak >= 7 && !g.achievements.iter().any(|a| a == "signin7") {
        out.push("signin7".into());
    }
    out
}

pub fn mood_tier(mood: f32) -> &'static str {
    if mood < 40.0 {
        "low"
    } else if mood < 70.0 {
        "mid"
    } else {
        "high"
    }
}

pub struct BondUnlocks {
    pub action: bool,
    pub badge: bool,
    pub egg: bool,
}

pub fn bond_unlocks(level: i32) -> BondUnlocks {
    BondUnlocks {
        action: level >= BOND_LV3_ACTION,
        badge: level >= BOND_LV5_BADGE,
        egg: level >= BOND_LV7_EGG,
    }
}

pub fn achievement_name<'a>(id: &'a str) -> &'a str {
    ACHIEVEMENTS.iter().find(|a| a.0 == id).map(|a| a.2).unwrap_or(id)
}

/* ============================ 每日任务 ============================ */

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuestSlot {
    pub id: String,
    pub progress: i32,
    pub claimed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Quests {
    pub date: String,
    pub slots: Vec<QuestSlot>,
    pub all_claimed: bool,
}

pub fn quest_def(id: &str) -> Option<(&'static str, &'static str, i32, i32, i32, bool)> {
    QUEST_POOL
        .iter()
        .find(|q| q.0 == id)
        .map(|q| (q.1, q.2, q.3, q.4, q.5, q.6))
}

pub fn refresh_quests(prev: Option<&Quests>, now: i64, rng: &mut Rng) -> Quests {
    let today = day_key(now);
    if let Some(p) = prev {
        if p.date == today && p.slots.len() == 3 {
            return p.clone();
        }
    }
    let mut picks: Vec<usize> = Vec::new();
    let mut pool: Vec<usize> = (0..QUEST_POOL.len()).collect();
    // 常驻任务（今日签到）优先
    if let Some(pos) = pool.iter().position(|&i| QUEST_POOL[i].6) {
        picks.push(pool.remove(pos));
    }
    let prev_ids: Vec<&str> = prev.map(|p| p.slots.iter().map(|s| s.id.as_str()).collect()).unwrap_or_default();
    let mut fresh: Vec<usize> = pool.iter().copied().filter(|&i| !prev_ids.contains(&QUEST_POOL[i].0)).collect();
    let source: &mut Vec<usize> = if fresh.len() >= 2 { &mut fresh } else { &mut pool };
    while picks.len() < 3 && !source.is_empty() {
        let idx = rng.idx(source.len());
        picks.push(source.remove(idx));
    }
    Quests {
        date: today,
        slots: picks
            .iter()
            .map(|&i| QuestSlot { id: QUEST_POOL[i].0.to_string(), progress: 0, claimed: false })
            .collect(),
        all_claimed: false,
    }
}

pub struct QuestPush {
    pub quests: Quests,
    pub completed: Vec<String>,
}

pub fn compute_quests(prev: Option<&Quests>, metric: &str, amount: i32, now: i64, rng: &mut Rng) -> QuestPush {
    let q = refresh_quests(prev, now, rng);
    let mut completed = Vec::new();
    let slots: Vec<QuestSlot> = q
        .slots
        .iter()
        .map(|s| {
            let def = quest_def(&s.id);
            match def {
                // quest_def → (desc, metric, target, affinity, mood, always)
                Some((_, m, target, _, _, _)) if !s.claimed && m == metric => QuestSlot {
                    id: s.id.clone(),
                    progress: (s.progress + amount).min(target),
                    claimed: s.claimed,
                },
                _ => s.clone(),
            }
        })
        .collect();
    for s in &slots {
        if let Some((_, _, target, _, _, _)) = quest_def(&s.id) {
            if s.progress >= target && !s.claimed {
                completed.push(s.id.clone());
            }
        }
    }
    QuestPush {
        quests: Quests { date: q.date, slots, all_claimed: q.all_claimed },
        completed,
    }
}

pub struct ClaimResult {
    pub quests: Quests,
    pub claimed: bool,
    pub newly_all: bool,
    pub affinity: i32,
    pub mood: i32,
}

pub fn claim_quest(prev: Option<&Quests>, id: &str, now: i64, rng: &mut Rng) -> ClaimResult {
    let q = refresh_quests(prev, now, rng);
    let mut did = false;
    let slots: Vec<QuestSlot> = q
        .slots
        .iter()
        .map(|s| {
            if s.id == id && !s.claimed {
                if let Some((_, _, target, _, _, _)) = quest_def(&s.id) {
                    if s.progress >= target {
                        did = true;
                        return QuestSlot { id: s.id.clone(), progress: s.progress, claimed: true };
                    }
                }
            }
            s.clone()
        })
        .collect();
    if !did {
        return ClaimResult { quests: q, claimed: false, newly_all: false, affinity: 0, mood: 0 };
    }
    let all = slots.iter().all(|s| s.claimed);
    let (aff, md) = quest_def(id).map(|q| (q.3, q.4)).unwrap_or((0, 0));
    ClaimResult {
        quests: Quests { date: q.date, slots, all_claimed: all || q.all_claimed },
        claimed: true,
        newly_all: all && !q.all_claimed,
        affinity: aff,
        mood: md,
    }
}

/* ============================ 周签到 ============================ */

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WeekSignin {
    pub week: String,
    pub days: Vec<String>,
    pub rewarded1: bool,
    pub rewarded3: bool,
    pub rewarded7: bool,
}

pub struct WeekResult {
    pub week: WeekSignin,
    pub milestone: Option<&'static str>,
}

pub fn compute_week_signin(prev: Option<&WeekSignin>, day: &str, now: i64) -> WeekResult {
    let wk = week_key(now);
    let same = prev.map(|p| p.week == wk).unwrap_or(false);
    let mut days: Vec<String> = if same { prev.map(|p| p.days.clone()).unwrap_or_default() } else { Vec::new() };
    let (mut r1, mut r3, mut r7) = if same {
        let p = prev.unwrap();
        (p.rewarded1, p.rewarded3, p.rewarded7)
    } else {
        (false, false, false)
    };
    if !day.is_empty() && !days.iter().any(|d| d == day) {
        days.push(day.to_string());
        days.sort();
    }
    let mut milestone = None;
    if !r1 && days.len() >= 1 {
        r1 = true;
        milestone = Some("1");
    } else if !r3 && days.len() >= 3 {
        r3 = true;
        milestone = Some("3");
    } else if !r7 && days.len() >= 7 {
        r7 = true;
        milestone = Some("7");
    }
    WeekResult {
        week: WeekSignin { week: wk, days, rewarded1: r1, rewarded3: r3, rewarded7: r7 },
        milestone,
    }
}

/* ============================ 小游戏：戳泡泡 ============================ */

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bubble {
    Normal,
    Star,
    Bomb,
}

#[derive(Clone)]
pub struct GameState {
    pub board: Vec<Option<(Bubble, i64)>>,
    pub score: i32,
    pub combo: i32,
    pub combo_at: i64,
    pub combo_max: i32,
    pub remaining_ms: i64,
    pub next_spawn_at: i64,
    pub last_at: i64,
    pub status: &'static str, // playing | ended
}

pub fn game_new_state(now: i64) -> GameState {
    GameState {
        board: vec![None; Game1::GRID * Game1::GRID],
        score: 0,
        combo: 0,
        combo_at: 0,
        combo_max: 0,
        remaining_ms: Game1::DURATION_MS,
        next_spawn_at: now + Game1::SPAWN_INTERVAL_MS,
        last_at: now,
        status: "playing",
    }
}

pub fn game_tick(st: &mut GameState, now: i64, rng: &mut Rng) {
    if st.status != "playing" {
        return;
    }
    let dt = (now - st.last_at).max(0);
    for cell in st.board.iter_mut() {
        if let Some((kind, born)) = *cell {
            let life = if kind == Bubble::Star { Game1::STAR_LIFE_MS } else { Game1::BUBBLE_LIFE_MS };
            if now - born >= life {
                *cell = None;
            }
        }
    }
    if now >= st.next_spawn_at {
        let empties: Vec<usize> = st.board.iter().enumerate().filter(|(_, c)| c.is_none()).map(|(i, _)| i).collect();
        if !empties.is_empty() {
            let cell = empties[rng.idx(empties.len())];
            let r = rng.f64();
            let kind = if r < Game1::BOMB_P {
                Bubble::Bomb
            } else if r < Game1::BOMB_P + Game1::STAR_P {
                Bubble::Star
            } else {
                Bubble::Normal
            };
            st.board[cell] = Some((kind, now));
            st.next_spawn_at = now + Game1::SPAWN_INTERVAL_MS;
        }
    }
    st.remaining_ms = (st.remaining_ms - dt).max(0);
    st.last_at = now;
    if st.remaining_ms <= 0 {
        st.status = "ended";
    }
}

pub struct PopResult {
    pub hit: bool,
    pub kind: Option<Bubble>,
    pub delta: i32,
    pub combo: i32,
}

pub fn game_pop(st: &mut GameState, cell: usize, now: i64) -> PopResult {
    if st.status != "playing" || cell >= st.board.len() {
        return PopResult { hit: false, kind: None, delta: 0, combo: st.combo };
    }
    let bubble = match st.board[cell] {
        Some(b) => b,
        None => return PopResult { hit: false, kind: None, delta: 0, combo: st.combo },
    };
    st.board[cell] = None;
    if bubble.0 == Bubble::Bomb {
        st.combo = 0;
        st.combo_at = 0;
        return PopResult { hit: true, kind: Some(Bubble::Bomb), delta: Game1::BOMB_SCORE, combo: 0 };
    }
    let combo = if now - st.combo_at <= Game1::COMBO_WINDOW_MS && st.combo > 0 { st.combo + 1 } else { 1 };
    let base = if bubble.0 == Bubble::Star { Game1::STAR_SCORE } else { Game1::BASE };
    let delta = base + combo.min(Game1::COMBO_CAP) * 2;
    st.score += delta;
    st.combo = combo;
    st.combo_at = now;
    st.combo_max = st.combo_max.max(combo);
    PopResult { hit: true, kind: Some(bubble.0), delta, combo }
}

pub fn game_grade(score: i32) -> &'static str {
    if score >= Game1::WIN_SCORE {
        "win"
    } else if score >= Game1::DRAW_SCORE {
        "draw"
    } else {
        "lose"
    }
}

pub fn game_reward(grade: &str) -> &'static str {
    match grade {
        "win" => "game-win",
        "draw" => "game-draw",
        _ => "game-lose",
    }
}

/* ============================ 小游戏 2：接零食 ============================ */

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Snack {
    Cake,
    Star,
    Bomb,
}

#[derive(Clone)]
pub struct CatchItem {
    pub x: f32,
    pub y: f32,
    pub kind: Snack,
    pub resolved: bool,
}

#[derive(Clone)]
pub struct CatchState {
    pub items: Vec<CatchItem>,
    pub basket_x: f32,
    pub score: i32,
    pub combo: i32,
    pub combo_at: i64,
    pub combo_max: i32,
    pub caught: i32,
    pub missed: i32,
    pub remaining_ms: i64,
    pub next_spawn_at: i64,
    pub last_at: i64,
    pub status: &'static str,
}

pub fn catch_new_state(now: i64) -> CatchState {
    CatchState {
        items: Vec::new(),
        basket_x: 0.5,
        score: 0,
        combo: 0,
        combo_at: 0,
        combo_max: 0,
        caught: 0,
        missed: 0,
        remaining_ms: Catch1::DURATION_MS,
        next_spawn_at: now + Catch1::SPAWN_INTERVAL_MS,
        last_at: now,
        status: "playing",
    }
}

pub fn catch_tick(st: &mut CatchState, now: i64, rng: &mut Rng) {
    if st.status != "playing" {
        return;
    }
    let dt = ((now - st.last_at).max(0)) as f32 / 1000.0;
    if now >= st.next_spawn_at {
        let r = rng.f64();
        let kind = if r < Catch1::BOMB_P {
            Snack::Bomb
        } else if r < Catch1::BOMB_P + Catch1::STAR_P {
            Snack::Star
        } else {
            Snack::Cake
        };
        st.items.push(CatchItem { x: 0.08 + rng.f64() as f32 * 0.84, y: -0.06, kind, resolved: false });
        st.next_spawn_at = now + Catch1::SPAWN_INTERVAL_MS;
    }
    let progress = 1.0 - st.remaining_ms as f32 / Catch1::DURATION_MS as f32;
    let speed = Catch1::FALL_BASE + progress * (Catch1::FALL_MAX - Catch1::FALL_BASE);
    let basket = st.basket_x;
    let mut next = Vec::new();
    for it in st.items.iter_mut() {
        it.y += speed * dt;
        if !it.resolved && it.y >= Catch1::BASKET_Y - Catch1::CATCH_BAND {
            it.resolved = true;
            let got = (it.x - basket).abs() <= Catch1::BASKET_W / 2.0 + 0.03;
            if got {
                let base = match it.kind {
                    Snack::Star => Catch1::STAR_SCORE,
                    Snack::Bomb => Catch1::BOMB_SCORE,
                    Snack::Cake => Catch1::CAKE_SCORE,
                };
                if it.kind == Snack::Bomb {
                    st.combo = 0;
                    st.combo_at = 0;
                    st.score = (st.score + base).max(0);
                } else {
                    let c = if now - st.combo_at <= Catch1::COMBO_WINDOW_MS && st.combo > 0 { st.combo + 1 } else { 1 };
                    st.combo = c;
                    st.combo_at = now;
                    st.combo_max = st.combo_max.max(c);
                    st.score += base + c.min(10) * 2;
                }
                st.caught += 1;
            } else {
                st.missed += 1;
                if it.kind != Snack::Bomb {
                    st.combo = 0;
                    st.combo_at = 0;
                }
            }
        }
        if !it.resolved && it.y < 1.05 {
            next.push(it.clone());
        }
    }
    st.items = next;
    st.remaining_ms = (st.remaining_ms as f32 - dt * 1000.0).max(0.0) as i64;
    st.last_at = now;
    if st.remaining_ms <= 0 {
        st.status = "ended";
    }
}

pub fn catch_move(st: &mut CatchState, x: f32) {
    st.basket_x = x.clamp(0.02, 0.98);
}

/* ============================ 天气 ============================ */

pub struct WeatherInfo {
    pub emoji: &'static str,
    pub label: &'static str,
    pub kind: &'static str,
}

pub fn weather_text(code: i32) -> WeatherInfo {
    for (c, e, l, k) in WEATHER_MAP {
        if *c == code {
            return WeatherInfo { emoji: e, label: l, kind: k };
        }
    }
    WeatherInfo { emoji: "🌈", label: "天气未知", kind: "unknown" }
}

/// 天气视觉特效：优先 thunder > snow > rain > fog > hot > cold > wind > cloudy/sunny
#[derive(Clone, Debug)]
pub struct WeatherFx {
    pub kind: &'static str,
    pub intensity: i32,
    pub mode: &'static str, // motion | static | flash
    pub count: i32,
    pub speed: f32,
    pub opacity: f32,
    pub length: f32,
    pub drift: f32,
    pub size: f32,
    pub bands: i32,
}

pub fn weather_fx(code: i32, temp: Option<f32>, wind: Option<f32>) -> Option<WeatherFx> {
    let base = weather_text(code);
    if base.kind == "unknown" {
        return None;
    }
    let t = temp.unwrap_or(f32::NAN);
    let w = wind.unwrap_or(0.0);
    let cs = code.to_string();
    match base.kind {
        "thunder" => {
            let i = if cs == "96" || cs == "99" { 3 } else { 2 };
            Some(WeatherFx {
                kind: "thunder",
                intensity: i,
                mode: "flash",
                count: if i == 3 { 50 } else { 30 },
                speed: 640.0,
                opacity: if i == 3 { 0.70 } else { 0.50 },
                length: 18.0,
                drift: 0.0,
                size: 2.0,
                bands: 0,
            })
        }
        "snow" => {
            let i = match cs.as_str() { "73" | "86" => 2, "75" => 3, _ => 1 };
            let (c, sp, op, sz) = match i { 2 => (60, 110.0, 0.70, 4.0), 3 => (90, 130.0, 0.85, 5.0), _ => (30, 90.0, 0.55, 3.0) };
            Some(WeatherFx { kind: "snow", intensity: i, mode: "motion", count: c, speed: sp, opacity: op, length: 0.0, drift: 24.0, size: sz, bands: 0 })
        }
        "rain" => {
            let i = match cs.as_str() { "63" | "81" => 2, "65" | "82" => 3, _ => 1 };
            let (c, sp, op, ln) = match i { 2 => (90, 640.0, 0.42, 18.0), 3 => (140, 760.0, 0.55, 22.0), _ => (40, 520.0, 0.30, 14.0) };
            Some(WeatherFx { kind: "rain", intensity: i, mode: "motion", count: c, speed: sp, opacity: op, length: ln, drift: 0.0, size: 2.0, bands: 0 })
        }
        "fog" => {
            let i = if cs == "48" { 2 } else { 1 };
            Some(WeatherFx { kind: "fog", intensity: i, mode: "motion", count: 0, speed: if i == 2 { 12.0 } else { 8.0 }, opacity: if i == 2 { 0.24 } else { 0.16 }, length: 0.0, drift: 0.0, size: 0.0, bands: if i == 2 { 4 } else { 3 } })
        }
        _ => {
            if t.is_finite() && t >= 30.0 {
                let i = if t >= 38.0 { 3 } else if t >= 34.0 { 2 } else { 1 };
                Some(WeatherFx { kind: "hot", intensity: i, mode: "motion", count: 0, speed: 40.0 + 20.0 * (i - 1) as f32, opacity: 0.06 + 0.04 * (i - 1) as f32, length: 0.0, drift: 0.0, size: 0.0, bands: if i == 1 { 2 } else { 3 } })
            } else if t.is_finite() && t <= 0.0 {
                let i = if t <= -13.0 { 3 } else if t <= -6.0 { 2 } else { 1 };
                Some(WeatherFx { kind: "cold", intensity: i, mode: "static", count: 0, speed: 10.0 + 4.0 * (i - 1) as f32, opacity: 0.08 + 0.06 * (i - 1) as f32, length: 0.0, drift: 0.0, size: 0.0, bands: if i == 3 { 4 } else if i == 2 { 3 } else { 2 } })
            } else if w >= 39.0 {
                let i = if w >= 62.0 { 3 } else if w >= 50.0 { 2 } else { 1 };
                Some(WeatherFx { kind: "wind", intensity: i, mode: "motion", count: 12 + 6 * (i - 1), speed: 900.0 + 400.0 * (i - 1) as f32, opacity: 0.18 + 0.08 * (i - 1) as f32, length: 60.0 + 40.0 * (i - 1) as f32, drift: 0.0, size: 0.0, bands: 0 })
            } else if base.kind == "cloudy" {
                let i = if cs == "3" { 2 } else { 1 };
                Some(WeatherFx { kind: "cloudy", intensity: i, mode: "static", count: 0, speed: 0.0, opacity: if i == 2 { 0.10 } else { 0.04 }, length: 0.0, drift: 0.0, size: 0.0, bands: 0 })
            } else {
                Some(WeatherFx { kind: "sunny", intensity: 1, mode: "static", count: 0, speed: 0.0, opacity: 0.05, length: 0.0, drift: 0.0, size: 0.0, bands: 0 })
            }
        }
    }
}

/// 天气对应的立绘（三态：打伞 / 冷 / 雪天，其余按 kind 复用）
pub fn weather_pose(kind: &str, temp: Option<f32>) -> Option<&'static str> {
    Some(match kind {
        "rain" => "weather-umbrella",
        "thunder" => "weather-thunder",
        "snow" => "weather-snow",
        "fog" => "weather-cold",
        "hot" => "daily-melt",
        "cold" => "weather-cold",
        "wind" => "tail-swing",
        "sunny" | "cloudy" => match temp {
            Some(t) if t >= 30.0 => "daily-melt",
            Some(t) if t <= 0.0 => "weather-cold",
            _ => "weather-rain-happy",
        },
        _ => return None,
    })
}
