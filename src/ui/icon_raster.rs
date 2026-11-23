pub fn mania_icon(size: u32) -> Vec<u8> {
    let n = size as usize;
    let s = size as f32;
    let mut px = vec![[0.0f32; 4]; n * n];
    let mut fill = |x0: f32, y0: f32, x1: f32, y1: f32, r: f32, top: [f32; 4], bottom: [f32; 4]| {
        let (x0, y0, x1, y1, r) = (x0 * s, y0 * s, x1 * s, y1 * s, r * s);
        let lo_x = (x0.floor().max(0.0)) as usize;
        let hi_x = (x1.ceil().min(s)) as usize;
        let lo_y = (y0.floor().max(0.0)) as usize;
        let hi_y = (y1.ceil().min(s)) as usize;
        for y in lo_y..hi_y {
            for x in lo_x..hi_x {
                let mut cover = 0.0;
                for sy in 0..4 {
                    for sx in 0..4 {
                        let fx = x as f32 + (sx as f32 + 0.5) / 4.0;
                        let fy = y as f32 + (sy as f32 + 0.5) / 4.0;
                        if fx < x0 || fx > x1 || fy < y0 || fy > y1 {
                            continue;
                        }
                        let cx = fx.clamp(x0 + r, x1 - r);
                        let cy = fy.clamp(y0 + r, y1 - r);
                        if (fx - cx).powi(2) + (fy - cy).powi(2) <= r * r {
                            cover += 1.0 / 16.0;
                        }
                    }
                }
                if cover <= 0.0 {
                    continue;
                }
                let t = ((y as f32 + 0.5 - y0) / (y1 - y0).max(1.0)).clamp(0.0, 1.0);
                let mut c = [0.0; 4];
                for i in 0..4 {
                    c[i] = top[i] + (bottom[i] - top[i]) * t;
                }
                let a = c[3] * cover;
                let d = &mut px[y * n + x];
                for i in 0..3 {
                    d[i] = c[i] * a + d[i] * (1.0 - a);
                }
                d[3] = a + d[3] * (1.0 - a);
            }
        }
    };
    let rgb =
        |r: u8, g: u8, b: u8, a: f32| [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a];
    fill(
        0.0,
        0.0,
        1.0,
        1.0,
        0.22,
        rgb(36, 48, 92, 1.0),
        rgb(10, 14, 32, 1.0),
    );
    let (left, right) = (0.17, 0.83);
    let col_w = (right - left) / 4.0;
    for i in 0..4 {
        let x = left + col_w * i as f32;
        fill(
            x + 0.006,
            0.08,
            x + col_w - 0.006,
            0.86,
            0.02,
            rgb(255, 255, 255, 0.05),
            rgb(255, 255, 255, 0.10),
        );
    }
    let note = |i: usize,
                y: f32,
                colour: [f32; 4],
                fill: &mut dyn FnMut(f32, f32, f32, f32, f32, [f32; 4], [f32; 4])| {
        let x = left + col_w * i as f32;
        fill(
            x + 0.018,
            y - 0.045,
            x + col_w - 0.018,
            y + 0.045,
            0.03,
            colour,
            [colour[0] * 0.8, colour[1] * 0.8, colour[2] * 0.8, colour[3]],
        );
    };
    let pink = rgb(255, 102, 170, 1.0);
    let x1 = left + col_w;
    fill(
        x1 + 0.04,
        0.16,
        x1 + col_w - 0.04,
        0.58,
        0.04,
        rgb(255, 160, 205, 0.95),
        rgb(255, 102, 170, 0.95),
    );
    note(1, 0.58, pink, &mut fill);
    note(0, 0.44, rgb(245, 248, 255, 1.0), &mut fill);
    note(2, 0.26, rgb(102, 204, 255, 1.0), &mut fill);
    note(3, 0.64, rgb(245, 248, 255, 1.0), &mut fill);
    fill(
        left - 0.02,
        0.70,
        right + 0.02,
        0.725,
        0.01,
        rgb(255, 255, 255, 0.95),
        rgb(255, 255, 255, 0.95),
    );
    fill(
        left + col_w * 3.0,
        0.52,
        right,
        0.70,
        0.02,
        rgb(102, 204, 255, 0.0),
        rgb(102, 204, 255, 0.45),
    );
    for i in 0..4 {
        let x = left + col_w * i as f32;
        let c = if i == 3 {
            rgb(102, 204, 255, 1.0)
        } else {
            rgb(70, 86, 140, 1.0)
        };
        fill(
            x + 0.02,
            0.76,
            x + col_w - 0.02,
            0.86,
            0.03,
            c,
            [c[0] * 0.7, c[1] * 0.7, c[2] * 0.7, 1.0],
        );
    }
    px.iter()
        .flat_map(|p| {
            [
                (p[0] * 255.0) as u8,
                (p[1] * 255.0) as u8,
                (p[2] * 255.0) as u8,
                (p[3] * 255.0) as u8,
            ]
        })
        .collect()
}
