//! 表现层：GDI 绘制
//!
//! 上游是 DOM + CSS；原生版把同样的版面用 GDI 画进一张 32bpp DIB，再交给
//! `UpdateLayeredWindow` 合成。所以这里是「CSS 的手写等价物」：
//! 圆角卡片 = 逐像素圆角填充，气泡 = 圆角卡片 + 换行文本，粒子 = 多边形/椭圆。
//!
//! ⚠️ GDI 画到 32bpp DIB 时会把 alpha 清零（经典坑），所以文字画完后要在
//! **文字矩形范围内**用亮度补回 alpha，再统一预乘 —— 见 `finish()`。

use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, POINT, RECT, SIZE};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, GetTextExtentPoint32W,
    SelectObject, SetBkMode, SetTextColor, TextOutW, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS,
    FONT_CHARSET, FONT_CLIP_PRECISION, FONT_OUTPUT_PRECISION, FONT_QUALITY, HBRUSH, HDC, HFONT, HGDIOBJ,
};

use crate::assets::Frame;
use crate::core::{Bubble, GameState, Snack, WeatherFx, CatchState};
use crate::state::{rel_time, AppState};

/* ============================ 主题 ============================ */

#[derive(Clone, Copy)]
pub struct Theme {
    pub dark: bool,
}

impl Theme {
    pub fn panel(self) -> (u8, u8, u8, u8) {
        // 浅色对齐系统右键菜单：纯白不透明（半透明会透出桌面发灰发脏）
        if self.dark { (34, 36, 44, 236) } else { (255, 255, 255, 255) }
    }
    pub fn panel2(self) -> (u8, u8, u8, u8) {
        if self.dark { (46, 49, 58, 230) } else { (245, 246, 249, 255) }
    }
    pub fn text(self) -> (u8, u8, u8, u8) {
        if self.dark { (236, 239, 246, 255) } else { (31, 33, 37, 255) }
    }
    pub fn dim(self) -> (u8, u8, u8, u8) {
        if self.dark { (150, 156, 170, 255) } else { (118, 122, 132, 255) }
    }
    pub fn accent(self) -> (u8, u8, u8, u8) {
        if self.dark { (122, 190, 255, 255) } else { (0, 110, 210, 255) }
    }
    pub fn good(self) -> (u8, u8, u8, u8) {
        (58, 190, 120, 255)
    }
    pub fn warn(self) -> (u8, u8, u8, u8) {
        (235, 150, 60, 255)
    }
    pub fn line(self) -> (u8, u8, u8, u8) {
        if self.dark { (70, 74, 86, 255) } else { (225, 227, 232, 255) }
    }
}

/* ============================ 通用结构 ============================ */

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Pet,
    Settings,
    Achievements,
    Journal,
    Quests,
    Game,
}

#[derive(Clone, Debug)]
pub struct HitRect {
    pub id: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl HitRect {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

#[derive(Clone, Copy)]
pub struct Particle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub life: f32,
    pub max: f32,
    pub kind: u8, // 0=爱心 1=星星 2=圆点
    pub hue: u8,  // 0=粉 1=金 2=蓝
}

/* ============================ 画布 ============================ */

pub struct Canvas {
    pub w: i32,
    pub h: i32,
    pub scale: f32,
    dc: HDC,
    dib: windows::Win32::Graphics::Gdi::HBITMAP,
    bits: usize,
    fonts: Vec<HFONT>,
    emoji_fonts: Vec<HFONT>,
    text_rects: Vec<(i32, i32, i32, i32, bool, u8)>,
}

unsafe impl Send for Canvas {}

impl Canvas {
    pub fn new(w: i32, h: i32, scale: f32) -> Option<Canvas> {
        unsafe {
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                return None;
            }
            let mut bmi = BITMAPINFO::default();
            bmi.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // 负 = top-down，省去逐行翻转
                biPlanes: 1,
                biBitCount: 32,
                biCompression: 0, // BI_RGB
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            };
            let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
            let dib = match CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
                Ok(b) => b,
                Err(_) => {
                    let _ = DeleteDC(dc);
                    return None;
                }
            };
            if bits.is_null() {
                let _ = DeleteDC(dc);
                return None;
            }
            let _old = SelectObject(dc, HGDIOBJ(dib.0));
            SetBkMode(dc, windows::Win32::Graphics::Gdi::BACKGROUND_MODE(1)); // TRANSPARENT

            let mut fonts = Vec::new();
            for (px, weight) in [(11.0, 400i32), (12.0, 400), (13.0, 700), (15.0, 700), (10.0, 400), (14.5, 400)] {
                fonts.push(make_font(-(px * scale) as i32, weight));
            }
            let mut emoji_fonts = Vec::new();
            for px in [11.0, 12.0, 13.0, 15.0, 10.0, 14.5f32] {
                emoji_fonts.push(make_font_face(-(px * scale) as i32, 400, "Segoe UI Emoji"));
            }
            Some(Canvas { w, h, scale, dc, dib, bits: bits as usize, fonts, emoji_fonts, text_rects: Vec::new() })
        }
    }

    pub fn hdc(&self) -> HDC {
        self.dc
    }

    /// 像素首地址（用于逐像素 alpha 命中测试）
    pub fn bits_ptr(&self) -> usize {
        self.bits
    }

    pub fn clear(&mut self) {
        self.text_rects.clear();
        let n = (self.w * self.h * 4) as usize;
        unsafe {
            std::ptr::write_bytes(self.bits as *mut u8, 0, n);
        }
    }

    /// 直通 alpha 合成一个像素
    fn px(&mut self, x: i32, y: i32, c: (u8, u8, u8, u8)) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h || c.3 == 0 {
            return;
        }
        let i = ((y * self.w + x) * 4) as usize;
        let base = self.bits;
        unsafe {
            let p = (base + i) as *mut u8;
            let a = c.3 as f32 / 255.0;
            let ia = 1.0 - a;
            // 32bpp BI_RGB DIB 内存字节序是 B,G,R,X（小端），源色按 BGR 摆放，
            // 否则立绘/色块 R/B 互换（蓝鲸娘变橙、皮肤发蓝）
            for k in 0..3 {
                let dst = *p.add(k) as f32;
                let src = [c.2, c.1, c.0][k] as f32;
                *p.add(k) = (src * a + dst * ia) as u8;
            }
            let da = *p.add(3) as f32;
            *p.add(3) = (c.3 as f32 + da * ia) as u8;
        }
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: (u8, u8, u8, u8)) {
        let x1 = (x + w).min(self.w);
        let y1 = (y + h).min(self.h);
        for yy in y.max(0)..y1 {
            for xx in x.max(0)..x1 {
                self.px(xx, yy, c);
            }
        }
    }

    /// 圆角矩形（半径物理像素）
    pub fn fill_round(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, c: (u8, u8, u8, u8)) {
        let x1 = (x + w).min(self.w);
        let y1 = (y + h).min(self.h);
        let r = r.min(w / 2).min(h / 2).max(0);
        for yy in y.max(0)..y1 {
            for xx in x.max(0)..x1 {
                // 四角圆化处理
                let mut inside = true;
                if r > 0 {
                    let (cx, cy) = if xx < x + r && yy < y + r {
                        (x + r, y + r)
                    } else if xx >= x1 - r && yy < y + r {
                        (x1 - r, y + r)
                    } else if xx < x + r && yy >= y1 - r {
                        (x + r, y1 - r)
                    } else if xx >= x1 - r && yy >= y1 - r {
                        (x1 - r, y1 - r)
                    } else {
                        (0, 0)
                    };
                    if cx != 0 || cy != 0 {
                        let dx = xx - cx;
                        let dy = yy - cy;
                        inside = dx * dx + dy * dy <= r * r + r;
                    }
                }
                if inside {
                    self.px(xx, yy, c);
                }
            }
        }
    }

    pub fn stroke_round(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, c: (u8, u8, u8, u8)) {
        self.fill_rect(x + r, y, w - 2 * r, 1, c);
        self.fill_rect(x + r, y + h - 1, w - 2 * r, 1, c);
        self.fill_rect(x, y + r, 1, h - 2 * r, c);
        self.fill_rect(x + w - 1, y + r, 1, h - 2 * r, c);
    }

    /// 把立绘画到指定矩形（可选旋转角度，用于拖拽摇摆与三连击旋转）
    pub fn blit(&mut self, f: &Frame, dx: i32, dy: i32, dw: i32, dh: i32, alpha: f32, angle: f32) {
        if dw <= 0 || dh <= 0 {
            return;
        }
        let a = (alpha.clamp(0.0, 1.0) * 255.0) as u8;
        let rot = angle.abs() > 0.01;
        let (cosv, sinv) = (angle.to_radians().cos(), angle.to_radians().sin());
        let cx = dw as f32 / 2.0;
        let cy = dh as f32 / 2.0;
        let sx = f.w as f32 / dw as f32;
        let sy = f.h as f32 / dh as f32;

        for y in 0..dh {
            for x in 0..dw {
                let (u, v) = if rot {
                    let px = x as f32 - cx;
                    let py = y as f32 - cy;
                    let rx = px * cosv - py * sinv;
                    let ry = px * sinv + py * cosv;
                    (rx + cx, ry + cy)
                } else {
                    (x as f32, y as f32)
                };
                let fx = (u * sx) as i32;
                let fy = (v * sy) as i32;
                if fx < 0 || fy < 0 || fx >= f.w || fy >= f.h {
                    continue;
                }
                let i = ((fy * f.w + fx) * 4) as usize;
                let sa = f.rgba[i + 3];
                if sa == 0 {
                    continue;
                }
                let aa = ((sa as f32) * (a as f32 / 255.0)) as u8;
                if aa == 0 {
                    continue;
                }
                self.px(dx + x, dy + y, (f.rgba[i], f.rgba[i + 1], f.rgba[i + 2], aa));
            }
        }
    }

    /// 椭圆（粒子、光晕用）
    pub fn ellipse(&mut self, cx: i32, cy: i32, rx: i32, ry: i32, c: (u8, u8, u8, u8)) {
        for y in (cy - ry)..=(cy + ry) {
            for x in (cx - rx)..=(cx + rx) {
                let dx = (x - cx) as f32 / rx.max(1) as f32;
                let dy = (y - cy) as f32 / ry.max(1) as f32;
                if dx * dx + dy * dy <= 1.02 {
                    self.px(x, y, c);
                }
            }
        }
    }

    pub fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: (u8, u8, u8, u8), thick: i32) {
        let steps = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
        for s in 0..=steps {
            let x = x0 + (x1 - x0) * s / steps;
            let y = y0 + (y1 - y0) * s / steps;
            for dy in 0..thick {
                self.px(x, y + dy, c);
            }
        }
    }

    /* ---------------- 文本 ---------------- */

    fn font(&self, size_idx: usize) -> HFONT {
        self.fonts[size_idx.min(self.fonts.len() - 1)]
    }

    pub fn text(&mut self, x: i32, y: i32, s: &str, size_idx: usize, c: (u8, u8, u8, u8)) -> i32 {
        let s = strip_emoji(s);
        if s.is_empty() {
            return 0;
        }
        let wide: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            let old = SelectObject(self.dc, HGDIOBJ(self.font(size_idx).0));
            SetTextColor(self.dc, COLORREF(rgb(c.0, c.1, c.2)));
            let mut sz = SIZE::default();
            GetTextExtentPoint32W(self.dc, &wide[..wide.len() - 1], &mut sz);
            let _ = TextOutW(self.dc, x, y, &wide[..wide.len() - 1]);
            let _ = SelectObject(self.dc, old);
            let pad = (6.0 * self.scale) as i32;
            // alpha 重建规则按字色亮度自动选：GDI 写像素会把 alpha 清零，
            // finish() 再从亮度反推。深色字（浅色主题正文）若走「alpha=亮度」
            // 会得到 alpha≈33 → 文字近乎透明（浅色网页上尤明显）。
            // 亮字(>128)走 alpha=亮度；深字走 dark=true → 有字像素 alpha=255，
            // RGB 已被 GDI 混合到背景色，结果正确带抗锯齿。
            let lum = (c.0 as u32 + c.1 as u32 + c.2 as u32) / 3;
            self.text_rects.push((
                (x - pad).max(0),
                (y - pad).max(0),
                (sz.cx + pad * 2).min(self.w),
                (sz.cy + pad * 2).min(self.h),
                lum < 128,
                255,
            ));
            sz.cx
        }
    }

    pub fn text_width(&self, s: &str, size_idx: usize) -> i32 {
        if s.is_empty() {
            return 0;
        }
        let mut total = 0i32;
        unsafe {
            let old = SelectObject(self.dc, HGDIOBJ(self.font(size_idx).0));
            for (e, run) in split_runs(s) {
                let f = if e { self.emoji_font(size_idx) } else { self.font(size_idx) };
                let _ = SelectObject(self.dc, HGDIOBJ(f.0));
                let wide: Vec<u16> = run.encode_utf16().collect();
                let mut sz = SIZE::default();
                let _ = GetTextExtentPoint32W(self.dc, &wide, &mut sz);
                total += sz.cx;
            }
            let _ = SelectObject(self.dc, old);
        }
        total
    }

    fn emoji_font(&self, size_idx: usize) -> HFONT {
        self.emoji_fonts[size_idx.min(self.emoji_fonts.len() - 1)]
    }

    /// 混排绘制：emoji 用 Segoe UI Emoji 单色渲染，其余走普通字体。
    /// dark=true 用于深色文字（白底气泡），alpha 重建按 255-亮度。
    pub fn text_mixed(&mut self, x: i32, y: i32, s: &str, size_idx: usize, c: (u8, u8, u8, u8), dark: bool) -> i32 {
        self.text_mixed_ex(x, y, s, size_idx, c, dark, 255)
    }

    /// 带整体 alpha 系数的混排（气泡淡入淡出用）：GDI 写不了 alpha，
    /// 靠 finish() 的矩形重建把文字 alpha 乘上 amul，等效真透明度衰减。
    pub fn text_mixed_ex(&mut self, x: i32, y: i32, s: &str, size_idx: usize, c: (u8, u8, u8, u8), dark: bool, amul: u8) -> i32 {
        if s.is_empty() {
            return 0;
        }
        let mut cx = x;
        unsafe {
            let old = SelectObject(self.dc, HGDIOBJ(self.font(size_idx).0));
            SetTextColor(self.dc, COLORREF(rgb(c.0, c.1, c.2)));
            for (e, run) in split_runs(s) {
                if e {
                    // 彩色 emoji：逐字符贴 Twemoji PNG（72x72 缩到字号），缺图回落单色字形
                    for ch in run.chars() {
                        let wide: Vec<u16> = ch.to_string().encode_utf16().collect();
                        let mut sz2 = SIZE::default();
                        let _ = SelectObject(self.dc, HGDIOBJ(self.emoji_font(size_idx).0));
                        let _ = GetTextExtentPoint32W(self.dc, &wide, &mut sz2);
                        let adv = sz2.cx.max(1);
                        match crate::assets::load_emoji(&format!("{:x}", ch as u32)) {
                            Some(f) => {
                                let dy = y + sz2.cy - adv; // 与文字底对齐
                                self.blit(&f, cx, dy, adv, adv, 1.0, 0.0);
                            }
                            None => {
                                SetTextColor(self.dc, COLORREF(rgb(120, 120, 120)));
                                let _ = TextOutW(self.dc, cx, y, &wide);
                                let pad = (6.0 * self.scale) as i32;
                                self.text_rects.push((
                                    (cx - pad).max(0),
                                    (y - pad).max(0),
                                    (sz2.cx + pad * 2).min(self.w),
                                    (sz2.cy + pad * 2).min(self.h),
                                    dark,
                                    amul,
                                ));
                            }
                        }
                        cx += adv;
                    }
                    let _ = SelectObject(self.dc, HGDIOBJ(self.font(size_idx).0));
                    continue;
                }
                let _ = SelectObject(self.dc, HGDIOBJ(self.font(size_idx).0));
                let wide: Vec<u16> = run.encode_utf16().collect();
                let mut sz = SIZE::default();
                let _ = GetTextExtentPoint32W(self.dc, &wide, &mut sz);
                let _ = TextOutW(self.dc, cx, y, &wide);
                let pad = (6.0 * self.scale) as i32;
                self.text_rects.push((
                    (cx - pad).max(0),
                    (y - pad).max(0),
                    (sz.cx + pad * 2).min(self.w),
                    (sz.cy + pad * 2).min(self.h),
                    dark,
                    amul,
                ));
                cx += sz.cx;
            }
            let _ = SelectObject(self.dc, old);
        }
        cx - x
    }

    /// 在给定宽度内按字符折行（中文按字断行即可）
    pub fn wrap(&self, s: &str, size_idx: usize, max_w: i32) -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();
        let mut cur = String::new();
        for ch in s.chars() {
            let mut test = cur.clone();
            test.push(ch);
            if self.text_width(&test, size_idx) > max_w && !cur.is_empty() {
                lines.push(cur.clone());
                cur.clear();
                if ch == ' ' {
                    continue;
                }
            }
            cur.push(ch);
        }
        if !cur.is_empty() {
            lines.push(cur);
        }
        if lines.is_empty() {
            lines.push(String::new());
        }
        lines
    }

    /// 收尾：① 在文字矩形内用亮度补回 alpha（GDI 把 alpha 清零了）
    /// ② 整块预乘，交给 UpdateLayeredWindow
    pub fn finish(&mut self) {
        self.flush_text();
    }

    /// 立即重建已记录文字的 alpha（同 finish 第一步）。
    /// 用途：半透明遮罩/浮层盖在文字上之前必须先调用——px() 是预乘混合，
    /// 目的像素 alpha=0（GDI 字刚写完的状态）时 dst RGB 会被整体丢弃，
    /// 遮罩下的文字颜色直接被毁（设置页弹输入框、字被洗白的根因）。
    pub fn flush_text(&mut self) {
        let base = self.bits;
        let (w, h) = (self.w, self.h);
        let rects = std::mem::take(&mut self.text_rects);
        for (rx, ry, rw, rh, dark, amul) in rects {
            for y in ry..(ry + rh).min(h) {
                for x in rx..(rx + rw).min(w) {
                    let i = ((y * w + x) * 4) as usize;
                    unsafe {
                        let p = (base + i) as *mut u8;
                        let r = *p;
                        let g = *p.add(1);
                        let b = *p.add(2);
                        let a = *p.add(3);
                        if a == 0 {
                            let lum = r.max(g).max(b);
                            // 深色字（白底气泡）：RGB 已是 GDI 混合后的最终色（非预乘），
                            // 有字的像素按 amul 比例重建 alpha（淡入淡出时整体半透明，
                            // 消除「字芯浅、边缘深」的空心描边伪影）；
                            // 未写入像素 (0,0,0) 保持透明。
                            // 浅色字（亮字透明底）：alpha=亮度 × amul 的旧逻辑不变。
                            let na = if dark {
                                if lum > 0 { ((255u32 * amul as u32) / 255) as u8 } else { 0 }
                            } else {
                                ((lum as u32 * amul as u32) / 255) as u8
                            };
                            if na > 0 {
                                // GDI 写的是非预乘 RGB，这里补一次预乘（ULW 要求预乘格式）
                                *p = ((r as u32 * na as u32) / 255) as u8;
                                *p.add(1) = ((g as u32 * na as u32) / 255) as u8;
                                *p.add(2) = ((b as u32 * na as u32) / 255) as u8;
                                *p.add(3) = na;
                            }
                        }
                    }
                }
            }
        }
        // ★ 全局预乘已删除：px() 直画的内容（立绘/特效/气泡底）在透明画布上混合，
        //   本身就是预乘格式；旧的全局二次预乘把半透明彩色特效（天气热浪/寒冷/雾）
        //   的 RGB 压扁归零 → 屏幕上只剩中性灰（「跳动灰线」「浅灰方框」的真凶）。
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        unsafe {
            for f in self.fonts.drain(..) {
                let _ = DeleteObject(HGDIOBJ(f.0));
            }
            for f in self.emoji_fonts.drain(..) {
                let _ = DeleteObject(HGDIOBJ(f.0));
            }
            let _ = DeleteObject(HGDIOBJ(self.dib.0));
            let _ = DeleteDC(self.dc);
        }
    }
}

fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

/// GDI 画不出彩色 emoji（Segoe UI Emoji 是 COLR 字体，GDI 不支持），
/// 渲染前剥掉，避免冒出一串豆腐块。
pub fn strip_emoji(s: &str) -> std::borrow::Cow<'_, str> {
    if !s.chars().any(is_emoji) {
        return std::borrow::Cow::Borrowed(s);
    }
    let out: String = s.chars().filter(|c| !is_emoji(*c)).collect();
    std::borrow::Cow::Owned(out.trim_end().to_string())
}

fn is_emoji(c: char) -> bool {
    let u = c as u32;
    matches!(u,
        0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2190..=0x21FF | 0x2B00..=0x2BFF
        | 0xFE00..=0xFE0F | 0x1F1E6..=0x1F1FF | 0x20E3 | 0x3030 | 0x303D | 0x2049 | 0x203C
        | 0x200D
    )
}

/// 按 emoji / 非 emoji 切分文本 run；FE0F（变体选择符）与 200D（ZWJ）直接丢弃，
/// 避免 GDI 单色渲染时冒出中间字形。
fn split_runs(s: &str) -> Vec<(bool, String)> {
    let mut runs: Vec<(bool, String)> = Vec::new();
    for ch in s.chars() {
        let u = ch as u32;
        if u == 0xFE0F || u == 0x200D {
            continue;
        }
        let e = is_emoji(ch);
        match runs.last_mut() {
            Some((pe, buf)) if *pe == e => buf.push(ch),
            _ => runs.push((e, ch.to_string())),
        }
    }
    runs
}

unsafe fn make_font(height: i32, weight: i32) -> HFONT {
    make_font_face(height, weight, "Microsoft YaHei UI")
}

unsafe fn make_font_face(height: i32, weight: i32, face: &str) -> HFONT {
    let name: Vec<u16> = face.encode_utf16().chain(std::iter::once(0)).collect();
    CreateFontW(
        height,
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        FONT_CHARSET(1),          // DEFAULT_CHARSET
        FONT_OUTPUT_PRECISION(0),
        FONT_CLIP_PRECISION(0),
        FONT_QUALITY(2),          // PROOF_QUALITY
        0u32,                     // FF_DONTCARE
        PCWSTR(name.as_ptr()),
    )
}

/* ============================ 字体档位 ============================ */
pub const FS_SMALL: usize = 0;  // 11
pub const FS_BODY: usize = 1;   // 12
pub const FS_BOLD: usize = 2;   // 13 bold
pub const FS_TITLE: usize = 3;  // 15 bold
pub const FS_TINY: usize = 4;   // 10
pub const FS_BUBBLE: usize = 5; // 14.5 regular 气泡正文（原版 13px CSS，实测字高对齐）

/* ============================ 布局常量（逻辑像素） ============================ */

pub const WIN_W: i32 = 300;
pub const WIN_H: i32 = 400;

const PAD: i32 = 10;
const HEAD_H: i32 = 26;

/* ============================ 页面绘制 ============================ */

pub struct PetView<'a> {
    pub st: &'a AppState,
    pub theme: Theme,
    pub pose: &'a str,
    pub angle: f32,
    /// 宽/高缩放（squash 体系，1.0=原大）
    pub sx: f32,
    pub sy: f32,
    /// 垂直位移（逻辑像素，负=向上）
    pub mdy: f32,
    pub bubble: &'a str,
    pub bubble_alpha: f32,
    pub bubble_dy: f32,
    pub badge_kind: u8, // 0=中性 1=工作中 2=成功 3=异常（仅用于天气特效降档）
    pub particles: &'a [Particle],
    /// 头顶大表情 (Twemoji codepoint, 开始时刻)
    pub bursts: &'a [(u32, i64)],
    pub now: i64,
    pub fx: Option<WeatherFx>,
    pub fx_t: f32,
    pub focus: bool,
    pub focus_row: i32,
}

pub fn draw_pet(cv: &mut Canvas, v: &PetView, hits: &mut Vec<HitRect>) {
    let s = cv.scale;
    let th = v.theme;
    let m = |x: i32| -> i32 { (x as f32 * s) as i32 };

    // ── 天气氛围层（窗口内）：工作态降档
    if let Some(fx) = v.fx.as_ref() {
        draw_fx(cv, &fx, v.fx_t, m(WIN_W), m(WIN_H), v.badge_kind == 1);
    }

    // ── 工作态蓝色光晕（对齐原版：工作时角色周围一圈淡蓝辉光，呼吸式起伏）
    // 画在立绘之前 = 垫在角色身后；径向渐隐无硬边，不露窗口矩形。
    if v.badge_kind == 1 {
        let cx = m(WIN_W / 2);
        let cy = m(176); // 立绘中心（200x200 逻辑，底边 y=276）
        let breathe = 0.72 + 0.28 * (v.fx_t * 1.6).sin();
        let base = 30.0 * breathe / 3.5; // 7 层同心椭圆 Σ(1-k)≈3，中心叠加后≈30
        let steps = 7;
        for i in (1..=steps).rev() {
            let k = i as f32 / steps as f32; // 1 = 最外圈
            let al = (base * (1.0 - k) * 1.15) as u8;
            if al == 0 {
                continue;
            }
            cv.ellipse(
                cx,
                cy,
                (m(125) as f32 * k) as i32,
                (m(118) as f32 * k) as i32,
                (155, 205, 255, al),
            );
        }
    }

    // ── 立绘（底部锚定：squash 时脚不离地，对齐原版 transform-origin 50% 92%）
    let dw = m((200.0 * v.sx) as i32);
    let dh = m((200.0 * v.sy) as i32);
    let dx = m((WIN_W as f32 - 200.0 * v.sx) as i32);
    let dy = m((276.0 - 200.0 * v.sy + v.mdy) as i32);
    if let Some(f) = crate::assets::get(v.pose) {
        cv.blit(&f, dx, dy, dw, dh, 1.0, v.angle);
    } else if let Some(f) = crate::assets::get("idle-cute") {
        cv.blit(&f, dx, dy, dw, dh, 0.9, v.angle);
    }

    // ── 头顶大表情（原版 burst 的 wm-burst 900ms 关键帧）
    for (code, t0) in v.bursts {
        let tt = ((v.now - *t0) as f32 / 900.0).clamp(0.0, 1.0);
        if tt >= 1.0 {
            continue;
        }
        let (o, bdy, sc) = if tt < 0.2 {
            let k = tt / 0.2;
            (k, 6.0 * (1.0 - k), 0.4 + 0.85 * k)
        } else if tt < 0.7 {
            let k = (tt - 0.2) / 0.5;
            (1.0, -4.0 * k, 1.25 - 0.25 * k)
        } else {
            let k = (tt - 0.7) / 0.3;
            (1.0 - k, -4.0 - 8.0 * k, 1.0 - 0.1 * k)
        };
        if let Some(f) = crate::assets::load_emoji(&format!("{code:x}")) {
            let sz = (22.0 * sc * s) as i32;
            let bx = m(WIN_W / 2) - sz / 2;
            let by = m(2) + (bdy * s) as i32;
            cv.blit(&f, bx, by, sz, sz, o, 0.0);
        }
    }

    // ── 气泡（对齐原版样式：白底 #fff / 深字 #1a1a1a / 18% 黑边框 / 12px 圆角 /
    //    13px 字号 × 1.5 行高 / 居中悬在头顶 / 底部小尾巴；alpha 支持 pop/out 动画）
    if !v.bubble.is_empty() {
        let max_w = m(250);
        let lines = cv.wrap(v.bubble, FS_BUBBLE, max_w);
        let lh = m(22); // 14.5px × 1.5
        let pad_x = m(12);
        let pad_y = m(8);
        let mut bw = m(24);
        for l in &lines {
            bw = bw.max(cv.text_width(l, FS_BUBBLE) + pad_x * 2);
        }
        let bw = bw.min(m(WIN_W - PAD * 2));
        let bh = (lines.len() as i32) * lh + pad_y * 2 - m(3);
        let by0 = (m(76) - m(13) - bh).max(m(2));
        let bdy = (v.bubble_dy * s) as i32;
        let by = by0 + bdy;
        let bx = m(WIN_W / 2) - bw / 2;
        let ba = v.bubble_alpha.clamp(0.0, 1.0);
        // box-shadow: 0 8px 20px rgb(0 0 0/18%) 近似（向下偏移的多层半透明圆角矩形）
        for i in (1..=12).rev() {
            let a = (46.0 * (1.0 - i as f32 / 13.0) * ba) as u8;
            if a == 0 { continue; }
            cv.fill_round(bx - i, by - i + m(8), bw + 2 * i, bh + 2 * i, m(12) + i, (0, 0, 0, a));
        }
        cv.fill_round(bx, by, bw, bh, m(12), (233, 236, 242, (255.0 * ba) as u8));
        cv.stroke_round(bx, by, bw, bh, m(12), (0, 0, 0, (46.0 * ba) as u8));
        // 底部小尾巴（原版 ::after 旋转方块，近似倒三角）
        let tcx = m(WIN_W / 2);
        let tt = m(7);
        for i in 0..tt {
            let w = (tt - i) * 2;
            cv.fill_rect(tcx - w / 2, by + bh + i, w, 1, (233, 236, 242, (255.0 * ba) as u8));
        }
        // 文字淡出：颜色保持不变，alpha 随气泡整体衰减（text_mixed_ex 重建时乘 amul）。
        // 不能用「颜色向底色靠拢」模拟：GDI 抗锯齿边缘像素混色后亮度与字芯不同，
        // alpha 全量重建会把边缘显成深色 → 淡出瞬间出现「黑边空心字」。
        for (i, l) in lines.iter().enumerate() {
            cv.text_mixed_ex(bx + pad_x, by + pad_y + (i as i32) * lh, l, FS_BUBBLE, (15, 17, 21, 255), true, (255.0 * ba) as u8);
        }
    }

    // ── 粒子（原版无底部状态面板，footer 已删）
    for p in v.particles {
        let a = ((p.life / p.max).clamp(0.0, 1.0) * 235.0) as u8;
        if a < 6 {
            continue;
        }
        let col = match p.hue {
            0 => (255, 140, 180, a),
            1 => (255, 210, 90, a),
            _ => (130, 190, 255, a),
        };
        let x = m(p.x as i32);
        let y = m(p.y as i32);
        match p.kind {
            0 => {
                // 爱心：两个圆 + 一个三角
                let r = m(4);
                cv.ellipse(x - r, y - r / 2, r, r, col);
                cv.ellipse(x + r, y - r / 2, r, r, col);
                for i in 0..(r * 2) {
                    let wdt = (r * 2 - i) / 2 + 1;
                    for k in -wdt..=wdt {
                        cv.px(x + k, y - r / 2 + i, col);
                    }
                }
            }
            1 => {
                // 星星：五角
                let r = m(6);
                for i in 0..10 {
                    let ang = -90.0 + i as f32 * 36.0;
                    let rr = if i % 2 == 0 { r } else { r / 2 };
                    let (ex, ey) = (
                        (ang.to_radians().cos() * rr as f32) as i32,
                        (ang.to_radians().sin() * rr as f32) as i32,
                    );
                    cv.line(x, y, x + ex, y + ey, col, 2);
                }
            }
            _ => cv.ellipse(x, y, m(3), m(3), col),
        }
    }

    // ── 无障碍焦点框
    if v.focus {
        cv.stroke_round(m(2), m(2), m(WIN_W - 4), m(WIN_H - 4), m(10), th.accent());
    }

    // 命中区：整块立绘区可摸（分区判定在 main.rs 里按归一化坐标做）
    hits.push(HitRect { id: "mascot".into(), x: m(50), y: m(76), w: m(200), h: m(200) });
    if !v.bubble.is_empty() {
        hits.push(HitRect { id: "bubble".into(), x: m(PAD), y: m(PAD), w: m(WIN_W - PAD * 2), h: m(30) });
    }
    void_scroll(hits);
}

fn void_scroll(_hits: &mut Vec<HitRect>) {}

/* ---------------- 通用：标题栏 + 返回 ---------------- */

fn draw_header(cv: &mut Canvas, th: Theme, title: &str, hits: &mut Vec<HitRect>) {
    let s = cv.scale;
    let m = |x: i32| -> i32 { (x as f32 * s) as i32 };
    cv.fill_round(0, 0, m(WIN_W), m(HEAD_H + 6), m(10), th.panel());
    cv.text(m(12), m(6), title, FS_TITLE, th.text());
    let bw = m(46);
    let bx = m(WIN_W) - bw - m(10);
    cv.fill_round(bx, m(5), bw, m(20), m(8), th.panel2());
    cv.stroke_round(bx, m(5), bw, m(20), m(8), th.line());
    cv.text(bx + m(10), m(9), "返回", FS_SMALL, th.text());
    // 命中区比可视按钮大一圈：46x20 太小，实际点按常落空
    hits.push(HitRect { id: "back".into(), x: bx - m(8), y: m(1), w: bw + m(14), h: m(28) });
}

/* ---------------- 设置页 ---------------- */

pub fn draw_settings(cv: &mut Canvas, st: &AppState, th: Theme, scroll: i32, focus: i32, hits: &mut Vec<HitRect>) {
    let s = cv.scale;
    let m = |x: i32| -> i32 { (x as f32 * s) as i32 };
    // 页面底色：列表页必须不透明，否则卡片间隙 alpha=0 会被 NCHITTEST
    // 判穿透，点空隙直接漏给下层窗口（拖动滚动也随之失效）
    cv.fill_rect(0, 0, m(WIN_W), m(WIN_H), th.panel());
    draw_header(cv, th, "设置", hits);
    let top = m(HEAD_H + 8) - scroll;
    let mut y = top;
    let row_h = m(30);

    // 分组：陪伴表现 / 天气 / 日常与养成（入口）
    let groups: [(&str, &[(&str, &str, bool)]); 3] = [
        ("陪伴表现", &[
            ("pet", "看板娘", st.settings.pet),
            ("bubble", "台词气泡", st.settings.bubble),
            ("particles", "粒子效果", st.settings.particles),
            ("game", "小游戏", st.settings.game),
            ("keywords", "关键词感知（读聊天内容）", st.settings.keywords),
            ("slack", "摸鱼提醒", st.settings.slack),
            ("night", "深夜模式（23:00–5:59 不打扰）", st.settings.night),
            ("weather_fx", "天气特效", st.settings.weather_fx),
            ("tool_pose", "工具细分姿态", st.settings.tool_pose),
            ("drag_physics", "拖拽惯性", st.settings.drag_physics),
            ("proactive", "主动关怀", st.settings.proactive),
            ("a11y", "无障碍模式（键盘可达）", st.settings.a11y),
        ]),
        ("天气", &[
            ("weather_city", &format!("城市：{} ▸点击输入", if st.settings.weather_city.is_empty() { "（未设置）".to_string() } else { st.settings.weather_city.clone() }), false),
            ("weather_key", &format!("API Key：{} ▸点击输入", if st.settings.weather_key.is_empty() { "（未设置）".to_string() } else { "已填写".to_string() }), false),
            ("weather_test", "测 试 连 接", false),
        ]),
        ("日常与养成", &[
            ("page_quests", "今日任务 / 周签到 / 称号", false),
            ("page_achv", "成就墙", false),
            ("page_journal", "成长日记", false),
            ("reset_pos", "重置悬浮位置", false),
            ("reset_growth", "重置养成数据", false),
        ]),
    ];

    let mut idx = 0i32;
    for (gname, rows) in groups.iter() {
        cv.text(m(12), y, gname, FS_BOLD, th.accent());
        y += m(20);
        for (id, label, on) in *rows {
            if y > m(WIN_H) || y + row_h < 0 {
                y += row_h;
                idx += 1;
                continue;
            }
            let is_switch = matches!(*id,
                "pet" | "bubble" | "particles" | "game" | "keywords" | "slack" | "night"
                | "weather_fx" | "tool_pose" | "drag_physics" | "proactive" | "a11y");
            if *id == "weather_test" {
                // 按钮样式：强调色底 + 白字居中，一眼看出可点
                let bcol = th.accent();
                cv.fill_round(m(10), y, m(WIN_W - 20), row_h - m(4), m(8), bcol);
                let tw = cv.text_width(label, FS_BOLD);
                cv.text_mixed(m(WIN_W / 2) - tw / 2, y + m(5), label, FS_BOLD, (255, 255, 255, 255), true);
            } else {
                cv.fill_round(m(10), y, m(WIN_W - 20), row_h - m(4), m(8), th.panel2());
                if focus == idx {
                    cv.stroke_round(m(10), y, m(WIN_W - 20), row_h - m(4), m(8), th.accent());
                }
                cv.text(m(18), y + m(6), label, FS_BODY, th.text());
            }
            if is_switch {
                // 胶囊开关
                let sw = m(34);
                let sh = m(16);
                let sx = m(WIN_W - 20) - sw - m(8);
                let sy = y + m(5);
                let col = if *on { th.good() } else { th.dim() };
                cv.fill_round(sx, sy, sw, sh, sh / 2, col);
                let kx = if *on { sx + sw - sh / 2 - m(2) } else { sx + m(2) };
                cv.ellipse(kx + sh / 2, sy + sh / 2, sh / 2 - m(2), sh / 2 - m(2), th.panel());
                let _ = on;
            }
            hits.push(HitRect { id: (*id).to_string(), x: m(10), y, w: m(WIN_W - 20), h: row_h - m(4) });
            y += row_h;
            idx += 1;
        }
        y += m(6);
    }
    // 标题栏最后画 = sticky 置顶（滚动条目从底下穿过，不盖「返回」）
    draw_header(cv, th, "设置", hits);
}

/* ---------------- 成就墙 ---------------- */

pub fn draw_achievements(cv: &mut Canvas, st: &AppState, th: Theme, scroll: i32, hits: &mut Vec<HitRect>) {
    let s = cv.scale;
    let m = |x: i32| -> i32 { (x as f32 * s) as i32 };
    let total = crate::data::ACHIEVEMENTS.len();
    let got = crate::data::ACHIEVEMENTS.iter().filter(|a| st.growth.achievements.iter().any(|x| x == a.0)).count();
    // 页面底色（同设置页：间隙不透空、不吃穿透）
    cv.fill_rect(0, 0, m(WIN_W), m(WIN_H), th.panel());

    let top = m(HEAD_H + 8) - scroll;
    let cols = 3;
    let cell_w = m((WIN_W - 24) / cols);
    let cell_h = m(48);
    let mut i = 0;
    for (id, _icon, name, _desc) in crate::data::ACHIEVEMENTS.iter() {
        let r = i / cols;
        let c = i % cols;
        let x = m(12) + c * cell_w;
        let y = top + r * (cell_h + m(6));
        if y + cell_h < 0 || y > m(WIN_H) {
            i += 1;
            continue;
        }
        let unlocked = st.growth.achievements.iter().any(|a| a == id);
        let bg = if unlocked { th.accent() } else { th.panel2() };
        cv.fill_round(x, y, cell_w - m(6), cell_h, m(8), bg);
        let col = if unlocked { (255, 255, 255, 255) } else { th.dim() };
        cv.text(x + m(8), y + m(6), name, FS_SMALL, col);
        cv.text(x + m(8), y + m(24), if unlocked { "已解锁" } else { "未解锁" }, FS_TINY, col);
        hits.push(HitRect { id: format!("achv:{}", id), x, y, w: cell_w - m(6), h: cell_h });
        i += 1;
    }
    // 标题栏最后画 = sticky 置顶
    draw_header(cv, th, &format!("成就墙 {}/{}", got, total), hits);
}

/// 列表页（设置/成就/日记）顶部的反馈气泡浮层：
/// 台词原本只在宠物页立绘头顶绘制，切到设置页后点「测试连接」等操作
/// 完全看不到反馈（用户以为按钮坏了）。这里用同款白底气泡画在页面顶部。
pub fn draw_bubble_overlay(cv: &mut Canvas, th: Theme, text: &str) {
    if text.is_empty() {
        return;
    }
    let s = cv.scale;
    let m = |x: i32| -> i32 { (x as f32 * s) as i32 };
    let max_w = m(260);
    let lines = cv.wrap(text, FS_BUBBLE, max_w);
    let lh = m(22);
    let pad_x = m(12);
    let pad_y = m(8);
    let mut bw = m(24);
    for l in &lines {
        bw = bw.max(cv.text_width(l, FS_BUBBLE) + pad_x * 2);
    }
    let bw = bw.min(m(WIN_W - PAD * 2));
    let bh = (lines.len() as i32) * lh + pad_y * 2 - m(3);
    let bx = m(WIN_W / 2) - bw / 2;
    let by = m(HEAD_H + 6);
    for i in (1..=10).rev() {
        let a = (46.0 * (1.0 - i as f32 / 11.0)) as u8;
        if a == 0 { continue; }
        cv.fill_round(bx - i, by - i + m(6), bw + 2 * i, bh + 2 * i, m(12) + i, (0, 0, 0, a));
    }
    cv.fill_round(bx, by, bw, bh, m(12), (233, 236, 242, 255));
    cv.stroke_round(bx, by, bw, bh, m(12), th.accent());
    for (i, l) in lines.iter().enumerate() {
        cv.text_mixed(bx + pad_x, by + pad_y + (i as i32) * lh, l, FS_BUBBLE, (15, 17, 21, 255), true);
    }
}

/* ---------------- 成长日记 ---------------- *//// 输入框浮层：城市 / API Key 编辑时盖在页面上（不画就等于盲打，实测踩坑）
pub fn draw_edit_box(cv: &mut Canvas, th: Theme, label: &str, buf: &str, now: i64) {
    let s = cv.scale;
    let m = |x: i32| -> i32 { (x as f32 * s) as i32 };
    // 遮罩轻压背景：浅色页面上重遮罩会把整页压成暗灰（与设置配色割裂），
    // 改 27% 黑——对话框靠投影分层，页面保持浅色调
    cv.fill_rect(0, 0, m(WIN_W), m(WIN_H), (0, 0, 0, 70));
    let bw = m(276);
    let bh = m(96);
    let bx = m((WIN_W - 276) / 2);
    let by = m((WIN_H - 96) / 2);
    // 投影（多层半透明）让对话框浮起来
    for i in (1..=10).rev() {
        let a = (60.0 * (1.0 - i as f32 / 11.0)) as u8;
        if a == 0 { continue; }
        cv.fill_round(bx - i, by - i + m(6), bw + 2 * i, bh + 2 * i, m(12) + i, (0, 0, 0, a));
    }
    // 纯白不透明底 + 2px 蓝色实线边框（与设置页强调色一致）
    cv.fill_round(bx, by, bw, bh, m(12), (255, 255, 255, 255));
    cv.stroke_round(bx, by, bw, bh, m(12), th.accent());
    cv.stroke_round(bx + 1, by + 1, bw - 2, bh - 2, m(11), th.accent());
    // 标题行：强调色
    cv.text(bx + m(12), by + m(9), label, FS_BOLD, th.accent());
    // 输入行：淡蓝不透明底 + 2px 蓝色实线边框 + 蓝色文字
    // 与标题同色系（蓝）：白/灰底上、浅色壁纸前都不会隐身；框体不随主题变色（对话框底恒为浅色）
    let ix = bx + m(12);
    let iy = by + m(32);
    let iw = bw - m(24);
    let ih = m(28);
    cv.fill_round(ix, iy, iw, ih, m(6), (225, 239, 255, 255));
    cv.stroke_round(ix, iy, iw, ih, m(6), (0, 110, 210, 255));
    cv.stroke_round(ix + 1, iy + 1, iw - 2, ih - 2, m(5), (0, 110, 210, 255));
    let mut line = buf.to_string();
    // 光标 500ms 闪烁（块状，蓝色）
    if (now / 500) % 2 == 0 {
        line.push('\u{2588}');
    }
    let (shown, tcol) = if buf.is_empty() {
        ("在此输入…".to_string(), (140, 165, 195, 255))
    } else {
        (line, (0, 96, 195, 255))
    };
    cv.text_mixed(ix + m(8), iy + m(6), &shown, FS_BUBBLE, tcol, true);
    // 底部提示
    cv.text(bx + m(12), by + bh - m(18), "回车确认 · Esc 取消 · 点外面=保存", FS_SMALL, (90, 96, 110, 255));
}

pub fn draw_journal(cv: &mut Canvas, st: &AppState, th: Theme, scroll: i32, hits: &mut Vec<HitRect>, now: i64) {
    let s = cv.scale;
    let m = |x: i32| -> i32 { (x as f32 * s) as i32 };
    // 页面底色（同设置页）+ hits 走真命中表：
    // 之前传 &mut Vec::new()，「返回」命中区写进临时 vec 即丢 → 点了没反应
    cv.fill_rect(0, 0, m(WIN_W), m(WIN_H), th.panel());
    // 注意：draw_header 挪到内容之后调用（见函数末尾）——滚动条目不能盖住标题栏
    let top = m(HEAD_H + 6) - scroll;
    let mut y = top;
    if st.journal.is_empty() {
        cv.text(m(14), y + m(8), "还没有记录。摸摸头、完成个任务试试～", FS_BODY, th.dim());
        draw_header(cv, th, "成长日记", hits);
        return;
    }
    // 倒序展示最近 12 条
    for e in st.journal.iter().rev().take(12) {
        if y + m(34) < 0 || y > m(WIN_H) {
            y += m(34);
            continue;
        }
        cv.fill_round(m(10), y, m(WIN_W - 20), m(30), m(8), th.panel2());
        cv.text(m(18), y + m(4), &e.text, FS_SMALL, th.text());
        cv.text(m(18), y + m(18), &rel_time(e.at, now), FS_TINY, th.dim());
        y += m(34);
    }
    // 标题栏最后画 = sticky 置顶：滚动内容从它底下穿过，不再覆盖「返回」
    draw_header(cv, th, "成长日记", hits);
}

/* ---------------- 每日任务 / 周签到 / 称号 ---------------- */

pub fn draw_quests(cv: &mut Canvas, st: &AppState, th: Theme, hits: &mut Vec<HitRect>) {
    let s = cv.scale;
    let m = |x: i32| -> i32 { (x as f32 * s) as i32 };
    cv.fill_rect(0, 0, m(WIN_W), m(WIN_H), th.panel());
    let mut y = m(HEAD_H + 8);

    // 今日任务
    cv.text(m(12), y, "今日任务", FS_BOLD, th.accent());
    y += m(20);
    if let Some(q) = &st.quests {
        for slot in q.slots.iter() {
            let def = crate::core::quest_def(&slot.id);
            let (desc, target) = def.map(|d| (d.0, d.2)).unwrap_or(("", 1));
            cv.fill_round(m(10), y, m(WIN_W - 20), m(34), m(8), th.panel2());
            cv.text(m(18), y + m(4), desc, FS_SMALL, th.text());
            cv.text(m(18), y + m(18), &format!("{}/{}", slot.progress, target), FS_TINY, th.dim());
            if slot.claimed {
                cv.text(m(WIN_W - 68), y + m(10), "已领取", FS_SMALL, th.dim());
            } else if slot.progress >= target {
                let bw = m(52);
                let bx = m(WIN_W - 20) - bw - m(8);
                cv.fill_round(bx, y + m(6), bw, m(22), m(8), th.good());
                cv.text(bx + m(10), y + m(11), "领取", FS_SMALL, (255, 255, 255, 255));
                hits.push(HitRect { id: format!("claim:{}", slot.id), x: bx, y: y + m(6), w: bw, h: m(22) });
            }
            y += m(38);
        }
    } else {
        cv.text(m(14), y, "今天还没有生成任务", FS_SMALL, th.dim());
        y += m(24);
    }

    // 周签到
    y += m(8);
    cv.text(m(12), y, "本周签到", FS_BOLD, th.accent());
    y += m(20);
    let cell = m(30);
    let days = st.week.as_ref().map(|w| w.days.len()).unwrap_or(0);
    for i in 0..7 {
        let x = m(12) + i * (cell + m(6));
        let on = i < days as i32;
        let col = if on { th.good() } else { th.panel2() };
        cv.fill_round(x, y, cell, cell, m(6), col);
        let label = ["一", "二", "三", "四", "五", "六", "日"][i as usize];
        cv.text(x + m(9), y + m(8), label, FS_SMALL, if on { (255, 255, 255, 255) } else { th.dim() });
    }
    y += cell + m(10);

    // 称号
    cv.text(m(12), y, "称号", FS_BOLD, th.accent());
    y += m(20);
    let badge = if st.badge.is_empty() { "（未解锁）" } else { &st.badge };
    cv.fill_round(m(10), y, m(WIN_W - 20), m(26), m(8), th.panel2());
    cv.text(m(18), y + m(5), badge, FS_SMALL, th.text());
    let lvl = st.growth.level;
    cv.text(m(12), y + m(32), &format!("羁绊 Lv{} · 好感 {:.0} · 心情 {:.0} · 饱食 {:.0}", lvl, st.growth.affinity, st.growth.mood, st.growth.satiety), FS_TINY, th.dim());
    // 标题栏最后画 = sticky 置顶
    draw_header(cv, th, "今日任务 · 周签到", hits);
}

/* ---------------- 小游戏：戳泡泡 ---------------- */

pub struct GameView<'a> {
    pub g: &'a GameState,
    pub th: Theme,
    pub cursor: usize,
}

pub fn draw_game(cv: &mut Canvas, v: &GameView, hits: &mut Vec<HitRect>) {
    let s = cv.scale;
    let m = |x: i32| -> i32 { (x as f32 * s) as i32 };
    let th = v.th;
    draw_header(cv, th, "戳泡泡 · 泡泡派对", hits);
    let top = m(HEAD_H + 6);

    // 计分条
    cv.text(m(12), top, &format!("得分 {}", v.g.score), FS_BOLD, th.text());
    cv.text(m(110), top, &format!("剩余 {}s", (v.g.remaining_ms + 999) / 1000), FS_SMALL, th.dim());
    cv.text(m(200), top, &format!("连击 {}", v.g.combo), FS_SMALL, th.accent());

    // 4×4 网格
    let grid_top = top + m(24);
    let cell = m(56);
    let gap = m(6);
    let gx = m((WIN_W - (4 * 56 + 3 * 6)) / 2);
    for i in 0..16usize {
        let r = i / 4;
        let c = i % 4;
        let x = gx + (c as i32) * (cell + gap);
        let y = grid_top + (r as i32) * (cell + gap);
        let bubble = v.g.board[i];
        let (col, label) = match bubble {
            Some((Bubble::Bomb, _)) => ((240, 90, 90, 255), "炸弹"),
            Some((Bubble::Star, _)) => ((255, 196, 60, 255), "星星"),
            Some((Bubble::Normal, born)) => {
                let age = (crate::now_ms() - born) as f32 / crate::core::Game1::BUBBLE_LIFE_MS as f32;
                let a = (255.0 * (1.0 - age * 0.55)) as u8;
                ((120, 190, 250, a), "泡泡")
            }
            None => (th.panel2(), ""),
        };
        cv.fill_round(x, y, cell, cell, m(10), col);
        if i == v.cursor {
            cv.stroke_round(x, y, cell, cell, m(10), th.accent());
        }
        if !label.is_empty() {
            let tw = cv.text_width(label, FS_TINY);
            cv.text(x + (cell - tw) / 2, y + cell / 2 - m(6), label, FS_TINY, (255, 255, 255, 255));
        }
        hits.push(HitRect { id: format!("cell:{}", i), x, y, w: cell, h: cell });
    }

    let ty = grid_top + 4 * (cell + gap) + m(8);
    cv.text(m(12), ty, "鼠标点击泡泡 · 方向键移动 · Enter 引爆 · Esc 退出", FS_TINY, th.dim());
    if v.g.status == "ended" {
        cv.fill_round(m(10), ty + m(20), m(WIN_W - 20), m(40), m(8), th.panel());
        let grade = crate::core::game_grade(v.g.score);
        let txt = match grade { "win" => "泡泡之王！", "draw" => "打得不错～", _ => "再来一局？" };
        cv.text(m(20), ty + m(28), &format!("本局 {} 分 · 最高连击 {} · {}", v.g.score, v.g.combo_max, txt), FS_SMALL, th.text());
        let bw = m(70);
        let bx = m(WIN_W - 20) - bw - m(8);
        cv.fill_round(bx, ty + m(26), bw, m(24), m(8), th.accent());
        cv.text(bx + m(14), ty + m(31), "再来一局", FS_SMALL, (255, 255, 255, 255));
        hits.push(HitRect { id: "again".into(), x: bx, y: ty + m(26), w: bw, h: m(24) });
    }
}

/* ---------------- 小游戏 2：接零食 ---------------- */

pub fn draw_catch(cv: &mut Canvas, c: &CatchState, th: Theme, hits: &mut Vec<HitRect>) {
    let s = cv.scale;
    let m = |x: i32| -> i32 { (x as f32 * s) as i32 };
    draw_header(cv, th, "接零食", hits);
    let top = m(HEAD_H + 6);
    let field_h = m(240);
    cv.fill_round(m(10), top, m(WIN_W - 20), field_h, m(8), th.panel2());
    cv.text(m(12), top - m(16), &format!("得分 {} · 接到 {} · 漏掉 {} · 剩余 {}s",
        c.score, c.caught, c.missed, (c.remaining_ms + 999) / 1000), FS_SMALL, th.dim());

    for it in c.items.iter() {
        let x = m(10) + (it.x as f32 * (WIN_W - 20) as f32 * s) as i32;
        let y = top + (it.y as f32 * field_h as f32) as i32;
        let col = match it.kind {
            Snack::Cake => (240, 170, 90, 255),
            Snack::Star => (255, 210, 70, 255),
            Snack::Bomb => (90, 95, 110, 255),
        };
        cv.ellipse(x, y, m(8), m(8), col);
    }
    // 篮子
    let bx = m(10) + (c.basket_x as f32 * (WIN_W - 20) as f32 * s) as i32;
    let by = top + (crate::core::Catch1::BASKET_Y * field_h as f32) as i32;
    cv.fill_round(bx - m(26), by, m(52), m(10), m(4), th.accent());
    let ty = top + field_h + m(8);
    cv.text(m(12), ty, "左右方向键 / 移动鼠标控制篮子 · Esc 退出", FS_TINY, th.dim());
}

/* ---------------- 天气特效 ---------------- */

fn draw_fx(cv: &mut Canvas, fx: &WeatherFx, t: f32, w: i32, h: i32, downgrade: bool) {
    let n = if downgrade { fx.count / 3 } else { fx.count };
    let op = if downgrade { fx.opacity * 0.5 } else { fx.opacity };
    let a = (op * 255.0) as u8;
    if a < 4 {
        return;
    }
    match fx.kind {
        "rain" | "thunder" => {
            for i in 0..n {
                let seed = i as f32 * 97.13;
                let x = ((seed * 13.7 + t * 11.0) % w as f32).abs();
                let y = ((seed * 7.3 + t * fx.speed) % (h + 40) as f32).abs() - 20.0;
                cv.line(x as i32, y as i32, (x - 3.0) as i32, (y + fx.length) as i32, (170, 200, 240, a), 1);
            }
        }
        "snow" => {
            for i in 0..n {
                let seed = i as f32 * 61.7;
                let x = ((seed * 11.3 + (t * fx.drift).sin() * 18.0) % w as f32).abs();
                let y = ((seed * 5.1 + t * fx.speed * 0.4) % h as f32).abs();
                cv.ellipse(x as i32, y as i32, fx.size as i32, fx.size as i32, (255, 255, 255, a));
            }
        }
        "wind" => {
            for i in 0..n {
                let seed = i as f32 * 43.9;
                let y = ((seed * 9.7) % h as f32).abs();
                let x = ((seed * 3.1 + t * fx.speed) % (w + 160) as f32).abs() - 80.0;
                cv.line(x as i32, y as i32, (x + fx.length) as i32, y as i32, (200, 215, 235, a), 1);
            }
        }
        "fog" => {
            for b in 0..fx.bands {
                let y = ((b as f32 / fx.bands as f32) * h as f32 + (t * fx.speed).sin() * 8.0) as i32;
                cv.fill_rect(0, y, w, 10, (220, 225, 235, a / 3));
            }
        }
        "hot" => {
            for b in 0..fx.bands {
                let y = ((b as f32 / fx.bands as f32) * h as f32 + (t * fx.speed).sin() * 6.0) as i32;
                cv.fill_rect(0, y, w, 8, (255, 180, 120, a / 3));
            }
        }
        "cold" | "cloudy" | "sunny" => {
            // 满窗平涂会把整个窗口矩形显形成「浅灰方框」（原版没有），
            // 改成以角色为中心的柔和光晕：多层同心椭圆径向渐隐，无硬边。
            let col = match fx.kind {
                "cold" => (200, 225, 245),
                "cloudy" => (150, 158, 172),
                _ => (255, 218, 140), // sunny 暖光
            };
            // 中心总亮度 ≈ 原平涂 alpha：8 层同心椭圆叠加系数 Σ(1-k)≈3.5，先除掉
            let peak = match fx.kind {
                "cold" => a as f32 * 0.25,
                "cloudy" => a as f32 * 0.5,
                _ => a as f32,
            } / 3.5;
            let cx = w / 2;
            let cy = h * 3 / 5;
            let steps = 8;
            for i in (1..=steps).rev() {
                let k = i as f32 / steps as f32; // 1 = 最外圈
                let al = (peak * (1.0 - k)) as u8;
                if al == 0 { continue; }
                cv.ellipse(cx, cy, (w as f32 * 0.62 * k) as i32, (h as f32 * 0.55 * k) as i32, (col.0, col.1, col.2, al));
            }
        }
        _ => {}
    }
}

/* ---------------- 唤回按钮（关闭桌宠后） ---------------- */

/// 唤回按钮：关掉她之后留一枚小按钮，点一下就能叫回来（其余区域保持透明可穿透）
pub fn draw_recall(cv: &mut Canvas, th: Theme, hits: &mut Vec<HitRect>) {
    let s = cv.scale;
    let m = |x: i32| -> i32 { (x as f32 * s) as i32 };
    let size = m(64);
    let x = m(8);
    let y = m(WIN_H - 72);
    cv.fill_round(x, y, size, size, size / 2, th.panel());
    cv.stroke_round(x, y, size, size, size / 2, th.accent());
    // GDI 画不出彩色 emoji，用「鲸」字代替 🐋
    let t = "鲸";
    let tw = cv.text_width(t, FS_TITLE);
    cv.text(x + (size - tw) / 2, y + size / 2 - m(10), t, FS_TITLE, th.accent());
    hits.push(HitRect { id: "recall".into(), x, y, w: size, h: size });
}

/// 未使用占位，避免 dead_code 警告
#[allow(dead_code)]
fn _unused(_b: HBRUSH, _r: RECT, _p: POINT) {}
