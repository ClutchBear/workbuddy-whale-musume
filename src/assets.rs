//! 立绘资源：解码、缩放、缓存、后台预取
//!
//! 上游在浏览器里靠 `<img>` 预解码；原生版要在进程内自己解。
//! 策略对齐上游的「首屏 5 张 + 空闲时逐张预取」：
//! 常用 5 张启动时同步解码，其余交给后台线程慢慢解，取用没解好就先给占位（透明）。

use image::imageops::FilterType;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// 缓存尺寸。桌宠显示约 200 逻辑像素，1.75 DPI 下约 350 物理像素，取 360 足够清晰。
const TARGET: u32 = 360;

#[derive(Clone)]
pub struct Frame {
    pub w: i32,
    pub h: i32,
    /// 直通 RGBA（未预乘），行优先
    pub rgba: Vec<u8>,
}

static STORE: OnceLock<Mutex<HashMap<String, Arc<Frame>>>> = OnceLock::new();
static DIR: OnceLock<PathBuf> = OnceLock::new();
/// 正在解码 / 已失败的名字，避免重复排队
static INFLIGHT: OnceLock<Mutex<HashMap<String, u8>>> = OnceLock::new();

fn store() -> &'static Mutex<HashMap<String, Arc<Frame>>> {
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn inflight() -> &'static Mutex<HashMap<String, u8>> {
    INFLIGHT.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 立绘目录：exe 同目录的 assets/
pub fn init(dir: PathBuf) {
    let _ = DIR.set(dir);
}

pub fn dir() -> PathBuf {
    DIR.get().cloned().unwrap_or_else(|| PathBuf::from("assets"))
}

fn decode_file(path: &Path) -> Option<Frame> {
    let bytes = std::fs::read(path).ok()?;
    let img = image::load_from_memory(&bytes).ok()?;
    let img = img.to_rgba8();
    let (w, h) = (img.width(), img.height());
    let img = if w > TARGET || h > TARGET {
        let scale = TARGET as f32 / w.max(h) as f32;
        let nw = (w as f32 * scale).max(1.0) as u32;
        let nh = (h as f32 * scale).max(1.0) as u32;
        image::imageops::resize(&img, nw, nh, FilterType::Triangle)
    } else {
        img
    };
    Some(Frame { w: img.width() as i32, h: img.height() as i32, rgba: img.into_raw() })
}

/// 取一张立绘；没解好就返回 None（调用方回落到底层状态立绘）
pub fn get(name: &str) -> Option<Arc<Frame>> {
    store().lock().ok()?.get(name).cloned()
}

/// 同步解码并缓存
pub fn load(name: &str) -> Option<Arc<Frame>> {
    if let Some(f) = get(name) {
        return Some(f);
    }
    let path = dir().join(format!("{}.webp", name));
    let frame = decode_file(&path)?;
    let arc = Arc::new(frame);
    if let Ok(mut m) = store().lock() {
        m.insert(name.to_string(), arc.clone());
    }
    Some(arc)
}

/// 彩色 emoji（Twemoji PNG，72x72）：exe 同目录 assets/emoji/{codepoint}.png
pub fn load_emoji(code: &str) -> Option<Arc<Frame>> {
    let key = format!("emoji:{code}");
    if let Some(f) = store().lock().ok()?.get(&key).cloned() {
        return Some(f);
    }
    let path = dir().join("emoji").join(format!("{code}.png"));
    let frame = decode_file(&path)?;
    let arc = Arc::new(frame);
    if let Ok(mut m) = store().lock() {
        m.insert(key, arc.clone());
    }
    Some(arc)
}

/// 后台预取一张（幂等，失败静默）
fn prefetch_one(name: &str) {
    {
        let mut inf = match inflight().lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        if inf.contains_key(name) {
            return;
        }
        inf.insert(name.to_string(), 1);
    }
    if get(name).is_some() {
        return;
    }
    let _ = load(name);
}

/// 启动时同步解码的常用姿势（对齐上游「首屏预载 5 张」）
pub fn priority_list() -> [&'static str; 6] {
    ["idle-cute", "running", "afk", "blush", "thinking", "pick-up"]
}

/// 后台线程：其余立绘每 120ms 解一张，冷启动切姿势不迟滞
pub fn start_prefetch(all: Vec<String>) {
    std::thread::spawn(move || {
        for name in priority_list() {
            let _ = load(name);
        }
        for name in all {
            prefetch_one(&name);
            std::thread::sleep(std::time::Duration::from_millis(120));
        }
    });
}

/// 扫描 assets 目录里有哪些立绘（用于预取清单与缺图诊断）
pub fn scan() -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir()) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) == Some("webp") {
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    out.push(stem.to_string());
                }
            }
        }
    }
    out.sort();
    out
}
