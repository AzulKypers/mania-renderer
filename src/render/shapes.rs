use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};

pub fn hex(value: &str) -> [u8; 4] {
    let v = value.trim().trim_start_matches('#');
    let p = |i: usize| u8::from_str_radix(v.get(i..i + 2).unwrap_or("ff"), 16).unwrap_or(255);
    [p(0), p(2), p(4), 255]
}

fn paint(color: [u8; 4], opacity: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(Color::from_rgba8(
        color[0],
        color[1],
        color[2],
        (color[3] as f32 * opacity.clamp(0.0, 1.0)) as u8,
    ));
    p.anti_alias = true;
    p
}

fn rounded(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let k = 0.552_284_8 * r;
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish()
}

pub fn fill_rect(dst: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, color: [u8; 4], opacity: f32) {
    if let Some(rect) = Rect::from_xywh(x, y, w.max(0.0), h.max(0.0)) {
        if w > 0.0 && h > 0.0 {
            dst.fill_rect(rect, &paint(color, opacity), Transform::identity(), None);
        }
    }
}

pub fn fill_round(
    dst: &mut Pixmap,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
    color: [u8; 4],
    opacity: f32,
) {
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    if let Some(path) = rounded(x, y, w, h, r) {
        dst.fill_path(
            &path,
            &paint(color, opacity),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

pub fn stroke_round(
    dst: &mut Pixmap,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
    width: f32,
    color: [u8; 4],
    opacity: f32,
) {
    let half = width / 2.0;
    if let Some(path) = rounded(
        x + half,
        y + half,
        w - width,
        h - width,
        (r - half).max(0.0),
    ) {
        let stroke = Stroke {
            width,
            ..Default::default()
        };
        dst.stroke_path(
            &path,
            &paint(color, opacity),
            &stroke,
            Transform::identity(),
            None,
        );
    }
}
