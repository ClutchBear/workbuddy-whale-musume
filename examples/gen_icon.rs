// 从 dist/assets/idle-cute.webp 生成 tools/pet.ico（透明底，内容自适应裁剪+方形化）
// 运行：cargo run --release --example gen_icon
use std::io::Cursor;

fn main() {
    let img = image::open("dist/assets/idle-cute.webp").expect("打不开 idle-cute.webp");
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    println!("源图 {w}x{h}");

    // alpha 包围盒（阈值 8，忽略边缘噪点）
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0u32, 0u32);
    for y in 0..h {
        for x in 0..w {
            if rgba.get_pixel(x, y).0[3] > 8 {
                x0 = x0.min(x); y0 = y0.min(y);
                x1 = x1.max(x); y1 = y1.max(y);
            }
        }
    }
    assert!(x1 > x0 && y1 > y0, "整图透明？");
    let (bw, bh) = (x1 - x0 + 1, y1 - y0 + 1);
    println!("内容包围盒 ({x0},{y0})-({x1},{y1}) {bw}x{bh}");

    // 四周留 3% padding 后裁剪
    let pad = (bw.max(bh) as f32 * 0.03) as u32;
    let cx0 = x0.saturating_sub(pad); let cy0 = y0.saturating_sub(pad);
    let cx1 = (x1 + pad).min(w - 1); let cy1 = (y1 + pad).min(h - 1);
    let cropped = image::imageops::crop_imm(&rgba, cx0, cy0, cx1 - cx0 + 1, cy1 - cy0 + 1).to_image();
    let (cw, ch) = cropped.dimensions();

    // 方形化：短边补透明，内容居中
    let side = cw.max(ch);
    let mut square = image::RgbaImage::new(side, side);
    image::imageops::overlay(&mut square, &cropped, ((side - cw) / 2) as i64, ((side - ch) / 2) as i64);

    // 各尺寸：256 走 PNG 帧，其余 32bpp BMP DIB 帧
    let mut frames: Vec<(u32, Vec<u8>, bool)> = Vec::new();
    for &s in &[256u32, 64, 48, 32, 16] {
        let resized = image::imageops::resize(&square, s, s, image::imageops::FilterType::Triangle);
        if s == 256 {
            let mut png = Vec::new();
            image::DynamicImage::ImageRgba8(resized.clone())
                .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
                .expect("PNG 编码失败");
            frames.push((s, png, true));
        } else {
            frames.push((s, bmp_dib(&resized), false));
        }
    }
    // 预览图（人工核对用）
    image::DynamicImage::ImageRgba8(
        image::imageops::resize(&square, 256, 256, image::imageops::FilterType::Triangle),
    )
    .save("_tmp/icon_preview.png")
    .ok();

    write_ico("tools/pet.ico", &frames);
    println!("OK tools/pet.ico");
}

/// RGBA → 32bpp BITMAPINFOHEADER DIB（行序 bottom-up + 全 0 AND 掩码）
fn bmp_dib(img: &image::RgbaImage) -> Vec<u8> {
    let (w, h) = img.dimensions();
    let mut out = Vec::with_capacity(40 + (w * h * 4 + ((w + 31) / 32) * 4 * h) as usize);
    out.extend_from_slice(&40u32.to_le_bytes());              // biSize
    out.extend_from_slice(&(w as i32).to_le_bytes());          // biWidth
    out.extend_from_slice(&((h as i32) * 2).to_le_bytes());    // biHeight（XOR+AND 两倍）
    out.extend_from_slice(&1u16.to_le_bytes());                // biPlanes
    out.extend_from_slice(&32u16.to_le_bytes());               // biBitCount
    out.extend_from_slice(&0u32.to_le_bytes());                // biCompression=BI_RGB
    out.extend_from_slice(&(w * h * 4).to_le_bytes());         // biSizeImage
    out.extend_from_slice(&[0u8; 16]);                         // 其余字段全 0
    for y in (0..h).rev() {
        for x in 0..w {
            let p = img.get_pixel(x, y).0;
            out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);  // BGRA
        }
    }
    let and_stride = ((w + 31) / 32) * 4;
    out.extend(std::iter::repeat(0u8).take((and_stride * h) as usize));
    out
}

fn write_ico(path: &str, frames: &[(u32, Vec<u8>, bool)]) {
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // type=icon
    out.extend_from_slice(&(frames.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * frames.len();
    for (s, data, is_png) in frames {
        out.push((*s % 256) as u8);
        out.push((*s % 256) as u8);
        out.push(0); // 调色板
        out.push(0); // 保留
        out.extend_from_slice(&1u16.to_le_bytes()); // 平面
        out.extend_from_slice(&(if *is_png { 0u16 } else { 32u16 }).to_le_bytes()); // bpp（PNG 帧填 0）
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += data.len();
    }
    for (_, data, _) in frames {
        out.extend_from_slice(data);
    }
    std::fs::write(path, out).expect("写 ico 失败");
}
