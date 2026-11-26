use super::shapes::{fill_rect, fill_round, hex, stroke_round};
use super::text::{Align, TextRenderer};
use super::Scene;
use crate::game::{Judgement, Snapshot};
use crate::overlays::catalog::{colour, def, pastel, OverlayConfig};
use tiny_skia::{
    FilterQuality, Paint, PathBuilder, Pixmap, PixmapPaint, Stroke as SkStroke, Transform,
};

const WHITE: [u8; 4] = [255, 255, 255, 255];

pub struct P<'a> {
    pub f: &'a mut Pixmap,
    pub k: f32,
    pub ox: f32,
    pub oy: f32,
    pub alpha: f32,
}

impl P<'_> {
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, c: [u8; 4]) {
        let a = c[3] as f32 / 255.0 * self.alpha;
        if r <= 0.0 {
            fill_rect(
                self.f,
                self.ox + x * self.k,
                self.oy + y * self.k,
                w * self.k,
                h * self.k,
                [c[0], c[1], c[2], 255],
                a,
            );
        } else {
            fill_round(
                self.f,
                self.ox + x * self.k,
                self.oy + y * self.k,
                w * self.k,
                h * self.k,
                r * self.k,
                [c[0], c[1], c[2], 255],
                a,
            );
        }
    }
    pub fn stroke(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, width: f32, c: [u8; 4]) {
        stroke_round(
            self.f,
            self.ox + x * self.k,
            self.oy + y * self.k,
            w * self.k,
            h * self.k,
            r * self.k,
            width * self.k,
            [c[0], c[1], c[2], 255],
            c[3] as f32 / 255.0 * self.alpha,
        );
    }
    pub fn text(
        &mut self,
        tx: &TextRenderer,
        s: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        size: f32,
        c: [u8; 4],
        align: Align,
        bold: bool,
    ) {
        let size_px = size * self.k;
        let ax = match align {
            Align::Left => x,
            Align::Center => x + w / 2.0,
            Align::Right => x + w,
        };
        let col = [c[0], c[1], c[2], (c[3] as f32 * self.alpha) as u8];
        tx.draw_fit(
            self.f,
            s,
            self.ox + ax * self.k,
            self.oy + y * self.k + (h * self.k - size_px) / 2.0,
            w * self.k,
            size_px,
            col,
            align,
            bold,
        );
    }
    pub fn shadow_text(
        &mut self,
        tx: &TextRenderer,
        s: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        size: f32,
        c: [u8; 4],
        align: Align,
        bold: bool,
    ) {
        self.text(
            tx,
            s,
            x - 1.0,
            y + 1.0,
            w,
            h,
            size,
            [0, 0, 0, 160],
            align,
            bold,
        );
        self.text(tx, s, x, y, w, h, size, c, align, bold);
    }
    pub fn image(&mut self, img: &Pixmap, x: f32, y: f32, w: f32, h: f32, opacity: f32) {
        let paint = PixmapPaint {
            opacity: (opacity * self.alpha).clamp(0.0, 1.0),
            quality: FilterQuality::Bilinear,
            ..Default::default()
        };
        let t = Transform::from_row(
            w * self.k / img.width() as f32,
            0.0,
            0.0,
            h * self.k / img.height() as f32,
            self.ox + x * self.k,
            self.oy + y * self.k,
        );
        self.f.draw_pixmap(0, 0, img.as_ref(), &paint, t, None);
    }
}

fn judgement_colour(i: usize) -> [u8; 4] {
    [
        [240, 255, 255, 255],
        [255, 165, 0, 255],
        [34, 139, 34, 255],
        [30, 144, 255, 255],
        [255, 105, 180, 255],
        [220, 20, 60, 255],
    ][i]
}

fn fmt_time(ms: f64) -> String {
    let s = (ms / 1000.0).max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

impl P<'_> {
    pub fn vgradient(&mut self, x: f32, y: f32, w: f32, h: f32, c: [u8; 3], a_top: f32, a_bottom: f32) {
        if h <= 0.0 || w <= 0.0 {
            return;
        }
        let (x0, y0) = (self.ox + x * self.k, self.oy + y * self.k);
        let (x1, y1) = (x0 + w * self.k, y0 + h * self.k);
        let col = |a: f32| tiny_skia::Color::from_rgba8(c[0], c[1], c[2], (255.0 * a * self.alpha).clamp(0.0, 255.0) as u8);
        let shader = tiny_skia::LinearGradient::new(
            tiny_skia::Point::from_xy(x0, y0),
            tiny_skia::Point::from_xy(x0, y1),
            vec![tiny_skia::GradientStop::new(0.0, col(a_top)), tiny_skia::GradientStop::new(1.0, col(a_bottom))],
            tiny_skia::SpreadMode::Pad,
            Transform::identity(),
        );
        if let (Some(shader), Some(rect)) = (shader, tiny_skia::Rect::from_ltrb(x0, y0, x1, y1)) {
            let mut paint = tiny_skia::Paint::default();
            paint.shader = shader;
            paint.anti_alias = false;
            self.f.fill_rect(rect, &paint, Transform::identity(), None);
        }
    }

    pub fn polyline(&mut self, pts: &[(f32, f32)], width: f32, c: [u8; 4]) {
        if pts.len() < 2 {
            return;
        }
        let mut pb = tiny_skia::PathBuilder::new();
        pb.move_to(self.ox + pts[0].0 * self.k, self.oy + pts[0].1 * self.k);
        for &(x, y) in &pts[1..] {
            pb.line_to(self.ox + x * self.k, self.oy + y * self.k);
        }
        if let Some(path) = pb.finish() {
            let mut paint = tiny_skia::Paint::default();
            paint.set_color_rgba8(c[0], c[1], c[2], (c[3] as f32 * self.alpha) as u8);
            paint.anti_alias = true;
            let stroke = tiny_skia::Stroke { width: width * self.k, line_join: tiny_skia::LineJoin::Round, ..Default::default() };
            self.f.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }
}

impl Scene {
    fn judged_in_last_second(&self, t: f64) -> u32 {
        let end = self.sim.judgements.partition_point(|j| j.time <= t);
        let start = self.sim.judgements.partition_point(|j| j.time <= t - 1000.0 * self.rate);
        (end - start) as u32
    }

    fn ov_nps_line(&self, p: &mut P, t: f64) {
        let tx = &self.text;
        let step = 200.0 * self.rate;
        let first = self.map.first_time() as f64 - 1000.0 * self.rate;
        let sample = |k: f64| -> f64 { ((t - first) / step).floor() * step + first - k * step };
        let now = if t >= first { self.judged_in_last_second(sample(0.0)) } else { 0 };
        let mut max = 0u32;
        let mut s = first;
        let last = sample(0.0);
        while s <= last {
            max = max.max(self.judged_in_last_second(s));
            s += step;
        }
        let (gx, gy, gw, gh) = (40.0f32, 50.0f32, 320.0f32, 280.0f32);
        p.rect(gx, gy, gw, gh, 0.0, [26, 26, 26, 255]);
        p.stroke(gx, gy, gw, gh, 0.0, 1.0, [255, 255, 255, 255]);
        let n = 100;
        let mut pts = Vec::with_capacity(n);
        for i in 0..n {
            let ts = sample((n - 1 - i) as f64);
            let v = if ts >= first && t >= first { self.judged_in_last_second(ts) } else { 0 };
            let y = if max > 0 { gy + gh - v as f32 / max as f32 * gh } else { gy + gh };
            pts.push((gx + gw * (i as f32 + 0.5) / n as f32, y));
        }
        p.polyline(&pts, 1.5, [255, 255, 255, 255]);
        p.text(tx, &format!("{now} Nps"), 0.0, 0.0, 360.0, 44.0, 32.0, WHITE, Align::Right, true);
    }

    pub(super) fn pp_at(&self, t: f64, snap: &Snapshot) -> f64 {
        let i = self.pp_curve.partition_point(|c| c.0 <= t);
        let stars = if i == 0 {
            self.pp_curve.first().map(|c| c.1).unwrap_or(0.0)
        } else {
            self.pp_curve[i - 1].1
        };
        crate::game::pp::mania_pp(stars, snap.counts, self.mods)
    }

    fn last_change(&self, kind: usize, t: f64) -> Option<f64> {
        let end = self.sim.judgements.partition_point(|j| j.time <= t);
        self.sim.judgements[..end]
            .iter()
            .rev()
            .take(400)
            .find(|j| j.kind.index() == kind)
            .map(|j| j.time)
    }

    pub(super) fn draw_overlays(
        &self,
        f: &mut Pixmap,
        t: f64,
        snap: &Snapshot,
        overlays: &[OverlayConfig],
    ) {
        let w = self.opts.width as f32 / 1920.0;
        let out_ms = (t - self.start) / self.rate;
        let fade = if self.no_fade {
            1.0
        } else {
            (out_ms / 600.0).clamp(0.0, 1.0) as f32
        };
        for cfg in overlays.iter().filter(|c| c.enabled) {
            let mut p = P {
                f,
                k: cfg.scale * w,
                ox: cfg.x * w,
                oy: cfg.y * w,
                alpha: fade,
            };
            match cfg.id.as_str() {
                "kps" => self.ov_kps(&mut p, cfg, t),
                "panel" => self.ov_panel(&mut p, cfg, t, snap),
                "hiterror_osu" => self.ov_hiterror_osu(&mut p, t),
                "hiterror_bar" => self.ov_hiterror_bar(&mut p, t, false),
                "hiterror_colorful" => self.ov_hiterror_bar(&mut p, t, true),
                "hitcount_text" => self.ov_hitcount_text(&mut p, t, snap),
                "hitcount_simple" => self.ov_hitcount_simple(&mut p, snap),
                "nowplaying" => self.ov_nowplaying(&mut p),
                "nowplaying_disk" => self.ov_disk(&mut p, cfg, t, snap),
                "hitcount_bar" => self.ov_hitcount_bar(&mut p, cfg, t, snap),
                "nps_chart" => self.ov_nps(&mut p, cfg, t),
                "osu_fps" => self.ov_osu_fps(&mut p, cfg, t),
                "mania_ingame" => {
                    let on = |k: &str| cfg.options.get(k).map(|v| v != "off").unwrap_or(true);
                    if on("hiterror") {
                        self.ov_hiterror_bar(&mut p, t, false);
                    }
                    if on("hitcount") {
                        p.oy += 80.0 * p.k;
                        self.ov_hitcount_text(&mut p, t, snap);
                    }
                }
                "osukps" => self.ov_osukps(&mut p, cfg, t),
                "mods" => self.ov_mods(&mut p, cfg, t),
                "maniacat" => self.ov_cat(&mut p, cfg, t),
                _ => {}
            }
        }
    }

    pub fn overlay_size(id: &str) -> (f32, f32) {
        def(id).map(|d| d.size).unwrap_or((100.0, 100.0))
    }

    fn ov_mods(&self, p: &mut P, cfg: &OverlayConfig, t: f64) {
        let (bw, bh) = Self::overlay_size("mods");
        let unit = self.opts.width as f32 / 1920.0;
        let left = cfg
            .options
            .get("align")
            .map(|v| v == "left")
            .unwrap_or(false);
        let mode = cfg.options.get("spacing").map(|v| v.as_str()).unwrap_or("osu");
        let overlap = mode == "overlap";
        let stacked = mode == "play";
        if stacked {
            let secs = (t - self.start) / self.rate / 1000.0;
            let fade = (1.0 - (secs - 5.0) / 0.5).clamp(0.0, 1.0) as f32;
            if fade <= 0.0 {
                return;
            }
            p.alpha *= fade;
        }
        let cy = bh / 2.0;
        let icons: Vec<&(String, Option<Pixmap>)> = if left {
            self.mod_icons.iter().collect()
        } else {
            self.mod_icons.iter().rev().collect()
        };
        let mut edge = if left { 0.0 } else { bw };
        let mut first = true;
        for (short, icon) in icons {
            let (w, h) = match icon {
                Some(img) => (img.width() as f32 / unit * 1.13, img.height() as f32 / unit * 1.13),
                None => (66.0, 66.0),
            };
            if !first {
                if stacked {
                    edge = if left { 0.0 } else { bw };
                } else {
                    let shift = if overlap { -w * 0.4 } else { 8.0 };
                    edge += if left { shift } else { -shift };
                }
            }
            first = false;
            let x = if left { edge } else { edge - w };
            match icon {
                Some(img) => p.image(img, x, cy - h / 2.0, w, h, 1.0),
                None => {
                    p.rect(x, cy - h / 2.0, w, h, 11.0, [40, 40, 50, 217]);
                    p.text(
                        &self.text,
                        short,
                        x,
                        cy - h / 2.0,
                        w,
                        h,
                        27.0,
                        WHITE,
                        Align::Center,
                        true,
                    );
                }
            }
            edge = if left { x + w } else { x };
        }
    }

    fn ov_cat(&self, p: &mut P, cfg: &OverlayConfig, t: f64) {
        let opt = |k: &str, d: &str| cfg.options.get(k).cloned().unwrap_or_else(|| d.to_string());
        let (fur, line, eye) = match opt("color", "white").as_str() {
            "gray" => ([164, 170, 180], [40, 42, 48], [30, 30, 36]),
            "orange" => ([244, 170, 96], [70, 40, 20], [40, 26, 18]),
            "black" => ([48, 48, 56], [14, 14, 18], [250, 214, 80]),
            _ => ([248, 248, 250], [40, 40, 46], [30, 30, 36]),
        };
        let tr = Transform::from_row(p.k, 0.0, 0.0, p.k, p.ox, p.oy);
        let a = p.alpha;
        let paint = |c: [u8; 3], alpha: f32| {
            let mut pt = Paint::default();
            pt.set_color_rgba8(
                c[0],
                c[1],
                c[2],
                (255.0 * alpha * a).clamp(0.0, 255.0) as u8,
            );
            pt.anti_alias = true;
            pt
        };
        let stroke = |w: f32| SkStroke {
            width: w,
            line_cap: tiny_skia::LineCap::Round,
            line_join: tiny_skia::LineJoin::Round,
            ..Default::default()
        };
        let f = &mut *p.f;
        let oval = |f: &mut Pixmap,
                    cx: f32,
                    cy: f32,
                    rx: f32,
                    ry: f32,
                    fill: [u8; 3],
                    outline: Option<[u8; 3]>| {
            if let Some(path) = tiny_skia::Rect::from_xywh(cx - rx, cy - ry, rx * 2.0, ry * 2.0)
                .and_then(PathBuilder::from_oval)
            {
                f.fill_path(
                    &path,
                    &paint(fill, 1.0),
                    tiny_skia::FillRule::Winding,
                    tr,
                    None,
                );
                if let Some(o) = outline {
                    f.stroke_path(&path, &paint(o, 1.0), &stroke(4.0), tr, None);
                }
            }
        };
        let poly = |f: &mut Pixmap, pts: &[(f32, f32)], fill: [u8; 3], outline: Option<[u8; 3]>| {
            let mut pb = PathBuilder::new();
            pb.move_to(pts[0].0, pts[0].1);
            for q in &pts[1..] {
                pb.line_to(q.0, q.1);
            }
            pb.close();
            if let Some(path) = pb.finish() {
                f.fill_path(
                    &path,
                    &paint(fill, 1.0),
                    tiny_skia::FillRule::Winding,
                    tr,
                    None,
                );
                if let Some(o) = outline {
                    f.stroke_path(&path, &paint(o, 1.0), &stroke(4.0), tr, None);
                }
            }
        };
        let line_path = |f: &mut Pixmap, pts: &[(f32, f32)], c: [u8; 3], w: f32| {
            let mut pb = PathBuilder::new();
            pb.move_to(pts[0].0, pts[0].1);
            for q in &pts[1..] {
                pb.line_to(q.0, q.1);
            }
            if let Some(path) = pb.finish() {
                f.stroke_path(&path, &paint(c, 1.0), &stroke(w), tr, None);
            }
        };
        if opt("background", "none") == "dark" {
            if let Some(path) = tiny_skia::Rect::from_xywh(0.0, 0.0, 360.0, 240.0).and_then(|r| {
                let mut pb = PathBuilder::new();
                pb.push_rect(r);
                pb.finish()
            }) {
                f.fill_path(
                    &path,
                    &paint([20, 20, 24], 0.85),
                    tiny_skia::FillRule::Winding,
                    tr,
                    None,
                );
            }
        }
        let keys = self.map.keys.max(1);
        let pressed: Vec<bool> = (0..keys).map(|c| self.key_down(c, t).is_some()).collect();
        let (kx0, kx1) = (52.0f32, 308.0f32);
        let key_x = |c: usize| kx0 + (kx1 - kx0) * (c as f32 + 0.5) / keys as f32;
        let half = keys / 2;
        let any = pressed.iter().any(|d| *d);

        oval(f, 180.0, 186.0, 124.0, 52.0, fur, Some(line));
        for side in [-1.0f32, 1.0] {
            let ear = [
                (180.0 + side * 70.0, 92.0),
                (180.0 + side * 64.0, 30.0),
                (180.0 + side * 22.0, 66.0),
            ];
            poly(f, &ear, fur, Some(line));
            let inner = [
                (180.0 + side * 62.0, 84.0),
                (180.0 + side * 60.0, 46.0),
                (180.0 + side * 34.0, 68.0),
            ];
            poly(f, &inner, [246, 170, 186], None);
        }
        oval(f, 180.0, 104.0, 80.0, 58.0, fur, Some(line));
        if any {
            for side in [-1.0f32, 1.0] {
                let x = 180.0 + side * 28.0;
                line_path(
                    f,
                    &[
                        (x + side * 9.0, 96.0),
                        (x - side * 5.0, 103.0),
                        (x + side * 9.0, 110.0),
                    ],
                    eye,
                    4.0,
                );
            }
        } else {
            oval(f, 152.0, 103.0, 6.5, 8.0, eye, None);
            oval(f, 208.0, 103.0, 6.5, 8.0, eye, None);
        }
        oval(f, 136.0, 122.0, 10.0, 6.0, [250, 160, 176], None);
        oval(f, 224.0, 122.0, 10.0, 6.0, [250, 160, 176], None);
        line_path(
            f,
            &[
                (168.0, 120.0),
                (174.0, 127.0),
                (180.0, 121.0),
                (186.0, 127.0),
                (192.0, 120.0),
            ],
            line,
            3.0,
        );

        if let Some(path) = tiny_skia::Rect::from_xywh(0.0, 172.0, 360.0, 68.0).and_then(|r| {
            let mut pb = PathBuilder::new();
            pb.push_rect(r);
            pb.finish()
        }) {
            f.fill_path(
                &path,
                &paint([58, 62, 74], 1.0),
                tiny_skia::FillRule::Winding,
                tr,
                None,
            );
            line_path(f, &[(0.0, 172.0), (360.0, 172.0)], line, 4.0);
        }
        if opt("keys", "on") == "on" {
            let kw = (kx1 - kx0) / keys as f32;
            for c in 0..keys {
                let x = kx0 + kw * c as f32 + 3.0;
                let col = if pressed[c] {
                    [255, 236, 150]
                } else {
                    [96, 102, 118]
                };
                if let Some(path) =
                    tiny_skia::Rect::from_xywh(x, 190.0, kw - 6.0, 22.0).and_then(|r| {
                        let mut pb = PathBuilder::new();
                        pb.push_rect(r);
                        pb.finish()
                    })
                {
                    f.fill_path(
                        &path,
                        &paint(col, 1.0),
                        tiny_skia::FillRule::Winding,
                        tr,
                        None,
                    );
                    f.stroke_path(&path, &paint(line, 1.0), &stroke(2.5), tr, None);
                }
            }
        }

        for (side, cols) in [
            (-1.0f32, 0..half.max(1)),
            (1.0, half.max(1).min(keys)..keys),
        ] {
            let down: Vec<usize> = cols.clone().filter(|c| pressed[*c]).collect();
            let shoulder = (180.0 + side * 62.0, 150.0);
            let (px, py, ry) = if down.is_empty() {
                (180.0 + side * 74.0, 150.0, 20.0)
            } else {
                (
                    down.iter().map(|c| key_x(*c)).sum::<f32>() / down.len() as f32,
                    196.0,
                    15.0,
                )
            };
            line_path(f, &[shoulder, (px, py)], line, 38.0);
            line_path(f, &[shoulder, (px, py)], fur, 30.0);
            oval(f, px, py, 24.0, ry, fur, Some(line));
            for b in [-9.0f32, 0.0, 9.0] {
                oval(f, px + b, py - ry * 0.25, 3.5, 3.0, [246, 170, 186], None);
            }
        }
    }

    fn ov_osu_fps(&self, p: &mut P, cfg: &OverlayConfig, t: f64) {
        let tx = &self.text;
        let limit: f64 = cfg.options.get("limit").and_then(|v| v.parse().ok()).unwrap_or(240.0);
        let spike_rate = match cfg.options.get("spikes").map(String::as_str).unwrap_or("few") {
            "none" => 0.0,
            "many" => 1.0 / 400.0,
            _ => 1.0 / 1800.0,
        };
        let base = 1000.0 / limit.max(1.0);
        let hash = |k: i64, salt: u64| -> f64 {
            let mut x = (k as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ salt.wrapping_mul(0xD1B5_4A32_D192_ED03);
            x ^= x >> 31;
            x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
            x ^= x >> 29;
            (x >> 11) as f64 / (1u64 << 53) as f64
        };
        let frame_time = |k: i64| -> f64 {
            let jitter = (hash(k, 1) - 0.4) * 0.16 + (hash(k / 37, 2) - 0.5) * 0.10;
            let mut ft = base * (1.0 + jitter);
            if hash(k, 3) < spike_rate {
                ft = base * (1.8 + hash(k, 4) * 4.0);
            }
            ft.max(base * 0.85)
        };
        let now = ((t - self.start) / self.rate).max(0.0);
        let k_now = (now / base) as i64;
        let window = (3000.0 / base) as i64;
        let mut shown = base;
        let mut sum = 0.0;
        let mut count = 0.0;
        let mut spikes: Vec<(f64, f64)> = Vec::new();
        for k in (k_now - window).max(0)..=k_now {
            let ft = frame_time(k);
            shown += (ft - shown) * (1.0 - (-ft / 60.0f64).exp());
            let age = (k_now - k) as f64 * base;
            if age <= 500.0 {
                sum += ft;
                count += 1.0;
            }
            if ft > base * 1.7 {
                spikes.push((age, ft / base));
            }
        }
        let fps = if count > 0.0 { (1000.0 / (sum / count)).round().min(limit) } else { limit };
        let ratio = fps / limit;
        let fps_col = if ratio >= 0.95 {
            [155, 204, 17, 255]
        } else if ratio >= 0.7 {
            [255, 204, 34, 255]
        } else if ratio >= 0.4 {
            [255, 149, 24, 255]
        } else {
            [255, 70, 40, 255]
        };
        let rel = shown / base;
        let ms_col = if rel <= 1.4 {
            [164, 210, 26, 255]
        } else if rel <= 2.2 {
            [255, 204, 30, 255]
        } else if rel <= 4.0 {
            [255, 149, 24, 255]
        } else {
            [255, 70, 40, 255]
        };
        let ms_text = if shown < 10.0 { format!("{shown:.1}ms").replace('.', ",") } else { format!("{:.0}ms", shown) };
        let dark = [34, 34, 34, 255];
        let bx = 115.0;
        p.rect(bx, 4.0, 90.0, 28.0, 4.0, fps_col);
        let big = format!("{fps:.0}");
        let big_w = tx.measure(&big, 20.0 * p.k, false) / p.k;
        p.text(tx, &big, bx + 7.0, 4.0, 80.0, 28.0, 20.0, dark, Align::Left, false);
        p.text(tx, &format!("/{limit:.0}fps"), bx + 7.0 + big_w + 2.0, 10.0, 80.0, 24.0, 12.0, [60, 60, 60, 220], Align::Left, false);
        p.rect(bx, 37.0, 90.0, 29.0, 4.0, ms_col);
        p.text(tx, &ms_text, bx, 37.0, 90.0, 29.0, 20.0, dark, Align::Center, false);
        for (n, (age, sev)) in spikes.iter().rev().take(9).enumerate() {
            let a = (1.0 - age / 3000.0).clamp(0.0, 1.0) as f32;
            let c = if *sev > 4.0 {
                [255, 50, 30]
            } else if *sev > 2.5 {
                [255, 110, 20]
            } else {
                [255, 190, 40]
            };
            p.rect(bx - 14.0 - n as f32 * 13.0, 54.0, 10.0, 10.0, 2.0, [c[0], c[1], c[2], (255.0 * a) as u8]);
        }
    }

    fn ov_kps(&self, p: &mut P, cfg: &OverlayConfig, t: f64) {
        let jkps = cfg.options.get("style").map(String::as_str) != Some("panel");
        let space_colour = match cfg.options.get("space").map(String::as_str).unwrap_or("same") {
            "orange" => Some([255, 180, 0]),
            "blue" => Some([102, 204, 255]),
            "pink" => Some([255, 102, 170]),
            "green" => Some([136, 220, 60]),
            "yellow" => Some([255, 220, 40]),
            "purple" => Some([170, 136, 255]),
            "red" => Some([255, 80, 80]),
            _ => None,
        };
        let hud = &self.opts.hud;
        let tx = &self.text;
        let area = 304.0f32;
        let count = self.map.keys.max(1);
        let gap = if count > 6 { 4.0 } else { 8.0 };
        let key = ((area - gap * (count as f32 - 1.0)) / count as f32).floor();
        let radius = (key / 3.0).floor().min(12.0);
        let longest = hud
            .labels
            .iter()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(1)
            .max(1) as f32;
        let mut font = (key * 0.55).floor().min(24.0);
        if longest > 1.0 {
            font = font.min((key * 1.3 / longest).floor());
        }
        if !jkps {
            p.rect(0.0, 0.0, 475.0, 425.0, 24.0, hex("#141414"));
        }
        let key_top = 16.0 + 393.0 - key;
        let max_h = 393.0 - key;
        let speed = max_h / 700.0;
        for c in 0..count {
            let x = 16.0 + c as f32 * (key + gap);
            for &(down, up) in &self.intervals[c.min(self.intervals.len() - 1)] {
                if down > t {
                    break;
                }
                let top_age = ((t - down) / self.rate) as f32;
                let bottom_age = if up > t {
                    0.0
                } else {
                    ((t - up) / self.rate) as f32
                };
                if bottom_age > 700.0 {
                    continue;
                }
                let y_top = (key_top - 8.0 - top_age * speed).max(16.0);
                let y_bottom = key_top - 8.0 - bottom_age * speed;
                if y_bottom > y_top {
                    let is_space = hud.labels.get(c).map(|l| {
                        let l = l.to_ascii_uppercase();
                        l == "SPC" || l == "SPACE"
                    }).unwrap_or(false);
                    let colour = match (is_space, space_colour) {
                        (true, Some(sc)) => sc,
                        _ => [hud.bar_color[0], hud.bar_color[1], hud.bar_color[2]],
                    };
                    if jkps {
                        let base = key_top - 8.0;
                        let a_top = (1.0 - (base - y_top) / max_h).clamp(0.0, 1.0);
                        let a_bottom = (1.0 - (base - y_bottom) / max_h).clamp(0.0, 1.0);
                        p.vgradient(x, y_top, key, y_bottom - y_top, colour, a_top, a_bottom);
                    } else {
                        p.rect(x, y_top, key, y_bottom - y_top, 0.0, [colour[0], colour[1], colour[2], 217]);
                    }
                }
            }
            let col = c.min(self.map.keys - 1);
            let down = self.key_down(col, t).is_some();
            if jkps {
                p.rect(x, key_top, key, key, radius, [42, 42, 42, 255]);
                let glow = if down {
                    1.0
                } else {
                    self.last_release(col, t).map(|r| (1.0 - ((t - r) / self.rate) as f32 / 83.0).max(0.0)).unwrap_or(0.0)
                };
                if glow > 0.0 {
                    p.stroke(x + 1.5, key_top + 1.5, key - 3.0, key - 3.0, radius, 3.0, [255, 180, 0, (255.0 * glow) as u8]);
                }
            } else {
                p.rect(x, key_top, key, key, radius, hex("#1d1d1d"));
                if down {
                    p.rect(x, key_top, key, key, radius, [255, 255, 255, 71]);
                }
                p.stroke(x, key_top, key, key, radius, 2.0, hex("#e0a800"));
            }
            let name = hud.labels.get(c).cloned().unwrap_or_default();
            p.text(
                tx,
                &name,
                x,
                key_top,
                key,
                key,
                font,
                WHITE,
                Align::Center,
                false,
            );
        }
        let (kps, total) = self.sim.kps_at(t);
        let stat_x = 16.0 + area + 14.0;
        let row_top = 16.0 + 393.0 - 3.0 * 34.0;
        for (i, text) in [
            format!("KPS: {kps}"),
            format!("Total: {total}"),
            format!("BPM: {}", kps * 15),
        ]
        .iter()
        .enumerate()
        {
            p.text(
                tx,
                text,
                stat_x,
                row_top + i as f32 * 34.0,
                125.0,
                34.0,
                19.0,
                WHITE,
                Align::Left,
                true,
            );
        }
    }

    fn ov_panel(&self, p: &mut P, cfg: &OverlayConfig, t: f64, snap: &Snapshot) {
        let tx = &self.text;
        let opt = |k: &str| cfg.options.get(k).map(String::as_str).unwrap_or("");
        let bg = match opt("background") {
            "black" => [0, 0, 0, 204],
            "white" => [255, 255, 255, 31],
            _ => [26, 26, 26, 204],
        };
        let subtitle = if opt("subtitle") == "difficulty" {
            self.map.version.clone()
        } else {
            self.map.creator.clone()
        };
        let ur = if snap.counts.iter().sum::<u32>() == 0 {
            "--".to_string()
        } else {
            format!("{:.2}", snap.ur)
        };
        let flash =
            |p: &mut P, i: usize, x: f32, y: f32, w: f32, h: f32, size: f32, label: &str| {
                if let Some(at) = self.last_change(i, t) {
                    let age = ((t - at) / self.rate) as f32;
                    if age < 1600.0 {
                        let c = judgement_colour(i);
                        p.text(
                            tx,
                            label,
                            x,
                            y,
                            w,
                            h,
                            size,
                            [c[0], c[1], c[2], ((1.0 - age / 1600.0) * 255.0) as u8],
                            Align::Center,
                            true,
                        );
                    }
                }
            };
        let labels = ["320", "300", "200", "100", "50", "miss"];
        if opt("mode") == "simple" {
            p.rect(0.0, 0.0, 260.0, 260.0, 30.0, bg);
            p.text(
                tx,
                &format!("{} - {}", self.map.artist, self.map.title),
                30.0,
                12.0,
                200.0,
                36.0,
                17.0 * 1.333,
                WHITE,
                Align::Center,
                false,
            );
            p.rect(20.0, 58.0, 220.0, 3.0, 0.0, hex("#898989"));
            for i in 0..6 {
                let col = (i % 3) as f32;
                let row = (i / 3) as f32;
                let x = 22.0 + col * 74.0;
                let y = 72.0 + row * 64.0;
                p.text(
                    tx,
                    labels[i],
                    x,
                    y + 3.0,
                    74.0,
                    20.0,
                    11.0 * 1.333,
                    hex("#898989"),
                    Align::Center,
                    true,
                );
                flash(p, i, x, y + 3.0, 74.0, 20.0, 11.0 * 1.333, labels[i]);
                p.text(
                    tx,
                    &snap.counts[i].to_string(),
                    x,
                    y + 26.0,
                    74.0,
                    30.0,
                    17.0 * 1.333,
                    WHITE,
                    Align::Center,
                    true,
                );
            }
            p.text(
                tx,
                "UR",
                22.0,
                142.0 + 60.0,
                100.0,
                30.0,
                11.0 * 1.333,
                hex("#898989"),
                Align::Center,
                true,
            );
            p.text(
                tx,
                &ur,
                128.0,
                142.0 + 60.0,
                110.0,
                30.0,
                17.0 * 1.333,
                WHITE,
                Align::Center,
                true,
            );
            p.rect(20.0, 240.0, 220.0, 3.0, 0.0, hex("#898989"));
            return;
        }
        p.rect(0.0, 0.0, 500.0, 220.0, 30.0, bg);
        let lc = colour(opt("left_color"));
        let rc = colour(opt("right_color"));
        p.rect(35.0, 20.0, 230.0, 33.0, 16.5, lc);
        p.rect(330.0, 20.0, 135.0, 33.0, 16.5, rc);
        let (ltext, lval) = match opt("left") {
            "acc" => ("ACCURACY", format!("{:.2}%", snap.accuracy)),
            "combo" => ("MAX COMBO", format!("{}x", snap.max_combo)),
            _ => ("STAR RATE", format!("{:.2}", self.stars)),
        };
        let (rtext, rval) = match opt("right") {
            "pp" => ("PP", format!("{:.0}", self.pp_at(t, snap))),
            _ => ("UR", ur.clone()),
        };
        p.text(
            tx,
            ltext,
            52.0,
            20.0,
            120.0,
            33.0,
            13.0 * 1.333,
            hex("#141414"),
            Align::Left,
            true,
        );
        p.rect(170.0, 24.0, 90.0, 25.0, 12.5, [0, 0, 0, 153]);
        p.text(
            tx,
            &lval,
            170.0,
            24.0,
            90.0,
            25.0,
            15.0 * 1.333,
            WHITE,
            Align::Center,
            true,
        );
        p.text(
            tx,
            rtext,
            345.0,
            20.0,
            40.0,
            33.0,
            13.0 * 1.333,
            hex("#141414"),
            Align::Left,
            true,
        );
        p.rect(380.0, 24.0, 80.0, 25.0, 12.5, [0, 0, 0, 153]);
        p.text(
            tx,
            &rval,
            380.0,
            24.0,
            80.0,
            25.0,
            15.0 * 1.333,
            WHITE,
            Align::Center,
            true,
        );
        p.text(
            tx,
            &format!("{} - {}", self.map.artist, self.map.title),
            30.0,
            62.0,
            440.0,
            32.0,
            17.0 * 1.333,
            WHITE,
            Align::Center,
            false,
        );
        p.text(
            tx,
            &subtitle,
            30.0,
            96.0,
            440.0,
            28.0,
            14.0 * 1.333,
            WHITE,
            Align::Center,
            false,
        );
        p.rect(30.0, 133.0, 440.0, 4.0, 0.0, hex("#898989"));
        for i in 0..6 {
            let x = 30.0 + i as f32 * 74.0;
            p.text(
                tx,
                labels[i],
                x,
                148.0,
                74.0,
                20.0,
                12.0 * 1.333,
                hex("#898989"),
                Align::Center,
                true,
            );
            flash(p, i, x, 148.0, 74.0, 20.0, 12.0 * 1.333, labels[i]);
            p.text(
                tx,
                &snap.counts[i].to_string(),
                x,
                170.0,
                74.0,
                34.0,
                18.0 * 1.333,
                WHITE,
                Align::Center,
                true,
            );
        }
    }

    fn ov_hiterror_osu(&self, p: &mut P, t: f64) {
        let w = &self.sim.windows;
        let px = 1.125;
        let to_x = |ms: f64| (ms / self.rate) as f32 * px;
        let cx = 180.0;
        let bottom = 50.0;
        let bar_h = 6.0;
        let bar_y = bottom - 10.0 - bar_h;
        let zones = [
            (w.meh, [218, 174, 70, 255]),
            (w.ok, [218, 174, 70, 255]),
            (w.good, [87, 227, 19, 255]),
            (w.great, [50, 188, 231, 255]),
        ];
        let half = to_x(w.meh);
        p.rect(
            cx - half - 4.0,
            bar_y - 2.0,
            half * 2.0 + 8.0,
            bar_h + 4.0,
            0.0,
            [0, 0, 0, 128],
        );
        for (ms, color) in zones {
            let hw = to_x(ms);
            p.rect(cx - hw, bar_y, hw * 2.0, bar_h, 0.0, color);
        }
        let tick_h = 25.0;
        let start = self
            .sim
            .judgements
            .partition_point(|j| j.time < t - 3000.0 * self.rate);
        let end = self.sim.judgements.partition_point(|j| j.time <= t);
        let mut recent = Vec::new();
        for j in &self.sim.judgements[start..end] {
            let Some(offset) = j.offset else { continue };
            let age = ((t - j.time) / self.rate) as f32;
            let c = match j.kind {
                Judgement::Max | Judgement::Great => [50, 188, 231],
                Judgement::Good => [87, 227, 19],
                _ => [218, 174, 70],
            };
            let x = cx + to_x(offset.clamp(-w.meh, w.meh));
            p.rect(
                x - 2.0,
                bottom - tick_h,
                4.0,
                tick_h,
                0.0,
                [
                    c[0],
                    c[1],
                    c[2],
                    ((1.0 - age / 3000.0).clamp(0.0, 1.0) * 230.0) as u8,
                ],
            );
            recent.push(offset);
        }
        p.rect(
            cx - 2.0,
            bottom - tick_h - 4.0,
            4.0,
            tick_h + 4.0,
            0.0,
            WHITE,
        );
        if !recent.is_empty() {
            let tail = &recent[recent.len().saturating_sub(10)..];
            let avg = tail.iter().sum::<f64>() / tail.len() as f64;
            let ax = cx + to_x(avg.clamp(-w.meh, w.meh));
            p.rect(ax - 4.0, bottom - tick_h - 14.0, 8.0, 7.0, 2.0, WHITE);
        }
    }

    fn ov_hiterror_bar(&self, p: &mut P, t: f64, colorful: bool) {
        let tx = &self.text;
        let od = self.map.od;
        let full = (-10.0 * od + 199.5).max(20.0);
        p.rect(20.0, 40.0, 300.0, 3.0, 0.0, WHITE);
        p.text(
            tx,
            "EARLY",
            40.0,
            8.0,
            80.0,
            24.0,
            18.0,
            hex("#B3D7FF"),
            Align::Left,
            true,
        );
        p.text(
            tx,
            "LATE",
            220.0,
            8.0,
            80.0,
            24.0,
            18.0,
            hex("#FFB3B3"),
            Align::Right,
            true,
        );
        let start = self
            .sim
            .judgements
            .partition_point(|j| j.time < t - 1200.0 * self.rate);
        let end = self.sim.judgements.partition_point(|j| j.time <= t);
        let w = &self.sim.windows;
        for j in &self.sim.judgements[start..end] {
            let Some(off) = j.offset else { continue };
            let err = off / self.rate;
            let age = ((t - j.time) / self.rate) as f32;
            let x = 167.5 + (err / full * 145.0).clamp(-160.0, 160.0) as f32;
            let c = if colorful {
                let a = off.abs();
                if a <= 16.0 * self.rate {
                    [255, 255, 255, 255]
                } else if a <= w.great {
                    hex("#FFA500")
                } else if a <= w.good {
                    hex("#228B22")
                } else if a <= w.ok {
                    hex("#1E90FF")
                } else if a <= w.meh {
                    hex("#FF69B4")
                } else {
                    hex("#DC143C")
                }
            } else {
                WHITE
            };
            p.rect(
                x,
                31.5,
                5.0,
                20.0,
                0.0,
                [
                    c[0],
                    c[1],
                    c[2],
                    ((1.0 - age / 1200.0).clamp(0.0, 1.0) * 255.0) as u8,
                ],
            );
        }
        p.rect(167.5, 29.0, 5.0, 25.0, 0.0, [255, 40, 40, 255]);
        let done = &self.sim.judgements[..end];
        let mut avg = 0.0;
        for j in done
            .iter()
            .rev()
            .take(40)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            if let Some(o) = j.offset {
                avg = avg * 0.9 + (o / self.rate) * 0.1;
            }
        }
        let shift = (avg / full * 150.0) as f32;
        let ac = if shift > 2.5 {
            hex("#FF4040")
        } else if shift < -2.5 {
            hex("#1985FF")
        } else {
            WHITE
        };
        let ax = 170.0 + shift.clamp(-150.0, 150.0);
        let mut pb = tiny_skia::PathBuilder::new();
        pb.move_to(p.ox + (ax - 10.0) * p.k, p.oy + 12.0 * p.k);
        pb.line_to(p.ox + (ax + 10.0) * p.k, p.oy + 12.0 * p.k);
        pb.line_to(p.ox + ax * p.k, p.oy + 24.0 * p.k);
        pb.close();
        if let Some(path) = pb.finish() {
            let mut paint = tiny_skia::Paint::default();
            paint.set_color_rgba8(ac[0], ac[1], ac[2], (255.0 * p.alpha) as u8);
            paint.anti_alias = true;
            p.f.fill_path(
                &path,
                &paint,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }

    fn ov_hitcount_text(&self, p: &mut P, t: f64, snap: &Snapshot) {
        let tx = &self.text;
        let c = snap.counts;
        let others = c[1] + c[2] + c[3] + c[4] + c[5];
        let ratio = if others == 0 {
            if c[0] == 0 {
                "NaN:1".to_string()
            } else {
                "∞:1".to_string()
            }
        } else {
            format!("{:.1}:1", c[0] as f64 / others as f64)
        };
        let lines = [
            format!("{:.0}pp", self.pp_at(t, snap)),
            format!("UR : {:.2}", snap.ur),
            format!("Ratio : {ratio}"),
            String::new(),
            format!("MAX : {}", c[0]),
            format!("Perf : {}", c[1]),
            format!("Great : {}", c[2]),
            format!("Good : {}", c[3]),
            format!("Bad : {}", c[4]),
            format!("Miss : {}", c[5]),
        ];
        for (i, l) in lines.iter().enumerate() {
            p.shadow_text(
                tx,
                l,
                20.0,
                10.0 + i as f32 * 33.0,
                200.0,
                33.0,
                26.0,
                WHITE,
                Align::Left,
                false,
            );
        }
    }

    fn ov_hitcount_simple(&self, p: &mut P, snap: &Snapshot) {
        let tx = &self.text;
        p.rect(0.0, 0.0, 200.0, 270.0, 0.0, hex("#262626"));
        let labels = ["320", "300", "200", "100", "50", "0"];
        for i in 0..6 {
            let y = 10.0 + 4.0 + i as f32 * 35.0;
            p.shadow_text(
                tx,
                labels[i],
                20.0,
                y,
                80.0,
                30.0,
                26.0,
                judgement_colour(i),
                Align::Left,
                true,
            );
            p.text(
                tx,
                &snap.counts[i].to_string(),
                80.0,
                y,
                98.0,
                30.0,
                26.0,
                hex("#eaeaea"),
                Align::Right,
                true,
            );
        }
        let y = 10.0 + 4.0 + 6.0 * 35.0;
        p.shadow_text(
            tx,
            "UR",
            20.0,
            y,
            80.0,
            30.0,
            26.0,
            WHITE,
            Align::Left,
            false,
        );
        p.text(
            tx,
            &format!("{:.2}", snap.ur),
            80.0,
            y,
            98.0,
            30.0,
            26.0,
            hex("#eaeaea"),
            Align::Right,
            true,
        );
        let total: u32 = snap.counts.iter().sum();
        p.rect(0.0, 265.0, 200.0, 5.0, 0.0, [105, 105, 105, 255]);
        if total > 0 {
            let part = |n: u32| (n as f32 / total as f32 * 200.0).floor();
            let w300 = part(snap.counts[1]);
            let w200 = part(snap.counts[2]);
            let wbad = part(snap.counts[3] + snap.counts[4] + snap.counts[5]);
            let w320 = 200.0 - w300 - w200 - wbad;
            let mut x = 0.0;
            for (w, c) in [
                (w320, [240, 255, 255, 255]),
                (w300, [255, 165, 0, 255]),
                (w200, [34, 139, 34, 255]),
                (wbad, [220, 20, 60, 255]),
            ] {
                p.rect(x, 265.0, w, 5.0, 0.0, c);
                x += w;
            }
        }
    }

    fn ov_nowplaying(&self, p: &mut P) {
        let tx = &self.text;
        let card_w = 480.0;
        let card_h = 170.0;
        let k = p.k;
        let mut card = match Pixmap::new(
            (card_w * k).ceil() as u32 + 2,
            (card_h * k).ceil() as u32 + 2,
        ) {
            Some(c) => c,
            None => return,
        };
        {
            let mut q = P {
                f: &mut card,
                k,
                ox: 0.0,
                oy: 0.0,
                alpha: 1.0,
            };
            let sr = self.stars;
            let box_c = if sr < 2.0 {
                hex("#88B300")
            } else if sr < 2.5 {
                hex("#66CCFF")
            } else if sr < 4.0 {
                hex("#FFCC22")
            } else if sr < 5.3 {
                hex("#FF66AA")
            } else if sr < 6.5 {
                hex("#AA88FF")
            } else {
                hex("#787878")
            };
            q.rect(0.0, 0.0, card_w, card_h, 10.0, [box_c[0], box_c[1], box_c[2], 255]);
            q.rect(80.0, 0.0, 400.0, card_h, 0.0, [0, 0, 0, 128]);
            if let Some(cover) = &self.cover {
                q.image(cover, 30.0, 20.0, 170.0, 126.0, 1.0);
            } else {
                q.rect(30.0, 20.0, 170.0, 126.0, 10.0, [60, 60, 60, 255]);
            }
            q.rect(192.0, 20.0, 260.0, 48.0, 0.0, [0, 0, 0, 128]);
            q.text(
                tx,
                &self.map.title,
                212.0,
                22.0,
                240.0,
                44.0,
                26.0,
                WHITE,
                Align::Left,
                false,
            );
            q.shadow_text(
                tx,
                &self.map.artist,
                214.0,
                70.0,
                225.0,
                26.0,
                20.0,
                WHITE,
                Align::Left,
                false,
            );
            q.shadow_text(
                tx,
                &format!("by {}", self.map.creator),
                214.0,
                96.0,
                225.0,
                26.0,
                20.0,
                WHITE,
                Align::Left,
                false,
            );
            q.shadow_text(
                tx,
                &format!("{:.2} star", sr),
                300.0,
                120.0,
                170.0,
                26.0,
                20.0,
                WHITE,
                Align::Right,
                false,
            );
            q.rect(240.0, 153.0, 240.0, 7.0, 0.0, [0, 0, 0, 102]);
            let bar = (sr / 10.0).min(1.0) as f32 * 240.0;
            let bc = if sr < 6.5 {
                [box_c[0], box_c[1], box_c[2], 220]
            } else {
                hex("#999999")
            };
            q.rect(480.0 - bar, 153.0, bar, 7.0, 0.0, bc);
        }
        let paint = PixmapPaint {
            opacity: p.alpha,
            quality: FilterQuality::Bilinear,
            ..Default::default()
        };
        let skew = (8.0f32).to_radians().tan();
        let t = Transform::from_row(
            1.0,
            0.0,
            -skew,
            1.0,
            p.ox + 30.0 * k + skew * card_h * k,
            p.oy + 15.0 * k,
        );
        p.f.draw_pixmap(0, 0, card.as_ref(), &paint, t, None);
    }

    fn ov_disk(&self, p: &mut P, cfg: &OverlayConfig, t: f64, snap: &Snapshot) {
        let k = p.k;
        let (w, h) = ((430.0 * k).ceil() as u32, (550.0 * k).ceil() as u32);
        let Some(mut layer) = Pixmap::new(w.max(1), h.max(1)) else {
            return;
        };
        {
            let mut q = P {
                f: &mut layer,
                k,
                ox: 0.0,
                oy: 0.0,
                alpha: 1.0,
            };
            self.ov_disk_inner(&mut q, cfg, t, snap);
        }
        let mut mask = match tiny_skia::Mask::new(w.max(1), h.max(1)) {
            Some(m) => m,
            None => return,
        };
        let mut pb = tiny_skia::PathBuilder::new();
        let r = 30.0 * k;
        let (fw, fh) = (w as f32, h as f32);
        pb.push_rect(tiny_skia::Rect::from_xywh(r, 0.0, fw - 2.0 * r, fh).unwrap());
        pb.push_rect(tiny_skia::Rect::from_xywh(0.0, r, fw, fh - 2.0 * r).unwrap());
        for (cx, cy) in [(r, r), (fw - r, r), (r, fh - r), (fw - r, fh - r)] {
            pb.push_circle(cx, cy, r);
        }
        if let Some(path) = pb.finish() {
            mask.fill_path(
                &path,
                tiny_skia::FillRule::Winding,
                true,
                Transform::identity(),
            );
        }
        for (px, m) in layer.data_mut().chunks_exact_mut(4).zip(mask.data()) {
            if *m < 255 {
                for c in px.iter_mut() {
                    *c = (*c as u16 * *m as u16 / 255) as u8;
                }
            }
        }
        let anim = cfg.options.get("anim").map(String::as_str).unwrap_or("rs");
        let out_ms = ((t - self.start) / self.rate) as f32;
        let e = if self.no_fade { 1.0 } else { (out_ms / 500.0).clamp(0.0, 1.0) };
        let ease = 1.0 - (1.0 - e).powi(2);
        let (dx, dy, fade) = match anim {
            "ls" => (-500.0 * (1.0 - ease), 0.0, 1.0),
            "us" => (0.0, 700.0 * (1.0 - ease), 1.0),
            "bk" => (0.0, 0.0, ease),
            _ => (500.0 * (1.0 - ease), 0.0, 1.0),
        };
        let paint = PixmapPaint {
            opacity: p.alpha * fade,
            quality: FilterQuality::Nearest,
            ..Default::default()
        };
        p.f.draw_pixmap(
            (p.ox + dx * k).round() as i32,
            (p.oy + dy * k).round() as i32,
            layer.as_ref(),
            &paint,
            Transform::identity(),
            None,
        );
    }

    fn ov_disk_inner(&self, p: &mut P, cfg: &OverlayConfig, t: f64, snap: &Snapshot) {
        let tx = &self.text;
        p.rect(0.0, 0.0, 430.0, 550.0, 30.0, [0, 0, 0, 255]);
        if let Some(blur) = &self.cover_blur {
            p.image(blur, 0.0, 0.0, 430.0, 550.0, 1.0);
        }
        p.rect(0.0, 0.0, 430.0, 550.0, 30.0, [0, 0, 0, 90]);
        let out_ms = ((t - self.start) / self.rate) as f32;
        let (dcx, dcy, dr) = (408.0, 150.0, 160.0);
        p.rect(
            dcx - dr,
            dcy - dr,
            dr * 2.0,
            dr * 2.0,
            dr,
            [18, 18, 18, 255],
        );
        for ring in 1..7 {
            let r = dr - ring as f32 * 18.0;
            p.stroke(
                dcx - r,
                dcy - r,
                r * 2.0,
                r * 2.0,
                r,
                1.0,
                [60, 60, 60, 200],
            );
        }
        if let Some(cover) = &self.cover_round {
            let angle = (out_ms / 6000.0 * 360.0) % 360.0;
            let size = 170.0 * p.k;
            let paint = PixmapPaint {
                opacity: p.alpha,
                quality: FilterQuality::Bilinear,
                ..Default::default()
            };
            let s = size / cover.width() as f32;
            let tr = Transform::from_translate(p.ox + dcx * p.k, p.oy + dcy * p.k)
                .pre_rotate(angle)
                .pre_scale(s, s)
                .pre_translate(
                    -(cover.width() as f32) / 2.0,
                    -(cover.height() as f32) / 2.0,
                );
            p.f.draw_pixmap(0, 0, cover.as_ref(), &paint, tr, None);
        }
        p.rect(dcx - 6.0, dcy - 6.0, 12.0, 12.0, 6.0, [200, 200, 200, 255]);
        p.text(
            tx,
            &self.map.title,
            30.0,
            70.0,
            220.0,
            48.0,
            42.0,
            WHITE,
            Align::Left,
            false,
        );
        p.text(
            tx,
            &self.map.artist,
            30.0,
            140.0,
            180.0,
            30.0,
            26.0,
            [255, 255, 255, 178],
            Align::Left,
            false,
        );
        let (bx, by) = (45.0, 235.0);
        p.rect(bx, by + 4.0, 42.0, 42.0, 21.0, [255, 255, 255, 60]);
        p.rect(bx + 70.0, by - 18.0, 76.0, 76.0, 38.0, [255, 255, 255, 89]);
        p.rect(bx + 98.0, by + 6.0, 7.0, 28.0, 2.0, WHITE);
        p.rect(bx + 111.0, by + 6.0, 7.0, 28.0, 2.0, WHITE);
        p.rect(bx + 178.0, by + 4.0, 42.0, 42.0, 21.0, [255, 255, 255, 60]);
        let first = self.map.first_time() as f64;
        let last = self.map.last_time() as f64;
        let prog = ((t - first) / (last - first).max(1.0)).clamp(0.0, 1.0) as f32;
        p.rect(35.0, 330.0, 360.0, 8.0, 4.0, [255, 255, 255, 89]);
        p.rect(35.0, 330.0, 360.0 * prog, 8.0, 4.0, WHITE);
        p.rect(35.0 + 360.0 * prog - 12.5, 321.5, 25.0, 25.0, 12.5, WHITE);
        p.text(
            tx,
            &fmt_time((t - first).max(0.0) / self.rate),
            35.0,
            345.0,
            100.0,
            30.0,
            24.0,
            [255, 255, 255, 178],
            Align::Left,
            true,
        );
        p.text(
            tx,
            &fmt_time((last - first) / self.rate),
            295.0,
            345.0,
            100.0,
            30.0,
            24.0,
            [255, 255, 255, 178],
            Align::Right,
            true,
        );
        let phase = (((t - first).max(0.0) / self.rate / 15000.0) as u64) % 2;
        p.rect(35.0, 400.0, 360.0, 120.0, 20.0, [0, 0, 0, 153]);
        if phase == 0 {
            let (label, value) = if cfg.options.get("sub").map(String::as_str) == Some("ur") {
                ("UR", format!("{:.2}", snap.ur))
            } else {
                ("PP", format!("{:.1}", self.pp_at(t, snap)))
            };
            let remain = self.map.notes.len() as u32
                - snap
                    .counts
                    .iter()
                    .sum::<u32>()
                    .min(self.map.notes.len() as u32);
            p.text(
                tx,
                label,
                35.0,
                410.0,
                180.0,
                30.0,
                26.0,
                [255, 255, 255, 178],
                Align::Center,
                false,
            );
            p.text(
                tx,
                &value,
                35.0,
                448.0,
                180.0,
                50.0,
                31.0,
                WHITE,
                Align::Center,
                false,
            );
            p.text(
                tx,
                "Remain",
                215.0,
                410.0,
                180.0,
                30.0,
                26.0,
                [255, 255, 255, 178],
                Align::Center,
                false,
            );
            p.text(
                tx,
                &remain.to_string(),
                215.0,
                448.0,
                180.0,
                50.0,
                31.0,
                WHITE,
                Align::Center,
                false,
            );
        } else {
            let bpm = 60000.0 / self.map.dominant_beat_length() * self.rate;
            let items = [
                ("BPM", format!("{bpm:.0}"), [255, 0, 0, 255]),
                ("OD", format!("{:.1}", self.map.od), [251, 255, 0, 255]),
                ("HP", format!("{:.1}", self.map.hp), [21, 255, 0, 255]),
                ("SR", format!("{:.2}", self.stars), [0, 132, 255, 255]),
            ];
            for (i, (label, value, c)) in items.iter().enumerate() {
                let x = 35.0 + i as f32 * 90.0;
                p.text(
                    tx,
                    value,
                    x,
                    412.0,
                    90.0,
                    40.0,
                    33.0,
                    WHITE,
                    Align::Center,
                    false,
                );
                p.rect(x + 25.0, 460.0, 40.0, 4.0, 2.0, *c);
                p.text(
                    tx,
                    label,
                    x,
                    470.0,
                    90.0,
                    30.0,
                    24.0,
                    [255, 255, 255, 200],
                    Align::Center,
                    false,
                );
            }
        }
        if out_ms < 3500.0 {
            let amount = if out_ms < 2500.0 {
                0.0
            } else {
                ((out_ms - 2500.0) / 1000.0).min(1.0) as f32
            };
            let (slide, drop) = match cfg.options.get("anim").map(String::as_str).unwrap_or("rs") {
                "ls" => (-560.0 * amount, 0.0),
                "us" | "bk" => (0.0, 700.0 * amount),
                _ => (560.0 * amount, 0.0),
            };
            let saved = p.oy;
            p.oy += drop * p.k;
            p.rect(slide, 0.0, 430.0, 550.0, 30.0, [30, 30, 34, 255]);
            let gauge = (out_ms / 1500.0).min(1.0);
            p.stroke(
                slide + 65.0,
                260.0,
                300.0,
                21.0,
                10.5,
                3.0,
                [179, 179, 179, 255],
            );
            p.rect(
                slide + 68.0,
                263.0,
                294.0 * gauge,
                15.0,
                7.5,
                [179, 179, 179, 255],
            );
            let label = if gauge >= 1.0 {
                "Completed"
            } else {
                "Loading..."
            };
            p.text(
                tx,
                label,
                slide + 65.0,
                295.0,
                300.0,
                30.0,
                22.0,
                [200, 200, 200, 255],
                Align::Center,
                false,
            );
            p.oy = saved;
        }
    }

    fn ov_hitcount_bar(&self, p: &mut P, cfg: &OverlayConfig, t: f64, snap: &Snapshot) {
        let tx = &self.text;
        let colour_name = cfg
            .options
            .get("color")
            .map(String::as_str)
            .unwrap_or("blue");
        let colour_name = if colour_name == "random" {
            let seed = self.title.bytes().chain(self.map.version.bytes()).fold(self.mods as u64 ^ 0x9E37_79B9, |h, b| h.wrapping_mul(31).wrapping_add(b as u64)) ^ self.sim.final_snapshot().score as u64;
            ["red", "orange", "yellow", "green", "blue", "purple"][(seed % 6) as usize]
        } else {
            colour_name
        };
        let accent = pastel(colour_name);
        p.rect(0.0, 0.0, 910.0, 150.0, 24.0, [10, 10, 14, 200]);
        let (label, value) = if cfg.options.get("left").map(String::as_str) == Some("ur") {
            ("Unstable Rate", format!("{:.3}", snap.ur))
        } else {
            (
                "Performance",
                format!("{:.1}pp / {:.1}%", self.pp_at(t, snap), snap.accuracy),
            )
        };
        p.text(
            tx,
            label,
            30.0,
            25.0,
            240.0,
            30.0,
            22.0,
            [accent[0], accent[1], accent[2], 255],
            Align::Left,
            false,
        );
        p.text(
            tx,
            &value,
            30.0,
            65.0,
            240.0,
            50.0,
            30.0,
            WHITE,
            Align::Left,
            true,
        );
        p.rect(285.0, 25.0, 2.0, 100.0, 0.0, [255, 255, 255, 60]);
        let labels = ["320", "300", "200", "100", "50", "miss"];
        for i in 0..6 {
            let x = 300.0 + i as f32 * 100.0;
            p.text(
                tx,
                labels[i],
                x,
                25.0,
                100.0,
                30.0,
                22.0,
                [accent[0], accent[1], accent[2], 255],
                Align::Center,
                false,
            );
            p.text(
                tx,
                &snap.counts[i].to_string(),
                x,
                62.0,
                100.0,
                44.0,
                32.0,
                WHITE,
                Align::Center,
                true,
            );
            let mut a = 60.0f32;
            if let Some(at) = self.last_change(i, t) {
                let age = ((t - at) / self.rate) as f32;
                if age < 1000.0 {
                    a = 60.0 + 195.0 * (1.0 - age / 1000.0);
                }
            }
            p.rect(
                x + 25.0,
                115.0,
                50.0,
                4.0,
                2.0,
                [accent[0], accent[1], accent[2], a as u8],
            );
        }
    }

    fn ov_osukps(&self, p: &mut P, cfg: &OverlayConfig, t: f64) {
        let tx = &self.text;
        let opt = |k: &str| cfg.options.get(k).map(String::as_str).unwrap_or("");
        let idle = match opt("idle") {
            "pink" => [255, 102, 170, 255],
            "purple" => [140, 80, 230, 255],
            "green" => [40, 170, 80, 255],
            "gray" => [70, 70, 80, 255],
            _ => [0, 0, 255, 255],
        };
        let active = match opt("active") {
            "yellow" => [255, 230, 60, 255],
            "cyan" => [80, 230, 255, 255],
            _ => [255, 255, 255, 255],
        };
        let count = self.map.keys.max(1);
        if opt("background") != "none" {
            p.rect(0.0, 0.0, 480.0, 72.0, 0.0, [0, 0, 0, 255]);
        }
        let slot = (320.0 / count as f32).min(80.0);
        let size = slot * 0.9;
        for c in 0..count {
            let x = c as f32 * slot;
            let mut blend = 0.0f32;
            if self.key_down(c.min(self.map.keys - 1), t).is_some() {
                blend = 1.0;
            } else if let Some(up) = self.last_release(c.min(self.map.keys - 1), t) {
                let age = ((t - up) / self.rate) as f32;
                blend = (1.0 - age / 250.0).max(0.0);
            }
            let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * blend) as u8;
            let col = [
                mix(idle[0], active[0]),
                mix(idle[1], active[1]),
                mix(idle[2], active[2]),
                255,
            ];
            p.rect(x, (72.0 - size) / 2.0, size, size, 0.0, col);
            if opt("labels") != "off" {
                let label = self.opts.hud.labels.get(c).cloned().unwrap_or_default();
                let lum = col[0] as u32 * 3 + col[1] as u32 * 6 + col[2] as u32;
                let tc = if lum > 1500 { [0, 0, 0, 255] } else { WHITE };
                p.text(
                    tx,
                    &label,
                    x,
                    (72.0 - size) / 2.0,
                    size,
                    size,
                    size * 0.42,
                    tc,
                    Align::Center,
                    true,
                );
            }
        }
        let (kps, total) = self.sim.kps_at(t);
        let end = self.sim.presses.partition_point(|&x| x <= t);
        let mut max = 0usize;
        let mut lo = 0usize;
        for hi in 0..end {
            while self.sim.presses[hi] - self.sim.presses[lo] > 1000.0 {
                lo += 1;
            }
            max = max.max(hi - lo + 1);
        }
        let (text, colour) = if kps == 0 {
            (format!("{max} Max"), WHITE)
        } else if kps >= 10 {
            (format!("{kps} Kps"), [248, 0, 0, 255])
        } else if kps >= 5 {
            (format!("{kps} Kps"), [0, 190, 255, 255])
        } else {
            (format!("{kps} Kps"), WHITE)
        };
        p.text(
            tx,
            &text,
            330.0,
            4.0,
            145.0,
            34.0,
            26.0,
            colour,
            Align::Left,
            true,
        );
        p.text(
            tx,
            &total.to_string(),
            330.0,
            36.0,
            145.0,
            32.0,
            24.0,
            WHITE,
            Align::Left,
            false,
        );
    }

    fn ov_nps(&self, p: &mut P, cfg: &OverlayConfig, t: f64) {
        if cfg.options.get("style").map(String::as_str) != Some("bars") {
            self.ov_nps_line(p, t);
            return;
        }
        let tx = &self.text;
        p.rect(0.0, 0.0, 400.0, 230.0, 20.0, [10, 10, 14, 200]);
        let bins = &self.nps;
        if bins.is_empty() {
            return;
        }
        let max = bins.iter().copied().max().unwrap_or(1).max(1) as f32;
        let first = self.map.first_time() as f64;
        let cur_bin = (((t - first) / 1000.0).max(0.0)) as usize;
        let (gx, gy, gw, gh) = (20.0, 50.0, 360.0, 150.0);
        let bw = gw / bins.len() as f32;
        for (i, &n) in bins.iter().enumerate() {
            let h = n as f32 / max * gh;
            let c = if i <= cur_bin {
                [162, 213, 255, 230]
            } else {
                [255, 255, 255, 60]
            };
            p.rect(gx + i as f32 * bw, gy + gh - h, bw.max(1.0), h, 0.0, c);
        }
        let now = bins.get(cur_bin.min(bins.len() - 1)).copied().unwrap_or(0);
        p.text(
            tx,
            &format!("NPS {now}"),
            20.0,
            12.0,
            180.0,
            30.0,
            22.0,
            WHITE,
            Align::Left,
            true,
        );
        p.text(
            tx,
            &format!("max {}", max as u32),
            200.0,
            12.0,
            180.0,
            30.0,
            20.0,
            [255, 255, 255, 170],
            Align::Right,
            false,
        );
    }
}
