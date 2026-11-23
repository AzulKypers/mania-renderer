use std::path::Path;
use tiny_skia::{FilterQuality, IntSize, Pixmap, PixmapPaint, Transform};

pub fn load_pixmap(path: &Path) -> Option<Pixmap> {
    let img = image::open(path).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let mut data = img.into_raw();
    for px in data.chunks_exact_mut(4) {
        let a = px[3] as u16;
        px[0] = ((px[0] as u16 * a + 127) / 255) as u8;
        px[1] = ((px[1] as u16 * a + 127) / 255) as u8;
        px[2] = ((px[2] as u16 * a + 127) / 255) as u8;
    }
    Pixmap::from_vec(data, IntSize::from_wh(w, h)?)
}

pub fn scaled(src: &Pixmap, w: f32, h: f32) -> Option<Pixmap> {
    let (tw, th) = (w.round().max(1.0) as u32, h.round().max(1.0) as u32);
    let mut current: Option<Pixmap> = None;
    loop {
        let cur = current.as_ref().unwrap_or(src);
        let nw = if cur.width() / 2 >= tw {
            cur.width() / 2
        } else {
            cur.width()
        };
        let nh = if cur.height() / 2 >= th {
            cur.height() / 2
        } else {
            cur.height()
        };
        if (nw, nh) == (cur.width(), cur.height()) {
            break;
        }
        let mut half = Pixmap::new(nw.max(1), nh.max(1))?;
        let t = Transform::from_scale(
            nw as f32 / cur.width() as f32,
            nh as f32 / cur.height() as f32,
        );
        half.draw_pixmap(
            0,
            0,
            cur.as_ref(),
            &PixmapPaint {
                quality: FilterQuality::Bilinear,
                ..Default::default()
            },
            t,
            None,
        );
        current = Some(half);
    }
    let cur = current.as_ref().unwrap_or(src);
    let mut out = Pixmap::new(tw, th)?;
    let t = Transform::from_scale(
        tw as f32 / cur.width() as f32,
        th as f32 / cur.height() as f32,
    );
    out.draw_pixmap(
        0,
        0,
        cur.as_ref(),
        &PixmapPaint {
            quality: FilterQuality::Bicubic,
            ..Default::default()
        },
        t,
        None,
    );
    Some(out)
}

pub fn tinted(src: &Pixmap, rgba: [u8; 4]) -> Pixmap {
    let mut out = src.clone();
    for px in out.data_mut().chunks_exact_mut(4) {
        px[0] = (px[0] as u16 * rgba[0] as u16 / 255) as u8;
        px[1] = (px[1] as u16 * rgba[1] as u16 / 255) as u8;
        px[2] = (px[2] as u16 * rgba[2] as u16 / 255) as u8;
        px[3] = (px[3] as u16 * rgba[3] as u16 / 255) as u8;
    }
    out
}

pub fn crop_rows(src: &Pixmap, max_rows: u32) -> Pixmap {
    if src.height() <= max_rows {
        return src.clone();
    }
    let row = src.width() as usize * 4;
    let data = src.data()[..row * max_rows as usize].to_vec();
    Pixmap::from_vec(data, IntSize::from_wh(src.width(), max_rows).unwrap()).unwrap()
}

pub fn blit(dst: &mut Pixmap, src: &Pixmap, x: f32, y: f32, opacity: f32) {
    if opacity <= 0.0 {
        return;
    }
    dst.draw_pixmap(
        x.round() as i32,
        y.round() as i32,
        src.as_ref(),
        &PixmapPaint {
            opacity: opacity.min(1.0),
            quality: FilterQuality::Nearest,
            ..Default::default()
        },
        Transform::identity(),
        None,
    );
}

pub fn blit_scaled(dst: &mut Pixmap, src: &Pixmap, x: f32, y: f32, scale: f32, opacity: f32) {
    if opacity <= 0.0 || scale <= 0.0 {
        return;
    }
    let paint = PixmapPaint {
        opacity: opacity.min(1.0),
        quality: FilterQuality::Bilinear,
        ..Default::default()
    };
    dst.draw_pixmap(
        0,
        0,
        src.as_ref(),
        &paint,
        Transform::from_row(scale, 0.0, 0.0, scale, x, y),
        None,
    );
}

pub fn rotated_ccw(src: &Pixmap) -> Pixmap {
    let (w, h) = (src.width() as usize, src.height() as usize);
    let mut out = Pixmap::new(h as u32, w as u32).unwrap();
    let s = src.data();
    let d = out.data_mut();
    for y in 0..h {
        for x in 0..w {
            let si = (y * w + x) * 4;
            let dx = y;
            let dy = w - 1 - x;
            let di = (dy * h + dx) * 4;
            d[di..di + 4].copy_from_slice(&s[si..si + 4]);
        }
    }
    out
}

pub fn flipped_vertical(src: &Pixmap) -> Pixmap {
    let mut out = src.clone();
    let row = src.width() as usize * 4;
    let h = src.height() as usize;
    let data = src.data();
    let dst = out.data_mut();
    for y in 0..h {
        dst[y * row..(y + 1) * row].copy_from_slice(&data[(h - 1 - y) * row..(h - y) * row]);
    }
    out
}

pub fn blit_sub(dst: &mut Pixmap, src: &Pixmap, x: f32, y: f32, opacity: f32) {
    let op = (opacity.clamp(0.0, 1.0) * 256.0) as u32;
    if op == 0 {
        return;
    }
    let xi = x.round() as i64;
    let yf = y.floor();
    let fy = ((y - yf) * 256.0) as u32;
    let yi = yf as i64;
    let (sw, sh) = (src.width() as i64, src.height() as i64);
    let (dw, dh) = (dst.width() as i64, dst.height() as i64);
    let cx0 = xi.max(0);
    let cx1 = (xi + sw).min(dw);
    if cx0 >= cx1 {
        return;
    }
    const M: u32 = 0x00FF_00FF;
    let s = src.data();
    let read = |row: i64, col: i64| -> u32 {
        if row < 0 || row >= sh {
            0
        } else {
            unsafe {
                (s.as_ptr().add(((row * sw + col) * 4) as usize) as *const u32).read_unaligned()
            }
        }
    };
    let dptr = dst.data_mut().as_mut_ptr() as *mut u32;
    let first = (yi).max(0);
    let last = (yi + sh + 1).min(dh);
    for dy in first..last {
        let r1 = dy - yi;
        let r0 = r1 - 1;
        for dx in cx0..cx1 {
            let col = dx - xi;
            let a = read(r0, col);
            let b = read(r1, col);
            if (a | b) >> 24 == 0 {
                continue;
            }
            let lo = (((a & M) * fy + (b & M) * (256 - fy)) >> 8) & M;
            let hi = ((((a >> 8) & M) * fy + ((b >> 8) & M) * (256 - fy)) >> 8) & M;
            let lo = (lo * op >> 8) & M;
            let hi = (hi * op >> 8) & M;
            let alpha = (hi >> 16).min(255);
            let inv = 256 - alpha;
            let dp = unsafe { dptr.add((dy * dw + dx) as usize) };
            let dv = unsafe { dp.read_unaligned() };
            let mut olo = lo + (((dv & M) * inv >> 8) & M);
            let mut ohi = hi + ((((dv >> 8) & M) * inv >> 8) & M);
            let ov = olo & 0x0100_0100;
            olo = (olo | (ov - (ov >> 8))) & M;
            let ov = ohi & 0x0100_0100;
            ohi = (ohi | (ov - (ov >> 8))) & M;
            unsafe { dp.write_unaligned(olo | (ohi << 8)) };
        }
    }
}
