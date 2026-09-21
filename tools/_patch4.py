import io

ROOT = r"D:\work\demo\workbuddy-pet-native"


def patch(path, pairs):
    p = ROOT + "\\" + path
    s = io.open(p, encoding="utf-8").read()
    for old, new in pairs:
        if old not in s:
            print("  !! 未命中 [%s]: %r" % (path, old[:70]))
            continue
        s = s.replace(old, new, 1)
    io.open(p, "w", encoding="utf-8", newline="").write(s)
    print("  已改 %s" % path)


# ---------- 1) 唤回按钮：关掉她之后在窗口左下角留一枚「鲸」按钮 ----------
patch("src/render.rs", [
    ("""pub fn draw_recall(cv: &mut Canvas, th: Theme, hits: &mut Vec<HitRect>) {
    let s = cv.scale;
    let m = |x: i32| -> i32 { (x as f32 * s) as i32 };
    cv.fill_round(0, 0, m(WIN_W), m(WIN_H), m(10), th.panel());
    cv.text(m(90), m(150), "🐋", FS_TITLE, th.text());
    cv.text(m(70), m(190), "点一下把她叫回来", FS_SMALL, th.dim());
    hits.push(HitRect { id: "recall".into(), x: 0, y: 0, w: m(WIN_W), h: m(WIN_H) });
}""",
     """/// 唤回按钮：关掉她之后留一枚小按钮，点一下就能叫回来（其余区域保持透明可穿透）
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
}"""),
])

patch("src/main.rs", [
    # 结构里补两个节流字段
    ("    next_save_at: i64,\n    last_click_at: i64,",
     "    next_save_at: i64,\n    last_redraw_at: i64,\n    tray_sig: String,\n    last_click_at: i64,"),
    ("            next_save_at: now + 30_000,\n            last_click_at: 0,",
     "            next_save_at: now + 30_000,\n            last_redraw_at: 0,\n            tray_sig: String::new(),\n            last_click_at: 0,"),
    # 主页面：关掉她 → 只画唤回按钮
    ("""    match app.page {
        Page::Pet => {
            let pose = if app.dragging { "pick-up".to_string() } else { app.pose.clone() };""",
     """    if !app.st.settings.pet {
        render::draw_recall(&mut app.canvas, theme, &mut app.hits);
        app.canvas.finish();
        present(app);
        return;
    }

    match app.page {
        Page::Pet => {
            let pose = if app.dragging { "pick-up".to_string() } else { app.pose.clone() };"""),
    # 定时器：按需重绘 + 托盘仅在变化时刷新
    ("""        WM_TIMER => {
            if a.inertia.2 > 0.0 {
                step_inertia(a);
            }
            tick(a, now);
            update_tray(a.hwnd, &a.pose.clone(), tray_label(a.state_name));
            redraw(a);
            return LRESULT(0);
        }""",
     """        WM_TIMER => {
            if a.inertia.2 > 0.0 {
                step_inertia(a);
            }
            tick(a, now);
            // 托盘只在立绘 / 状态文案变化时才刷新（否则 30fps 空转刷 Shell_NotifyIcon）
            let sig = format!("{}|{}", a.pose, a.state_name);
            if sig != a.tray_sig {
                a.tray_sig = sig;
                update_tray(a.hwnd, &a.pose.clone(), tray_label(a.state_name));
            }
            // 有动画才按帧重绘，静止时半秒一次，别白烧 CPU
            let animating = !a.particles.is_empty()
                || a.trans < 1.0
                || a.page == Page::Game
                || a.dragging
                || a.inertia.2 > 0.0
                || a.mood.as_ref().map(|m| m.2).unwrap_or(false)
                || (a.st.settings.weather_fx && a.weather.is_some());
            if animating || now - a.last_redraw_at >= 500 {
                a.last_redraw_at = now;
                redraw(a);
            }
            return LRESULT(0);
        }"""),
])
print("done")
