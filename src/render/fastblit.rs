use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};

pub struct Mips {
    pub levels: Vec<Pixmap>,
    pub opaque: bool,
}

impl Mips {
    pub fn new(src: Pixmap) -> Mips {
        let opaque = src.data().chunks_exact(4).all(|p| p[3] == 255);
        let mut levels = vec![src];
        loop {
            let last = levels.last().unwrap();
            if last.width() < 32 || last.height() < 32 {
                break;
            }
            let (w, h) = (last.width() / 2, last.height() / 2);
            let mut half = Pixmap::new(w.max(1), h.max(1)).unwrap();
            let s = last.data();
            let sw = last.width() as usize;
            let d = half.data_mut();
            for y in 0..h as usize {
                for x in 0..w as usize {
                    for c in 0..4 {
                        let a = s[((2 * y) * sw + 2 * x) * 4 + c] as u32;
                        let b = s[((2 * y) * sw + 2 * x + 1) * 4 + c] as u32;
                        let e = s[((2 * y + 1) * sw + 2 * x) * 4 + c] as u32;
                        let f = s[((2 * y + 1) * sw + 2 * x + 1) * 4 + c] as u32;
                        d[(y * w as usize + x) * 4 + c] = ((a + b + e + f + 2) / 4) as u8;
                    }
                }
            }
            levels.push(half);
        }
        Mips { levels, opaque }
    }

    pub fn base(&self) -> &Pixmap {
        &self.levels[0]
    }
}

pub fn draw(
    dst: &mut Pixmap,
    mips: &Mips,
    px: f32,
    py: f32,
    ox: f32,
    oy: f32,
    sx: f32,
    sy: f32,
    rotation: f32,
    opacity: f32,
    additive: bool,
) {
    let op = (opacity.clamp(0.0, 1.0) * 256.0) as u32;
    if op == 0 || sx == 0.0 || sy == 0.0 {
        return;
    }
    let base = mips.base();
    let (bw, bh) = (base.width() as f32, base.height() as f32);
    if rotation.abs() > 1e-4 {
        let paint = PixmapPaint {
            opacity: opacity.clamp(0.0, 1.0),
            blend_mode: if additive {
                tiny_skia::BlendMode::Plus
            } else {
                tiny_skia::BlendMode::SourceOver
            },
            quality: FilterQuality::Bilinear,
        };
        let t = Transform::from_translate(px, py)
            .pre_rotate(rotation.to_degrees())
            .pre_scale(sx, sy)
            .pre_translate(-ox, -oy);
        dst.draw_pixmap(0, 0, base.as_ref(), &paint, t, None);
        return;
    }
    let x = px - ox * sx;
    let y = py - oy * sy;
    let scale = sx.abs().min(sy.abs());
    let mut level = 0;
    while level + 1 < mips.levels.len() && scale * (1u32 << (level + 1)) as f32 <= 1.0 {
        level += 1;
    }
    let src = &mips.levels[level];
    let (sw, sh) = (src.width() as i64, src.height() as i64);
    let kx = sw as f32 / bw;
    let ky = sh as f32 / bh;
    let x0 = x.min(x + bw * sx);
    let x1 = x.max(x + bw * sx);
    let y0 = y.min(y + bh * sy);
    let y1 = y.max(y + bh * sy);
    let (dw, dh) = (dst.width() as i64, dst.height() as i64);
    let cx0 = (x0.floor() as i64).max(0);
    let cx1 = (x1.ceil() as i64).min(dw);
    let cy0 = (y0.floor() as i64).max(0);
    let cy1 = (y1.ceil() as i64).min(dh);
    if cx0 >= cx1 || cy0 >= cy1 {
        return;
    }
    let solid = sw == 1 && sh == 1;
    let nearest =
        (sx.abs() / kx.recip() - 1.0).abs() < 0.002 && (sy.abs() / ky.recip() - 1.0).abs() < 0.002;
    let edge = |p: f32, lo: f32, hi: f32| -> u32 {
        (((p + 1.0).min(hi) - p.max(lo)).clamp(0.0, 1.0) * 256.0) as u32
    };
    let n = (cx1 - cx0) as usize;
    let mut col_a = Vec::with_capacity(n);
    let mut col_b = Vec::with_capacity(n);
    let mut col_f = Vec::with_capacity(n);
    let mut col_c = Vec::with_capacity(n);
    for dx in cx0..cx1 {
        let u = (((dx as f32 + 0.5 - x) / sx) * kx - 0.5).clamp(0.0, (sw - 1) as f32);
        let ui = u.floor() as usize;
        col_a.push(ui * 4);
        col_b.push((ui + 1).min(sw as usize - 1) * 4);
        col_f.push(if nearest {
            0u32
        } else {
            ((u - ui as f32) * 256.0) as u32
        });
        col_c.push(edge(dx as f32, x0, x1));
    }
    let sdata = src.data();
    let stride_s = sw as usize;
    let stride_d = dw as usize;
    const M: u32 = 0x00FF_00FF;
    #[inline(always)]
    fn lerp2(a: u32, b: u32, f: u32) -> u32 {
        ((a * (256 - f) + b * f) >> 8) & M
    }
    let read = |idx: usize| -> u32 {
        unsafe { (sdata.as_ptr().add(idx * 4) as *const u32).read_unaligned() }
    };
    let solid_px = if solid { read(0) } else { 0 };
    let d = dst.data_mut();
    let dptr = d.as_mut_ptr() as *mut u32;
    for dy in cy0..cy1 {
        let cover_y = edge(dy as f32, y0, y1);
        if cover_y == 0 {
            continue;
        }
        let v = (((dy as f32 + 0.5 - y) / sy) * ky - 0.5).clamp(0.0, (sh - 1) as f32);
        let vi = v.floor() as usize;
        let vi2 = (vi + 1).min(sh as usize - 1);
        let fy = if nearest {
            0u32
        } else {
            ((v - vi as f32) * 256.0) as u32
        };
        let ra = vi * stride_s;
        let rb = vi2 * stride_s;
        let opy = op * cover_y >> 8;
        let base_d = dy as usize * stride_d + cx0 as usize;
        for i in 0..n {
            let k = opy * unsafe { *col_c.get_unchecked(i) } >> 8;
            if k == 0 {
                continue;
            }
            let p = if solid {
                solid_px
            } else {
                let (a, b, fx) = unsafe {
                    (
                        *col_a.get_unchecked(i) / 4,
                        *col_b.get_unchecked(i) / 4,
                        *col_f.get_unchecked(i),
                    )
                };
                let (p00, p01, p10, p11) = (read(ra + a), read(ra + b), read(rb + a), read(rb + b));
                if (p00 | p01 | p10 | p11) >> 24 == 0 && !additive {
                    continue;
                }
                let lo = lerp2(lerp2(p00 & M, p01 & M, fx), lerp2(p10 & M, p11 & M, fx), fy);
                let hi = lerp2(
                    lerp2((p00 >> 8) & M, (p01 >> 8) & M, fx),
                    lerp2((p10 >> 8) & M, (p11 >> 8) & M, fx),
                    fy,
                );
                lo | (hi << 8)
            };
            let lo = ((p & M) * k >> 8) & M;
            let hi = (((p >> 8) & M) * k >> 8) & M;
            let dp = unsafe { dptr.add(base_d + i) };
            let dv = unsafe { dp.read_unaligned() };
            let out = if additive {
                let mut slo = lo + (dv & M);
                let mut shi = (hi & 0xFF) + ((dv >> 8) & 0xFF) | ((dv >> 8) & 0x00FF_0000);
                let ov = slo & 0x0100_0100;
                slo = (slo | (ov - (ov >> 8))) & M;
                if shi & 0x100 != 0 {
                    shi = (shi & !0x1FF) | 0xFF;
                }
                slo | ((shi & M) << 8)
            } else {
                let a = (hi >> 16).min(255);
                let inv = 256 - a;
                let dlo = ((dv & M) * inv >> 8) & M;
                let dhi = (((dv >> 8) & M) * inv >> 8) & M;
                let mut olo = lo + dlo;
                let mut ohi = hi + dhi;
                let ov = olo & 0x0100_0100;
                olo = (olo | (ov - (ov >> 8))) & M;
                let ov = ohi & 0x0100_0100;
                ohi = (ohi | (ov - (ov >> 8))) & M;
                olo | (ohi << 8)
            };
            unsafe { dp.write_unaligned(out) };
        }
    }
}
