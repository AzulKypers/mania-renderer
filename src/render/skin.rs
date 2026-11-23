use super::assets::{crop_rows, load_pixmap, rotated_ccw, scaled, tinted};
use crate::osu::skin::{parse_colour, parse_list, SkinFiles};
use std::collections::HashMap;
use tiny_skia::Pixmap;

pub struct Digits {
    pub glyphs: Vec<Pixmap>,
    pub dot: Option<Pixmap>,
    pub comma: Option<Pixmap>,
    pub percent: Option<Pixmap>,
    pub x: Option<Pixmap>,
    pub overlap: f32,
}

#[derive(Default)]
pub struct Ranking {
    pub panel: Option<Pixmap>,
    pub title: Option<Pixmap>,
    pub maxcombo: Option<Pixmap>,
    pub accuracy: Option<Pixmap>,
    pub grades: Vec<(&'static str, Pixmap)>,
}

pub struct ColumnSprites {
    pub note: Option<Pixmap>,
    pub head: Option<Pixmap>,
    pub tail: Option<Pixmap>,
    pub body: Option<Pixmap>,
    pub body_style: u8,
    pub key: Option<Pixmap>,
    pub key_down: Option<Pixmap>,
    pub light: Option<Pixmap>,
    pub lighting_n: Vec<Pixmap>,
    pub lighting_l: Vec<Pixmap>,
}

pub struct SkinSet {
    pub scale: f32,
    pub columns_x: Vec<f32>,
    pub widths: Vec<f32>,
    pub hit_y: f32,
    pub light_y: f32,
    pub judge_y: f32,
    pub combo_y: f32,
    pub stage_x0: f32,
    pub stage_x1: f32,
    pub keys_under_notes: bool,
    pub column_colours: Vec<[u8; 4]>,
    pub column_lines: Vec<f32>,
    pub column_line_colour: [u8; 4],
    pub judgement_line: Option<[u8; 4]>,
    pub cols: Vec<ColumnSprites>,
    pub stage_left: Option<Pixmap>,
    pub stage_right: Option<Pixmap>,
    pub stage_bottom: Option<Pixmap>,
    pub stage_hint: Option<Pixmap>,
    pub judgements: Vec<Vec<Pixmap>>,
    pub score_digits: Option<Digits>,
    pub combo_digits: Option<Digits>,
    pub ranking: Ranking,
    pub default_note_colours: Vec<[u8; 4]>,
    pub scorebar_bg: Option<Pixmap>,
    pub scorebar_colour: Vec<Pixmap>,
}

fn pattern(keys: usize) -> Vec<char> {
    let p = match keys {
        1 => "S",
        2 => "11",
        3 => "1S1",
        4 => "1221",
        5 => "12S21",
        6 => "121121",
        7 => "121S121",
        8 => "S1212121",
        9 => "1212S2121",
        10 => "1212112121",
        _ => "",
    };
    if p.is_empty() {
        (0..keys)
            .map(|i| if i % 2 == 0 { '1' } else { '2' })
            .collect()
    } else {
        p.chars().collect()
    }
}

struct Loader<'a> {
    files: &'a SkinFiles,
    cache: HashMap<String, Option<(Pixmap, f32)>>,
}

impl Loader<'_> {
    fn get(&mut self, name: &str) -> Option<(Pixmap, f32)> {
        if name.is_empty() {
            return None;
        }
        let key = name.to_lowercase();
        if let Some(v) = self.cache.get(&key) {
            return v.clone();
        }
        let found = self
            .files
            .image(name)
            .and_then(|(path, density)| load_pixmap(&path).map(|p| (p, density)));
        self.cache.insert(key, found.clone());
        found
    }

    fn resolve(&mut self, custom: Option<String>, default: String) -> String {
        match custom
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
        {
            Some(v) if v.eq_ignore_ascii_case("null") || v.eq_ignore_ascii_case("none") => v,
            Some(v) if self.get(&v).is_some() || self.get(&format!("{v}-0")).is_some() => v,
            _ => default,
        }
    }

    fn frames(&mut self, name: &str) -> Vec<(Pixmap, f32)> {
        let mut out = Vec::new();
        for i in 0..120 {
            match self.get(&format!("{name}-{i}")) {
                Some(f) => out.push(f),
                None => break,
            }
        }
        if out.is_empty() {
            if let Some(f) = self.get(name) {
                out.push(f);
            }
        }
        out
    }
}

pub fn build(files: &SkinFiles, keys: usize, width: u32, height: u32) -> SkinSet {
    let ini = files.ini();
    let skin_version: f32 = ini
        .general
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("Version"))
        .map(|(_, v)| {
            if v.trim().eq_ignore_ascii_case("latest") {
                9.0
            } else {
                v.trim().parse().unwrap_or(1.0)
            }
        })
        .unwrap_or(if files.root.is_some() { 1.0 } else { 9.0 });
    let cfg = ini
        .mania
        .iter()
        .find(|m| m.keys == keys)
        .map(|m| m.values.clone())
        .unwrap_or_default();
    let get = |k: &str| {
        cfg.iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(k))
            .map(|(_, v)| v.clone())
    };
    let num = |k: &str, d: f32| {
        get(k)
            .and_then(|v| {
                v.split(',')
                    .next()
                    .and_then(|x| x.trim().parse::<f32>().ok())
            })
            .unwrap_or(d)
    };
    let s = height as f32 / 480.0;
    let mut widths_u = get("ColumnWidth")
        .map(|v| parse_list(&v))
        .unwrap_or_default();
    widths_u.resize(keys, *widths_u.last().unwrap_or(&30.0));
    let mut spacing = get("ColumnSpacing")
        .map(|v| parse_list(&v))
        .unwrap_or_default();
    spacing.resize(keys.saturating_sub(1), *spacing.last().unwrap_or(&0.0));
    let mut lines = get("ColumnLineWidth")
        .map(|v| parse_list(&v))
        .unwrap_or_default();
    lines.resize(keys + 1, *lines.last().unwrap_or(&2.0));
    let total_u: f32 = widths_u.iter().sum::<f32>() + spacing.iter().sum::<f32>();
    let screen_u = 480.0 * width as f32 / height as f32;
    let column_start = match get("ColumnStart").and_then(|v| v.trim().parse::<f32>().ok()) {
        Some(v) => v,
        None => (screen_u - total_u) / 2.0,
    };
    let mut x = column_start;
    let mut columns_x = Vec::new();
    for i in 0..keys {
        columns_x.push(x * s);
        x += widths_u[i] + spacing.get(i).copied().unwrap_or(0.0);
    }
    let widths: Vec<f32> = widths_u.iter().map(|w| w * s).collect();
    let stage_x0 = columns_x[0];
    let stage_x1 = columns_x[keys - 1] + widths[keys - 1];
    let hit_pos = num("HitPosition", 402.0).clamp(240.0, 480.0);
    let light_pos = num("LightPosition", 413.0);
    let min_width = widths_u.iter().cloned().fold(f32::MAX, f32::min);
    let note_height_width = num("WidthForNoteHeightScale", 0.0);
    let note_height_width = if note_height_width > 0.0 {
        note_height_width
    } else {
        min_width
    };
    let body_styles: Vec<u8> = {
        let mut v: Vec<u8> = get("NoteBodyStyle")
            .map(|v| parse_list(&v).into_iter().map(|x| x as u8).collect())
            .unwrap_or_default();
        if v.is_empty() {
            v.push(1);
        }
        v
    };
    let mut lighting_widths = get("LightingNWidth")
        .map(|v| parse_list(&v))
        .unwrap_or_default();
    lighting_widths.resize(keys, 0.0);
    let mut lighting_l_widths = get("LightingLWidth")
        .map(|v| parse_list(&v))
        .unwrap_or_default();
    lighting_l_widths.resize(keys, 0.0);

    let mut loader = Loader {
        files,
        cache: HashMap::new(),
    };
    let pat = pattern(keys);
    let natural = |p: &Pixmap, d: f32| {
        (
            p.width() as f32 / (1.6 * d) * s,
            p.height() as f32 / (1.6 * d) * s,
        )
    };
    let mut cols = Vec::new();
    for i in 0..keys {
        let t = pat[i];
        let w = widths[i];
        let name = |loader: &mut Loader, k: &str, default: String| {
            loader.resolve(get(&format!("{k}{i}")), default)
        };
        let note_scaled = |img: Option<(Pixmap, f32)>| {
            img.and_then(|(p, d)| {
                let units_w = p.width() as f32 / (1.6 * d);
                let h = p.height() as f32 / (1.6 * d) * (note_height_width / units_w) * s;
                scaled(&p, w, h.max(1.0))
            })
        };
        let n = name(&mut loader, "NoteImage", format!("mania-note{t}"));
        let note_raw = loader.get(&n);
        let n = loader.resolve(get(&format!("NoteImage{i}H")), format!("mania-note{t}H"));
        let head_raw = loader.get(&n).or_else(|| note_raw.clone());
        let n = loader.resolve(get(&format!("NoteImage{i}T")), format!("mania-note{t}T"));
        let tail_raw = loader.get(&n).or_else(|| head_raw.clone());
        let body_name = loader.resolve(get(&format!("NoteImage{i}L")), format!("mania-note{t}L"));
        let body_raw = loader
            .get(&body_name)
            .or_else(|| loader.get(&format!("{body_name}-0")));
        let body = body_raw.and_then(|(p, d)| {
            let tile_h = p.height() as f32 / (1.6 * d) * s;
            let max_rows =
                ((8192.0 / tile_h.max(0.01)) * p.height() as f32).min(p.height() as f32) as u32;
            let p = crop_rows(&p, max_rows.max(1));
            let tile_h = p.height() as f32 / (1.6 * d) * s;
            scaled(&p, w, tile_h.max(1.0))
        });
        let key_scaled = |img: Option<(Pixmap, f32)>| {
            img.and_then(|(p, d)| {
                let (_, h) = natural(&p, d);
                scaled(&p, w, h)
            })
        };
        let n = name(&mut loader, "KeyImage", format!("mania-key{t}"));
        let key = key_scaled(loader.get(&n));
        let n = loader.resolve(get(&format!("KeyImage{i}D")), format!("mania-key{t}D"));
        let key_down = key_scaled(loader.get(&n));
        let light_colour = get(&format!("ColourLight{}", i + 1))
            .and_then(|v| parse_colour(&v))
            .unwrap_or([55, 255, 255, 255]);
        let n = loader.resolve(get("StageLight"), "mania-stage-light".into());
        let light = loader.get(&n).and_then(|(p, d)| {
            let (_, h) = natural(&p, d);
            scaled(&p, w, h).map(|p| tinted(&p, light_colour))
        });
        let light_k = |custom: f32| {
            if skin_version < 2.5 {
                1.0
            } else {
                (if custom > 0.0 { custom } else { widths_u[i] }) / 30.0
            }
        };
        let lw = light_k(lighting_widths[i]);
        let llw = light_k(lighting_l_widths[i]);
        let scale_frames = |frames: Vec<(Pixmap, f32)>, k: f32| -> Vec<Pixmap> {
            frames
                .into_iter()
                .filter_map(|(p, d)| {
                    let (nw, nh) = natural(&p, d);
                    scaled(&p, nw * k, nh * k)
                })
                .collect()
        };
        let n = loader.resolve(get("LightingN"), "lightingN".into());
        let lighting_n = scale_frames(loader.frames(&n), lw);
        let n = loader.resolve(get("LightingL"), "lightingL".into());
        let lighting_l = scale_frames(loader.frames(&n), llw);
        let tail = note_scaled(tail_raw).map(|p| super::assets::flipped_vertical(&p));
        cols.push(ColumnSprites {
            note: note_scaled(note_raw),
            head: note_scaled(head_raw),
            tail,
            body,
            body_style: *body_styles.get(i).or(body_styles.last()).unwrap_or(&1),
            key,
            key_down,
            light,
            lighting_n,
            lighting_l,
        });
    }
    let stage_w = stage_x1 - stage_x0;
    let full_h = height as f32;
    let side = |loader: &mut Loader, k: &str, d: &str| {
        let n = loader.resolve(get(k), d.into());
        loader.get(&n).and_then(|(p, den)| {
            let (nw, _) = natural(&p, den);
            scaled(&p, nw, full_h)
        })
    };
    let stage_left = side(&mut loader, "StageLeft", "mania-stage-left");
    let stage_right = side(&mut loader, "StageRight", "mania-stage-right");
    let n = loader.resolve(get("StageBottom"), "mania-stage-bottom".into());
    let stage_bottom = loader.get(&n).and_then(|(p, d)| {
        let (nw, nh) = natural(&p, d);
        scaled(&p, nw * 1.6, nh * 1.6)
    });
    let n = loader.resolve(get("StageHint"), "mania-stage-hint".into());
    let stage_hint = loader.get(&n).and_then(|(p, d)| {
        let (_, nh) = natural(&p, d);
        scaled(&p, stage_w, nh * 0.9 * 1.6025)
    });
    let judge_names = [
        ("Hit300g", "mania-hit300g"),
        ("Hit300", "mania-hit300"),
        ("Hit200", "mania-hit200"),
        ("Hit100", "mania-hit100"),
        ("Hit50", "mania-hit50"),
        ("Hit0", "mania-hit0"),
    ];
    let judgements = judge_names
        .iter()
        .map(|(k, d)| {
            let n = loader.resolve(get(k), (*d).into());
            loader
                .frames(&n)
                .into_iter()
                .filter_map(|(p, den)| {
                    let (nw, nh) = natural(&p, den);
                    scaled(&p, nw, nh)
                })
                .collect()
        })
        .collect();
    let digits =
        |loader: &mut Loader, prefix: &str, overlap: f32, height_scale: f32| -> Option<Digits> {
            let mut glyphs = Vec::new();
            for n in 0..10 {
                let (p, d) = loader.get(&format!("{prefix}-{n}"))?;
                let (nw, nh) = natural(&p, d);
                glyphs.push(scaled(&p, nw * height_scale, nh * height_scale)?);
            }
            let extra = |loader: &mut Loader, name: &str| {
                loader.get(&format!("{prefix}-{name}")).and_then(|(p, d)| {
                    let (nw, nh) = natural(&p, d);
                    scaled(&p, nw * height_scale, nh * height_scale)
                })
            };
            Some(Digits {
                dot: extra(loader, "dot"),
                comma: extra(loader, "comma"),
                percent: extra(loader, "percent"),
                x: extra(loader, "x"),
                glyphs,
                overlap: overlap * s * height_scale / 1.6,
            })
        };
    let font = |k: &str| {
        ini.fonts
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(k))
            .map(|(_, v)| v.replace('\\', "/"))
    };
    let score_prefix = font("ScorePrefix").unwrap_or("score".into());
    let combo_prefix = font("ComboPrefix").unwrap_or(score_prefix.clone());
    let score_overlap = font("ScoreOverlap")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);
    let combo_overlap = font("ComboOverlap")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);
    let score_digits = digits(&mut loader, &score_prefix, score_overlap, 1.0);
    let combo_digits = digits(&mut loader, &combo_prefix, combo_overlap, 1.0);
    let ranking = {
        let u = height as f32 / 768.0;
        let default_files = SkinFiles::scan(Some(&crate::default_skin::path()));
        let nat = |loader: &mut Loader, name: &str| {
            let found = loader.get(name).or_else(|| {
                default_files
                    .image(name)
                    .and_then(|(path, d)| load_pixmap(&path).map(|p| (p, d)))
            });
            found.and_then(|(p, d)| scaled(&p, p.width() as f32 / d * u, p.height() as f32 / d * u))
        };
        let grades = ["X", "XH", "S", "SH", "A", "B", "C", "D"]
            .iter()
            .filter_map(|g| nat(&mut loader, &format!("ranking-{g}")).map(|p| (*g, p)))
            .collect();
        Ranking {
            panel: nat(&mut loader, "ranking-panel"),
            title: nat(&mut loader, "ranking-title"),
            maxcombo: nat(&mut loader, "ranking-maxcombo"),
            accuracy: nat(&mut loader, "ranking-accuracy"),
            grades,
        }
    };
    let bar_scale = |p: &Pixmap, d: f32| -> Option<Pixmap> {
        let (nw, nh) = natural(p, d);
        scaled(p, nw * 0.715, nh * 0.715).map(|q| rotated_ccw(&q))
    };
    let scorebar_bg = loader
        .get("scorebar-bg")
        .and_then(|(p, d)| bar_scale(&p, d));
    let scorebar_colour = loader
        .frames("scorebar-colour")
        .into_iter()
        .filter_map(|(p, d)| bar_scale(&p, d))
        .collect();
    let colour = |k: &str, d: [u8; 4]| get(k).and_then(|v| parse_colour(&v)).unwrap_or(d);
    let column_colours = (0..keys)
        .map(|i| colour(&format!("Colour{}", i + 1), [0, 0, 0, 255]))
        .collect();
    let judgement_line = if get("JudgementLine")
        .map(|v| v.trim() == "1")
        .unwrap_or(false)
    {
        Some(colour("ColourJudgementLine", [255, 255, 255, 255]))
    } else {
        None
    };
    let default_note_colours = pat
        .iter()
        .map(|c| match c {
            '1' => [235, 235, 245, 255],
            '2' => [90, 170, 255, 255],
            _ => [255, 205, 70, 255],
        })
        .collect();
    SkinSet {
        scale: s,
        columns_x,
        widths,
        hit_y: hit_pos * s,
        light_y: light_pos * s,
        judge_y: num("ScorePosition", 325.0) * s,
        combo_y: num("ComboPosition", 111.0) * s,
        stage_x0,
        stage_x1,
        keys_under_notes: get("KeysUnderNotes")
            .map(|v| v.trim() == "1")
            .unwrap_or(false),
        column_colours,
        column_lines: lines.iter().map(|l| l * s * 0.5).collect(),
        column_line_colour: colour("ColourColumnLine", [255, 255, 255, 255]),
        judgement_line,
        cols,
        stage_left,
        stage_right,
        stage_bottom,
        stage_hint,
        judgements,
        score_digits,
        combo_digits,
        ranking,
        default_note_colours,
        scorebar_bg,
        scorebar_colour,
    }
}

pub fn mod_icons(
    files: &SkinFiles,
    mods: u32,
    autoplay: bool,
    height: u32,
) -> Vec<(String, Option<Pixmap>)> {
    let s = height as f32 / 480.0;
    let table: [(u32, &str, &str); 31] = [
        (1 << 0, "nofail", "NF"),
        (1 << 1, "easy", "EZ"),
        (1 << 2, "touchdevice", "TD"),
        (1 << 3, "hidden", "HD"),
        (1 << 4, "hardrock", "HR"),
        (1 << 5, "suddendeath", "SD"),
        (1 << 6, "doubletime", "DT"),
        (1 << 7, "relax", "RX"),
        (1 << 8, "halftime", "HT"),
        (1 << 9, "nightcore", "NC"),
        (1 << 10, "flashlight", "FL"),
        (1 << 11, "autoplay", "AT"),
        (1 << 22, "cinema", "CN"),
        (1 << 12, "spunout", "SO"),
        (1 << 13, "relax2", "AP"),
        (1 << 14, "perfect", "PF"),
        (1 << 26, "key1", "1K"),
        (1 << 28, "key2", "2K"),
        (1 << 27, "key3", "3K"),
        (1 << 15, "key4", "4K"),
        (1 << 16, "key5", "5K"),
        (1 << 17, "key6", "6K"),
        (1 << 18, "key7", "7K"),
        (1 << 19, "key8", "8K"),
        (1 << 24, "key9", "9K"),
        (1 << 25, "keycoop", "CO"),
        (1 << 20, "fadein", "FI"),
        (1 << 21, "random", "RD"),
        (1 << 29, "scorev2", "V2"),
        (1 << 23, "target", "TP"),
        (1 << 30, "mirror", "MR"),
    ];
    let mut mods = mods;
    if mods & (1 << 9) != 0 {
        mods &= !(1 << 6);
    }
    if mods & (1 << 22) != 0 {
        mods &= !(1 << 11);
    }
    if mods & (1 << 14) != 0 {
        mods &= !(1 << 5);
    }
    if autoplay {
        mods |= 1 << 11;
    }
    let default = SkinFiles::scan(Some(&crate::default_skin::path()));
    let mut out = Vec::new();
    for (bit, name, short) in table {
        if mods & bit == 0 {
            continue;
        }
        let icon = files
            .image(&format!("selection-mod-{name}"))
            .or_else(|| default.image(&format!("selection-mod-{name}")))
            .and_then(|(path, d)| {
                let p = load_pixmap(&path)?;
                let k = 0.89 * s / (1.6 * d);
                scaled(&p, p.width() as f32 * k, p.height() as f32 * k)
            });
        out.push((short.to_string(), icon));
    }
    if let Some(i) = out.iter().position(|(s, _)| s == "MR") {
        let free = files.image("selection-mod-freemodallowed").and_then(|(path, d)| {
            let p = load_pixmap(&path)?;
            let k = 0.89 * s / (1.6 * d);
            scaled(&p, p.width() as f32 * k, p.height() as f32 * k)
        });
        if free.is_some() {
            out.insert(i, ("FM".to_string(), free));
        }
    }
    out
}
