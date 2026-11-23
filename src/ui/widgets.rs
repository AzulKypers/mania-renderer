use eframe::egui;
use egui::{
    pos2, vec2, Align2, Color32, FontId, Pos2, Rect, Response, Rounding, Sense, Shape, Stroke, Ui,
};

pub mod theme {
    use eframe::egui::Color32;
    pub const TOP: Color32 = Color32::from_rgb(24, 27, 32);
    pub const BOTTOM: Color32 = Color32::from_rgb(15, 17, 20);
    pub const PANEL: Color32 = Color32::from_rgb(28, 31, 36);
    pub const BORDER: Color32 = Color32::from_rgb(44, 48, 55);
    pub const FIELD: Color32 = Color32::from_rgb(36, 40, 46);
    pub const FIELD_OFF: Color32 = Color32::from_rgb(31, 34, 39);
    pub const MUTED: Color32 = Color32::from_rgb(92, 99, 109);
    pub const TEXT: Color32 = Color32::from_rgb(231, 233, 236);
    pub const DIM: Color32 = Color32::from_rgb(150, 157, 167);
    pub const SALMON: Color32 = Color32::from_rgb(214, 219, 226);
    pub const GOLD: Color32 = Color32::from_rgb(120, 190, 178);
    pub const TEAL: Color32 = Color32::from_rgb(88, 160, 150);
    pub const CORAL: Color32 = Color32::from_rgb(98, 150, 214);
    pub const BUTTON: Color32 = Color32::from_rgb(42, 47, 54);
    pub const ACCENT: Color32 = Color32::from_rgb(224, 128, 128);
}

pub fn shift(c: Color32, amount: i32) -> Color32 {
    let f = |v: u8| (v as i32 + amount).clamp(0, 255) as u8;
    Color32::from_rgb(f(c.r()), f(c.g()), f(c.b()))
}

pub fn font(size: f32) -> FontId {
    FontId::proportional(size)
}

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407))
    }
    pub fn next(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as f32) / (1u64 << 31) as f32
    }
}

pub fn section(ui: &Ui, rect: Rect, title: &str) {
    let p = ui.painter();
    p.rect(
        rect,
        Rounding::same(10.0),
        theme::PANEL,
        Stroke::new(1.0_f32, theme::BORDER),
    );
    p.circle_filled(rect.min + vec2(16.0, 18.0), 3.0, theme::GOLD);
    p.text(
        rect.min + vec2(25.0, 9.0),
        Align2::LEFT_TOP,
        title,
        font(14.0),
        theme::SALMON,
    );
}

pub fn label(ui: &Ui, rect: Rect, text: &str, color: Color32, size: f32) {
    ui.painter().text(
        pos2(rect.min.x, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        font(size),
        color,
    );
}

pub fn note(ui: &Ui, rect: Rect, text: &str, color: Color32) {
    let galley = ui
        .painter()
        .layout(text.to_string(), font(11.5), color, rect.width());
    ui.painter().galley(rect.min, galley, color);
}

pub fn toggle(ui: &mut Ui, rect: Rect, on: &mut bool, text: &str, enabled: bool) -> Response {
    let resp = ui.allocate_rect(
        rect,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    if resp.clicked() {
        *on = !*on;
    }
    let t = ui.ctx().animate_bool(resp.id, *on);
    let p = ui.painter();
    let top = rect.center().y - 9.0;
    let track_rect = Rect::from_min_size(pos2(rect.min.x, top), vec2(34.0, 18.0));
    let track = if !enabled {
        theme::FIELD
    } else {
        let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t) as u8;
        Color32::from_rgb(
            lerp(theme::MUTED.r(), theme::TEAL.r()),
            lerp(theme::MUTED.g(), theme::TEAL.g()),
            lerp(theme::MUTED.b(), theme::TEAL.b()),
        )
    };
    p.rect_filled(track_rect, Rounding::same(9.0), track);
    p.circle_filled(
        pos2(rect.min.x + 9.0 + t * 16.0, top + 9.0),
        7.0,
        if enabled {
            Color32::from_rgb(242, 248, 250)
        } else {
            theme::MUTED
        },
    );
    p.text(
        pos2(rect.min.x + 42.0, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        font(13.5),
        if enabled { theme::TEXT } else { theme::MUTED },
    );
    resp
}

pub fn segment(ui: &mut Ui, rect: Rect, checked: bool, text: &str, enabled: bool) -> Response {
    let resp = ui.allocate_rect(
        rect,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let hover = ui
        .ctx()
        .animate_bool_with_time(resp.id, resp.hovered() && enabled, 0.12);
    let fill = if !enabled {
        theme::FIELD_OFF
    } else if checked {
        theme::TEAL
    } else {
        mix(theme::FIELD, shift(theme::FIELD, 10), hover)
    };
    ui.painter().rect_filled(rect, Rounding::same(6.0), fill);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        font(13.0),
        if enabled {
            Color32::WHITE
        } else {
            theme::MUTED
        },
    );
    resp
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t.clamp(0.0, 1.0)) as u8;
    Color32::from_rgba_unmultiplied(
        l(a.r(), b.r()),
        l(a.g(), b.g()),
        l(a.b(), b.b()),
        l(a.a(), b.a()),
    )
}

pub struct TriStyle {
    pub base: Color32,
    pub triangles: bool,
    pub seed: u64,
    pub size: f32,
    pub drift: bool,
    pub fore: Color32,
}

impl Default for TriStyle {
    fn default() -> TriStyle {
        TriStyle {
            base: theme::BUTTON,
            triangles: false,
            seed: 7,
            size: 13.0,
            drift: false,
            fore: Color32::WHITE,
        }
    }
}

pub fn tri_button(
    ui: &mut Ui,
    rect: Rect,
    text: &str,
    sub: Option<&str>,
    style: &TriStyle,
    enabled: bool,
    _time: f64,
) -> Response {
    let resp = ui.allocate_rect(
        rect,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let ctx = ui.ctx();
    let hover = ctx.animate_bool_with_time(resp.id.with("h"), resp.hovered() && enabled, 0.15);
    let press = ctx.animate_bool_with_time(
        resp.id.with("p"),
        resp.is_pointer_button_down_on() && enabled,
        0.08,
    );
    let base = if enabled {
        style.base
    } else {
        theme::FIELD_OFF
    };
    let fill = mix(mix(base, shift(base, 16), hover), shift(base, -10), press);
    let r = rect.shrink(press * 1.0);
    let p = ui.painter();
    p.rect_filled(r, Rounding::same(8.0), fill);
    let lite = ui.ctx().data(|d| {
        d.get_temp::<bool>(egui::Id::new("ui_lite"))
            .unwrap_or(false)
    });
    if enabled && style.triangles && !lite {
        let clip = p.with_clip_rect(r.shrink(1.0));
        let count = ((r.width() * r.height()) / 700.0).clamp(6.0, 30.0) as usize;
        let t = if style.drift
            && ui
                .ctx()
                .data(|d| d.get_temp::<bool>(egui::Id::new("anim_on")).unwrap_or(true))
        {
            _time
        } else {
            0.0
        };
        triangles(&clip, r, fill, count, style.seed, t, (20.0, 60.0), 0.04);
    }
    p.rect_stroke(
        r,
        Rounding::same(8.0),
        Stroke::new(1.0_f32, Color32::from_white_alpha((8.0 + 22.0 * hover) as u8)),
    );
    let fore = if enabled { style.fore } else { theme::MUTED };
    match sub {
        Some(s) => {
            p.text(
                r.center() - vec2(0.0, 9.0),
                Align2::CENTER_CENTER,
                text,
                font(style.size + 2.0),
                fore,
            );
            p.text(
                r.center() + vec2(0.0, 13.0),
                Align2::CENTER_CENTER,
                s,
                font(11.0),
                Color32::from_white_alpha(170),
            );
        }
        None => {
            p.text(
                r.center(),
                Align2::CENTER_CENTER,
                text,
                font(style.size),
                fore,
            );
        }
    }
    resp
}

pub fn key_box(ui: &mut Ui, rect: Rect, name: &str, capturing: bool, enabled: bool) -> Response {
    let resp = ui.allocate_rect(
        rect,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let hover = ui
        .ctx()
        .animate_bool_with_time(resp.id, resp.hovered() && enabled, 0.12);
    let fill = if !enabled {
        theme::FIELD_OFF
    } else if capturing {
        theme::CORAL
    } else {
        mix(theme::FIELD, shift(theme::FIELD, 12), hover)
    };
    ui.painter().rect_filled(rect, Rounding::same(6.0), fill);
    let chars = name.chars().count().max(1) as f32;
    let size = (if chars > 2.0 { 10.5_f32 } else { 13.0 }).min((rect.width() - 4.0) / (chars * 0.62));
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        if capturing { "?" } else { name },
        font(size),
        if enabled {
            Color32::WHITE
        } else {
            theme::MUTED
        },
    );
    resp
}

pub fn drop_zone(ui: &Ui, rect: Rect, text: &str, active: bool) {
    let p = ui.painter();
    let color = if active { theme::GOLD } else { theme::MUTED };
    if active {
        p.rect_filled(
            rect,
            Rounding::same(8.0),
            Color32::from_rgba_unmultiplied(120, 190, 178, 24),
        );
    }
    let r = rect.shrink(0.5);
    let corners = [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
        r.left_top(),
    ];
    for w in corners.windows(2) {
        dashed(ui, w[0], w[1], color);
    }
    let galley = p.layout(
        text.to_string(),
        font(13.0),
        theme::TEXT,
        rect.width() - 20.0,
    );
    let pos = rect.center() - galley.size() / 2.0;
    p.galley(pos, galley, theme::TEXT);
}

fn dashed(ui: &Ui, a: Pos2, b: Pos2, color: Color32) {
    let len = a.distance(b);
    let dir = (b - a) / len.max(1.0);
    let mut d = 0.0;
    while d < len {
        let e = (d + 6.0).min(len);
        ui.painter()
            .line_segment([a + dir * d, a + dir * e], Stroke::new(1.2_f32, color));
        d += 10.0;
    }
}

pub fn progress(ui: &Ui, rect: Rect, value: f32) {
    let p = ui.painter();
    p.rect_filled(rect, Rounding::same(2.0), theme::FIELD);
    if value > 0.0 {
        let mut r = rect;
        r.max.x = rect.min.x + rect.width() * value.clamp(0.0, 1.0);
        p.rect_filled(r, Rounding::same(2.0), theme::GOLD);
    }
}

fn hash(a: u64, b: u64) -> f32 {
    let mut x = a.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ b.wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    x ^= x >> 31;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 29;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

fn shade(base: Color32, amount: f32) -> Color32 {
    let f = |c: u8| {
        let c = c as f32;
        (if amount >= 0.0 {
            c + (255.0 - c) * amount
        } else {
            c * (1.0 + amount)
        })
        .clamp(0.0, 255.0) as u8
    };
    Color32::from_rgb(f(base.r()), f(base.g()), f(base.b()))
}

fn triangle_list(
    rect: Rect,
    base: Color32,
    count: usize,
    seed: u64,
    time: f64,
    size: (f32, f32),
    range: f32,
) -> Vec<([Pos2; 3], Color32)> {
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let id = seed * 1000 + i as u64;
        let speed = (hash(id, 1).max(0.1) * 0.06) as f64;
        let phase = hash(id, 2) as f64;
        let progress = time * speed + phase;
        let cycle = progress.floor() as u64;
        let frac = (progress - progress.floor()) as f32;
        let tri = size.0 + hash(id, cycle * 7 + 3) * (size.1 - size.0);
        let x = rect.min.x + hash(id, cycle * 7 + 4) * (rect.width() + tri) - tri / 2.0;
        let y = rect.min.y + (1.0 - frac) * (rect.height() + tri);
        let k = (hash(id, cycle * 7 + 5) * 2.0 - 1.0) * range;
        out.push((
            [
                pos2(x - tri / 2.0, y),
                pos2(x + tri / 2.0, y),
                pos2(x, y - tri),
            ],
            shade(base, k),
        ));
    }
    out
}

pub fn triangles(
    painter: &egui::Painter,
    rect: Rect,
    base: Color32,
    count: usize,
    seed: u64,
    time: f64,
    size: (f32, f32),
    range: f32,
) {
    for (pts, c) in triangle_list(rect, base, count, seed, time, size, range) {
        painter.add(Shape::convex_polygon(pts.to_vec(), c, Stroke::NONE));
    }
}

const BG_TRIANGLES: (usize, u64, (f32, f32), f32) = (26, 4, (60.0, 180.0), 0.05);

pub fn background_image(rect: Rect, ppp: f32) -> egui::ColorImage {
    let (w, h) = (
        (rect.width() * ppp).round() as u32,
        (rect.height() * ppp).round() as u32,
    );
    let mut pm = tiny_skia::Pixmap::new(w.max(1), h.max(1)).unwrap();
    let b = theme::BOTTOM;
    pm.fill(tiny_skia::Color::from_rgba8(b.r(), b.g(), b.b(), 255));
    let (count, seed, size, range) = BG_TRIANGLES;
    for (pts, c) in triangle_list(rect, theme::BOTTOM, count, seed, 0.0, size, range) {
        let mut pb = tiny_skia::PathBuilder::new();
        let at = |p: Pos2| ((p.x - rect.min.x) * ppp, (p.y - rect.min.y) * ppp);
        let (x0, y0) = at(pts[0]);
        pb.move_to(x0, y0);
        for p in &pts[1..] {
            let (x, y) = at(*p);
            pb.line_to(x, y);
        }
        pb.close();
        if let Some(path) = pb.finish() {
            let mut paint = tiny_skia::Paint::default();
            paint.set_color_rgba8(c.r(), c.g(), c.b(), 255);
            paint.anti_alias = true;
            pm.fill_path(
                &path,
                &paint,
                tiny_skia::FillRule::Winding,
                tiny_skia::Transform::identity(),
                None,
            );
        }
    }
    egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], pm.data())
}

pub fn background(
    ui: &Ui,
    rect: Rect,
    time: f64,
    animated: bool,
    still: Option<&egui::TextureHandle>,
) {
    let p = ui.painter();
    if let (Some(tex), false) = (still, animated) {
        p.image(
            tex.id(),
            rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        return;
    }
    p.rect_filled(rect, Rounding::ZERO, theme::BOTTOM);
    let t = if animated { time } else { 0.0 };
    let (count, seed, size, range) = BG_TRIANGLES;
    triangles(p, rect, theme::BOTTOM, count, seed, t, size, range);
}

pub fn logo(ui: &Ui, rect: Rect, tex: &egui::TextureHandle) {
    ui.painter().image(
        tex.id(),
        rect,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::WHITE,
    );
}

pub fn logo_icon(size: u32) -> egui::IconData {
    egui::IconData {
        rgba: super::icon_raster::mania_icon(size),
        width: size,
        height: size,
    }
}
