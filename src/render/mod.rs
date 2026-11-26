pub mod assets;
pub mod fastblit;
mod overlays;
mod results;
pub mod shapes;
pub mod skin;
pub mod text;

use crate::game::{Judgement, Simulation};
use crate::osu::beatmap::Beatmap;
use assets::{blit, blit_sub, load_pixmap, scaled};
use shapes::{fill_rect, fill_round, stroke_round};
use skin::{Digits, SkinSet};
use text::{Align, TextRenderer};
use tiny_skia::{BlendMode, FilterQuality, Pixmap, PixmapPaint, Transform};

#[derive(Clone)]
pub struct HudOptions {
    pub kps: bool,
    pub panel: bool,
    pub labels: Vec<String>,
    pub bar_color: [u8; 4],
    pub score: bool,
}

#[derive(Clone)]
pub struct SceneOptions {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub scroll: f64,
    pub dim: f32,
    pub bpm_scale: bool,
    pub decimal_comma: bool,
    pub storyboard: bool,
    pub video: bool,
    pub hud: HudOptions,
    pub overlays: Vec<crate::overlays::catalog::OverlayConfig>,
    pub results: bool,
    pub player: String,
    pub played_at: String,
    pub replay_stats: Option<ReplayStats>,
    pub map_hs_volume: f32,
    pub skin_hs_volume: f32,
}

#[derive(Clone, Copy)]
pub struct ReplayStats {
    pub counts: [u32; 6],
    pub score: u64,
    pub max_combo: u32,
}

struct Segment {
    time: f64,
    dist: f64,
    mult: f64,
}

pub struct Scene {
    pub map: Beatmap,
    pub sim: Simulation,
    pub rate: f64,
    pub stars: f64,
    pub opts: SceneOptions,
    pub start: f64,
    pub end: f64,
    pub total_frames: u64,
    pub autoplay: bool,
    pub no_fade: bool,
    skin: SkinSet,
    text: TextRenderer,
    base: Pixmap,
    bg_raw: Option<Pixmap>,
    storyboard: Option<crate::storyboard::Storyboard>,
    sb_images: crate::storyboard::ImageCache,
    sb_tinted: std::sync::RwLock<
        std::collections::HashMap<(String, [u8; 3]), std::sync::Arc<fastblit::Mips>>,
    >,
    segments: Vec<Segment>,
    note_d: Vec<f64>,
    end_d: Vec<f64>,
    prefix_end: Vec<f64>,
    intervals: Vec<Vec<(f64, f64)>>,
    hits: Vec<Vec<f64>>,
    holds: Vec<Vec<(f64, f64)>>,
    title: String,
    mod_icons: Vec<(String, Option<Pixmap>)>,
    pp_curve: Vec<(f64, f64)>,
    mods: u32,
    nps: Vec<u32>,
    cover: Option<Pixmap>,
    cover_round: Option<Pixmap>,
    cover_blur: Option<Pixmap>,
    play_end: Option<f64>,
    results_bg: Option<Pixmap>,
}

const UNITS_PER_MS: f64 = 402.0 / 11485.0;

impl Scene {
    pub fn new(
        map: Beatmap,
        sim: Simulation,
        mods: u32,
        skin_files: &crate::osu::skin::SkinFiles,
        opts: SceneOptions,
        autoplay: bool,
    ) -> Scene {
        let rate = crate::osu::replay::rate_for_mods(mods);
        let stars = crate::game::stars::star_rating(&map, rate);
        let skin = skin::build(skin_files, map.keys, opts.width, opts.height);
        let mod_icons = skin::mod_icons(
            skin_files,
            if autoplay { 0 } else { mods },
            autoplay,
            opts.height,
        );
        let base = build_background(&map, &opts);
        let storyboard = if opts.storyboard {
            crate::storyboard::load(&map.path, &map.folder, map.background.as_deref())
        } else {
            None
        };
        let mut sb_images: crate::storyboard::ImageCache = std::collections::HashMap::new();
        if let Some(sb) = &storyboard {
            for o in &sb.objects {
                for p in o.all_paths() {
                    let key = p.to_lowercase();
                    if !sb_images.contains_key(&key) {
                        let path = crate::osu::beatmap::find_case_insensitive(&map.folder, &p);
                        sb_images.insert(
                            key,
                            load_pixmap(&path).map(|p| std::sync::Arc::new(fastblit::Mips::new(p))),
                        );
                    }
                }
            }
        }
        let bg_raw = if storyboard.is_some() || opts.video {
            build_background_raw(&map, &opts)
        } else {
            None
        };
        let mut pp_curve = Vec::new();
        if opts.overlays.iter().any(|o| {
            o.enabled
                && matches!(
                    o.id.as_str(),
                    "panel" | "hitcount_text" | "nowplaying_disk" | "hitcount_bar"
                )
        }) {
            let mut t = map.first_time() as f64;
            let last = map.last_time() as f64;
            while t <= last + 1000.0 {
                pp_curve.push((t, crate::game::stars::star_rating_until(&map, rate, t)));
                t += 1000.0;
            }
        }
        let first = map.first_time();
        let bins = ((map.last_time() - first) / 1000 + 1).max(1) as usize;
        let mut nps = vec![0u32; bins];
        for n in &map.notes {
            let b = ((n.time - first) / 1000).max(0) as usize;
            if b < bins {
                nps[b] += 1;
            }
        }
        let (cover, cover_round, cover_blur) = build_covers(&map);
        let segments = build_segments(&map, opts.scroll, rate, opts.bpm_scale);
        let dist = |t: f64| distance(&segments, t);
        let note_d: Vec<f64> = map.notes.iter().map(|n| dist(n.time as f64)).collect();
        let end_d: Vec<f64> = map.notes.iter().map(|n| dist(n.end() as f64)).collect();
        let mut prefix_end = Vec::with_capacity(end_d.len());
        let mut m = f64::MIN;
        for v in &end_d {
            m = m.max(*v);
            prefix_end.push(m);
        }
        let mut intervals = vec![Vec::new(); map.keys];
        let mut open: Vec<Option<f64>> = vec![None; map.keys];
        for e in &sim.key_events {
            if e.column >= map.keys {
                continue;
            }
            if e.down {
                open[e.column].get_or_insert(e.time as f64);
            } else if let Some(d) = open[e.column].take() {
                intervals[e.column].push((d, e.time as f64));
            }
        }
        for (c, o) in open.into_iter().enumerate() {
            if let Some(d) = o {
                intervals[c].push((d, f64::MAX));
            }
        }
        let mut hits = vec![Vec::new(); map.keys];
        for j in &sim.judgements {
            if j.kind != Judgement::Miss {
                let note = &map.notes[j.note];
                let t = sim.notes[j.note].head_hit.unwrap_or(j.time);
                hits[note.column].push(t);
            }
        }
        for h in &mut hits {
            h.sort_by(|a, b| a.partial_cmp(b).unwrap());
        }
        let mut holds = vec![Vec::new(); map.keys];
        for (i, n) in map.notes.iter().enumerate() {
            let st = &sim.notes[i];
            if let (true, Some(a), Some(b), false) = (n.is_long(), st.head_hit, st.done, st.missed)
            {
                holds[n.column].push((
                    a,
                    if st.broken {
                        st.released.unwrap_or(b)
                    } else {
                        b
                    },
                ));
            }
        }
        let lead = 2000.0 * rate;
        let start = map.first_time() as f64 - lead;
        let last_judge = sim.judgements.last().map(|j| j.time).unwrap_or(0.0);
        let end = (map.last_time() as f64).max(last_judge) + 2500.0 * rate;
        let play_end = opts.results.then_some(end);
        let end = if opts.results {
            end + results::RESULTS_MS * rate
        } else {
            end
        };
        let results_bg = if opts.results {
            build_background_raw(&map, &opts)
        } else {
            None
        };
        let frame_ms = 1000.0 / opts.fps as f64;
        let total_frames = (((end - start) / rate) / frame_ms).ceil() as u64;
        let title = if map.title.is_empty() {
            map.version.clone()
        } else {
            map.display_title()
        };
        Scene {
            map,
            sim,
            rate,
            stars,
            opts,
            start,
            end,
            total_frames,
            autoplay,
            no_fade: false,
            skin,
            text: TextRenderer::new(),
            base,
            segments,
            note_d,
            end_d,
            prefix_end,
            intervals,
            hits,
            holds,
            title,
            mod_icons,
            pp_curve,
            mods,
            nps,
            cover,
            cover_round,
            cover_blur,
            bg_raw,
            storyboard,
            sb_images,
            sb_tinted: Default::default(),
            play_end,
            results_bg,
        }
    }

    pub fn clip(&mut self, from: f64, to: f64) {
        self.start = from.max(self.start);
        self.end = to.min(self.end).max(self.start + 1.0);
        self.total_frames =
            (((self.end - self.start) / self.rate) / (1000.0 / self.opts.fps as f64)).ceil() as u64;
    }

    pub fn scroll_distance(&self, t: f64) -> f64 {
        distance(&self.segments, t) * self.skin.scale as f64
    }

    pub fn time_of_frame(&self, frame: f64) -> f64 {
        self.start + frame * (1000.0 / self.opts.fps as f64) * self.rate
    }

    pub fn output_seconds(&self) -> f64 {
        self.total_frames as f64 / self.opts.fps as f64
    }

    fn key_down(&self, column: usize, t: f64) -> Option<(f64, f64)> {
        let list = &self.intervals[column];
        let i = list.partition_point(|iv| iv.0 <= t);
        if i == 0 {
            return None;
        }
        let iv = list[i - 1];
        if t < iv.1 {
            Some(iv)
        } else {
            None
        }
    }

    fn last_release(&self, column: usize, t: f64) -> Option<f64> {
        let list = &self.intervals[column];
        let i = list.partition_point(|iv| iv.0 <= t);
        if i == 0 {
            return None;
        }
        let iv = list[i - 1];
        if t >= iv.1 {
            Some(iv.1)
        } else {
            None
        }
    }

    pub fn has_layers(&self) -> bool {
        (self.storyboard.is_some() || self.opts.video) && self.opts.dim < 0.999
    }

    pub fn video_file(&self) -> Option<(std::path::PathBuf, f64)> {
        if !self.opts.video || self.opts.dim >= 1.0 {
            return None;
        }
        let (offset, file) = self.map.video.clone()?;
        let path = crate::osu::beatmap::find_case_insensitive(&self.map.folder, &file);
        path.is_file().then_some((path, offset as f64))
    }

    pub fn storyboard_samples(&self) -> Vec<crate::storyboard::Sample> {
        self.storyboard
            .as_ref()
            .map(|s| s.samples.clone())
            .unwrap_or_default()
    }

    pub fn render(&self, t: f64) -> Pixmap {
        self.render_with_video(t, None)
    }

    pub fn render_with_video(&self, t: f64, video: Option<&[u8]>) -> Pixmap {
        self.render_full(t, video, &self.opts.overlays)
    }

    pub fn render_full(
        &self,
        t: f64,
        video: Option<&[u8]>,
        overlays: &[crate::overlays::catalog::OverlayConfig],
    ) -> Pixmap {
        let mut f = if !self.has_layers() {
            self.base.clone()
        } else {
            let mut f = Pixmap::new(self.opts.width, self.opts.height).unwrap();
            f.fill(tiny_skia::Color::BLACK);
            match video {
                Some(v) if v.len() == f.data().len() => f.data_mut().copy_from_slice(v),
                _ => {
                    let hide = self
                        .storyboard
                        .as_ref()
                        .map(|s| s.uses_background)
                        .unwrap_or(false);
                    if let (Some(bg), false) = (&self.bg_raw, hide) {
                        f.data_mut().copy_from_slice(bg.data());
                    }
                }
            }
            for layer in [
                crate::storyboard::Layer::Background,
                crate::storyboard::Layer::Pass,
                crate::storyboard::Layer::Foreground,
            ] {
                self.draw_storyboard_layer(&mut f, t, layer, 1.0);
            }
            if self.opts.dim > 0.0 {
                fill_rect(
                    &mut f,
                    0.0,
                    0.0,
                    self.opts.width as f32,
                    self.opts.height as f32,
                    [0, 0, 0, 255],
                    self.opts.dim,
                );
            }
            f
        };
        let sk = &self.skin;
        let s = sk.scale;
        let h = self.opts.height as f32;
        let now_d = distance(&self.segments, t);
        let y_of = |d: f64| sk.hit_y - ((d - now_d) * s as f64) as f32;
        let stage_w = sk.stage_x1 - sk.stage_x0;
        let keys = self.map.keys;

        for c in 0..keys {
            let col = sk.column_colours[c];
            fill_rect(
                &mut f,
                sk.columns_x[c],
                0.0,
                sk.widths[c],
                h,
                [col[0], col[1], col[2], 255],
                col[3] as f32 / 255.0,
            );
        }
        if let Some(hint) = &sk.stage_hint {
            blit(
                &mut f,
                hint,
                sk.stage_x0,
                sk.hit_y - hint.height() as f32 / 2.0,
                1.0,
            );
        }
        for (i, w) in sk.column_lines.iter().enumerate() {
            if *w <= 0.0 {
                continue;
            }
            let x = if i < keys {
                sk.columns_x[i]
            } else {
                sk.stage_x1
            };
            fill_rect(
                &mut f,
                x - w / 2.0,
                0.0,
                *w,
                sk.hit_y,
                sk.column_line_colour,
                sk.column_line_colour[3] as f32 / 255.0,
            );
        }
        if sk.keys_under_notes {
            self.draw_keys(&mut f, t);
        }
        for c in 0..keys {
            let alpha = if self.key_down(c, t).is_some() {
                1.0
            } else if let Some(r) = self.last_release(c, t) {
                1.0 - ((t - r) / self.rate / 120.0) as f32
            } else {
                0.0
            };
            if alpha > 0.0 {
                if let Some(light) = &sk.cols[c].light {
                    blit(
                        &mut f,
                        light,
                        sk.columns_x[c],
                        sk.light_y - light.height() as f32,
                        alpha,
                    );
                }
            }
        }

        let visible_up = (sk.hit_y / s) as f64 + 40.0;
        let visible_down = ((h - sk.hit_y) / s) as f64 + 40.0;
        let low = now_d - visible_down;
        let high = now_d + visible_up;
        let first = self.prefix_end.partition_point(|&e| e < low);
        let last = self.note_d.partition_point(|&d| d <= high);
        for i in first..last.min(self.map.notes.len()) {
            if self.end_d[i] < low {
                continue;
            }
            self.draw_note(&mut f, i, t, &y_of);
        }

        self.draw_lighting(&mut f, t);
        if !sk.keys_under_notes {
            self.draw_keys(&mut f, t);
        }
        if let Some(left) = &sk.stage_left {
            blit(&mut f, left, sk.stage_x0 - left.width() as f32, 0.0, 1.0);
        }
        if let Some(right) = &sk.stage_right {
            blit(&mut f, right, sk.stage_x1, 0.0, 1.0);
        }
        if let Some(bottom) = &sk.stage_bottom {
            blit(
                &mut f,
                bottom,
                (sk.stage_x0 + sk.stage_x1 - bottom.width() as f32) * 0.5,
                h - bottom.height() as f32,
                1.0,
            );
        }
        if let Some(c) = sk.judgement_line {
            fill_rect(&mut f, sk.stage_x0, sk.hit_y - s * 0.5, stage_w, s, c, 0.9);
        }

        let snap = self.sim.snapshot_at(t);
        self.draw_judgement(&mut f, t);
        if snap.combo > 0 {
            let idx = self.sim.snapshots.partition_point(|x| x.time <= t);
            let bump = if idx > 0 {
                ((t - self.sim.snapshots[idx - 1].time) / self.rate) as f32
            } else {
                1000.0
            };
            let p = (bump / 300.0).min(1.0);
            let scale = 1.0 + 0.4 * (1.0 - p).powi(3);
            let cx = sk.stage_x0 + stage_w / 2.0;
            let glyph_h = sk
                .combo_digits
                .as_ref()
                .map(|d| d.glyphs[0].height() as f32)
                .unwrap_or(26.0 * s);
            let top = sk.combo_y - glyph_h * 0.5 * scale;
            draw_number(
                &mut f,
                &self.text,
                sk.combo_digits.as_ref(),
                &snap.combo.to_string(),
                cx,
                top,
                scale,
                Align::Center,
                26.0 * s,
            );
        }
        let w = self.opts.width as f32;
        if self.opts.hud.score {
            let score_text = format!("{:08}", snap.score.round() as u64);
            draw_number(
                &mut f,
                &self.text,
                sk.score_digits.as_ref(),
                &score_text,
                w - 7.0 * s,
                0.0,
                0.66,
                Align::Right,
                17.0 * s,
            );
            let mut acc_text = format!("{:.2}%", snap.accuracy);
            if self.opts.decimal_comma {
                acc_text = acc_text.replace('.', ",");
            }
            let acc_right = w - 14.0 * s;
            let acc_top = 23.6 * s;
            draw_number(
                &mut f,
                &self.text,
                sk.score_digits.as_ref(),
                &acc_text,
                acc_right,
                acc_top,
                0.38,
                Align::Right,
                10.0 * s,
            );
            let acc_w = number_width(
                &self.text,
                sk.score_digits.as_ref(),
                &acc_text,
                0.38,
                10.0 * s,
            );
            let acc_h = sk
                .score_digits
                .as_ref()
                .map(|d| d.glyphs[0].height() as f32 * 0.38)
                .unwrap_or(10.0 * s);
            let first = self.map.first_time() as f64;
            let last = self.map.last_time() as f64;
            let progress = ((t - first) / (last - first).max(1.0)).clamp(0.0, 1.0) as f32;
            draw_pie(
                &mut f,
                acc_right - acc_w - 9.0 * s - 10.0 * s,
                acc_top + acc_h / 2.0,
                10.0 * s,
                progress,
                t >= first,
                s,
            );
        }
        self.draw_health(&mut f, snap.hp as f32, t);

        self.draw_storyboard_layer(
            &mut f,
            t,
            crate::storyboard::Layer::Overlay,
            1.0 - self.opts.dim,
        );
        self.draw_overlays(&mut f, t, &snap, overlays);
        let out_ms = (t - self.start) / self.rate;
        let total_ms = (self.end - self.start) / self.rate;
        self.draw_results(&mut f, t);
        let fade = (out_ms / 500.0)
            .min((total_ms - out_ms) / 700.0)
            .clamp(0.0, 1.0) as f32;
        if fade < 1.0 && !self.no_fade {
            fill_rect(&mut f, 0.0, 0.0, w, h, [0, 0, 0, 255], 1.0 - fade);
        }
        f
    }

    fn draw_keys(&self, f: &mut Pixmap, t: f64) {
        let sk = &self.skin;
        let h = self.opts.height as f32;
        for c in 0..self.map.keys {
            let down = self.key_down(c, t).is_some()
                || self
                    .last_release(c, t)
                    .map(|r| (t - r) / self.rate < 80.0)
                    .unwrap_or(false);
            let cs = &sk.cols[c];
            let img = if down {
                cs.key_down.as_ref().or(cs.key.as_ref())
            } else {
                cs.key.as_ref()
            };
            match img {
                Some(p) => blit(f, p, sk.columns_x[c], h - p.height() as f32, 1.0),
                None => {
                    let size = sk.widths[c] * 0.86;
                    let x = sk.columns_x[c] + (sk.widths[c] - size) / 2.0;
                    let y = sk.hit_y + (h - sk.hit_y - size) / 2.0;
                    let col = sk.default_note_colours[c];
                    if down {
                        fill_round(f, x, y, size, size, size / 2.0, col, 0.85);
                    }
                    stroke_round(
                        f,
                        x,
                        y,
                        size,
                        size,
                        size / 2.0,
                        (size * 0.08).max(2.0),
                        [150, 150, 160, 255],
                        1.0,
                    );
                }
            }
        }
    }

    fn draw_note(&self, f: &mut Pixmap, i: usize, t: f64, y_of: &dyn Fn(f64) -> f32) {
        let sk = &self.skin;
        let note = &self.map.notes[i];
        let st = &self.sim.notes[i];
        let c = note.column;
        let cs = &sk.cols[c];
        let x = sk.columns_x[c];
        let w = sk.widths[c];
        if !note.is_long() {
            if let Some(hit) = st.head_hit {
                if t >= hit && !st.missed {
                    return;
                }
            }
            let y = y_of(self.note_d[i]);
            match &cs.note {
                Some(p) => blit_sub(f, p, x, y - p.height() as f32, 1.0),
                None => default_note(f, x, y, w, sk.default_note_colours[c], 1.0, sk.scale),
            }
            return;
        }
        let holding = st.head_hit.map(|hh| t >= hh).unwrap_or(false)
            && !st.missed
            && st.done.map(|d| t < d).unwrap_or(true);
        if let Some(done) = st.done {
            if t >= done && !st.missed && !st.broken {
                return;
            }
        }
        let dimmed = (st.missed && st.done.map(|d| t >= d).unwrap_or(true)) || (st.broken && st.released.map(|r| t >= r).unwrap_or(false));
        let alpha = if dimmed { 0.45 } else { 1.0 };
        let head_h = cs
            .head
            .as_ref()
            .map(|p| p.height() as f32)
            .unwrap_or(w * 0.35);
        let mut head_y = y_of(self.note_d[i]);
        if holding {
            head_y = head_y.min(sk.hit_y);
        }
        let tail_y = y_of(self.end_d[i]);
        let body_top = tail_y - head_h / 2.0;
        let body_bottom = head_y - head_h / 2.0;
        if body_bottom > body_top {
            match &cs.body {
                Some(tile) => draw_body(f, tile, cs.body_style, x, body_top, body_bottom, alpha),
                None => fill_rect(
                    f,
                    x + w * 0.12,
                    body_top,
                    w * 0.76,
                    body_bottom - body_top,
                    sk.default_note_colours[c],
                    0.55 * alpha,
                ),
            }
        }
        if let Some(tail) = &cs.tail {
            blit_sub(f, tail, x, tail_y - tail.height() as f32, alpha);
        }
        match &cs.head {
            Some(p) => blit_sub(f, p, x, head_y - p.height() as f32, alpha),
            None => default_note(f, x, head_y, w, sk.default_note_colours[c], alpha, sk.scale),
        }
    }

    fn draw_lighting(&self, f: &mut Pixmap, t: f64) {
        let sk = &self.skin;
        let paint = |opacity: f32| PixmapPaint {
            opacity: opacity.clamp(0.0, 1.0),
            blend_mode: BlendMode::Plus,
            quality: FilterQuality::Bilinear,
        };
        for c in 0..self.map.keys {
            let cs = &sk.cols[c];
            let cx = sk.columns_x[c] + sk.widths[c] / 2.0;
            let held = self.holds[c].iter().any(|&(a, b)| t >= a && t < b);
            if held && !cs.lighting_l.is_empty() {
                let age = (t / self.rate) as f32;
                let frame_ms = (1000.0f32 / 60.0).max(170.0 / cs.lighting_l.len() as f32);
                let frame = &cs.lighting_l[((age / frame_ms) as usize) % cs.lighting_l.len()];
                f.draw_pixmap(
                    (cx - frame.width() as f32 / 2.0) as i32,
                    (sk.hit_y - frame.height() as f32 / 2.0) as i32,
                    frame.as_ref(),
                    &paint(1.0),
                    Transform::identity(),
                    None,
                );
                continue;
            }
            let hits = &self.hits[c];
            let idx = hits.partition_point(|&h| h <= t);
            if idx == 0 || cs.lighting_n.is_empty() {
                continue;
            }
            let age = ((t - hits[idx - 1]) / self.rate) as f32;
            let n = cs.lighting_n.len();
            if age > 200.0 {
                continue;
            }
            let frame_ms = (1000.0f32 / 60.0).max(170.0 / n as f32);
            let k = ((age / frame_ms) as usize).min(n - 1);
            let opacity = if age < 80.0 {
                age / 80.0
            } else {
                1.0 - (age - 80.0) / 120.0
            };
            let (frame, opacity, scale) = (&cs.lighting_n[k], opacity, 1.0f32);
            let fw = frame.width() as f32 * scale;
            let fh = frame.height() as f32 * scale;
            f.draw_pixmap(
                0,
                0,
                frame.as_ref(),
                &paint(opacity),
                Transform::from_row(scale, 0.0, 0.0, scale, cx - fw / 2.0, sk.hit_y - fh / 2.0),
                None,
            );
        }
    }

    fn draw_judgement(&self, f: &mut Pixmap, t: f64) {
        let Some(j) = self.sim.last_judgement_before(t) else {
            return;
        };
        let age = ((t - j.time) / self.rate) as f32;
        if age > 220.0 {
            return;
        }
        let sk = &self.skin;
        let cx = sk.stage_x0 + (sk.stage_x1 - sk.stage_x0) / 2.0;
        let frames = &sk.judgements[j.kind.index()];
        if frames.is_empty() {
            let (label, color) = match j.kind {
                Judgement::Max => ("MAX", [255, 255, 255, 255]),
                Judgement::Great => ("300", [255, 210, 80, 255]),
                Judgement::Good => ("200", [120, 230, 120, 255]),
                Judgement::Ok => ("100", [90, 170, 255, 255]),
                Judgement::Meh => ("50", [170, 170, 170, 255]),
                Judgement::Miss => ("MISS", [255, 80, 80, 255]),
            };
            let opacity = judge_opacity(age);
            let size = 30.0 * sk.scale;
            self.text.draw(
                f,
                label,
                cx,
                sk.judge_y - size / 2.0,
                size,
                [color[0], color[1], color[2], (255.0 * opacity) as u8],
                Align::Center,
                true,
            );
            return;
        }
        let k = ((age / 50.0) as usize).min(frames.len() - 1);
        let img = &frames[k];
        let miss = j.kind == Judgement::Miss;
        let scale = if frames.len() > 1 {
            1.0
        } else if miss {
            let p = (age / 100.0).min(1.0);
            1.2 - 0.2 * (1.0 - (1.0 - p).powi(2))
        } else if age < 40.0 {
            0.8 + 0.2 * (age / 40.0)
        } else if age < 80.0 {
            0.85 - 0.15 * ((age - 40.0) / 40.0)
        } else if age < 180.0 {
            0.7
        } else {
            0.7 - 0.3 * ((age - 180.0) / 40.0).min(1.0).powi(2)
        };
        let angle = if miss {
            let seed = ((j.time as i64).wrapping_mul(2654435761) as u64 >> 8) % 1000;
            let target = (seed as f32 / 1000.0 * 2.0 - 1.0) * 5.73f32.to_radians();
            target * (1.0 - (1.0 - (age / 100.0).min(1.0)).powi(2))
        } else {
            0.0
        };
        let w = img.width() as f32;
        let hgt = img.height() as f32;
        let paint = PixmapPaint {
            opacity: judge_opacity(age),
            quality: FilterQuality::Bilinear,
            ..Default::default()
        };
        let transform = Transform::from_translate(cx, sk.judge_y)
            .pre_rotate(angle.to_degrees())
            .pre_scale(scale, scale)
            .pre_translate(-w / 2.0, -hgt / 2.0);
        f.draw_pixmap(0, 0, img.as_ref(), &paint, transform, None);
    }

    fn storyboard_image(
        &self,
        path: &str,
        colour: [f64; 3],
    ) -> Option<std::sync::Arc<fastblit::Mips>> {
        let base = self.sb_images.get(&path.to_lowercase())?.clone()?;
        let rgb = [
            (colour[0] * 255.0).round() as u8,
            (colour[1] * 255.0).round() as u8,
            (colour[2] * 255.0).round() as u8,
        ];
        if rgb == [255, 255, 255] {
            return Some(base);
        }
        let q = [rgb[0] & 0xf8, rgb[1] & 0xf8, rgb[2] & 0xf8];
        let key = (path.to_lowercase(), q);
        if let Some(p) = self.sb_tinted.read().unwrap().get(&key) {
            return Some(p.clone());
        }
        let tinted = std::sync::Arc::new(fastblit::Mips::new(assets::tinted(
            base.base(),
            [q[0], q[1], q[2], 255],
        )));
        let mut cache = self.sb_tinted.write().unwrap();
        if cache.len() > 512 {
            cache.clear();
        }
        cache.insert(key, tinted.clone());
        Some(tinted)
    }

    fn draw_storyboard_layer(
        &self,
        f: &mut Pixmap,
        t: f64,
        layer: crate::storyboard::Layer,
        fade: f32,
    ) {
        let Some(sb) = &self.storyboard else { return };
        if fade <= 0.001 || self.opts.dim >= 0.999 {
            return;
        }
        let k = self.opts.height as f32 / 480.0;
        let cx = self.opts.width as f32 / 2.0;
        let (sw, sh) = (self.opts.width as f32, self.opts.height as f32);
        let mut items = Vec::new();
        for o in sb.objects.iter().filter(|o| o.layer == layer) {
            let Some(st) = o.state(t) else { continue };
            if st.sx == 0.0 || st.sy == 0.0 {
                continue;
            }
            let Some(img) = self.storyboard_image(&o.frame_path(t), st.colour) else {
                continue;
            };
            let (fx, fy) = o.origin.factors();
            let (w, h) = (img.base().width() as f32, img.base().height() as f32);
            let px = (st.x as f32 - 320.0) * k + cx;
            let py = st.y as f32 * k;
            let sx = st.sx as f32 * k * if st.flip_h { -1.0 } else { 1.0 };
            let sy = st.sy as f32 * k * if st.flip_v { -1.0 } else { 1.0 };
            let opacity = st.opacity as f32 * fade;
            let x0 = px - fx * w * sx;
            let y0 = py - fy * h * sy;
            let (lx, hx) = (x0.min(x0 + w * sx), x0.max(x0 + w * sx));
            let (ly, hy) = (y0.min(y0 + h * sy), y0.max(y0 + h * sy));
            if hx <= 0.0 || hy <= 0.0 || lx >= sw || ly >= sh {
                continue;
            }
            let covers = img.opaque
                && !st.additive
                && opacity >= 0.999
                && st.rotation.abs() < 1e-4
                && lx <= 0.0
                && ly <= 0.0
                && hx >= sw
                && hy >= sh;
            items.push((
                img,
                px,
                py,
                fx * w,
                fy * h,
                sx,
                sy,
                st.rotation as f32,
                opacity,
                st.additive,
                covers,
            ));
        }
        let first = items.iter().rposition(|it| it.10).unwrap_or(0);
        for (img, px, py, ox, oy, sx, sy, rot, op, add, _) in items.into_iter().skip(first) {
            fastblit::draw(f, &img, px, py, ox, oy, sx, sy, rot, op, add);
        }
    }

    fn draw_health(&self, f: &mut Pixmap, hp: f32, t: f64) {
        let sk = &self.skin;
        let s = sk.scale;
        let h = self.opts.height as f32;
        let ox = sk.stage_x1 + 13.0 * s;
        let oy = h + 2.7 * s;
        if let Some(bg) = &sk.scorebar_bg {
            blit(f, bg, ox, oy - bg.height() as f32, 1.0);
        }
        if sk.scorebar_colour.is_empty() {
            if sk.scorebar_bg.is_none() {
                let len = 190.0 * s;
                fill_rect(
                    f,
                    ox,
                    h - 6.0 * s - len,
                    5.0 * s,
                    len,
                    [40, 40, 40, 255],
                    0.8,
                );
                fill_rect(
                    f,
                    ox,
                    h - 6.0 * s - len * hp,
                    5.0 * s,
                    len * hp,
                    [235, 70, 90, 255],
                    1.0,
                );
            }
            return;
        }
        let n = sk.scorebar_colour.len();
        let frame = &sk.scorebar_colour[((t / self.rate / 16.667).max(0.0) as usize) % n];
        let fh = frame.height() as f32;
        let visible = (fh * hp).round().max(0.0) as u32;
        if visible == 0 {
            return;
        }
        let cut = frame.height() - visible.min(frame.height());
        if let Some(rect) =
            tiny_skia::IntRect::from_xywh(0, cut as i32, frame.width(), visible.min(frame.height()))
        {
            if let Some(part) = frame.clone_rect(rect) {
                blit(f, &part, ox, oy - fh + cut as f32, 1.0);
            }
        }
    }
}

fn judge_opacity(age: f32) -> f32 {
    if age < 20.0 {
        age / 20.0
    } else if age < 180.0 {
        1.0
    } else {
        (1.0 - (age - 180.0) / 40.0).clamp(0.0, 1.0)
    }
}

fn default_note(f: &mut Pixmap, x: f32, y_bottom: f32, w: f32, color: [u8; 4], alpha: f32, s: f32) {
    let h = (w * 0.32).max(6.0 * s);
    fill_round(
        f,
        x + w * 0.04,
        y_bottom - h,
        w * 0.92,
        h,
        h * 0.35,
        color,
        alpha,
    );
}

fn draw_body(f: &mut Pixmap, tile: &Pixmap, style: u8, x: f32, top: f32, bottom: f32, alpha: f32) {
    let len = bottom - top;
    let th = tile.height() as f32;
    let paint = PixmapPaint {
        opacity: alpha,
        quality: FilterQuality::Bilinear,
        ..Default::default()
    };
    if style == 0 {
        f.draw_pixmap(
            0,
            0,
            tile.as_ref(),
            &paint,
            Transform::from_row(1.0, 0.0, 0.0, len / th, x, top),
            None,
        );
        return;
    }
    let screen_h = f.height() as f32;
    let mut drawn = 0.0f32;
    while drawn < len {
        let piece = (len - drawn).min(th);
        let y = if style == 2 {
            bottom - drawn - piece
        } else {
            top + drawn
        };
        if y + piece >= 0.0 && y <= screen_h {
            let src_y = if style == 2 { th - piece } else { 0.0 };
            if piece >= th - 0.5 {
                blit_sub(f, tile, x, y, alpha);
            } else if let Some(rect) = tiny_skia::IntRect::from_xywh(
                0,
                src_y.round() as i32,
                tile.width(),
                piece.round().max(1.0) as u32,
            ) {
                if let Some(part) = tile.clone_rect(rect) {
                    blit_sub(f, &part, x, y, alpha);
                }
            }
        }
        drawn += th;
    }
}

fn number_width(
    text: &TextRenderer,
    digits: Option<&Digits>,
    value: &str,
    scale: f32,
    fallback_size: f32,
) -> f32 {
    let Some(d) = digits else {
        return text.measure(value, fallback_size * scale, true);
    };
    let glyph = |c: char| -> Option<&Pixmap> {
        match c {
            '0'..='9' => d.glyphs.get(c as usize - '0' as usize),
            '.' => d.dot.as_ref(),
            ',' => d.comma.as_ref(),
            '%' => d.percent.as_ref(),
            'x' => d.x.as_ref(),
            _ => None,
        }
    };
    value
        .chars()
        .filter_map(glyph)
        .map(|g| g.width() as f32 * scale - d.overlap * scale)
        .sum::<f32>()
        + d.overlap * scale
}

fn draw_pie(f: &mut Pixmap, cx: f32, cy: f32, r: f32, progress: f32, started: bool, s: f32) {
    let mut paint = tiny_skia::Paint::default();
    paint.anti_alias = true;
    if started && progress > 0.0 {
        let mut pb = tiny_skia::PathBuilder::new();
        pb.move_to(cx, cy);
        let steps = (progress * 64.0).ceil().max(1.0) as i32;
        for i in 0..=steps {
            let a = -std::f32::consts::FRAC_PI_2
                + std::f32::consts::TAU * progress * i as f32 / steps as f32;
            pb.line_to(cx + r * a.cos(), cy + r * a.sin());
        }
        pb.close();
        if let Some(path) = pb.finish() {
            paint.set_color_rgba8(200, 200, 200, 190);
            f.fill_path(
                &path,
                &paint,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }
    if let Some(circle) = tiny_skia::PathBuilder::from_circle(cx, cy, r) {
        paint.set_color_rgba8(255, 255, 255, 255);
        let stroke = tiny_skia::Stroke {
            width: 1.4 * s,
            ..Default::default()
        };
        f.stroke_path(&circle, &paint, &stroke, Transform::identity(), None);
    }
    if let Some(dot) = tiny_skia::PathBuilder::from_circle(cx, cy, 1.0 * s) {
        paint.set_color_rgba8(255, 255, 255, 255);
        f.fill_path(
            &dot,
            &paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

fn draw_number(
    f: &mut Pixmap,
    text: &TextRenderer,
    digits: Option<&Digits>,
    value: &str,
    x: f32,
    y: f32,
    scale: f32,
    align: Align,
    fallback_size: f32,
) {
    let Some(d) = digits else {
        text.draw(
            f,
            value,
            x,
            y,
            fallback_size * scale,
            [255, 255, 255, 255],
            align,
            true,
        );
        return;
    };
    let glyph = |c: char| -> Option<&Pixmap> {
        match c {
            '0'..='9' => d.glyphs.get(c as usize - '0' as usize),
            '.' => d.dot.as_ref(),
            ',' => d.comma.as_ref(),
            '%' => d.percent.as_ref(),
            'x' => d.x.as_ref(),
            _ => None,
        }
    };
    let width: f32 = value
        .chars()
        .filter_map(glyph)
        .map(|g| g.width() as f32 * scale - d.overlap * scale)
        .sum::<f32>()
        + d.overlap * scale;
    let mut pen = match align {
        Align::Left => x,
        Align::Center => x - width / 2.0,
        Align::Right => x - width,
    };
    let base_h = d.glyphs[0].height() as f32 * scale;
    for c in value.chars() {
        if let Some(g) = glyph(c) {
            let gh = g.height() as f32 * scale;
            let paint = PixmapPaint {
                quality: FilterQuality::Bilinear,
                ..Default::default()
            };
            f.draw_pixmap(
                0,
                0,
                g.as_ref(),
                &paint,
                Transform::from_row(scale, 0.0, 0.0, scale, pen, y + (base_h - gh)),
                None,
            );
            pen += g.width() as f32 * scale - d.overlap * scale;
        }
    }
}

fn build_segments(map: &Beatmap, scroll: f64, rate: f64, bpm_scale: bool) -> Vec<Segment> {
    let base = UNITS_PER_MS * scroll / rate;
    let dominant = map.dominant_beat_length();
    let mut red = map
        .timing
        .iter()
        .find(|p| p.uninherited)
        .map(|p| p.beat_length.max(1.0))
        .unwrap_or(dominant);
    let mut segments: Vec<Segment> = Vec::new();
    for p in &map.timing {
        let sv = if p.uninherited {
            red = p.beat_length.max(1.0);
            1.0
        } else if p.beat_length < 0.0 {
            (-100.0 / p.beat_length).clamp(0.01, 10.0)
        } else {
            1.0
        };
        let mult = sv * if bpm_scale { dominant / red } else { 1.0 } * base;
        match segments.last_mut() {
            Some(last) if (p.time - last.time).abs() < 1e-9 => last.mult = mult,
            Some(last) => {
                let dist = last.dist + (p.time - last.time) * last.mult;
                segments.push(Segment {
                    time: p.time,
                    dist,
                    mult,
                });
            }
            None => segments.push(Segment {
                time: p.time,
                dist: 0.0,
                mult,
            }),
        }
    }
    if segments.is_empty() {
        segments.push(Segment {
            time: 0.0,
            dist: 0.0,
            mult: base,
        });
    }
    segments
}

fn distance(segments: &[Segment], t: f64) -> f64 {
    let i = segments.partition_point(|s| s.time <= t);
    let seg = if i == 0 {
        &segments[0]
    } else {
        &segments[i - 1]
    };
    seg.dist + (t - seg.time) * seg.mult
}

fn build_background(map: &Beatmap, opts: &SceneOptions) -> Pixmap {
    let (w, h) = (opts.width, opts.height);
    let mut base = Pixmap::new(w, h).unwrap();
    base.fill(tiny_skia::Color::BLACK);
    if opts.dim < 1.0 {
        if let Some(img) = map.background_path().and_then(|p| load_pixmap(&p)) {
            let k = (w as f32 / img.width() as f32).max(h as f32 / img.height() as f32);
            if let Some(sc) = scaled(&img, img.width() as f32 * k, img.height() as f32 * k) {
                blit(
                    &mut base,
                    &sc,
                    (w as f32 - sc.width() as f32) / 2.0,
                    (h as f32 - sc.height() as f32) / 2.0,
                    1.0,
                );
            }
            fill_rect(
                &mut base,
                0.0,
                0.0,
                w as f32,
                h as f32,
                [0, 0, 0, 255],
                opts.dim,
            );
        }
    }
    base
}

fn build_background_raw(map: &Beatmap, opts: &SceneOptions) -> Option<Pixmap> {
    let (w, h) = (opts.width, opts.height);
    let img = map.background_path().and_then(|p| load_pixmap(&p))?;
    let k = (w as f32 / img.width() as f32).max(h as f32 / img.height() as f32);
    let sc = scaled(&img, img.width() as f32 * k, img.height() as f32 * k)?;
    let mut base = Pixmap::new(w, h)?;
    base.fill(tiny_skia::Color::BLACK);
    blit(
        &mut base,
        &sc,
        (w as f32 - sc.width() as f32) / 2.0,
        (h as f32 - sc.height() as f32) / 2.0,
        1.0,
    );
    Some(base)
}

fn build_covers(map: &Beatmap) -> (Option<Pixmap>, Option<Pixmap>, Option<Pixmap>) {
    let Some(img) = map.background_path().and_then(|p| load_pixmap(&p)) else {
        return (None, None, None);
    };
    let crop = |w: f32, h: f32| -> Option<Pixmap> {
        let k = (w / img.width() as f32).max(h / img.height() as f32);
        let sc = scaled(&img, img.width() as f32 * k, img.height() as f32 * k)?;
        let mut out = Pixmap::new(w as u32, h as u32)?;
        blit(
            &mut out,
            &sc,
            (w - sc.width() as f32) / 2.0,
            (h - sc.height() as f32) / 2.0,
            1.0,
        );
        Some(out)
    };
    let cover = crop(340.0, 252.0);
    let round = crop(340.0, 340.0).map(|mut p| {
        let r = 170.0f32;
        let w = p.width() as usize;
        for (i, px) in p.data_mut().chunks_exact_mut(4).enumerate() {
            let (x, y) = ((i % w) as f32 + 0.5 - r, (i / w) as f32 + 0.5 - r);
            let d = (x * x + y * y).sqrt();
            let a = (r - d).clamp(0.0, 1.0);
            if a < 1.0 {
                for c in px.iter_mut() {
                    *c = (*c as f32 * a) as u8;
                }
            }
        }
        p
    });
    let blur = crop(22.0, 28.0).and_then(|small| scaled(&small, 430.0, 550.0));
    (cover, round, blur)
}
