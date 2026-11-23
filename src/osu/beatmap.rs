use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct TimingPoint {
    pub time: f64,
    pub beat_length: f64,
    pub sample_set: u8,
    pub sample_index: u8,
    pub volume: u8,
    pub uninherited: bool,
}

#[derive(Clone, Debug)]
pub struct Note {
    pub column: usize,
    pub time: i32,
    pub end_time: Option<i32>,
    pub hit_sound: u8,
    pub normal_set: u8,
    pub addition_set: u8,
    pub index: u8,
    pub volume: u8,
    pub filename: String,
}

impl Note {
    pub fn is_long(&self) -> bool {
        self.end_time.is_some()
    }
    pub fn end(&self) -> i32 {
        self.end_time.unwrap_or(self.time)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Beatmap {
    pub path: PathBuf,
    pub folder: PathBuf,
    pub audio_file: String,
    pub audio_lead_in: i32,
    pub preview_time: i32,
    pub mode: u8,
    pub sample_set: u8,
    pub title: String,
    pub title_unicode: String,
    pub artist: String,
    pub artist_unicode: String,
    pub creator: String,
    pub version: String,
    pub keys: usize,
    pub od: f64,
    pub hp: f64,
    pub background: Option<String>,
    pub video: Option<(i32, String)>,
    pub timing: Vec<TimingPoint>,
    pub notes: Vec<Note>,
}

fn unquote(value: &str) -> String {
    value.trim().trim_matches('"').replace('\\', "/")
}

pub fn parse_text(text: &str, path: &Path) -> Result<Beatmap> {
    let mut map = Beatmap {
        path: path.to_path_buf(),
        folder: path.parent().map(Path::to_path_buf).unwrap_or_default(),
        sample_set: 1,
        od: 5.0,
        hp: 5.0,
        ..Default::default()
    };
    let mut section = String::new();
    let mut raw_objects: Vec<String> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim_end();
        if line.trim().is_empty() || line.starts_with("//") {
            continue;
        }
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            section = trimmed[1..trimmed.len() - 1].to_string();
            continue;
        }
        match section.as_str() {
            "General" | "Metadata" | "Difficulty" => {
                let Some((key, value)) = line.split_once(':') else {
                    continue;
                };
                let value = value.trim();
                match key.trim() {
                    "AudioFilename" => map.audio_file = value.to_string(),
                    "AudioLeadIn" => map.audio_lead_in = value.parse().unwrap_or(0),
                    "PreviewTime" => map.preview_time = value.parse().unwrap_or(-1),
                    "Mode" => map.mode = value.parse().unwrap_or(0),
                    "SampleSet" => {
                        map.sample_set = match value {
                            "Soft" => 2,
                            "Drum" => 3,
                            _ => 1,
                        }
                    }
                    "Title" => map.title = value.to_string(),
                    "TitleUnicode" => map.title_unicode = value.to_string(),
                    "Artist" => map.artist = value.to_string(),
                    "ArtistUnicode" => map.artist_unicode = value.to_string(),
                    "Creator" => map.creator = value.to_string(),
                    "Version" => map.version = value.to_string(),
                    "CircleSize" => map.keys = value.parse::<f64>().unwrap_or(4.0).round() as usize,
                    "OverallDifficulty" => map.od = value.parse().unwrap_or(5.0),
                    "HPDrainRate" => map.hp = value.parse().unwrap_or(5.0),
                    _ => {}
                }
            }
            "Events" => {
                let parts: Vec<&str> = trimmed.split(',').collect();
                if parts.len() >= 3
                    && parts[0] == "0"
                    && parts[1] == "0"
                    && map.background.is_none()
                {
                    map.background = Some(unquote(parts[2]));
                } else if parts.len() >= 3
                    && (parts[0] == "Video" || parts[0] == "1")
                    && map.video.is_none()
                {
                    map.video = Some((parts[1].trim().parse().unwrap_or(0), unquote(parts[2])));
                }
            }
            "TimingPoints" => {
                let p: Vec<&str> = trimmed.split(',').map(str::trim).collect();
                if p.len() < 2 {
                    continue;
                }
                let (Ok(time), Ok(beat_length)) = (p[0].parse::<f64>(), p[1].parse::<f64>()) else {
                    continue;
                };
                let get = |i: usize, d: u8| {
                    p.get(i)
                        .and_then(|v| v.parse::<i32>().ok())
                        .map(|v| v.clamp(0, 255) as u8)
                        .unwrap_or(d)
                };
                map.timing.push(TimingPoint {
                    time,
                    beat_length,
                    sample_set: get(3, 0),
                    sample_index: get(4, 0),
                    volume: get(5, 100),
                    uninherited: p.get(6).map(|v| *v != "0").unwrap_or(true),
                });
            }
            "HitObjects" => raw_objects.push(trimmed.to_string()),
            _ => {}
        }
    }
    if map.keys == 0 {
        map.keys = 4;
    }
    let keys = map.keys;
    for raw in raw_objects {
        let p: Vec<&str> = raw.split(',').collect();
        if p.len() < 5 {
            continue;
        }
        let (Ok(x), Ok(time), Ok(kind)) = (
            p[0].trim().parse::<f64>(),
            p[2].trim().parse::<f64>(),
            p[3].trim().parse::<u32>(),
        ) else {
            continue;
        };
        let hit_sound = p[4].trim().parse::<u8>().unwrap_or(0);
        let column = ((x * keys as f64 / 512.0).floor() as i64).clamp(0, keys as i64 - 1) as usize;
        let mut end_time = None;
        let mut extras = p.get(5).copied().unwrap_or("");
        if kind & 128 != 0 {
            let (end, rest) = extras.split_once(':').unwrap_or((extras, ""));
            end_time = end.trim().parse::<f64>().ok().map(|v| v as i32);
            extras = rest;
        }
        let ex: Vec<&str> = extras.split(':').collect();
        let num = |i: usize| {
            ex.get(i)
                .and_then(|v| v.trim().parse::<i32>().ok())
                .unwrap_or(0)
                .clamp(0, 255) as u8
        };
        let time = time as i32;
        map.notes.push(Note {
            column,
            time,
            end_time: end_time.filter(|e| *e > time),
            hit_sound,
            normal_set: num(0),
            addition_set: num(1),
            index: num(2),
            volume: num(3),
            filename: ex.get(4).map(|v| v.trim().to_string()).unwrap_or_default(),
        });
    }
    map.notes.sort_by_key(|n| (n.time, n.column));
    map.timing.sort_by(|a, b| {
        a.time
            .partial_cmp(&b.time)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.uninherited.cmp(&a.uninherited))
    });
    Ok(map)
}

pub fn parse_file(path: &Path) -> Result<Beatmap> {
    let bytes = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes);
    let map = parse_text(&text, path)?;
    if map.notes.is_empty() {
        bail!("the map has no notes: {}", path.display());
    }
    Ok(map)
}

impl Beatmap {
    pub fn display_title(&self) -> String {
        format!("{} - {}", self.artist, self.title)
    }

    pub fn first_time(&self) -> i32 {
        self.notes.first().map(|n| n.time).unwrap_or(0)
    }

    pub fn last_time(&self) -> i32 {
        self.notes.iter().map(Note::end).max().unwrap_or(0)
    }

    pub fn timing_at(&self, time: f64) -> Option<&TimingPoint> {
        let mut found = None;
        for point in &self.timing {
            if point.time > time + 1.0 {
                break;
            }
            found = Some(point);
        }
        found.or(self.timing.first())
    }

    pub fn beat_length_at(&self, time: f64) -> f64 {
        let mut found = None;
        for point in self.timing.iter().filter(|p| p.uninherited) {
            if point.time > time + 1.0 {
                break;
            }
            found = Some(point.beat_length);
        }
        found
            .or(self
                .timing
                .iter()
                .find(|p| p.uninherited)
                .map(|p| p.beat_length))
            .unwrap_or(500.0)
            .max(1.0)
    }

    pub fn dominant_beat_length(&self) -> f64 {
        let reds: Vec<&TimingPoint> = self
            .timing
            .iter()
            .filter(|p| p.uninherited && p.beat_length > 0.0)
            .collect();
        if reds.is_empty() {
            return 500.0;
        }
        let end = self.last_time() as f64;
        let mut totals: Vec<(f64, f64)> = Vec::new();
        for (i, point) in reds.iter().enumerate() {
            let start = if i == 0 { f64::MIN } else { point.time };
            let stop = reds
                .get(i + 1)
                .map(|n| n.time)
                .unwrap_or(end.max(point.time));
            let duration = (stop - start.max(self.first_time() as f64).min(stop)).max(0.0);
            let key = (point.beat_length * 1000.0).round() / 1000.0;
            match totals.iter_mut().find(|(b, _)| (*b - key).abs() < 1e-6) {
                Some(entry) => entry.1 += duration,
                None => totals.push((key, duration)),
            }
        }
        totals
            .into_iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|t| t.0)
            .unwrap_or(500.0)
    }

    pub fn audio_path(&self) -> PathBuf {
        find_case_insensitive(&self.folder, &self.audio_file)
    }

    pub fn background_path(&self) -> Option<PathBuf> {
        self.background
            .as_ref()
            .map(|b| find_case_insensitive(&self.folder, b))
            .filter(|p| p.is_file())
    }
}

pub fn find_case_insensitive(folder: &Path, relative: &str) -> PathBuf {
    let direct = folder.join(relative);
    if direct.exists() {
        return direct;
    }
    let mut current = folder.to_path_buf();
    for part in relative.split(['/', '\\']).filter(|s| !s.is_empty()) {
        let wanted = part.to_lowercase();
        let next = std::fs::read_dir(&current).ok().and_then(|entries| {
            entries.flatten().map(|e| e.path()).find(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().to_lowercase() == wanted)
                    .unwrap_or(false)
            })
        });
        match next {
            Some(p) => current = p,
            None => return direct,
        }
    }
    current
}
