use crate::osu::beatmap::find_case_insensitive;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layer {
    Background,
    Fail,
    Pass,
    Foreground,
    Overlay,
}

#[derive(Clone, Copy, Debug)]
pub enum Origin {
    TopLeft,
    TopCentre,
    TopRight,
    CentreLeft,
    Centre,
    CentreRight,
    BottomLeft,
    BottomCentre,
    BottomRight,
}

impl Origin {
    pub fn factors(self) -> (f32, f32) {
        match self {
            Origin::TopLeft => (0.0, 0.0),
            Origin::TopCentre => (0.5, 0.0),
            Origin::TopRight => (1.0, 0.0),
            Origin::CentreLeft => (0.0, 0.5),
            Origin::Centre => (0.5, 0.5),
            Origin::CentreRight => (1.0, 0.5),
            Origin::BottomLeft => (0.0, 1.0),
            Origin::BottomCentre => (0.5, 1.0),
            Origin::BottomRight => (1.0, 1.0),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Fade,
    Move,
    MoveX,
    MoveY,
    Scale,
    Vector,
    Rotate,
    Colour,
    FlipH,
    FlipV,
    Additive,
}

#[derive(Clone, Debug)]
pub struct Command {
    pub kind: Kind,
    pub easing: u8,
    pub start: f64,
    pub end: f64,
    pub from: [f64; 3],
    pub to: [f64; 3],
}

#[derive(Clone, Debug)]
pub struct Object {
    pub layer: Layer,
    pub origin: Origin,
    pub path: String,
    pub x: f64,
    pub y: f64,
    pub frames: u32,
    pub frame_delay: f64,
    pub loop_once: bool,
    pub commands: Vec<Command>,
    pub start: f64,
    pub end: f64,
    pub by_kind: Vec<Vec<Command>>,
}

fn kind_index(k: Kind) -> usize {
    match k {
        Kind::Fade => 0,
        Kind::Move => 1,
        Kind::MoveX => 2,
        Kind::MoveY => 3,
        Kind::Scale => 4,
        Kind::Vector => 5,
        Kind::Rotate => 6,
        Kind::Colour => 7,
        Kind::FlipH => 8,
        Kind::FlipV => 9,
        Kind::Additive => 10,
    }
}

#[derive(Clone, Debug)]
pub struct Sample {
    pub time: f64,
    pub path: PathBuf,
    pub volume: f32,
}

pub struct Storyboard {
    pub objects: Vec<Object>,
    pub samples: Vec<Sample>,
    pub widescreen: bool,
    pub uses_background: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct SpriteState {
    pub x: f64,
    pub y: f64,
    pub sx: f64,
    pub sy: f64,
    pub rotation: f64,
    pub opacity: f64,
    pub colour: [f64; 3],
    pub flip_h: bool,
    pub flip_v: bool,
    pub additive: bool,
}

pub fn ease(e: u8, t: f64) -> f64 {
    use std::f64::consts::PI;
    let t = t.clamp(0.0, 1.0);
    let back = 1.70158;
    let back_io = back * 1.525;
    let elastic_out = |t: f64, period: f64| {
        if t == 0.0 || t == 1.0 {
            t
        } else {
            2f64.powf(-10.0 * t) * ((t - period / 4.0) * (2.0 * PI) / period).sin() + 1.0
        }
    };
    let bounce_out = |t: f64| {
        if t < 1.0 / 2.75 {
            7.5625 * t * t
        } else if t < 2.0 / 2.75 {
            let t = t - 1.5 / 2.75;
            7.5625 * t * t + 0.75
        } else if t < 2.5 / 2.75 {
            let t = t - 2.25 / 2.75;
            7.5625 * t * t + 0.9375
        } else {
            let t = t - 2.625 / 2.75;
            7.5625 * t * t + 0.984375
        }
    };
    let io = |f: &dyn Fn(f64) -> f64, t: f64| {
        if t < 0.5 {
            f(2.0 * t) / 2.0
        } else {
            1.0 - f(2.0 - 2.0 * t) / 2.0
        }
    };
    match e {
        0 => t,
        1 | 3 => 1.0 - (1.0 - t) * (1.0 - t),
        2 | 4 => t * t,
        5 => io(&|x| x * x, t),
        6 => t.powi(3),
        7 => 1.0 - (1.0 - t).powi(3),
        8 => io(&|x| x.powi(3), t),
        9 => t.powi(4),
        10 => 1.0 - (1.0 - t).powi(4),
        11 => io(&|x| x.powi(4), t),
        12 => t.powi(5),
        13 => 1.0 - (1.0 - t).powi(5),
        14 => io(&|x| x.powi(5), t),
        15 => 1.0 - (t * PI / 2.0).cos(),
        16 => (t * PI / 2.0).sin(),
        17 => 0.5 - (PI * t).cos() / 2.0,
        18 => {
            if t == 0.0 {
                0.0
            } else {
                2f64.powf(10.0 * (t - 1.0))
            }
        }
        19 => {
            if t == 1.0 {
                1.0
            } else {
                1.0 - 2f64.powf(-10.0 * t)
            }
        }
        20 => io(
            &|x| {
                if x == 0.0 {
                    0.0
                } else {
                    2f64.powf(10.0 * (x - 1.0))
                }
            },
            t,
        ),
        21 => 1.0 - (1.0 - t * t).max(0.0).sqrt(),
        22 => (1.0 - (t - 1.0) * (t - 1.0)).max(0.0).sqrt(),
        23 => io(&|x| 1.0 - (1.0 - x * x).max(0.0).sqrt(), t),
        24 => 1.0 - elastic_out(1.0 - t, 0.3),
        25 => elastic_out(t, 0.3),
        26 => elastic_out(t, 0.6),
        27 => elastic_out(t, 0.15),
        28 => io(&|x| 1.0 - elastic_out(1.0 - x, 0.45), t),
        29 => t * t * ((back + 1.0) * t - back),
        30 => {
            let u = t - 1.0;
            u * u * ((back + 1.0) * u + back) + 1.0
        }
        31 => io(&|x| x * x * ((back_io + 1.0) * x - back_io), t),
        32 => 1.0 - bounce_out(1.0 - t),
        33 => bounce_out(t),
        34 => io(&|x| 1.0 - bounce_out(1.0 - x), t),
        _ => t,
    }
}

fn parse_layer(s: &str) -> Option<Layer> {
    Some(match s.trim() {
        "Background" | "0" => Layer::Background,
        "Fail" | "1" => Layer::Fail,
        "Pass" | "2" => Layer::Pass,
        "Foreground" | "3" => Layer::Foreground,
        "Overlay" | "4" => Layer::Overlay,
        _ => return None,
    })
}

fn parse_origin(s: &str) -> Origin {
    match s.trim() {
        "TopLeft" | "0" | "Custom" => Origin::TopLeft,
        "Centre" | "1" => Origin::Centre,
        "CentreLeft" | "2" => Origin::CentreLeft,
        "TopRight" | "3" => Origin::TopRight,
        "BottomCentre" | "4" => Origin::BottomCentre,
        "TopCentre" | "5" => Origin::TopCentre,
        "CentreRight" | "7" => Origin::CentreRight,
        "BottomLeft" | "8" => Origin::BottomLeft,
        "BottomRight" | "9" => Origin::BottomRight,
        _ => Origin::TopLeft,
    }
}

fn unquote(s: &str) -> String {
    s.trim().trim_matches('"').replace('\\', "/")
}

struct LoopBlock {
    start: f64,
    count: u32,
    commands: Vec<Command>,
}

fn parse_command(parts: &[&str], offset: f64) -> Vec<Command> {
    let mut out = Vec::new();
    if parts.len() < 4 {
        return out;
    }
    let easing = parts[1].trim().parse::<u8>().unwrap_or(0);
    let Ok(start) = parts[2].trim().parse::<f64>() else {
        return out;
    };
    let end = parts[3].trim().parse::<f64>().unwrap_or(start);
    let start = start + offset;
    let end = end + offset;
    let values: Vec<f64> = parts[4..]
        .iter()
        .filter_map(|v| v.trim().parse::<f64>().ok())
        .collect();
    let (kind, width) = match parts[0].trim() {
        "F" => (Kind::Fade, 1),
        "M" => (Kind::Move, 2),
        "MX" => (Kind::MoveX, 1),
        "MY" => (Kind::MoveY, 1),
        "S" => (Kind::Scale, 1),
        "V" => (Kind::Vector, 2),
        "R" => (Kind::Rotate, 1),
        "C" => (Kind::Colour, 3),
        "P" => {
            let kind = match parts.get(4).map(|v| v.trim()) {
                Some("H") => Kind::FlipH,
                Some("V") => Kind::FlipV,
                Some("A") => Kind::Additive,
                _ => return out,
            };
            out.push(Command {
                kind,
                easing,
                start,
                end,
                from: [1.0; 3],
                to: [1.0; 3],
            });
            return out;
        }
        _ => return out,
    };
    if values.len() < width {
        return out;
    }
    let groups: Vec<[f64; 3]> = values
        .chunks(width)
        .filter(|c| c.len() == width)
        .map(|c| {
            let mut v = [0.0; 3];
            v[..width].copy_from_slice(c);
            v
        })
        .collect();
    let duration = end - start;
    if groups.len() == 1 {
        out.push(Command {
            kind,
            easing,
            start,
            end,
            from: groups[0],
            to: groups[0],
        });
        return out;
    }
    for i in 0..groups.len() - 1 {
        let s = start + duration * i as f64;
        out.push(Command {
            kind,
            easing,
            start: s,
            end: s + duration,
            from: groups[i],
            to: groups[i + 1],
        });
    }
    out
}

fn substitute(line: &str, vars: &[(String, String)]) -> String {
    if !line.contains('$') {
        return line.to_string();
    }
    let mut out = line.to_string();
    for (k, v) in vars {
        out = out.replace(k.as_str(), v);
    }
    out
}

fn parse_into(
    text: &str,
    objects: &mut Vec<Object>,
    samples: &mut Vec<Sample>,
    folder: &Path,
    widescreen: &mut bool,
) {
    let mut section = String::new();
    let mut vars: Vec<(String, String)> = Vec::new();
    let mut current: Option<Object> = None;
    let mut loop_block: Option<LoopBlock> = None;
    let mut in_trigger = false;
    let flush_loop = |obj: &mut Option<Object>, lb: &mut Option<LoopBlock>| {
        if let (Some(o), Some(l)) = (obj.as_mut(), lb.take()) {
            let length = l.commands.iter().map(|c| c.end).fold(0.0f64, f64::max);
            for k in 0..l.count.max(1) {
                let base = l.start + length * k as f64;
                for c in &l.commands {
                    let mut c = c.clone();
                    c.start += base;
                    c.end += base;
                    o.commands.push(c);
                }
            }
        }
    };
    for raw in text.lines() {
        if raw.trim().is_empty() || raw.trim_start().starts_with("//") {
            continue;
        }
        let trimmed = raw.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            section = trimmed.to_string();
            continue;
        }
        if section == "[General]" {
            if let Some(v) = trimmed.strip_prefix("WidescreenStoryboard:") {
                *widescreen = v.trim() == "1";
            }
            continue;
        }
        if section == "[Variables]" {
            if let Some((k, v)) = trimmed.split_once('=') {
                vars.push((k.trim().to_string(), v.trim().to_string()));
                vars.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
            }
            continue;
        }
        if section != "[Events]" {
            continue;
        }
        let line = substitute(raw.trim_end(), &vars);
        let depth = line.chars().take_while(|c| *c == ' ' || *c == '_').count();
        let body = &line[depth..];
        let parts: Vec<&str> = body.split(',').collect();
        if depth == 0 {
            flush_loop(&mut current, &mut loop_block);
            in_trigger = false;
            if let Some(o) = current.take() {
                objects.push(o);
            }
            match parts[0].trim() {
                "Sprite" | "4" if parts.len() >= 6 => {
                    if let Some(layer) = parse_layer(parts[1]) {
                        current = Some(Object {
                            layer,
                            origin: parse_origin(parts[2]),
                            path: unquote(parts[3]),
                            x: parts[4].trim().parse().unwrap_or(320.0),
                            y: parts[5].trim().parse().unwrap_or(240.0),
                            frames: 1,
                            frame_delay: 1.0,
                            loop_once: false,
                            commands: Vec::new(),
                            start: 0.0,
                            end: 0.0,
                            by_kind: Vec::new(),
                        });
                    }
                }
                "Animation" | "6" if parts.len() >= 8 => {
                    if let Some(layer) = parse_layer(parts[1]) {
                        current = Some(Object {
                            layer,
                            origin: parse_origin(parts[2]),
                            path: unquote(parts[3]),
                            x: parts[4].trim().parse().unwrap_or(320.0),
                            y: parts[5].trim().parse().unwrap_or(240.0),
                            frames: parts[6].trim().parse().unwrap_or(1).max(1),
                            frame_delay: parts[7].trim().parse::<f64>().unwrap_or(16.0).max(1.0),
                            loop_once: parts
                                .get(8)
                                .map(|v| v.trim() == "LoopOnce" || v.trim() == "1")
                                .unwrap_or(false),
                            commands: Vec::new(),
                            start: 0.0,
                            end: 0.0,
                            by_kind: Vec::new(),
                        });
                    }
                }
                "Sample" | "5" if parts.len() >= 4 => {
                    let layer = parse_layer(parts[2]);
                    if layer != Some(Layer::Fail) {
                        let path = find_case_insensitive(folder, &unquote(parts[3]));
                        samples.push(Sample {
                            time: parts[1].trim().parse().unwrap_or(0.0),
                            path,
                            volume: parts
                                .get(4)
                                .and_then(|v| v.trim().parse::<f32>().ok())
                                .unwrap_or(100.0)
                                / 100.0,
                        });
                    }
                }
                _ => {}
            }
            continue;
        }
        if current.is_none() {
            continue;
        }
        if depth == 1 {
            flush_loop(&mut current, &mut loop_block);
            in_trigger = false;
            match parts[0].trim() {
                "L" if parts.len() >= 3 => {
                    loop_block = Some(LoopBlock {
                        start: parts[1].trim().parse().unwrap_or(0.0),
                        count: parts[2].trim().parse().unwrap_or(1),
                        commands: Vec::new(),
                    });
                }
                "T" => in_trigger = true,
                _ => {
                    let cmds = parse_command(&parts, 0.0);
                    current.as_mut().unwrap().commands.extend(cmds);
                }
            }
        } else if let Some(l) = loop_block.as_mut() {
            l.commands.extend(parse_command(&parts, 0.0));
        } else if in_trigger {
            continue;
        }
    }
    flush_loop(&mut current, &mut loop_block);
    if let Some(o) = current.take() {
        objects.push(o);
    }
}

pub fn load(osu_path: &Path, folder: &Path, background: Option<&str>) -> Option<Storyboard> {
    let mut objects = Vec::new();
    let mut samples = Vec::new();
    let mut widescreen = false;
    if let Ok(entries) = std::fs::read_dir(folder) {
        let mut osbs: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .map(|e| e.eq_ignore_ascii_case("osb"))
                    .unwrap_or(false)
            })
            .collect();
        osbs.sort();
        if let Some(osb) = osbs.first() {
            if let Ok(b) = std::fs::read(osb) {
                parse_into(
                    &String::from_utf8_lossy(&b),
                    &mut objects,
                    &mut samples,
                    folder,
                    &mut widescreen,
                );
            }
        }
    }
    if let Ok(b) = std::fs::read(osu_path) {
        let mut ws = widescreen;
        parse_into(
            &String::from_utf8_lossy(&b),
            &mut objects,
            &mut samples,
            folder,
            &mut ws,
        );
        widescreen = ws;
    }
    for o in &mut objects {
        o.commands
            .sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
        let start = o.commands.iter().map(|c| c.start).fold(f64::MAX, f64::min);
        let mut end = o.commands.iter().map(|c| c.end).fold(f64::MIN, f64::max);
        let fades: Vec<&Command> = o.commands.iter().filter(|c| c.kind == Kind::Fade).collect();
        if let Some(last) = fades
            .iter()
            .max_by(|a, b| a.end.partial_cmp(&b.end).unwrap())
        {
            if last.to[0] <= 0.0 && last.end < end {
                end = end.min(last.end.max(start));
            }
        }
        o.start = start;
        o.end = end;
        o.by_kind = vec![Vec::new(); 11];
        for c in &o.commands {
            o.by_kind[kind_index(c.kind)].push(c.clone());
        }
        o.commands.clear();
        o.commands.shrink_to_fit();
    }
    objects.retain(|o| o.by_kind.iter().any(|v| !v.is_empty()) && o.end >= o.start);
    if objects.is_empty() && samples.is_empty() {
        return None;
    }
    let uses_background = background
        .map(|bg| {
            let bg = bg.to_lowercase();
            objects.iter().any(|o| o.path.to_lowercase() == bg)
        })
        .unwrap_or(false);
    samples.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
    Some(Storyboard {
        objects,
        samples,
        widescreen,
        uses_background,
    })
}

impl Object {
    fn has(&self, kind: Kind) -> bool {
        !self.by_kind[kind_index(kind)].is_empty()
    }

    fn value(&self, kind: Kind, t: f64, default: [f64; 3]) -> [f64; 3] {
        let list = &self.by_kind[kind_index(kind)];
        if list.is_empty() {
            return default;
        }
        let i = list.partition_point(|c| c.start <= t);
        if i == 0 {
            return list[0].from;
        }
        let c = &list[i - 1];
        if t >= c.end {
            return c.to;
        }
        let p = if c.end > c.start {
            ease(c.easing, (t - c.start) / (c.end - c.start))
        } else {
            1.0
        };
        let mut v = [0.0; 3];
        for k in 0..3 {
            v[k] = c.from[k] + (c.to[k] - c.from[k]) * p;
        }
        v
    }

    fn flag(&self, kind: Kind, t: f64) -> bool {
        let list = &self.by_kind[kind_index(kind)];
        let i = list.partition_point(|c| c.start <= t);
        list[..i]
            .iter()
            .rev()
            .take(8)
            .any(|c| t < c.end || c.start == c.end)
    }

    pub fn state(&self, t: f64) -> Option<SpriteState> {
        if t < self.start || t > self.end {
            return None;
        }
        let opacity = self.value(Kind::Fade, t, [1.0; 3])[0];
        if opacity <= 0.0 {
            return None;
        }
        let mv = self.value(Kind::Move, t, [self.x, self.y, 0.0]);
        let has_move = self.has(Kind::Move);
        let (mut x, mut y) = if has_move {
            (mv[0], mv[1])
        } else {
            (self.x, self.y)
        };
        if self.has(Kind::MoveX) {
            x = self.value(Kind::MoveX, t, [x; 3])[0];
        }
        if self.has(Kind::MoveY) {
            y = self.value(Kind::MoveY, t, [y; 3])[0];
        }
        let s = self.value(Kind::Scale, t, [1.0; 3])[0];
        let v = self.value(Kind::Vector, t, [1.0, 1.0, 0.0]);
        let c = self.value(Kind::Colour, t, [255.0; 3]);
        Some(SpriteState {
            x,
            y,
            sx: s * v[0],
            sy: s * v[1],
            rotation: self.value(Kind::Rotate, t, [0.0; 3])[0],
            opacity: opacity.min(1.0),
            colour: [c[0] / 255.0, c[1] / 255.0, c[2] / 255.0],
            flip_h: self.flag(Kind::FlipH, t),
            flip_v: self.flag(Kind::FlipV, t),
            additive: self.flag(Kind::Additive, t),
        })
    }

    pub fn frame_path(&self, t: f64) -> String {
        if self.frames <= 1 {
            return self.path.clone();
        }
        let elapsed = (t - self.start).max(0.0);
        let mut frame = (elapsed / self.frame_delay) as u32;
        frame = if self.loop_once {
            frame.min(self.frames - 1)
        } else {
            frame % self.frames
        };
        match self.path.rfind('.') {
            Some(dot) => format!("{}{}{}", &self.path[..dot], frame, &self.path[dot..]),
            None => format!("{}{}", self.path, frame),
        }
    }

    pub fn all_paths(&self) -> Vec<String> {
        if self.frames <= 1 {
            return vec![self.path.clone()];
        }
        (0..self.frames)
            .map(|f| match self.path.rfind('.') {
                Some(dot) => format!("{}{}{}", &self.path[..dot], f, &self.path[dot..]),
                None => format!("{}{}", self.path, f),
            })
            .collect()
    }
}

pub type ImageCache = HashMap<String, Option<std::sync::Arc<crate::render::fastblit::Mips>>>;
