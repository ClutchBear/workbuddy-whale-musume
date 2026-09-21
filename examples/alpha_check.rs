// 临时诊断：检查立绘的 alpha 通道是否真的存在
fn main() {
    for name in ["idle", "running", "sleep"] {
        let p = format!("assets/{name}.webp");
        match image::open(&p) {
            Ok(img) => {
                let rgba = img.to_rgba8();
                let (w, h) = rgba.dimensions();
                let (mut z, mut f, mut pa) = (0u64, 0u64, 0u64);
                for px in rgba.pixels() {
                    match px.0[3] {
                        0 => z += 1,
                        255 => f += 1,
                        _ => pa += 1,
                    }
                }
                println!(
                    "{name:8} {w}x{h}  alpha=0: {z:8}  alpha=255: {f:8}  半透明: {pa:8}  角落像素={:?}",
                    rgba.get_pixel(0, 0).0
                );
            }
            Err(e) => println!("{name:8} 打不开: {e}"),
        }
    }
}
