use super::shapes::{fill_rect, fill_round};
use super::text::Align;
use super::{blit, draw_number, Scene};
use crate::game::Snapshot;
use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};

pub const RESULTS_MS: f64 = 8000.0;
const FADE_MS: f64 = 600.0;
const HEAD: [(f32, f32); 3] = [(4.3, 29.0), (32.5, 23.5), (57.0, 21.3)];
const PANEL_Y: f32 = 103.0;
const SCORE_X: f32 = 252.0;
const COUNT_SCALE: f32 = 1.125;
const SCORE_SCALE: f32 = 1.42;
const GRADE_SCALE: f32 = 0.985;
const COLS: [(f32, f32); 2] = [(64.0, 129.5), (383.0, 448.5)];
const VALUES: (f32, f32, f32) = (24.7, 311.0, 450.4);

pub fn grade(snap: &Snapshot, mods: u32) -> &'static str {
    let silver = mods & ((1 << 3) | (1 << 10) | (1 << 20)) != 0;
    let judged: u32 = snap.counts.iter().sum();
    let acc = snap.accuracy;
    let g = if judged == 0 || acc >= 100.0 {
        "X"
    } else if acc > 95.0 {
        "S"
    } else if acc > 90.0 {
        "A"
    } else if acc > 80.0 {
        "B"
    } else if acc > 70.0 {
        "C"
    } else {
        "D"
    };
    match (g, silver) {
        ("X", true) => "XH",
        ("S", true) => "SH",
        _ => g,
    }
}

fn draw_scaled(f: &mut Pixmap, img: &Pixmap, x: f32, y: f32, k: f32, opacity: f32) {
    let paint = PixmapPaint {
        opacity: opacity.clamp(0.0, 1.0),
        quality: FilterQuality::Bilinear,
        ..Default::default()
    };
    f.draw_pixmap(
        0,
        0,
        img.as_ref(),
        &paint,
        Transform::from_row(k, 0.0, 0.0, k, x, y),
        None,
    );
}

impl Scene {
    pub(super) fn draw_results(&self, f: &mut Pixmap, t: f64) {
        let Some(play_end) = self.play_end else {
            return;
        };
        if t <= play_end {
            return;
        }
        let age = (t - play_end) / self.rate;
        let alpha = (age / FADE_MS).clamp(0.0, 1.0) as f32;
        let (w, h) = (self.opts.width as f32, self.opts.height as f32);
        let u = h / 768.0;
        let mut screen = Pixmap::new(self.opts.width, self.opts.height).unwrap();
        screen.fill(tiny_skia::Color::BLACK);
        if let Some(bg) = &self.results_bg {
            screen.data_mut().copy_from_slice(bg.data());
            fill_rect(&mut screen, 0.0, 0.0, w, h, [0, 0, 0, 255], 0.45);
        }
        let mut snap = self.sim.final_snapshot();
        if let Some(r) = &self.opts.replay_stats {
            snap.counts = r.counts;
            snap.score = r.score as f64;
            snap.max_combo = r.max_combo;
            let total: u32 = r.counts.iter().sum();
            let points = 300 * (r.counts[0] + r.counts[1]) + 200 * r.counts[2] + 100 * r.counts[3] + 50 * r.counts[4];
            snap.accuracy = if total > 0 { points as f64 / (300.0 * total as f64) * 100.0 } else { 100.0 };
        }
        let rk = &self.skin.ranking;
        let tx = &self.text;
        let white = [255, 255, 255, 255];

        if let Some(title) = &rk.title {
            blit(&mut screen, title, w - title.width() as f32 - 1.5 * u, -20.6 * u, 1.0);
        }
        match &rk.panel {
            Some(panel) => blit(&mut screen, panel, 0.0, PANEL_Y * u, 1.0),
            None => fill_round(
                &mut screen,
                10.0 * u,
                (PANEL_Y + 20.0) * u,
                580.0 * u,
                470.0 * u,
                18.0 * u,
                [20, 20, 28, 255],
                0.85,
            ),
        }
        fill_rect(&mut screen, 0.0, 0.0, w, 90.0 * u, [0, 0, 0, 255], 0.6);
        let song = format!("{} [{}]", self.title, self.map.version);
        tx.draw(
            &mut screen,
            &song,
            6.0 * u,
            HEAD[0].0 * u,
            HEAD[0].1 * u,
            white,
            Align::Left,
            false,
        );
        tx.draw(
            &mut screen,
            &format!("Beatmap by {}", self.map.creator),
            6.0 * u,
            HEAD[1].0 * u,
            HEAD[1].1 * u,
            white,
            Align::Left,
            false,
        );
        let played = if self.opts.played_at.is_empty() {
            format!("Played by {}", self.opts.player)
        } else {
            format!("Played by {} on {}", self.opts.player, self.opts.played_at)
        };
        tx.draw(
            &mut screen,
            &played,
            6.0 * u,
            HEAD[2].0 * u,
            HEAD[2].1 * u,
            white,
            Align::Left,
            false,
        );

        let digits = self.skin.score_digits.as_ref();
        let score_h = digits
            .map(|d| d.glyphs[0].height() as f32)
            .unwrap_or(26.0 * u)
            * SCORE_SCALE;
        draw_number(
            &mut screen,
            tx,
            digits,
            &format!("{:07}", snap.score.round() as u64),
            SCORE_X * u,
            (PANEL_Y + 47.4) * u - score_h / 2.0,
            SCORE_SCALE,
            Align::Left,
            26.0 * u,
        );
        let rows = [(1usize, 0usize), (2, 3), (4, 5)];
        for (r, (a, b)) in rows.iter().enumerate() {
            let cy = (PANEL_Y + 152.4 + 96.0 * r as f32) * u;
            for (k, (x, nx)) in [(*a, COLS[0]), (*b, COLS[1])] {
                if let Some(img) = self.skin.judgements.get(k).and_then(|v| v.first()) {
                    let fit = (200.0 * u / img.width() as f32).min(0.50);
                    let (iw, ih) = (img.width() as f32 * fit, img.height() as f32 * fit);
                    draw_scaled(&mut screen, img, x * u - iw / 2.0, cy - ih / 2.0, fit, 1.0);
                }
                let gh = digits
                    .map(|d| d.glyphs[0].height() as f32)
                    .unwrap_or(24.0 * u)
                    * COUNT_SCALE;
                draw_number(
                    &mut screen,
                    tx,
                    digits,
                    &format!("{}x", snap.counts[k]),
                    nx * u,
                    cy - gh / 2.0,
                    COUNT_SCALE,
                    Align::Left,
                    24.0 * u,
                );
            }
        }
        let label_cy = (PANEL_Y + 431.0) * u;
        match &rk.maxcombo {
            Some(img) => blit(
                &mut screen,
                img,
                8.0 * u,
                label_cy - img.height() as f32 / 2.0,
                1.0,
            ),
            None => tx.draw(
                &mut screen,
                "Max Combo",
                20.0 * u,
                label_cy - 10.0 * u,
                18.0 * u,
                white,
                Align::Left,
                true,
            ),
        }
        match &rk.accuracy {
            Some(img) => blit(
                &mut screen,
                img,
                291.0 * u,
                label_cy - img.height() as f32 / 2.0,
                1.0,
            ),
            None => tx.draw(
                &mut screen,
                "Accuracy",
                300.0 * u,
                label_cy - 10.0 * u,
                18.0 * u,
                white,
                Align::Left,
                true,
            ),
        }
        let value_h = digits.map(|d| d.glyphs[0].height() as f32).unwrap_or(24.0 * u) * COUNT_SCALE;
        let value_y = (PANEL_Y + VALUES.2) * u - value_h / 2.0;
        draw_number(
            &mut screen,
            tx,
            digits,
            &format!("{}x", snap.max_combo),
            VALUES.0 * u,
            value_y,
            COUNT_SCALE,
            Align::Left,
            24.0 * u,
        );
        let acc = format!("{:.2}%", snap.accuracy);
        draw_number(
            &mut screen,
            tx,
            digits,
            &acc,
            VALUES.1 * u,
            value_y,
            COUNT_SCALE,
            Align::Left,
            24.0 * u,
        );

        let g = grade(&snap, self.mods);
        let cx = w - 189.7 * u;
        let cy = (PANEL_Y + 218.7) * u;
        let pop = ((age - 300.0) / 400.0).clamp(0.0, 1.0) as f32;
        if pop > 0.0 {
            let k = GRADE_SCALE * (1.0 + 0.4 * (1.0 - pop).powi(3));
            match rk.grades.iter().find(|(n, _)| *n == g).map(|(_, p)| p) {
                Some(img) => draw_scaled(
                    &mut screen,
                    img,
                    cx - img.width() as f32 * k / 2.0,
                    cy - img.height() as f32 * k / 2.0,
                    k,
                    pop,
                ),
                None => {
                    let label = match g {
                        "X" | "XH" => "SS",
                        "SH" => "S",
                        other => other,
                    };
                    let col = if g.ends_with('H') {
                        [220, 225, 235, 255]
                    } else {
                        [255, 204, 34, 255]
                    };
                    tx.draw(
                        &mut screen,
                        label,
                        cx,
                        cy - 120.0 * u * k,
                        240.0 * u * k,
                        [col[0], col[1], col[2], (255.0 * pop) as u8],
                        Align::Center,
                        true,
                    );
                }
            }
        }
        let mut mx = w - 61.2 * u;
        let mcy = (PANEL_Y + 309.0) * u;
        let mut first = true;
        for (short, icon) in self.mod_icons.iter().rev() {
            match icon {
                Some(p) => {
                    let (iw, ih) = (p.width() as f32, p.height() as f32);
                    if !first {
                        mx -= iw * 0.39;
                    }
                    first = false;
                    draw_scaled(&mut screen, p, mx - iw / 2.0, mcy - ih / 2.0, 1.0, 1.0);
                }
                None => {
                    let size = 64.0 * u;
                    let my = mcy - size / 2.0;
                    fill_round(
                        &mut screen,
                        mx - size / 2.0,
                        my,
                        size,
                        size,
                        8.0 * u,
                        [40, 40, 50, 255],
                        0.85,
                    );
                    tx.draw(
                        &mut screen,
                        short,
                        mx,
                        my + 18.0 * u,
                        26.0 * u,
                        white,
                        Align::Center,
                        true,
                    );
                    mx -= size * 1.1;
                }
            }
        }
        draw_scaled(f, &screen, 0.0, 0.0, 1.0, alpha);
    }
}
