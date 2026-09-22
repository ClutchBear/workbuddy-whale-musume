#![allow(unused_imports)]

use super::*;

pub(crate) fn velocity(app: &App) -> (f32, f32) {
    let n = app.pos_samples.len();
    if n < 2 {
        return (0.0, 0.0);
    }
    let (x1, y1, t1) = app.pos_samples[n - 2];
    let (x2, y2, t2) = app.pos_samples[n - 1];
    let dt = (t2 - t1).max(1) as f32;
    ((x2 - x1) as f32 / dt, (y2 - y1) as f32 / dt)
}

pub(crate) fn step_inertia(app: &mut App) {
    let (vx, vy, k) = app.inertia;
    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(app.hwnd, &mut r);
    }
    let w = r.right - r.left;
    let h = r.bottom - r.top;
    let nx = (r.left as f32 + vx * 16.0) as i32;
    let ny = (r.top as f32 + vy * 16.0) as i32;
    let (sx, sy, sw, sh) = work_area(app.hwnd);
    let cx = nx.clamp(sx, (sx + sw - w).max(sx));
    let cy = ny.clamp(sy, (sy + sh - h).max(sy));
    let hit_edge = cx != nx || cy != ny;
    unsafe {
        let _ = SetWindowPos(app.hwnd, Some(HWND_TOPMOST), cx, cy, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
    }
    let k2 = k * 0.86;
    app.drag_angle *= 0.82;
    if hit_edge || k2 < 0.08 || (vx.abs() < 0.01 && vy.abs() < 0.01) {
        app.inertia = (0.0, 0.0, 0.0);
        app.drag_angle = 0.0;
        save_pos(app);
    } else {
        app.inertia = (vx * 0.86, vy * 0.86, k2);
    }
}

/// 忠实移植原版 applyReleasePhysics()（dsh-whale-moe.js v1.7.0）：
/// - speed < 6（轻放）：位置不动，squash 回弹 scale(1.05,0.96)→1，300ms cubic-bezier(.34,1.3,.64,1)
/// - 否则一次位移 clamp(v*2.4, ±70)（×scale 转物理像素），撞工作区边缘则停
/// - 倾斜：hit_edge ? 0 : clamp(vx*1.25, ±20)，420ms cubic-bezier(.2,.9,.3,1) 平滑回 0
pub(crate) fn apply_release(app: &mut App, now: i64) {
    let (vx, vy) = app.drag_last_delta;
    let speed = ((vx * vx + vy * vy) as f32).sqrt();
    let sc = app.scale;
    if trace() {
        logf(&format!("release: delta=({vx},{vy}) speed={speed:.1}"));
    }

    if speed < 6.0 {
        // 轻放：原地 squash 弹性回正（原版 scale(1.05,0.96)→1，300ms easeOutBack）
        app.release_squash = Some((now, 300));
        app.drag_angle = 0.0;
        app.angle_anim = None;
        app.inertia = (0.0, 0.0, 0.0);
        save_pos(app);
        return;
    }

    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(app.hwnd, &mut r);
    }
    let w = r.right - r.left;
    let h = r.bottom - r.top;
    let (sx, sy, sw, sh) = work_area(app.hwnd);
    let margin = (8.0 * sc) as i32;
    let max_left = (sx + sw - w - margin).max(sx + margin);
    let max_top = (sy + sh - h - margin).max(sy + margin);

    let step_x = ((vx as f32 * 2.4).clamp(-70.0, 70.0) * sc) as i32;
    let step_y = ((vy as f32 * 2.4).clamp(-70.0, 70.0) * sc) as i32;
    let nx = (r.left + step_x).clamp(sx + margin, max_left);
    let ny = (r.top + step_y).clamp(sy + margin, max_top);
    let hit_edge = (nx == sx + margin && step_x < 0)
        || (nx == max_left && step_x > 0)
        || (ny == sy + margin && step_y < 0)
        || (ny == max_top && step_y > 0);
    unsafe {
        let _ = SetWindowPos(app.hwnd, Some(HWND_TOPMOST), nx, ny, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
    }
    save_pos(app);

    let angle = if hit_edge { 0.0 } else { (vx as f32 * 1.25).clamp(-20.0, 20.0) };
    app.drag_angle = angle;
    app.angle_anim = Some((angle, now, 480));
    app.inertia = (0.0, 0.0, 0.0);
    app.release_squash = None;
}

/// easeOutBack（cubic-bezier(.34,1.3,.64,1) 近似，带轻微 overshoot）
pub(crate) fn ease_out_back(t: f32) -> f32 {
    let c1 = 1.70158f32;
    let c3 = c1 + 1.0;
    let u = t - 1.0;
    1.0 + c3 * u * u * u + c1 * u * u
}

/// easeOutCubic（cubic-bezier(.2,.9,.3,1) 近似）
pub(crate) fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t) * (1.0 - t) * (1.0 - t)
}
