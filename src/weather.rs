//! 天气陪伴（Open-Meteo）
//!
//! 与上游同样的约定：**城市留空 = 零联网**。填了城市才走 Open-Meteo（免费、无需 Key，
//! 除非用户自己填了 Key）。请求经 Windows 自带的 WinHTTP 发出，不引入额外依赖。
//! 失败一律静默：不弹错、不刷日志，桌宠该怎么待机还怎么待机。

use serde::Deserialize;
use std::sync::Mutex;
use windows::core::PCWSTR;
use windows::Win32::Networking::WinHttp::{
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
    WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
    WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
};

use crate::core::{weather_text, WeatherFx};

#[derive(Clone, Debug, Default)]
pub struct Weather {
    pub place: String,
    pub temp: Option<f32>,
    pub code: Option<i32>,
    pub wind: Option<f32>,
    pub humidity: Option<f32>,
    pub ok: bool,
}

impl Weather {
    pub fn label(&self) -> &'static str {
        match self.code {
            Some(c) => weather_text(c).label,
            None => "未获取",
        }
    }
    pub fn emoji(&self) -> &'static str {
        match self.code {
            Some(c) => weather_text(c).emoji,
            None => "",
        }
    }
    pub fn kind(&self) -> &'static str {
        match self.code {
            Some(c) => weather_text(c).kind,
            None => "unknown",
        }
    }
    pub fn fx(&self) -> Option<WeatherFx> {
        crate::core::weather_fx(self.code?, self.temp, self.wind)
    }
    /// 展示用：19.0°C 小雨
    pub fn summary(&self) -> String {
        match self.temp {
            Some(t) => format!("{:.0}°C {}", t, self.label()),
            None => self.label().to_string(),
        }
    }
}

static CACHE: Mutex<Option<(i64, Weather)>> = Mutex::new(None);
/// 刷新间隔：与上游「设置里手动测试」+ 自身节流对齐，10 分钟一次
pub const REFRESH_MS: i64 = 10 * 60_000;

/* ============================ WinHTTP 薄封装 ============================ */

fn to_wide(s: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsString::from(s).encode_wide().chain(std::iter::once(0)).collect()
}

/// 同步 HTTPS GET。失败返回 None（静默）。
fn https_get(host: &str, path: &str) -> Option<String> {
    unsafe {
        let agent = to_wide("dsh-whale-musume-native/1.0");
        let whost = to_wide(host);
        let wpath = to_wide(path);
        let wnull: Vec<u16> = vec![0];

        let session = WinHttpOpen(
            PCWSTR(agent.as_ptr()),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR(wnull.as_ptr()),
            PCWSTR(wnull.as_ptr()),
            0,
        );
        if session.is_null() {
            return None;
        }
        // 硬超时：默认 resolve 是无限等，线程卡死会让「测试连接」永远不出结果。
        // （解析 5s / 连接 5s / 发送 5s / 接收 10s，最坏 ~25s 必返回）
        if WinHttpSetTimeouts(session, 5000, 5000, 5000, 10000).is_err() {
            crate::logf("weather: set timeouts failed");
        }
        let connect = WinHttpConnect(session, PCWSTR(whost.as_ptr()), 443, 0);
        if connect.is_null() {
            let _ = WinHttpCloseHandle(session);
            return None;
        }
        let accept: Vec<u16> = to_wide("*/*");
        let arr: [*const u16; 2] = [accept.as_ptr(), std::ptr::null()];
        let req = WinHttpOpenRequest(
            connect,
            PCWSTR(to_wide("GET").as_ptr()),
            PCWSTR(wpath.as_ptr()),
            PCWSTR(wnull.as_ptr()),
            PCWSTR(wnull.as_ptr()),
            arr.as_ptr() as *const PCWSTR,
            WINHTTP_FLAG_SECURE,
        );
        if req.is_null() {
            let _ = WinHttpCloseHandle(connect);
            let _ = WinHttpCloseHandle(session);
            return None;
        }
        let mut out = String::new();
        let ok = WinHttpSendRequest(req, None, None, 0, 0, 0).is_ok()
            && WinHttpReceiveResponse(req, std::ptr::null_mut()).is_ok();
        if ok {
            loop {
                let mut avail: u32 = 0;
                if WinHttpQueryDataAvailable(req, &mut avail).is_err() || avail == 0 {
                    break;
                }
                let mut buf = vec![0u8; avail as usize];
                let mut read: u32 = 0;
                if WinHttpReadData(req, buf.as_mut_ptr() as *mut _, avail, &mut read).is_err() || read == 0 {
                    break;
                }
                out.push_str(&String::from_utf8_lossy(&buf[..read as usize]));
            }
        }
        let _ = WinHttpCloseHandle(req);
        let _ = WinHttpCloseHandle(connect);
        let _ = WinHttpCloseHandle(session);
        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    }
}

/* ============================ Open-Meteo ============================ */

#[derive(Deserialize)]
struct GeoResult {
    latitude: Option<f64>,
    longitude: Option<f64>,
    name: Option<String>,
    #[serde(default)]
    country: Option<String>,
}

#[derive(Deserialize)]
struct GeoResp {
    #[serde(default)]
    results: Vec<GeoResult>,
}

#[derive(Deserialize)]
struct Current {
    temperature_2m: Option<f64>,
    weather_code: Option<i32>,
    wind_speed_10m: Option<f64>,
    relative_humidity_2m: Option<f64>,
}

#[derive(Deserialize)]
struct ForecastResp {
    current: Option<Current>,
}

fn key_param(key: &str) -> String {
    if key.trim().is_empty() {
        String::new()
    } else {
        format!("&apikey={}", urlencode(key.trim()))
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// 拿一次实时天气。城市为空直接返回 None（零联网）。
pub fn fetch(city: &str, key: &str) -> Option<Weather> {
    let city = city.trim();
    if city.is_empty() {
        return None;
    }
    let geo_path = format!(
        "/v1/search?name={}&count=1&language=zh&format=json{}",
        urlencode(city),
        key_param(key)
    );
    let geo: GeoResp = serde_json::from_str(&https_get("geocoding-api.open-meteo.com", &geo_path)?).ok()?;
    let r = geo.results.into_iter().next()?;
    let (lat, lon) = match (r.latitude, r.longitude) {
        (Some(a), Some(b)) => (a, b),
        _ => return None,
    };
    let fc_path = format!(
        "/v1/forecast?latitude={}&longitude={}&current=temperature_2m,weather_code,wind_speed_10m,relative_humidity_2m&timezone=auto{}",
        lat, lon, key_param(key)
    );
    let fc: ForecastResp =
        serde_json::from_str(&https_get("api.open-meteo.com", &fc_path)?).ok()?;
    let c = fc.current?;
    let place = match r.country {
        Some(cc) if !cc.is_empty() => format!("{}·{}", r.name.unwrap_or_else(|| city.to_string()), cc),
        _ => r.name.unwrap_or_else(|| city.to_string()),
    };
    Some(Weather {
        place,
        temp: c.temperature_2m.map(|v| v as f32),
        code: c.weather_code,
        wind: c.wind_speed_10m.map(|v| v as f32),
        humidity: c.relative_humidity_2m.map(|v| v as f32),
        ok: true,
    })
}

/// 带缓存的取用：没到刷新时间就用旧值。城市为空 → 清缓存并返回 None。
pub fn current(city: &str, key: &str, now: i64) -> Option<Weather> {
    if city.trim().is_empty() {
        if let Ok(mut g) = CACHE.lock() {
            *g = None;
        }
        return None;
    }
    if let Ok(g) = CACHE.lock() {
        if let Some((at, w)) = &*g {
            if now - *at < REFRESH_MS && w.ok {
                return Some(w.clone());
            }
        }
    }
    match fetch(city, key) {
        Some(w) => {
            if let Ok(mut g) = CACHE.lock() {
                *g = Some((now, w.clone()));
            }
            Some(w)
        }
        None => None,
    }
}

/// 设置页「测试连接」：直接取一次，不读缓存
pub fn test_connection(city: &str, key: &str) -> Result<Weather, String> {
    if city.trim().is_empty() {
        return Err("城市为空".to_string());
    }
    fetch(city, key).ok_or_else(|| "取不到天气：城市名可能没匹配上，或网络不可用".to_string())
}
