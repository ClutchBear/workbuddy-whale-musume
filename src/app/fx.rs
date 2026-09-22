#![allow(unused_imports)]

use super::*;

/// 头顶大表情（原版 burst(symbol)，wm-burst 900ms 关键帧）
pub(crate) fn burst(app: &mut App, code: u32, now: i64) {
    app.bursts.push((code, now));
    if app.bursts.len() > 6 {
        app.bursts.remove(0);
    }
}

pub(crate) fn spawn_particles(app: &mut App, n: i32, kind: u8, hue: u8) {
    if !app.st.settings.particles {
        return;
    }
    for _ in 0..n {
        let a = app.rng.f64() * std::f64::consts::TAU;
        let sp = 0.6 + app.rng.f64() * 1.4;
        app.particles.push(Particle {
            x: (WIN_W / 2) as f32 + (app.rng.f64() as f32 - 0.5) * 90.0,
            y: 170.0 + (app.rng.f64() as f32 - 0.5) * 40.0,
            vx: (a.cos() * sp) as f32,
            vy: (-1.2 - app.rng.f64() * 0.9) as f32,
            life: 1.0,
            max: 1.0,
            kind,
            hue,
        });
    }
    if app.particles.len() > 90 {
        let n = app.particles.len() - 90;
        app.particles.drain(0..n);
    }
}
