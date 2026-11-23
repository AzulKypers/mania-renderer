use crate::game;
use crate::osu::{beatmap, library, replay, skin::SkinFiles};
use crate::render::{HudOptions, Scene, SceneOptions};
use anyhow::{anyhow, bail, Result};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub enum Source {
    Replay(PathBuf),
    Map(String),
}

#[derive(Clone)]
pub struct JobSettings {
    pub osu_dir: Option<PathBuf>,
    pub songs_dir: Option<PathBuf>,
    pub map_path: Option<String>,
    pub skin: Option<PathBuf>,
    pub skin_hd: bool,
    pub results: bool,
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
}

#[derive(Debug)]
pub enum JobError {
    NotInstalled,
    Converted(String),
    MapPathMismatch,
}

impl std::fmt::Display for JobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobError::NotInstalled => write!(f, "not installed"),
            JobError::Converted(name) => write!(f, "converted: {name}"),
            JobError::MapPathMismatch => write!(f, "map path mismatch"),
        }
    }
}

impl std::error::Error for JobError {}

pub fn data_dir() -> PathBuf {
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("data")
}

pub enum Progress {
    Searching(usize),
    MapFound { title: String, stars: f64 },
    MapPathMismatch,
    Extracting,
}

pub fn find_replay_map(
    replay: &replay::Replay,
    replay_path: &Path,
    settings: &JobSettings,
    progress: &(dyn Fn(Progress) + Sync),
) -> Result<PathBuf> {
    let md5 = replay.beatmap_md5.clone();
    if let Some(spec) = settings.map_path.as_ref().filter(|s| !s.trim().is_empty()) {
        let mut path = PathBuf::from(spec.trim().trim_matches('"'));
        if !path.is_absolute() {
            if let Some(songs) = &settings.songs_dir {
                path = songs.join(path);
            }
        }
        if path
            .extension()
            .map(|e| e.eq_ignore_ascii_case("osz"))
            .unwrap_or(false)
            && path.is_file()
        {
            progress(Progress::Extracting);
            path = library::extract_osz(&path, &data_dir().join("osz"))?;
        }
        let found = if path.is_dir() {
            library::find_in_folder(&path, &md5)
        } else if library::md5_file(&path).as_deref() == Some(md5.as_str()) {
            Some(path.clone())
        } else {
            None
        };
        match found {
            Some(p) => return Ok(p),
            None => progress(Progress::MapPathMismatch),
        }
    }
    let songs = settings
        .songs_dir
        .clone()
        .ok_or_else(|| anyhow!("Songs folder not set"))?;
    let hint = replay_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .and_then(|name| {
            let open = name.rfind(" (")?;
            let head = &name[..open];
            let start = head.rfind('[')?;
            Some(head[start + 1..].trim_end_matches(']').to_string())
        })
        .unwrap_or_default();
    match library::find_beatmap(
        &md5,
        &songs,
        settings.osu_dir.as_deref(),
        &data_dir().join("map-index.tsv"),
        &hint,
        &|p| progress(Progress::Searching(p)),
    )? {
        library::FindResult::Found(p) => Ok(p),
        library::FindResult::NotInstalled => Err(JobError::NotInstalled.into()),
    }
}

fn replay_date(ticks: i64) -> String {
    use chrono::TimeZone;
    let secs = ticks / 10_000_000 - 62_135_596_800;
    if secs <= 0 {
        return String::new();
    }
    match chrono::Local.timestamp_opt(secs, 0).single() {
        Some(d) => d.format("%d/%m/%Y %-I:%M:%S %p.").to_string(),
        None => String::new(),
    }
}

pub fn prepare(
    source: &Source,
    settings: &JobSettings,
    progress: &(dyn Fn(Progress) + Sync),
) -> Result<Scene> {
    let (map_path, rep) = match source {
        Source::Replay(path) => {
            let r = replay::parse_file(path)?;
            let map = find_replay_map(&r, path, settings, progress)?;
            (map, Some(r))
        }
        Source::Map(spec) => {
            if spec.contains("::") {
                progress(Progress::Extracting);
            }
            (
                library::resolve_beatmap_spec(spec, &data_dir().join("osz"))?,
                None,
            )
        }
    };
    let mut map = beatmap::parse_file(&map_path)?;
    if map.mode != 3 {
        bail!(JobError::Converted(
            map_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        ));
    }
    let (mods, events, autoplay) = match &rep {
        Some(r) => {
            if r.mods & replay::MOD_MIRROR != 0 {
                for n in &mut map.notes {
                    n.column = map.keys - 1 - n.column;
                }
            }
            (
                r.mods,
                replay::key_events(&r.frames, map.keys, false),
                false,
            )
        }
        None => (replay::MOD_AUTO, game::autoplay_events(&map), true),
    };
    let sim = game::simulate(&map, &events, mods);
    let default_dir = crate::default_skin::path();
    let files = SkinFiles::scan(Some(settings.skin.as_deref().unwrap_or(&default_dir)))
        .with_hd(settings.skin_hd);
    let opts = SceneOptions {
        width: settings.width,
        height: settings.height,
        fps: settings.fps,
        scroll: settings.scroll,
        dim: settings.dim,
        bpm_scale: settings.bpm_scale,
        decimal_comma: settings.decimal_comma,
        storyboard: settings.storyboard,
        video: settings.video,
        hud: HudOptions {
            labels: settings.hud.labels.iter().take(map.keys).cloned().collect(),
            ..settings.hud.clone()
        },
        overlays: settings.overlays.clone(),
        results: settings.results,
        player: rep
            .as_ref()
            .map(|r| r.player.clone())
            .unwrap_or_else(|| "osu!".into()),
        played_at: rep
            .as_ref()
            .map(|r| replay_date(r.timestamp))
            .unwrap_or_default(),
        replay_stats: rep.as_ref().map(|r| crate::render::ReplayStats {
            counts: [r.count_geki as u32, r.count_300 as u32, r.count_katu as u32, r.count_100 as u32, r.count_50 as u32, r.count_miss as u32],
            score: r.score as u64,
            max_combo: r.max_combo as u32,
        }),
    };
    let scene = Scene::new(map, sim, mods, &files, opts, autoplay);
    progress(Progress::MapFound {
        title: scene.map.display_title(),
        stars: scene.stars,
    });
    Ok(scene)
}
