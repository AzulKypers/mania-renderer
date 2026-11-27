mod help;
mod icon_raster;
mod widgets;
use eframe::egui;

use crate::job::{JobSettings, Source};
use crate::osu::{library, skin};
use crate::render::HudOptions;
use crate::settings::{Settings, Strings};
use crate::video::Encoder;
use crate::worker::{self, Event, ExportJob, Item};
use egui::{pos2, vec2, Align2, Color32, Key, Rect, Rounding, Sense, Stroke, TextureHandle, Ui};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use widgets::{theme, TriStyle};

pub fn export_icon(size: u32) -> Vec<u8> {
    icon_raster::mania_icon(size)
}

pub const APP_NAME: &str = "Mania Renderer";
const DEFAULT_SCROLL: &str = "30";

struct ClipPlayer {
    frames: Vec<Vec<u8>>,
    width: u32,
    height: u32,
    fps: u32,
    audio: Vec<u8>,
    tex: Option<TextureHandle>,
    shown: usize,
    started: Option<std::time::Instant>,
    offset: f64,
    output: Option<(rodio::OutputStream, rodio::Sink)>,
}

impl ClipPlayer {
    fn duration(&self) -> f64 {
        self.frames.len() as f64 / self.fps.max(1) as f64
    }

    fn position(&self) -> f64 {
        let p = self.offset
            + self
                .started
                .map(|s| s.elapsed().as_secs_f64())
                .unwrap_or(0.0);
        p.min(self.duration())
    }

    fn play(&mut self) {
        if self.started.is_some() {
            return;
        }
        if self.position() >= self.duration() - 0.001 {
            self.offset = 0.0;
        }
        self.output = None;
        if let Ok((stream, handle)) = rodio::OutputStream::try_default() {
            if let Ok(sink) = rodio::Sink::try_new(&handle) {
                if let Ok(decoder) = rodio::Decoder::new(std::io::Cursor::new(self.audio.clone())) {
                    use rodio::Source;
                    sink.append(
                        decoder.skip_duration(std::time::Duration::from_secs_f64(self.offset)),
                    );
                    self.output = Some((stream, sink));
                }
            }
        }
        self.started = Some(std::time::Instant::now());
    }

    fn pause(&mut self) {
        if self.started.is_some() {
            self.offset = self.position();
            self.started = None;
            self.output = None;
        }
    }

    fn restart(&mut self) {
        self.pause();
        self.offset = 0.0;
        self.play();
    }
}

struct Editor {
    scene: Option<Arc<crate::render::Scene>>,
    t: f64,
    tex: Option<TextureHandle>,
    selected: usize,
    rendering: bool,
    dirty: bool,
}

#[derive(PartialEq)]
enum Mode {
    Idle,
    Export,
    Preview,
}

struct App {
    s: Settings,
    overlays: Vec<crate::overlays::catalog::OverlayConfig>,
    strings: Strings,
    replay_items: Vec<Item>,
    map_items: Vec<Item>,
    selected: Vec<usize>,
    map_path: String,
    skins: Vec<(String, String)>,
    skin_index: usize,
    capturing: Option<usize>,
    last_mods: egui::Modifiers,
    log: Vec<String>,
    status: String,
    status_key: Option<&'static str>,
    progress: f32,
    mode: Mode,
    cancel: Arc<AtomicBool>,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    enc_rx: Receiver<Vec<(String, Encoder)>>,
    encoders: Option<Vec<(String, Encoder)>>,
    gpu_key: &'static str,
    gpu_index: usize,
    preview: Option<TextureHandle>,
    retired: Vec<TextureHandle>,
    clip: Option<ClipPlayer>,
    logo: Option<TextureHandle>,
    show_credits: bool,
    show_help: bool,
    installing_ffmpeg: bool,
    editor: Option<Editor>,
    show_welcome: bool,
    confirm_exit: bool,
    allow_close: bool,
    start_time: std::time::Instant,
    frames: u32,
    slow_renderer: bool,
    cpu_ms: f64,
    lite_applied: bool,
    bg_still: Option<TextureHandle>,
    dt_avg: f64,
    continuous_frames: u32,
    bench_start: Option<std::time::Instant>,
}

fn set_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "ubuntu".into(),
        egui::FontData::from_static(crate::render::text::UBUNTU),
    );
    fonts.font_data.insert(
        "jp".into(),
        egui::FontData::from_static(crate::render::text::NOTO_JP),
    );
    fonts.font_data.insert(
        "kr".into(),
        egui::FontData::from_static(crate::render::text::NOTO_KR),
    );
    let family = fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default();
    family.insert(0, "ubuntu".into());
    family.push("jp".into());
    family.push("kr".into());
    ctx.set_fonts(fonts);
    let mut style = (*ctx.style()).clone();
    let v = &mut style.visuals;
    v.dark_mode = true;
    v.extreme_bg_color = theme::FIELD;
    v.panel_fill = theme::PANEL;
    v.window_fill = theme::PANEL;
    v.override_text_color = Some(theme::TEXT);
    v.selection.bg_fill = theme::TEAL;
    v.widgets.inactive.bg_fill = theme::FIELD;
    v.widgets.inactive.weak_bg_fill = theme::FIELD;
    v.widgets.hovered.bg_fill = widgets::shift(theme::FIELD, 12);
    v.widgets.hovered.weak_bg_fill = widgets::shift(theme::FIELD, 12);
    v.widgets.active.weak_bg_fill = theme::TEAL;
    v.widgets.open.weak_bg_fill = theme::FIELD;
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, theme::BORDER);
    for w in [
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.rounding = Rounding::same(5.0);
        w.fg_stroke.color = theme::TEXT;
    }
    style.spacing.item_spacing = vec2(4.0, 2.0);
    ctx.set_style(style);
}

pub fn run() -> eframe::Result<()> {
    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([960.0, 778.0])
        .with_resizable(false)
        .with_decorations(false)
        .with_title(APP_NAME)
        .with_icon(widgets::logo_icon(64))
        .with_drag_and_drop(true);
    let attempt = |renderer: eframe::Renderer| {
        let options = eframe::NativeOptions {
            viewport: viewport.clone(),
            renderer,
            ..Default::default()
        };
        eframe::run_native(
            APP_NAME,
            options,
            Box::new(|cc| {
                set_fonts(&cc.egui_ctx);
                Ok(Box::new(App::new()))
            }),
        )
    };
    match attempt(eframe::Renderer::Wgpu) {
        Ok(()) => Ok(()),
        Err(_) => attempt(eframe::Renderer::Glow),
    }
}

fn key_name(key: Key) -> Option<String> {
    let s = match key {
        Key::Space => "SPC",
        Key::Enter => "ENT",
        Key::Tab => "TAB",
        Key::Backspace => "BKSP",
        Key::ArrowLeft => "LEFT",
        Key::ArrowUp => "UP",
        Key::ArrowRight => "RGHT",
        Key::ArrowDown => "DOWN",
        Key::Insert => "INS",
        Key::Delete => "DEL",
        Key::Home => "HOME",
        Key::End => "END",
        Key::PageUp => "PGUP",
        Key::PageDown => "PGDN",
        Key::Comma => ",",
        Key::Period => ".",
        Key::Semicolon => ";",
        Key::Slash => "/",
        Key::Minus => "-",
        Key::Equals => "=",
        Key::Plus => "+",
        Key::OpenBracket => "[",
        Key::CloseBracket => "]",
        Key::Backslash => "\\",
        Key::Backtick => "`",
        Key::Escape => return None,
        other => {
            let n = other.name();
            if n.chars().count() == 1 {
                return Some(n.to_uppercase());
            }
            return Some(n.chars().take(4).collect::<String>().to_uppercase());
        }
    };
    Some(s.to_string())
}

impl App {
    fn new() -> App {
        let s = Settings::load();
        let (tx, rx) = channel();
        let (enc_tx, enc_rx) = channel();
        worker::detect_encoders(enc_tx);
        let mut app = App {
            show_welcome: s.osu_dir.is_empty(),
            s,
            overlays: crate::settings::load_overlays(),
            strings: Strings::load(),
            replay_items: Vec::new(),
            map_items: Vec::new(),
            selected: Vec::new(),
            map_path: String::new(),
            skins: Vec::new(),
            skin_index: 0,
            capturing: None,
            last_mods: egui::Modifiers::default(),
            log: Vec::new(),
            status: String::new(),
            status_key: Some("ready"),
            progress: 0.0,
            mode: Mode::Idle,
            cancel: Arc::new(AtomicBool::new(false)),
            tx,
            rx,
            enc_rx,
            encoders: None,
            gpu_key: "gpu_checking",
            gpu_index: 0,
            preview: None,
            retired: Vec::new(),
            confirm_exit: false,
            allow_close: false,
            start_time: std::time::Instant::now(),
            frames: 0,
            slow_renderer: std::env::var("MR_FORCE_SLOW").is_ok(),
            cpu_ms: 0.0,
            lite_applied: false,
            bg_still: None,
            dt_avg: 0.033,
            continuous_frames: 0,
            bench_start: None,
            clip: None,
            logo: None,
            show_credits: false,
            show_help: false,
            installing_ffmpeg: false,
            editor: None,
        };
        if std::env::var("MR_TEST_HELP").is_ok() {
            app.show_help = true;
        }
        if std::env::var("MR_TEST_CREDITS").is_ok() {
            app.show_credits = true;
        }
        if let Ok(list) = std::env::var("MR_TEST_ADD") {
            app.add_paths(
                list.split('|')
                    .filter(|p| !p.is_empty())
                    .map(PathBuf::from)
                    .collect(),
            );
        }
        if !app.s.scroll_manual && !app.s.osu_dir.is_empty() {
            if let Some(v) = library::read_osu_config(Path::new(&app.s.osu_dir)).mania_speed {
                app.s.scroll = v.to_string();
            }
        }
        if app.s.scroll.is_empty() && !app.s.scroll_manual {
            app.s.scroll = DEFAULT_SCROLL.into();
        }
        let current = app.s.skin.clone();
        app.update_skins(&current);
        app
    }

    fn t(&self, key: &str) -> String {
        self.strings.get(&self.s.lang, key)
    }

    fn tf(&self, key: &str, args: &[String]) -> String {
        self.strings.fmt(&self.s.lang, key, args)
    }

    fn items(&self) -> &Vec<Item> {
        if self.s.auto {
            &self.map_items
        } else {
            &self.replay_items
        }
    }

    fn items_mut(&mut self) -> &mut Vec<Item> {
        if self.s.auto {
            &mut self.map_items
        } else {
            &mut self.replay_items
        }
    }

    fn skins_dir(&self) -> Option<PathBuf> {
        if self.s.osu_dir.is_empty() {
            return None;
        }
        let d = Path::new(&self.s.osu_dir).join("Skins");
        d.is_dir().then_some(d)
    }

    fn update_skins(&mut self, selected: &str) {
        let mut list = vec![(String::new(), String::new())];
        if let Some(dir) = self.skins_dir() {
            for (name, path) in skin::list_skins(&dir) {
                list.push((name, path.to_string_lossy().into_owned()));
            }
        }
        if !selected.is_empty()
            && !list.iter().any(|(_, p)| p == selected)
            && Path::new(selected).is_dir()
        {
            let name = Path::new(selected)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            list.push((name, selected.to_string()));
        }
        self.skin_index = list.iter().position(|(_, p)| p == selected).unwrap_or(0);
        self.skins = list;
    }

    fn selected_skin(&self) -> String {
        self.skins
            .get(self.skin_index)
            .map(|s| s.1.clone())
            .unwrap_or_default()
    }

    fn set_osu_folder(&mut self, dir: &Path) -> bool {
        let songs = library::songs_dir_for(dir);
        if !songs.is_dir() {
            self.message(self.tf("bad_osu", &[dir.display().to_string()]));
            return false;
        }
        self.s.osu_dir = dir.to_string_lossy().into_owned();
        self.s.songs_dir = songs.to_string_lossy().into_owned();
        let cfg = library::read_osu_config(dir);
        self.s.scroll_manual = false;
        if let Some(v) = cfg.mania_speed {
            self.s.scroll = v.to_string();
        }
        let skins = dir.join("Skins");
        let mut current = self.selected_skin();
        if !(current.starts_with(&*skins.to_string_lossy()) && Path::new(&current).is_dir()) {
            current = cfg
                .skin
                .map(|n| skins.join(n))
                .filter(|p| p.is_dir())
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
        }
        self.update_skins(&current);
        true
    }

    fn message(&mut self, text: String) {
        rfd::MessageDialog::new()
            .set_title(APP_NAME)
            .set_description(text)
            .set_level(rfd::MessageLevel::Info)
            .show();
    }

    fn log_line(&mut self, text: String) {
        self.log.push(text);
        if self.log.len() > 400 {
            self.log.remove(0);
        }
    }

    fn add_paths(&mut self, paths: Vec<PathBuf>) {
        if self.s.auto {
            for p in paths {
                let maps = library::list_mania_maps(&p);
                if maps.is_empty() {
                    let name = p
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let line = self.tf("log_nomania", &[name]);
                    self.log_line(line);
                }
                for (name, spec) in maps {
                    if !self.map_items.iter().any(|i| i.key() == spec) {
                        self.map_items.push(Item {
                            name,
                            source: Source::Map(spec),
                        });
                    }
                }
            }
            return;
        }
        for p in paths {
            let ext = p
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if ext == "osz" || ext == "osu" {
                self.map_path = p.to_string_lossy().into_owned();
            } else if ext == "osr" {
                let key = p.to_string_lossy().into_owned();
                if !self.replay_items.iter().any(|i| i.key() == key) {
                    self.replay_items.push(Item {
                        name: key,
                        source: Source::Replay(p),
                    });
                }
            }
        }
    }

    fn validation(&self, need_out: bool) -> Option<String> {
        if self.items().is_empty() {
            return Some(self.t(if self.s.auto {
                "need_mapitem"
            } else {
                "need_replay"
            }));
        }
        if !self.s.auto {
            if self.s.songs_dir.is_empty() || !Path::new(&self.s.songs_dir).is_dir() {
                return Some(self.t("need_songs"));
            }
            let mp = self.map_path.trim().trim_matches('"');
            if !mp.is_empty() {
                let full = if Path::new(mp).is_absolute() {
                    PathBuf::from(mp)
                } else {
                    Path::new(&self.s.songs_dir).join(mp)
                };
                if !full.exists() {
                    return Some(self.tf("need_map", &[full.display().to_string()]));
                }
            }
        }
        if need_out && !Path::new(self.s.out_dir.trim()).is_absolute() {
            return Some(self.t("need_out"));
        }
        if !self.s.scroll.is_empty() {
            let v: u32 = self.s.scroll.parse().unwrap_or(0);
            if !(1..=40).contains(&v) {
                return Some(self.t("need_scroll"));
            }
        }
        if self.s.kps && self.s.teclas.iter().any(|k| k.is_empty()) {
            return Some(self.tf("need_letters", &[self.s.teclas.len().to_string()]));
        }
        None
    }

    fn job_settings(&self) -> JobSettings {
        let osu_dir = (!self.s.osu_dir.is_empty()).then(|| PathBuf::from(&self.s.osu_dir));
        let cfg = osu_dir
            .as_deref()
            .map(library::read_osu_config)
            .unwrap_or_default();
        let color = crate::render::shapes::hex(&self.s.color);
        JobSettings {
            osu_dir,
            songs_dir: (!self.s.songs_dir.is_empty()).then(|| PathBuf::from(&self.s.songs_dir)),
            map_path: (!self.s.auto && !self.map_path.trim().is_empty())
                .then(|| self.map_path.trim().to_string()),
            skin: Some(self.selected_skin())
                .filter(|s| !s.is_empty())
                .map(PathBuf::from),
            skin_hd: self.s.skin_hd,
            results: self.s.show_results,
            map_hs_volume: self.s.map_hs_volume.min(100) as f32 / 100.0,
            skin_hs_volume: self.s.skin_hs_volume.min(100) as f32 / 100.0,
            width: match self.s.height { 720 => 1280, 480 => 854, _ => 1920 },
            height: match self.s.height { 720 => 720, 480 => 480, _ => 1080 },
            fps: match self.s.video_fps { 120 | 144 | 165 | 240 | 300 | 360 | 480 => self.s.video_fps, _ => 60 },
            scroll: self.s.scroll.parse().unwrap_or(30.0),
            dim: if self.s.bg_dim >= 0 {
                self.s.bg_dim.min(100) as f32 / 100.0
            } else {
                cfg.dim_level
                    .map(|d| d.min(100) as f32 / 100.0)
                    .unwrap_or(0.8)
            },
            bpm_scale: cfg.bpm_scale,
            decimal_comma: false,
            storyboard: self.s.story,
            video: self.s.bg_video,
            hud: HudOptions {
                kps: self.s.kps,
                panel: self.s.panel,
                labels: self.s.teclas.clone(),
                bar_color: color,
                score: self.s.show_score,
            },
            overlays: self.overlays.clone(),
        }
    }

    fn preview_settings(&self) -> JobSettings {
        let mut s = self.job_settings();
        s.width = 960;
        s.height = 540;
        s.results = false;
        s
    }

    fn open_editor(&mut self) {
        if let Some(problem) = self.validation(false) {
            self.message(problem);
            return;
        }
        let index = self.selected.first().copied().unwrap_or(0);
        let Some(item) = self.items().get(index).cloned() else {
            return;
        };
        self.editor = Some(Editor {
            scene: None,
            t: 0.0,
            tex: None,
            selected: 0,
            rendering: false,
            dirty: false,
        });
        worker::run_editor_scene(item, self.preview_settings(), self.tx.clone());
    }

    fn start_ffmpeg_install(&mut self) {
        if self.installing_ffmpeg {
            return;
        }
        self.installing_ffmpeg = true;
        self.status_key = None;
        self.status = self.tf("ffmpeg_downloading", &["0".into()]);
        worker::run_ffmpeg_install(self.tx.clone());
    }

    fn request_editor_frame(&mut self) {
        let Some(ed) = self.editor.as_mut() else {
            return;
        };
        let Some(scene) = ed.scene.clone() else {
            return;
        };
        if ed.rendering {
            ed.dirty = true;
            return;
        }
        ed.rendering = true;
        ed.dirty = false;
        worker::render_editor_frame(scene, ed.t, self.overlays.clone(), self.tx.clone());
    }

    fn encoder(&self) -> Encoder {
        if !self.s.use_gpu {
            return Encoder::X264;
        }
        self.encoders
            .as_ref()
            .and_then(|e| e.get(self.gpu_index))
            .map(|e| e.1)
            .unwrap_or(Encoder::X264)
    }

    fn start_export(&mut self) {
        if let Some(problem) = self.validation(true) {
            self.message(problem);
            return;
        }
        if std::fs::create_dir_all(self.s.out_dir.trim()).is_err() {
            self.message(self.tf("no_mkdir", &[self.s.out_dir.clone()]));
            return;
        }
        self.s.skin = self.selected_skin();
        self.s.save();
        self.log.clear();
        self.cancel = Arc::new(AtomicBool::new(false));
        self.mode = Mode::Export;
        let job = ExportJob {
            items: self.items().clone(),
            settings: self.job_settings(),
            out_dir: PathBuf::from(self.s.out_dir.trim()),
            encoder: self.encoder(),
            frame_samples: if self.s.smooth {
                match self.s.smooth_level {
                    1 => 4,
                    2 => 8,
                    _ => 2,
                }
            } else {
                1
            },
            profile: self.s.performance.min(2),
        };
        worker::run_export(job, self.cancel.clone(), self.tx.clone());
    }

    fn start_preview(&mut self) {
        if let Some(problem) = self.validation(false) {
            self.message(problem);
            return;
        }
        self.s.skin = self.selected_skin();
        self.s.save();
        let index = self.selected.first().copied().unwrap_or(0);
        let Some(item) = self.items().get(index).cloned() else {
            return;
        };
        self.mode = Mode::Preview;
        self.status_key = None;
        self.status = self.tf("clip_gen", &["0".into()]);
        worker::run_preview_clip(item, self.preview_settings(), self.tx.clone());
    }

    fn handle_events(&mut self, ctx: &egui::Context) {
        if self.encoders.is_none() {
            if let Ok(list) = self.enc_rx.try_recv() {
                if list.first().map(|e| e.0 == "__noffmpeg__").unwrap_or(false) {
                    self.gpu_key = "ffmpeg_missing";
                    self.encoders = Some(Vec::new());
                } else {
                    self.gpu_key = if list.is_empty() {
                        "gpu_none"
                    } else {
                        "gpu_ok"
                    };
                    self.gpu_index = list.iter().position(|e| e.0 == self.s.gpu).unwrap_or(0);
                    if list.is_empty() {
                        self.s.use_gpu = false;
                    }
                    self.encoders = Some(list);
                }
            }
        }
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Event::Started { index, total, name } => {
                    self.status_key = None;
                    self.status = self.tf(
                        "exporting",
                        &[(index + 1).to_string(), total.to_string(), name.clone()],
                    );
                    self.progress = 0.0;
                    self.log_line(format!("--- {name}"));
                }
                Event::Searching { percent, .. } => {
                    self.status_key = None;
                    self.status = self.tf("search_progress", &[percent.to_string()]);
                    self.progress = percent as f32 / 100.0;
                }
                Event::MapMismatch => {
                    let l = self.t("log_mismatch");
                    self.log_line(l);
                }
                Event::Extracting => {
                    let l = self.t("log_osz");
                    self.log_line(l);
                }
                Event::MapFound { title, stars } => {
                    let l = self.tf("log_map", &[title, format!("{stars:.2}")]);
                    self.log_line(l);
                }
                Event::Progress { done, total, fps } => {
                    let pct = done as f64 * 100.0 / total.max(1) as f64;
                    self.progress = (pct / 100.0) as f32;
                    self.status_key = None;
                    self.status = self.tf(
                        "progress",
                        &[
                            format!("{pct:.0}"),
                            done.to_string(),
                            total.to_string(),
                            format!("{fps:.0}"),
                        ],
                    );
                }
                Event::Saved(path) => {
                    let l = self.tf("log_saved", &[path.display().to_string()]);
                    self.log_line(l);
                }
                Event::Failed { key, detail } => {
                    let heading = self.t(if self.mode == Mode::Preview {
                        "preview_fail"
                    } else {
                        "failed"
                    });
                    self.log_line(heading);
                    let text = self.tf(key, &[detail]);
                    self.log_line(text);
                }
                Event::Finished { ok, total } => {
                    self.mode = Mode::Idle;
                    self.progress = 0.0;
                    self.status_key = None;
                    self.status = self.tf("done", &[ok.to_string(), total.to_string()]);
                    let l = self.status.clone();
                    self.log_line(l);
                }
                Event::Cancelled => {
                    self.mode = Mode::Idle;
                    self.progress = 0.0;
                    self.status_key = Some("cancelled");
                    let l = self.t("cancelled");
                    self.log_line(l);
                }
                Event::ClipProgress(pct) => {
                    self.status_key = None;
                    self.status = self.tf("clip_gen", &[pct.to_string()]);
                    self.progress = pct as f32 / 100.0;
                }
                Event::Clip {
                    frames,
                    width,
                    height,
                    fps,
                    audio,
                } => {
                    self.mode = Mode::Idle;
                    self.status_key = Some("ready");
                    self.progress = 0.0;
                    let mut player = ClipPlayer {
                        frames,
                        width,
                        height,
                        fps,
                        audio,
                        tex: None,
                        shown: usize::MAX,
                        started: None,
                        offset: 0.0,
                        output: None,
                    };
                    player.play();
                    self.clip = Some(player);
                }
                Event::FfmpegProgress(p) => {
                    self.status_key = None;
                    self.status = if p < 0 {
                        self.t("ffmpeg_extracting")
                    } else {
                        self.tf("ffmpeg_downloading", &[p.to_string()])
                    };
                    self.progress = if p < 0 { 1.0 } else { p as f32 / 100.0 };
                }
                Event::FfmpegDone(result) => {
                    self.installing_ffmpeg = false;
                    self.progress = 0.0;
                    match result {
                        Ok(()) => {
                            self.status_key = Some("ffmpeg_done");
                            let l = self.t("ffmpeg_done");
                            self.log_line(l);
                            let (enc_tx, enc_rx) = channel();
                            self.enc_rx = enc_rx;
                            self.encoders = None;
                            self.gpu_key = "gpu_checking";
                            worker::detect_encoders(enc_tx);
                        }
                        Err(e) => {
                            self.status_key = None;
                            self.status = self.tf("ffmpeg_failed", &[e.clone()]);
                            let l = self.status.clone();
                            self.log_line(l);
                        }
                    }
                }
                Event::EditorScene(scene, t) => {
                    if let Some(ed) = self.editor.as_mut() {
                        ed.scene = Some(scene);
                        ed.t = t;
                    }
                    self.request_editor_frame();
                }
                Event::EditorFrame(w, h, data) => {
                    if let Some(ed) = self.editor.as_mut() {
                        let img = egui::ColorImage::from_rgba_unmultiplied(
                            [w as usize, h as usize],
                            &data,
                        );
                        if let Some(old) = ed.tex.replace(ctx.load_texture(
                            "editor",
                            img,
                            egui::TextureOptions::LINEAR,
                        )) {
                            self.retired.push(old);
                        }
                        ed.rendering = false;
                        if ed.dirty {
                            self.request_editor_frame();
                        }
                    }
                }
                Event::Preview(image) => {
                    if image.is_none() {
                        self.close_editor();
                    }
                    self.mode = Mode::Idle;
                    self.status_key = Some("ready");
                    if let Some((w, h, data)) = image {
                        let img = egui::ColorImage::from_rgba_unmultiplied(
                            [w as usize, h as usize],
                            &data,
                        );
                        if let Some(old) = self.preview.replace(ctx.load_texture(
                            "preview",
                            img,
                            egui::TextureOptions::LINEAR,
                        )) {
                            self.retired.push(old);
                        }
                    }
                }
            }
        }
    }

    fn capture_keys(&mut self, ctx: &egui::Context) {
        let Some(index) = self.capturing else {
            self.last_mods = ctx.input(|i| i.modifiers);
            return;
        };
        let (events, mods) = ctx.input(|i| (i.events.clone(), i.modifiers));
        let mut captured: Option<Option<String>> = None;
        for e in events {
            if let egui::Event::Key {
                key, pressed: true, ..
            } = e
            {
                captured = Some(key_name(key));
                break;
            }
        }
        if captured.is_none() {
            if mods.shift && !self.last_mods.shift {
                captured = Some(Some("SHF".into()));
            } else if mods.ctrl && !self.last_mods.ctrl {
                captured = Some(Some("CTL".into()));
            } else if mods.alt && !self.last_mods.alt {
                captured = Some(Some("ALT".into()));
            }
        }
        self.last_mods = mods;
        if let Some(result) = captured {
            if let Some(name) = result {
                let previous = self.s.teclas[index].clone();
                for (i, other) in self.s.teclas.iter_mut().enumerate() {
                    if i != index && *other == name {
                        *other = previous.clone();
                    }
                }
                self.s.teclas[index] = name;
            }
            self.capturing = None;
        }
    }

    fn busy(&self) -> bool {
        self.mode != Mode::Idle
    }

    fn draw(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let time = self.start_time.elapsed().as_secs_f64();
        let full = ui.max_rect();
        let o = full.min;
        let r = |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(o + vec2(x, y), vec2(w, h));
        let animated = !self.busy() && !self.slow_renderer;
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("anim_on"), animated));
        if !animated && self.bg_still.is_none() {
            let img = widgets::background_image(full, ui.ctx().pixels_per_point());
            self.bg_still = Some(ui.ctx().load_texture(
                "bg_still",
                img,
                egui::TextureOptions::NEAREST,
            ));
        }
        widgets::background(ui, full, time, animated, self.bg_still.as_ref());
        let busy = self.busy();
        let enabled = !busy;

        let title_bar = ui.interact(
            r(0.0, 0.0, 700.0, 58.0),
            ui.id().with("drag"),
            Sense::click_and_drag(),
        );
        if title_bar.drag_started() {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        let logo = self.logo.get_or_insert_with(|| {
            let img =
                egui::ColorImage::from_rgba_unmultiplied([128, 128], &icon_raster::mania_icon(128));
            ctx.load_texture("logo", img, egui::TextureOptions::LINEAR)
        });
        widgets::logo(ui, r(16.0, 11.0, 38.0, 38.0), logo);
        ui.painter().text(
            o + vec2(64.0, 23.0),
            Align2::LEFT_CENTER,
            APP_NAME,
            widgets::font(20.0),
            Color32::WHITE,
        );
        ui.painter().text(
            o + vec2(65.0, 42.0),
            Align2::LEFT_CENTER,
            "osu!mania replay renderer",
            widgets::font(11.0),
            theme::DIM,
        );
        let credits_style = TriStyle {
            base: theme::PANEL,
            fore: theme::SALMON,
            size: 12.5,
            ..Default::default()
        };
        if widgets::tri_button(
            ui,
            r(526.0, 15.0, 110.0, 30.0),
            &self.t("credits_btn"),
            None,
            &credits_style,
            true,
            time,
        )
        .clicked()
        {
            self.show_credits = true;
        }
        if widgets::tri_button(
            ui,
            r(410.0, 15.0, 110.0, 30.0),
            &self.t("help_btn"),
            None,
            &credits_style,
            true,
            time,
        )
        .clicked()
        {
            self.show_help = true;
        }
        ui.painter().text(
            o + vec2(712.0, 30.0),
            Align2::RIGHT_CENTER,
            self.t("language"),
            widgets::font(12.0),
            theme::DIM,
        );
        ui.allocate_new_ui(
            egui::UiBuilder::new().max_rect(r(718.0, 18.0, 108.0, 24.0)),
            |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    let names = ["English", "Español"];
                    let mut idx = if self.s.lang == "es" { 1 } else { 0 };
                    egui::ComboBox::from_id_salt("lang")
                        .width(104.0)
                        .selected_text(names[idx])
                        .show_ui(ui, |ui| {
                            for (i, n) in names.iter().enumerate() {
                                ui.selectable_value(&mut idx, i, *n);
                            }
                        });
                    self.s.lang = if idx == 1 { "es".into() } else { "en".into() };
                });
            },
        );
        let min_style = TriStyle {
            base: theme::PANEL,
            ..Default::default()
        };
        if widgets::tri_button(
            ui,
            r(836.0, 14.0, 50.0, 32.0),
            "–",
            None,
            &min_style,
            true,
            time,
        )
        .clicked()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
        let close_style = TriStyle {
            base: theme::PANEL,
            fore: theme::ACCENT,
            ..Default::default()
        };
        let close_rect = r(890.0, 14.0, 54.0, 32.0);
        if widgets::tri_button(ui, close_rect, "", None, &close_style, true, time).clicked() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let c = close_rect.center();
        for (a, b) in [
            (vec2(-6.0, -6.0), vec2(6.0, 6.0)),
            (vec2(-6.0, 6.0), vec2(6.0, -6.0)),
        ] {
            ui.painter()
                .line_segment([c + a, c + b], Stroke::new(1.8_f32, theme::ACCENT));
        }

        let sec = r(16.0, 62.0, 456.0, 236.0);
        widgets::section(
            ui,
            sec,
            &self.t(if self.s.auto { "s_maps" } else { "s_replays" }),
        );
        if widgets::segment(
            ui,
            r(232.0, 68.0, 104.0, 22.0),
            !self.s.auto,
            &self.t("mode_replay"),
            enabled,
        )
        .clicked()
        {
            self.s.auto = false;
            self.selected.clear();
        }
        if widgets::segment(
            ui,
            r(340.0, 68.0, 104.0, 22.0),
            self.s.auto,
            &self.t("mode_auto"),
            enabled,
        )
        .clicked()
        {
            self.s.auto = true;
            self.selected.clear();
        }
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        widgets::drop_zone(
            ui,
            r(28.0, 96.0, 432.0, 48.0),
            &self.t(if self.s.auto { "drop_maps" } else { "drop" }),
            hovering,
        );
        let list_rect = r(28.0, 152.0, 322.0, 136.0);
        ui.painter()
            .rect_filled(list_rect, Rounding::same(5.0), theme::FIELD);
        ui.allocate_new_ui(
            egui::UiBuilder::new().max_rect(list_rect.shrink(4.0)),
            |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    egui::ScrollArea::both()
                        .id_salt("list")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let names: Vec<String> =
                                self.items().iter().map(|i| i.name.clone()).collect();
                            for (i, name) in names.iter().enumerate() {
                                let sel = self.selected.contains(&i);
                                let resp = ui.add(egui::SelectableLabel::new(
                                    sel,
                                    egui::RichText::new(name).size(12.5),
                                ));
                                if resp.clicked() {
                                    if ui.input(|inp| inp.modifiers.ctrl) {
                                        if sel {
                                            self.selected.retain(|x| *x != i);
                                        } else {
                                            self.selected.push(i);
                                        }
                                    } else {
                                        self.selected = vec![i];
                                    }
                                }
                            }
                        });
                });
            },
        );
        let flat = TriStyle::default();
        if widgets::tri_button(
            ui,
            r(358.0, 152.0, 102.0, 30.0),
            &self.t("add"),
            None,
            &flat,
            enabled,
            time,
        )
        .clicked()
        {
            let mut dialog = rfd::FileDialog::new();
            if self.s.auto {
                dialog = dialog.add_filter("osu!", &["osu", "osz"]);
                if Path::new(&self.s.songs_dir).is_dir() {
                    dialog = dialog.set_directory(&self.s.songs_dir);
                }
            } else {
                dialog = dialog.add_filter("osu! replay", &["osr"]);
                let replays = Path::new(&self.s.osu_dir).join("Replays");
                if replays.is_dir() {
                    dialog = dialog.set_directory(replays);
                }
            }
            if let Some(files) = dialog.pick_files() {
                self.add_paths(files);
            }
        }
        if widgets::tri_button(
            ui,
            r(358.0, 188.0, 102.0, 30.0),
            &self.t("remove"),
            None,
            &flat,
            enabled,
            time,
        )
        .clicked()
        {
            let mut sel = self.selected.clone();
            sel.sort_unstable_by(|a, b| b.cmp(a));
            for i in sel {
                if i < self.items().len() {
                    self.items_mut().remove(i);
                }
            }
            self.selected.clear();
            if !self.s.auto {
                self.map_path.clear();
            }
        }

        let sec = r(16.0, 308.0, 456.0, 236.0);
        widgets::section(ui, sec, &self.t("s_folders"));
        let rows = [
            ("osu_dir", 34.0),
            ("songs", 66.0),
            ("map_path", 98.0),
            ("skin", 130.0),
            ("out_dir", 162.0),
        ];
        for (key, y) in rows {
            widgets::label(
                ui,
                r(28.0, 308.0 + y, 104.0, 26.0),
                &self.t(key),
                theme::TEXT,
                13.0,
            );
        }
        let field = |ui: &mut Ui, rect: Rect, text: &mut String, hint: &str, on: bool| {
            ui.painter().rect_filled(
                rect,
                Rounding::same(6.0),
                if on { theme::FIELD } else { theme::FIELD_OFF },
            );
            ui.painter().rect_stroke(
                rect,
                Rounding::same(6.0),
                Stroke::new(
                    1.0_f32,
                    if on {
                        Color32::from_white_alpha(18)
                    } else {
                        Color32::from_white_alpha(6)
                    },
                ),
            );
            ui.put(
                rect.shrink2(vec2(6.0, 4.0)),
                egui::TextEdit::singleline(text)
                    .frame(false)
                    .hint_text(hint)
                    .interactive(on)
                    .font(widgets::font(12.5)),
            );
        };
        field(
            ui,
            r(134.0, 342.0, 226.0, 26.0),
            &mut self.s.osu_dir,
            "",
            enabled,
        );
        field(
            ui,
            r(134.0, 374.0, 226.0, 26.0),
            &mut self.s.songs_dir,
            "",
            enabled,
        );
        let cue = self.t("map_cue");
        field(
            ui,
            r(134.0, 406.0, 226.0, 26.0),
            &mut self.map_path,
            &cue,
            enabled && !self.s.auto,
        );
        field(
            ui,
            r(134.0, 470.0, 226.0, 26.0),
            &mut self.s.out_dir,
            "",
            enabled,
        );
        ui.allocate_new_ui(
            egui::UiBuilder::new().max_rect(r(134.0, 439.0, 140.0, 24.0)),
            |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    let default_name = self.t("skin_default");
                    let label_of = |i: usize, s: &Vec<(String, String)>| {
                        if i == 0 {
                            default_name.clone()
                        } else {
                            s[i].0.clone()
                        }
                    };
                    let current = label_of(self.skin_index, &self.skins);
                    let mut idx = self.skin_index;
                    egui::ComboBox::from_id_salt("skin")
                        .width(136.0)
                        .truncate()
                        .selected_text(current)
                        .height(320.0)
                        .show_ui(ui, |ui| {
                            for i in 0..self.skins.len() {
                                ui.selectable_value(&mut idx, i, label_of(i, &self.skins));
                            }
                        });
                    self.skin_index = idx;
                });
            },
        );
        let hd_tip = self.t("skin_hd_tip");
        widgets::toggle(
            ui,
            r(284.0, 438.0, 78.0, 26.0),
            &mut self.s.skin_hd,
            "@2x",
            enabled,
        )
        .on_hover_text(hd_tip);
        let browse = self.t("browse");
        if widgets::tri_button(
            ui,
            r(368.0, 342.0, 92.0, 26.0),
            &browse,
            None,
            &flat,
            enabled,
            time,
        )
        .clicked()
        {
            if let Some(dir) = rfd::FileDialog::new()
                .set_title(self.t("pick_osu"))
                .pick_folder()
            {
                self.set_osu_folder(&dir);
            }
        }
        if widgets::tri_button(
            ui,
            r(368.0, 374.0, 92.0, 26.0),
            &browse,
            None,
            &flat,
            enabled,
            time,
        )
        .clicked()
        {
            if let Some(dir) = rfd::FileDialog::new()
                .set_title(self.t("pick_songs"))
                .pick_folder()
            {
                self.s.songs_dir = dir.to_string_lossy().into_owned();
            }
        }
        if widgets::tri_button(
            ui,
            r(368.0, 406.0, 92.0, 26.0),
            &browse,
            None,
            &flat,
            enabled && !self.s.auto,
            time,
        )
        .clicked()
        {
            let mut d = rfd::FileDialog::new()
                .set_title(self.t("pick_map"))
                .add_filter("osu!", &["osz", "osu"]);
            if Path::new(&self.s.songs_dir).is_dir() {
                d = d.set_directory(&self.s.songs_dir);
            }
            if let Some(f) = d.pick_file() {
                self.map_path = f.to_string_lossy().into_owned();
            }
        }
        if widgets::tri_button(
            ui,
            r(368.0, 438.0, 92.0, 26.0),
            &self.t("skin_other"),
            None,
            &flat,
            enabled,
            time,
        )
        .clicked()
        {
            if let Some(dir) = rfd::FileDialog::new()
                .set_title(self.t("pick_skin"))
                .pick_folder()
            {
                self.update_skins(&dir.to_string_lossy());
            }
        }
        if widgets::tri_button(
            ui,
            r(368.0, 470.0, 92.0, 26.0),
            &browse,
            None,
            &flat,
            enabled,
            time,
        )
        .clicked()
        {
            if let Some(dir) = rfd::FileDialog::new()
                .set_title(self.t("pick_out"))
                .pick_folder()
            {
                self.s.out_dir = dir.to_string_lossy().into_owned();
            }
        }

        let sec = r(488.0, 62.0, 456.0, 206.0);
        widgets::section(ui, sec, &self.t("s_overlay"));
        let edit_style = TriStyle {
            base: theme::BUTTON,
            triangles: true,
            seed: 13,
            size: 13.5,
            drift: true,
            ..Default::default()
        };
        if widgets::tri_button(
            ui,
            r(500.0, 94.0, 210.0, 28.0),
            &self.t("overlays_btn"),
            None,
            &edit_style,
            enabled,
            time,
        )
        .clicked()
        {
            self.open_editor();
        }
        let active = self.overlays.iter().filter(|o| o.enabled).count();
        widgets::label(
            ui,
            r(722.0, 96.0, 210.0, 24.0),
            &self.tf("overlays_active", &[active.to_string()]),
            theme::DIM,
            13.0,
        );
        widgets::label(
            ui,
            r(500.0, 130.0, 104.0, 26.0),
            &self.t("keys"),
            theme::TEXT,
            13.0,
        );
        let on = enabled && self.s.kps;
        ui.allocate_new_ui(
            egui::UiBuilder::new().max_rect(r(606.0, 131.0, 80.0, 24.0)),
            |ui| {
                ui.add_enabled_ui(on, |ui| {
                    let mut count = self.s.teclas.len().clamp(1, 10);
                    egui::ComboBox::from_id_salt("keys")
                        .width(76.0)
                        .selected_text(format!("{count}K"))
                        .show_ui(ui, |ui| {
                            for k in 1..=10 {
                                ui.selectable_value(&mut count, k, format!("{k}K"));
                            }
                        });
                    let defaults = ["A", "S", "K", "L", "D", "F", "J", "G", "H", "SPC"];
                    while self.s.teclas.len() < count {
                        let next = defaults
                            .iter()
                            .find(|d| !self.s.teclas.iter().any(|t| t == *d))
                            .unwrap_or(&"")
                            .to_string();
                        self.s.teclas.push(next);
                    }
                    self.s.teclas.truncate(count);
                });
            },
        );
        widgets::label(
            ui,
            r(704.0, 130.0, 112.0, 26.0),
            &self.t("bar_color"),
            theme::TEXT,
            13.0,
        );
        let color = crate::render::shapes::hex(&self.s.color);
        let swatch = r(818.0, 130.0, 46.0, 26.0);
        let resp = ui.allocate_rect(swatch, if on { Sense::click() } else { Sense::hover() });
        ui.painter().rect_filled(
            swatch,
            Rounding::same(5.0),
            Color32::from_rgb(color[0], color[1], color[2]),
        );
        ui.painter()
            .rect_stroke(swatch, Rounding::same(5.0), Stroke::new(1.0_f32, theme::BORDER));
        if resp.clicked() {
            ui.memory_mut(|m| m.toggle_popup(egui::Id::new("color")));
        }
        let mut rgb = [color[0], color[1], color[2]];
        egui::popup_below_widget(
            ui,
            egui::Id::new("color"),
            &resp,
            egui::PopupCloseBehavior::CloseOnClickOutside,
            |ui| {
                let mut c = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                if egui::color_picker::color_picker_color32(
                    ui,
                    &mut c,
                    egui::color_picker::Alpha::Opaque,
                ) {
                    rgb = [c.r(), c.g(), c.b()];
                }
            },
        );
        self.s.color = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
        widgets::label(
            ui,
            r(870.0, 130.0, 70.0, 26.0),
            &self.s.color.clone(),
            theme::TEXT,
            12.5,
        );
        widgets::label(
            ui,
            r(500.0, 164.0, 104.0, 26.0),
            &self.t("assign"),
            theme::TEXT,
            13.0,
        );
        let count = self.s.teclas.len();
        let gap = 4.0;
        let key_w = ((326.0 - gap * (count as f32 - 1.0)) / count.max(1) as f32).min(40.0);
        for i in 0..count {
            let rect = r(606.0 + i as f32 * (key_w + gap), 164.0, key_w, 26.0);
            let name = self.s.teclas[i].clone();
            if widgets::key_box(ui, rect, &name, self.capturing == Some(i), on).clicked() {
                self.capturing = Some(i);
            }
        }
        let (hint, hint_color) = match self.capturing {
            Some(i) => (self.tf("key_wait", &[(i + 1).to_string()]), theme::CORAL),
            None => (self.t("key_hint"), theme::DIM),
        };
        let results_t = self.t("show_results");
        let score_t = self.t("show_score");
        widgets::toggle(
            ui,
            r(500.0, 200.0, 210.0, 24.0),
            &mut self.s.show_score,
            &score_t,
            enabled,
        );
        widgets::toggle(
            ui,
            r(720.0, 200.0, 212.0, 24.0),
            &mut self.s.show_results,
            &results_t,
            enabled,
        );
        let fps_t = self.t("show_fps");
        let mut fps_on = self.overlays.iter().any(|o| o.id == "osu_fps" && o.enabled);
        if widgets::toggle(ui, r(500.0, 232.0, 210.0, 24.0), &mut fps_on, &fps_t, enabled).clicked() {
            for o in self.overlays.iter_mut().filter(|o| o.id == "osu_fps") {
                o.enabled = fps_on;
            }
            crate::settings::save_overlays(&self.overlays);
        }
        if self.capturing.is_some() {
            widgets::note(ui, r(720.0, 238.0, 212.0, 18.0), &hint, hint_color);
        }

        let sec = r(488.0, 278.0, 456.0, 266.0);
        widgets::section(ui, sec, &self.t("s_export"));
        let has_gpu = self
            .encoders
            .as_ref()
            .map(|e| !e.is_empty())
            .unwrap_or(false);
        if widgets::segment(
            ui,
            r(500.0, 312.0, 212.0, 30.0),
            !self.s.use_gpu,
            &self.t("cpu"),
            enabled,
        )
        .clicked()
        {
            self.s.use_gpu = false;
        }
        if widgets::segment(
            ui,
            r(718.0, 312.0, 104.0, 30.0),
            self.s.use_gpu,
            &self.t("gpu"),
            enabled && has_gpu,
        )
        .clicked()
        {
            self.s.use_gpu = true;
        }
        ui.allocate_new_ui(
            egui::UiBuilder::new().max_rect(r(828.0, 315.0, 104.0, 24.0)),
            |ui| {
                ui.add_enabled_ui(enabled && has_gpu, |ui| {
                    let names: Vec<String> = self
                        .encoders
                        .as_ref()
                        .map(|e| e.iter().map(|x| x.0.clone()).collect())
                        .unwrap_or_default();
                    let current = names.get(self.gpu_index).cloned().unwrap_or_default();
                    let mut idx = self.gpu_index;
                    egui::ComboBox::from_id_salt("gpu")
                        .width(100.0)
                        .selected_text(current)
                        .show_ui(ui, |ui| {
                            for (i, n) in names.iter().enumerate() {
                                ui.selectable_value(&mut idx, i, n);
                            }
                        });
                    self.gpu_index = idx;
                    if let Some(n) = names.get(idx) {
                        self.s.gpu = n.clone();
                    }
                });
            },
        );
        if self.gpu_key == "ffmpeg_missing" || self.installing_ffmpeg {
            let text = if self.installing_ffmpeg {
                self.status.clone()
            } else {
                self.t("ffmpeg_missing")
            };
            widgets::note(
                ui,
                r(500.0, 348.0, 286.0, 20.0),
                &text,
                if self.installing_ffmpeg {
                    theme::DIM
                } else {
                    theme::ACCENT
                },
            );
            let style = TriStyle {
                base: theme::TEAL,
                size: 12.5,
                ..Default::default()
            };
            if widgets::tri_button(
                ui,
                r(792.0, 343.0, 140.0, 26.0),
                &self.t("ffmpeg_install"),
                None,
                &style,
                !self.installing_ffmpeg && enabled,
                time,
            )
            .clicked()
            {
                self.start_ffmpeg_install();
            }
        } else {
            widgets::note(
                ui,
                r(500.0, 348.0, 432.0, 20.0),
                &self.t(self.gpu_key),
                theme::DIM,
            );
        }
        widgets::label(
            ui,
            r(500.0, 376.0, 104.0, 26.0),
            &self.t("resolution"),
            theme::TEXT,
            13.0,
        );
        ui.allocate_new_ui(
            egui::UiBuilder::new().max_rect(r(606.0, 377.0, 54.0, 24.0)),
            |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    let mut h = self.s.height;
                    egui::ComboBox::from_id_salt("res")
                        .width(50.0)
                        .selected_text(match h { 720 => "720p", 480 => "480p", _ => "1080p" })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut h, 1080, "1080p");
                            ui.selectable_value(&mut h, 720, "720p");
                            ui.selectable_value(&mut h, 480, "480p");
                        });
                    self.s.height = h;
                });
            },
        );
        let fps_tip = self.t("fps_tip");
        ui.allocate_new_ui(
            egui::UiBuilder::new().max_rect(r(664.0, 377.0, 52.0, 24.0)),
            |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    let mut f = match self.s.video_fps { 120 | 144 | 165 | 240 | 300 | 360 | 480 => self.s.video_fps, _ => 60 };
                    egui::ComboBox::from_id_salt("fps")
                        .width(48.0)
                        .selected_text(f.to_string())
                        .show_ui(ui, |ui| {
                            for v in [60u32, 120, 144, 165, 240, 300, 360, 480] {
                                ui.selectable_value(&mut f, v, format!("{v} fps"));
                            }
                        })
                        .response
                        .on_hover_text(fps_tip);
                    self.s.video_fps = f;
                });
            },
        );
        widgets::label(
            ui,
            r(720.0, 376.0, 80.0, 26.0),
            &self.t("bg_dim"),
            theme::TEXT,
            13.0,
        );
        ui.allocate_new_ui(
            egui::UiBuilder::new().max_rect(r(802.0, 377.0, 130.0, 24.0)),
            |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    let osu_label = self.t("bg_dim_osu");
                    let label = |v: i32| {
                        if v < 0 {
                            osu_label.clone()
                        } else {
                            format!("{v}%")
                        }
                    };
                    let mut v = self.s.bg_dim;
                    egui::ComboBox::from_id_salt("bgdim")
                        .width(126.0)
                        .selected_text(label(v))
                        .height(320.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut v, -1, label(-1));
                            for p in (0..=100).step_by(10) {
                                ui.selectable_value(&mut v, p, label(p));
                            }
                        });
                    self.s.bg_dim = v;
                });
            },
        );
        let story_t = self.t("story");
        let bg_t = self.t("bgvideo");
        widgets::toggle(
            ui,
            r(500.0, 414.0, 210.0, 26.0),
            &mut self.s.story,
            &story_t,
            enabled,
        );
        widgets::toggle(
            ui,
            r(720.0, 414.0, 212.0, 26.0),
            &mut self.s.bg_video,
            &bg_t,
            enabled,
        );
        widgets::label(
            ui,
            r(500.0, 446.0, 104.0, 26.0),
            &self.t("scroll"),
            theme::TEXT,
            13.0,
        );
        let before = self.s.scroll.clone();
        field(
            ui,
            r(606.0, 446.0, 90.0, 26.0),
            &mut self.s.scroll,
            "",
            enabled,
        );
        let clean: String = self
            .s
            .scroll
            .chars()
            .filter(|c| c.is_ascii_digit())
            .take(2)
            .collect();
        self.s.scroll = clean;
        if self.s.scroll != before {
            self.s.scroll_manual = true;
        }
        let scroll_rect = r(500.0, 446.0, 196.0, 26.0);
        ui.interact(scroll_rect, egui::Id::new("scroll_tip"), Sense::hover())
            .on_hover_text(self.t("scroll_hint"));
        widgets::label(
            ui,
            r(720.0, 446.0, 80.0, 26.0),
            &self.t("perf"),
            theme::TEXT,
            13.0,
        );
        ui.allocate_new_ui(
            egui::UiBuilder::new().max_rect(r(802.0, 447.0, 130.0, 24.0)),
            |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    let names = [self.t("perf_0"), self.t("perf_1"), self.t("perf_2")];
                    let mut v = self.s.performance.min(2) as usize;
                    egui::ComboBox::from_id_salt("perf")
                        .width(126.0)
                        .selected_text(names[v].clone())
                        .show_ui(ui, |ui| {
                            for (i, n) in names.iter().enumerate() {
                                ui.selectable_value(&mut v, i, n);
                            }
                        })
                        .response
                        .on_hover_text(self.t("perf_tip"));
                    self.s.performance = v as u8;
                });
            },
        );
        let smooth_t = self.t("smooth");
        widgets::toggle(
            ui,
            r(500.0, 478.0, 210.0, 26.0),
            &mut self.s.smooth,
            &smooth_t,
            enabled,
        );
        ui.allocate_new_ui(
            egui::UiBuilder::new().max_rect(r(720.0, 479.0, 212.0, 24.0)),
            |ui| {
                ui.add_enabled_ui(enabled && self.s.smooth, |ui| {
                    let opts = [self.t("smooth_2"), self.t("smooth_4"), self.t("smooth_8")];
                    let mut idx = self.s.smooth_level.min(2);
                    egui::ComboBox::from_id_salt("smooth")
                        .width(204.0)
                        .truncate()
                        .selected_text(opts[idx].clone())
                        .show_ui(ui, |ui| {
                            for (i, o) in opts.iter().enumerate() {
                                ui.selectable_value(&mut idx, i, o);
                            }
                        });
                    self.s.smooth_level = idx;
                });
            },
        );

        let vol_rows = [(500.0f32, self.t("map_hs_vol"), 0usize), (720.0f32, self.t("skin_hs_vol"), 1usize)];
        for (x, label, which) in vol_rows {
            widgets::label(ui, r(x, 510.0, 104.0, 26.0), &label, theme::TEXT, 13.0);
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(r(x + (if which == 0 { 106.0 } else { 82.0 }), 512.0, if which == 0 { 100.0 } else { 130.0 }, 22.0)), |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    ui.spacing_mut().slider_width = if which == 0 { 60.0 } else { 88.0 };
                    let v = if which == 0 { &mut self.s.map_hs_volume } else { &mut self.s.skin_hs_volume };
                    ui.add(egui::Slider::new(v, 0..=100).suffix("%"));
                });
            });
        }

        let preview_style = TriStyle {
            base: theme::BUTTON,
            triangles: true,
            seed: 11,
            size: 15.0,
            drift: true,
            ..Default::default()
        };
        if widgets::tri_button(
            ui,
            r(16.0, 554.0, 928.0, 44.0),
            &self.t("preview"),
            None,
            &preview_style,
            enabled,
            time,
        )
        .clicked()
        {
            self.start_preview();
        }
        let sec = r(16.0, 608.0, 928.0, 88.0);
        widgets::section(ui, sec, &self.t("s_activity"));
        let status = match self.status_key {
            Some(k) => self.t(k),
            None => self.status.clone(),
        };
        ui.painter().text(
            o + vec2(932.0, 625.0),
            Align2::RIGHT_CENTER,
            status,
            widgets::font(12.5),
            theme::DIM,
        );
        ui.allocate_new_ui(
            egui::UiBuilder::new().max_rect(r(28.0, 638.0, 904.0, 44.0)),
            |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("log")
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in &self.log {
                            ui.label(egui::RichText::new(line).size(12.5).color(theme::DIM));
                        }
                    });
            },
        );
        widgets::progress(ui, r(28.0, 686.0, 904.0, 4.0), self.progress);

        let cancel_style = TriStyle {
            triangles: true,
            seed: 3,
            size: 15.0,
            drift: true,
            ..Default::default()
        };
        if widgets::tri_button(
            ui,
            r(16.0, 706.0, 220.0, 60.0),
            &self.t("cancel"),
            None,
            &cancel_style,
            busy && self.mode == Mode::Export,
            time,
        )
        .clicked()
        {
            self.cancel.store(true, Ordering::Relaxed);
        }
        let videos_style = TriStyle {
            triangles: true,
            seed: 5,
            size: 15.0,
            drift: true,
            ..Default::default()
        };
        if widgets::tri_button(
            ui,
            r(244.0, 706.0, 220.0, 60.0),
            &self.t("open_folder"),
            None,
            &videos_style,
            true,
            time,
        )
        .clicked()
        {
            let dir = PathBuf::from(self.s.out_dir.trim());
            let _ = std::fs::create_dir_all(&dir);
            open_folder(&dir);
        }
        let start_style = TriStyle {
            base: theme::TEAL,
            triangles: true,
            seed: 7,
            size: 17.0,
            drift: true,
            ..Default::default()
        };
        let sub = self.t(if self.s.auto {
            "export_sub_auto"
        } else {
            "export_sub"
        });
        if widgets::tri_button(
            ui,
            r(472.0, 706.0, 472.0, 60.0),
            &self.t("export"),
            Some(&sub),
            &start_style,
            enabled,
            time,
        )
        .clicked()
        {
            self.start_export();
        }
    }

    fn draw_preview(&mut self, ctx: &egui::Context) {
        let Some(tex) = self.preview.clone() else {
            return;
        };
        let mut close = false;
        egui::Area::new(egui::Id::new("preview_area"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos2(0.0, 0.0))
            .show(ctx, |ui| {
                let screen = ctx.screen_rect();
                let resp = ui.allocate_rect(screen, Sense::click());
                ui.painter()
                    .rect_filled(screen, Rounding::ZERO, Color32::from_black_alpha(235));
                let avail = screen.shrink2(vec2(16.0, 44.0));
                let size = tex.size_vec2();
                let k = (avail.width() / size.x).min(avail.height() / size.y);
                let img = Rect::from_center_size(avail.center() - vec2(0.0, 6.0), size * k);
                ui.painter().image(
                    tex.id(),
                    img,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
                ui.painter().text(
                    pos2(screen.min.x + 16.0, 24.0),
                    Align2::LEFT_CENTER,
                    self.t("preview_title"),
                    widgets::font(16.0),
                    Color32::WHITE,
                );
                let galley = ui.painter().layout(
                    self.t("preview_note"),
                    widgets::font(12.0),
                    theme::DIM,
                    screen.width() - 32.0,
                );
                ui.painter().galley(
                    pos2(screen.min.x + 16.0, screen.max.y - 30.0),
                    galley,
                    theme::DIM,
                );
                if resp.clicked() || ctx.input(|i| i.key_pressed(Key::Escape)) {
                    close = true;
                }
            });
        if close {
            self.close_preview();
        }
    }

    fn close_clip(&mut self) {
        if let Some(tex) = self.clip.take().and_then(|c| c.tex) {
            self.retired.push(tex);
        }
    }

    fn close_editor(&mut self) {
        if let Some(tex) = self.editor.take().and_then(|e| e.tex) {
            self.retired.push(tex);
        }
    }

    fn close_preview(&mut self) {
        if let Some(tex) = self.preview.take() {
            self.retired.push(tex);
        }
    }

    fn draw_clip(&mut self, ctx: &egui::Context) {
        let Some(player) = self.clip.as_mut() else {
            return;
        };
        let pos = player.position();
        if player.started.is_some() && pos >= player.duration() - 0.0005 {
            player.pause();
        }
        let index = ((pos * player.fps as f64) as usize).min(player.frames.len().saturating_sub(1));
        if index != player.shown && !player.frames.is_empty() {
            if let Ok(img) =
                image::load_from_memory_with_format(&player.frames[index], image::ImageFormat::Jpeg)
            {
                let rgb = img.to_rgb8();
                let color = egui::ColorImage::from_rgb(
                    [player.width as usize, player.height as usize],
                    rgb.as_raw(),
                );
                match player.tex.as_mut() {
                    Some(t) => t.set(color, egui::TextureOptions::LINEAR),
                    None => {
                        player.tex =
                            Some(ctx.load_texture("clip", color, egui::TextureOptions::LINEAR))
                    }
                }
                player.shown = index;
            }
        }
        let playing = player.started.is_some();
        let duration = player.duration();
        let tex = player.tex.clone();
        let (t_title, t_play, t_pause, t_restart, t_hint, t_close) = (
            self.t("clip_title"),
            self.t("clip_play"),
            self.t("clip_pause"),
            self.t("clip_restart"),
            self.t("clip_hint"),
            self.t("done"),
        );
        let time = self.start_time.elapsed().as_secs_f64();
        let mut action = 0;
        egui::Area::new(egui::Id::new("clip_area"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos2(0.0, 0.0))
            .show(ctx, |ui| {
                let screen = ctx.screen_rect();
                ui.allocate_rect(screen, Sense::click());
                ui.painter()
                    .rect_filled(screen, Rounding::ZERO, Color32::from_rgb(8, 14, 20));
                ui.painter().text(
                    pos2(16.0, 26.0),
                    Align2::LEFT_CENTER,
                    &t_title,
                    widgets::font(17.0),
                    Color32::WHITE,
                );
                let video = Rect::from_min_size(pos2(48.0, 52.0), vec2(864.0, 486.0));
                ui.painter()
                    .rect_filled(video, Rounding::same(6.0), Color32::BLACK);
                if let Some(tex) = &tex {
                    ui.painter().image(
                        tex.id(),
                        video,
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                let bar = Rect::from_min_size(pos2(48.0, 556.0), vec2(864.0, 6.0));
                widgets::progress(ui, bar, (pos / duration.max(0.001)) as f32);
                ui.painter().text(
                    pos2(912.0, 580.0),
                    Align2::RIGHT_CENTER,
                    format!("{:.1} / {:.1} s", pos, duration),
                    widgets::font(13.0),
                    theme::DIM,
                );
                let style = TriStyle {
                    base: theme::TEAL,
                    triangles: true,
                    seed: 21,
                    size: 15.0,
                    drift: true,
                    ..Default::default()
                };
                if widgets::tri_button(
                    ui,
                    Rect::from_min_size(pos2(48.0, 596.0), vec2(200.0, 50.0)),
                    if playing { &t_pause } else { &t_play },
                    None,
                    &style,
                    true,
                    time,
                )
                .clicked()
                {
                    action = 1;
                }
                if widgets::tri_button(
                    ui,
                    Rect::from_min_size(pos2(260.0, 596.0), vec2(200.0, 50.0)),
                    &t_restart,
                    None,
                    &TriStyle::default(),
                    true,
                    time,
                )
                .clicked()
                {
                    action = 2;
                }
                let close_style = TriStyle {
                    base: theme::TEAL,
                    triangles: true,
                    seed: 22,
                    size: 15.0,
                    drift: true,
                    ..Default::default()
                };
                if widgets::tri_button(
                    ui,
                    Rect::from_min_size(pos2(712.0, 596.0), vec2(200.0, 50.0)),
                    &t_close,
                    None,
                    &close_style,
                    true,
                    time,
                )
                .clicked()
                {
                    action = 3;
                }
                ui.painter().text(
                    pos2(480.0, 680.0),
                    Align2::CENTER_CENTER,
                    &t_hint,
                    widgets::font(12.5),
                    theme::DIM,
                );
            });
        if ctx.input(|i| i.key_pressed(Key::Space)) {
            action = 1;
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            action = 3;
        }
        let player = self.clip.as_mut().unwrap();
        match action {
            1 => {
                if playing {
                    player.pause()
                } else {
                    player.play()
                }
            }
            2 => player.restart(),
            3 => self.close_clip(),
            _ => {}
        }
        ctx.request_repaint();
    }

    fn draw_editor(&mut self, ctx: &egui::Context) {
        if self.editor.is_none() {
            return;
        }
        let time = self.start_time.elapsed().as_secs_f64();
        let lang = if self.s.lang == "es" { 1 } else { 0 };
        let mut changed = false;
        let mut close = false;
        let tex = self.editor.as_ref().unwrap().tex.clone();
        let loading = self.editor.as_ref().unwrap().scene.is_none();
        let mut selected = self.editor.as_ref().unwrap().selected;
        let (t_title, t_hint, t_done, t_scale, t_reset, t_loading) = (
            self.t("overlays_title"),
            self.t("overlays_hint"),
            self.t("done"),
            self.t("overlay_scale"),
            self.t("overlay_reset"),
            self.t("overlay_loading"),
        );
        egui::Area::new(egui::Id::new("editor_area"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos2(0.0, 0.0))
            .show(ctx, |ui| {
                let screen = ctx.screen_rect();
                ui.allocate_rect(screen, Sense::hover());
                ui.painter()
                    .rect_filled(screen, Rounding::ZERO, Color32::from_rgb(8, 14, 20));
                ui.painter().text(
                    pos2(16.0, 26.0),
                    Align2::LEFT_CENTER,
                    &t_title,
                    widgets::font(17.0),
                    Color32::WHITE,
                );
                let canvas = Rect::from_min_size(pos2(16.0, 52.0), vec2(640.0, 360.0));
                let k = canvas.width() / 1920.0;
                ui.painter()
                    .rect_filled(canvas, Rounding::same(4.0), Color32::BLACK);
                match &tex {
                    Some(t) => {
                        ui.painter().image(
                            t.id(),
                            canvas,
                            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                    None => {
                        ui.painter().text(
                            canvas.center(),
                            Align2::CENTER_CENTER,
                            &t_loading,
                            widgets::font(14.0),
                            theme::DIM,
                        );
                    }
                }
                for i in 0..self.overlays.len() {
                    let o = self.overlays[i].clone();
                    if !o.enabled {
                        continue;
                    }
                    let (w, h) = crate::render::Scene::overlay_size(&o.id);
                    let rect = Rect::from_min_size(
                        canvas.min + vec2(o.x * k, o.y * k),
                        vec2(w * o.scale * k, h * o.scale * k),
                    );
                    let resp = ui.interact(rect, egui::Id::new(("ov", i)), Sense::click_and_drag());
                    if resp.drag_started() || resp.clicked() {
                        selected = i;
                    }
                    if resp.dragged() {
                        let d = resp.drag_delta() / k;
                        let ov = &mut self.overlays[i];
                        ov.x = (ov.x + d.x).clamp(-w * ov.scale * 0.5, 1920.0 - w * ov.scale * 0.5);
                        ov.y = (ov.y + d.y).clamp(-h * ov.scale * 0.5, 1080.0 - h * ov.scale * 0.5);
                        selected = i;
                    }
                    if resp.drag_stopped() {
                        changed = true;
                    }
                    let cur = Rect::from_min_size(
                        canvas.min + vec2(self.overlays[i].x * k, self.overlays[i].y * k),
                        rect.size(),
                    );
                    let (col, width) = if i == selected {
                        (theme::CORAL, 2.0_f32)
                    } else if resp.hovered() {
                        (Color32::WHITE, 1.5_f32)
                    } else {
                        (Color32::from_white_alpha(140), 1.0_f32)
                    };
                    ui.painter()
                        .rect_stroke(cur, Rounding::same(3.0), Stroke::new(width, col));
                    if resp.dragged() {
                        ui.painter().rect_filled(
                            cur,
                            Rounding::same(3.0),
                            Color32::from_rgba_unmultiplied(232, 118, 104, 40),
                        );
                    }
                    let name = crate::overlays::catalog::def(&o.id)
                        .map(|d| d.name[lang])
                        .unwrap_or("");
                    ui.painter().text(
                        cur.left_top() + vec2(3.0, 2.0),
                        Align2::LEFT_TOP,
                        name,
                        widgets::font(10.5),
                        col,
                    );
                }
                ui.painter().text(
                    pos2(16.0, 428.0),
                    Align2::LEFT_CENTER,
                    &t_hint,
                    widgets::font(12.5),
                    theme::DIM,
                );
                let side = Rect::from_min_size(pos2(672.0, 52.0), vec2(272.0, 360.0));
                ui.painter()
                    .rect_filled(side, Rounding::same(6.0), theme::PANEL);
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(side.shrink(8.0)), |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("ovlist")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for i in 0..self.overlays.len() {
                                let id = self.overlays[i].id.clone();
                                let name = crate::overlays::catalog::def(&id)
                                    .map(|d| d.name[lang])
                                    .unwrap_or("");
                                ui.horizontal(|ui| {
                                    let mut on = self.overlays[i].enabled;
                                    if ui.checkbox(&mut on, "").changed() {
                                        self.overlays[i].enabled = on;
                                        changed = true;
                                        if on {
                                            selected = i;
                                        }
                                    }
                                    let text = egui::RichText::new(name).size(13.0).color(
                                        if i == selected {
                                            theme::SALMON
                                        } else {
                                            theme::TEXT
                                        },
                                    );
                                    if ui
                                        .add(egui::SelectableLabel::new(i == selected, text))
                                        .clicked()
                                    {
                                        selected = i;
                                    }
                                });
                            }
                        });
                });
                let opts_rect = Rect::from_min_size(pos2(16.0, 446.0), vec2(928.0, 200.0));
                ui.painter()
                    .rect_filled(opts_rect, Rounding::same(6.0), theme::PANEL);
                ui.allocate_new_ui(
                    egui::UiBuilder::new().max_rect(opts_rect.shrink(12.0)),
                    |ui| {
                        let Some(o) = self.overlays.get(selected).cloned() else {
                            return;
                        };
                        let Some(d) = crate::overlays::catalog::def(&o.id) else {
                            return;
                        };
                        ui.label(
                            egui::RichText::new(d.name[lang])
                                .size(15.0)
                                .color(theme::SALMON),
                        );
                        ui.add_space(4.0);
                        egui::Grid::new("ovopts")
                            .num_columns(4)
                            .spacing(vec2(14.0, 8.0))
                            .show(ui, |ui| {
                                let mut scale = o.scale;
                                ui.label(egui::RichText::new(&t_scale).size(13.0));
                                if ui
                                    .add(egui::Slider::new(&mut scale, 0.4..=2.0).fixed_decimals(2))
                                    .changed()
                                {
                                    self.overlays[selected].scale = scale;
                                }
                                if ui.input(|i| i.pointer.any_released())
                                    && (scale - o.scale).abs() < f32::EPSILON
                                    && self.overlays[selected].scale != o.scale
                                {
                                    changed = true;
                                }
                                if ui
                                    .button(egui::RichText::new(&t_reset).size(12.5))
                                    .clicked()
                                {
                                    self.overlays[selected].x = d.pos.0;
                                    self.overlays[selected].y = d.pos.1;
                                    self.overlays[selected].scale = 1.0;
                                    changed = true;
                                }
                                ui.end_row();
                                for (n, opt) in d.options.iter().enumerate() {
                                    ui.label(egui::RichText::new(opt.label[lang]).size(13.0));
                                    let current = self.overlays[selected]
                                        .options
                                        .get(opt.key)
                                        .cloned()
                                        .unwrap_or_else(|| opt.default.to_string());
                                    let shown = opt
                                        .choices
                                        .iter()
                                        .find(|c| c.0 == current)
                                        .map(|c| c.1[lang])
                                        .unwrap_or("");
                                    let mut value = current.clone();
                                    egui::ComboBox::from_id_salt(("opt", selected, n))
                                        .width(150.0)
                                        .selected_text(shown)
                                        .show_ui(ui, |ui| {
                                            for (v, labels) in opt.choices {
                                                ui.selectable_value(
                                                    &mut value,
                                                    v.to_string(),
                                                    labels[lang],
                                                );
                                            }
                                        });
                                    if value != current {
                                        self.overlays[selected]
                                            .options
                                            .insert(opt.key.to_string(), value);
                                        changed = true;
                                    }
                                    if n % 2 == 1 {
                                        ui.end_row();
                                    }
                                }
                            });
                    },
                );
                if ui.input(|i| i.pointer.any_released()) {
                    changed = true;
                }
                let done_style = TriStyle {
                    base: theme::TEAL,
                    triangles: true,
                    seed: 23,
                    size: 15.0,
                    drift: true,
                    ..Default::default()
                };
                if widgets::tri_button(
                    ui,
                    Rect::from_min_size(pos2(744.0, 700.0), vec2(200.0, 56.0)),
                    &t_done,
                    None,
                    &done_style,
                    true,
                    time,
                )
                .clicked()
                {
                    close = true;
                }
                if loading {
                    ui.painter().text(
                        pos2(16.0, 728.0),
                        Align2::LEFT_CENTER,
                        &t_loading,
                        widgets::font(13.0),
                        theme::DIM,
                    );
                }
            });
        if let Some(ed) = self.editor.as_mut() {
            ed.selected = selected;
        }
        if changed {
            crate::settings::save_overlays(&self.overlays);
            self.request_editor_frame();
        }
        if close || ctx.input(|i| i.key_pressed(Key::Escape)) {
            crate::settings::save_overlays(&self.overlays);
            self.close_editor();
        }
        ctx.request_repaint();
    }

    fn draw_help(&mut self, ctx: &egui::Context) {
        if !self.show_help {
            return;
        }
        let es = self.s.lang == "es";
        let time = self.start_time.elapsed().as_secs_f64();
        let sections = help::sections(es);
        let title = self.t("help_title");
        let done = self.t("done");
        let install = self.t("ffmpeg_install");
        let missing = self.gpu_key == "ffmpeg_missing" && !self.installing_ffmpeg;
        let mut close = false;
        let mut start_install = false;
        egui::Area::new(egui::Id::new("help"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos2(0.0, 0.0))
            .show(ctx, |ui| {
                let screen = ctx.screen_rect();
                ui.allocate_rect(screen, Sense::click());
                ui.painter()
                    .rect_filled(screen, Rounding::ZERO, Color32::from_black_alpha(190));
                let card = Rect::from_center_size(screen.center(), vec2(820.0, 700.0));
                widgets::section(ui, card, &title);
                let body =
                    Rect::from_min_max(card.min + vec2(24.0, 44.0), card.max - vec2(24.0, 76.0));
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(body), |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("helpscroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for (head, text) in &sections {
                                ui.label(
                                    egui::RichText::new(*head).size(15.0).color(Color32::WHITE),
                                );
                                ui.add_space(2.0);
                                ui.label(egui::RichText::new(*text).size(12.5).color(theme::DIM));
                                ui.add_space(12.0);
                            }
                        });
                });
                if missing {
                    let style = TriStyle {
                        base: theme::BUTTON,
                        size: 13.0,
                        ..Default::default()
                    };
                    if widgets::tri_button(
                        ui,
                        Rect::from_min_size(
                            pos2(card.min.x + 24.0, card.max.y - 58.0),
                            vec2(180.0, 40.0),
                        ),
                        &install,
                        None,
                        &style,
                        true,
                        time,
                    )
                    .clicked()
                    {
                        start_install = true;
                    }
                }
                let style = TriStyle {
                    base: theme::TEAL,
                    size: 14.0,
                    ..Default::default()
                };
                if widgets::tri_button(
                    ui,
                    Rect::from_min_size(
                        pos2(card.max.x - 184.0, card.max.y - 58.0),
                        vec2(160.0, 40.0),
                    ),
                    &done,
                    None,
                    &style,
                    true,
                    time,
                )
                .clicked()
                {
                    close = true;
                }
            });
        if start_install {
            self.start_ffmpeg_install();
        }
        if close || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.show_help = false;
        }
    }

    fn draw_credits(&mut self, ctx: &egui::Context) {
        if !self.show_credits {
            return;
        }
        let es = self.s.lang == "es";
        let time = self.start_time.elapsed().as_secs_f64();
        let rows: Vec<(&str, String, String, &str)> = vec![
            (
                if es {
                    "Creador del programa"
                } else {
                    "Program creator"
                },
                "[LN]Kypers89".into(),
                "osu.ppy.sh/users/25858907".into(),
                "https://osu.ppy.sh/users/25858907",
            ),
            (
                if es {
                    "Overlays (diseño original)"
                } else {
                    "Overlays (original design)"
                },
                "2ky (@s2skky)".into(),
                "github.com/2222zz/osu-custom-overlay".into(),
                "https://github.com/2222zz/osu-custom-overlay",
            ),
            (
                "osukps",
                "yugecin".into(),
                if es {
                    "github.com/yugecin/osukps · licencia MIT".into()
                } else {
                    "github.com/yugecin/osukps · MIT licence".into()
                },
                "https://github.com/yugecin/osukps",
            ),
            (
                if es {
                    "Skin y sonidos por defecto"
                } else {
                    "Default skin and hitsounds"
                },
                "ppy Pty Ltd".into(),
                "github.com/ppy/osu-resources · CC BY-NC 4.0".into(),
                "https://github.com/ppy/osu-resources",
            ),
            (
                if es { "Fuentes" } else { "Fonts" },
                "Ubuntu (Canonical) · Noto Sans (Google)".into(),
                "Ubuntu Font Licence · SIL OFL 1.1".into(),
                "https://design.ubuntu.com/font",
            ),
        ];
        let note = if es {
            "Este programa no está afiliado ni respaldado por ppy. \"osu!\" es una marca de ppy Pty Ltd. Los overlays se recrearon desde cero, sin copiar código, imágenes ni fuentes."
        } else {
            "This program is not affiliated with or endorsed by ppy. \"osu!\" is a trademark of ppy Pty Ltd. The overlays were re-created from scratch, without copying code, images or fonts."
        };
        let title = self.t("credits_btn");
        let done = self.t("done");
        let mut close = false;
        egui::Area::new(egui::Id::new("credits"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos2(0.0, 0.0))
            .show(ctx, |ui| {
                let screen = ctx.screen_rect();
                ui.allocate_rect(screen, Sense::click());
                ui.painter()
                    .rect_filled(screen, Rounding::ZERO, Color32::from_black_alpha(190));
                let card = Rect::from_center_size(screen.center(), vec2(620.0, 580.0));
                widgets::section(ui, card, &title);
                if let Some(logo) = &self.logo {
                    widgets::logo(
                        ui,
                        Rect::from_min_size(card.min + vec2(24.0, 44.0), vec2(64.0, 64.0)),
                        logo,
                    );
                }
                ui.painter().text(
                    card.min + vec2(104.0, 62.0),
                    Align2::LEFT_CENTER,
                    APP_NAME,
                    widgets::font(22.0),
                    Color32::WHITE,
                );
                ui.painter().text(
                    card.min + vec2(105.0, 90.0),
                    Align2::LEFT_CENTER,
                    "osu!mania replay renderer",
                    widgets::font(12.5),
                    theme::DIM,
                );
                let mut y = card.min.y + 128.0;
                for (n, (role, who, extra, link)) in rows.iter().enumerate() {
                    ui.painter().text(
                        pos2(card.min.x + 24.0, y),
                        Align2::LEFT_TOP,
                        *role,
                        widgets::font(12.0),
                        theme::DIM,
                    );
                    ui.painter().text(
                        pos2(card.min.x + 24.0, y + 17.0),
                        Align2::LEFT_TOP,
                        who,
                        widgets::font(15.0),
                        Color32::WHITE,
                    );
                    let galley = ui.painter().layout_no_wrap(
                        extra.clone(),
                        widgets::font(12.0),
                        theme::GOLD,
                    );
                    let area =
                        Rect::from_min_size(pos2(card.min.x + 24.0, y + 37.0), galley.size());
                    let resp = ui
                        .interact(area, egui::Id::new(("link", n)), Sense::click())
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    let color = if resp.hovered() {
                        Color32::WHITE
                    } else {
                        theme::GOLD
                    };
                    ui.painter().text(
                        area.min,
                        Align2::LEFT_TOP,
                        extra,
                        widgets::font(12.0),
                        color,
                    );
                    if resp.hovered() {
                        ui.painter().line_segment(
                            [area.left_bottom(), area.right_bottom()],
                            Stroke::new(1.0_f32, color),
                        );
                    }
                    if resp.clicked() {
                        open_url(link);
                    }
                    y += 62.0;
                }
                let galley = ui.painter().layout(
                    note.to_string(),
                    widgets::font(11.5),
                    theme::DIM,
                    card.width() - 48.0,
                );
                ui.painter().galley(
                    pos2(card.min.x + 24.0, card.max.y - 104.0),
                    galley,
                    theme::DIM,
                );
                let style = TriStyle {
                    base: theme::TEAL,
                    triangles: true,
                    seed: 31,
                    size: 14.0,
                    drift: true,
                    ..Default::default()
                };
                if widgets::tri_button(
                    ui,
                    Rect::from_min_size(
                        pos2(card.max.x - 184.0, card.max.y - 60.0),
                        vec2(160.0, 42.0),
                    ),
                    &done,
                    None,
                    &style,
                    true,
                    time,
                )
                .clicked()
                {
                    close = true;
                }
            });
        if close || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.show_credits = false;
        }
    }

    fn draw_welcome(&mut self, ctx: &egui::Context) {
        if !self.show_welcome {
            return;
        }
        let mut choose = false;
        let mut later = false;
        egui::Area::new(egui::Id::new("welcome"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos2(0.0, 0.0))
            .show(ctx, |ui| {
                let screen = ctx.screen_rect();
                ui.allocate_rect(screen, Sense::click());
                ui.painter()
                    .rect_filled(screen, Rounding::ZERO, Color32::from_black_alpha(170));
                let card = Rect::from_center_size(screen.center(), vec2(480.0, 200.0));
                widgets::section(ui, card, &self.t("welcome_title"));
                let galley = ui.painter().layout(
                    self.t("welcome_text"),
                    widgets::font(14.0),
                    theme::TEXT,
                    440.0,
                );
                ui.painter()
                    .galley(card.min + vec2(20.0, 44.0), galley, theme::TEXT);
                let time = self.start_time.elapsed().as_secs_f64();
                let pick = TriStyle {
                    base: theme::TEAL,
                    triangles: true,
                    seed: 9,
                    size: 14.0,
                    drift: true,
                    ..Default::default()
                };
                if widgets::tri_button(
                    ui,
                    Rect::from_min_size(card.min + vec2(20.0, 130.0), vec2(300.0, 48.0)),
                    &self.t("welcome_btn"),
                    None,
                    &pick,
                    true,
                    time,
                )
                .clicked()
                {
                    choose = true;
                }
                if widgets::tri_button(
                    ui,
                    Rect::from_min_size(card.min + vec2(328.0, 130.0), vec2(132.0, 48.0)),
                    &self.t("welcome_later"),
                    None,
                    &TriStyle::default(),
                    true,
                    time,
                )
                .clicked()
                {
                    later = true;
                }
            });
        if choose {
            if let Some(dir) = rfd::FileDialog::new()
                .set_title(self.t("pick_osu"))
                .pick_folder()
            {
                if self.set_osu_folder(&dir) {
                    self.show_welcome = false;
                }
            }
        }
        if later {
            self.show_welcome = false;
        }
    }
}

fn open_url(url: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .creation_flags(0x0800_0000)
            .spawn();
    }
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
}

fn open_folder(dir: &Path) {
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(dir).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(dir).spawn();
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let t0 = std::time::Instant::now();
        self.update_inner(ctx, frame);
        if std::env::var("MR_TEST_FPS").is_ok() {
            self.cpu_ms += t0.elapsed().as_secs_f64() * 1000.0;
        }
    }
}

impl App {
    fn update_inner(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.retired.clear();
        self.handle_events(ctx);
        self.capture_keys(ctx);
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if !dropped.is_empty() && !self.busy() {
            self.add_paths(dropped);
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            if self.busy() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.confirm_exit = true;
            } else {
                self.s.skin = self.selected_skin();
                self.s.save();
                crate::settings::save_overlays(&self.overlays);
            }
        }
        let lite = self.slow_renderer;
        if lite != self.lite_applied {
            self.lite_applied = lite;
            ctx.tessellation_options_mut(|t| t.feathering = !lite);
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("ui_lite"), lite));
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| self.draw(ui));
        self.draw_preview(ctx);
        self.draw_help(ctx);
        self.draw_credits(ctx);
        self.draw_clip(ctx);
        self.draw_editor(ctx);
        self.draw_welcome(ctx);
        if self.confirm_exit {
            self.confirm_exit = false;
            let answer = rfd::MessageDialog::new()
                .set_title(APP_NAME)
                .set_description(self.t("confirm_exit"))
                .set_buttons(rfd::MessageButtons::YesNo)
                .show();
            if answer == rfd::MessageDialogResult::Yes {
                self.cancel.store(true, Ordering::Relaxed);
                self.s.save();
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if let Ok(secs) = std::env::var("MR_TEST_IDLE")
            .unwrap_or_default()
            .parse::<f64>()
        {
            self.frames += 1;
            let start = *self.bench_start.get_or_insert_with(std::time::Instant::now);
            if start.elapsed().as_secs_f64() > secs {
                println!(
                    "{} redibujos en {:.0} s en reposo (lento={})",
                    self.frames, secs, self.slow_renderer
                );
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if let Ok(n) = std::env::var("MR_TEST_FPS")
            .unwrap_or_default()
            .parse::<u32>()
        {
            self.frames += 1;
            if self.frames == 30 {
                self.bench_start = Some(std::time::Instant::now());
            }
            if self.frames == 30 + n {
                let el = self.bench_start.unwrap().elapsed();
                println!("{} cuadros de interfaz en {:.2}s = {:.1} fps ({:.1} ms por cuadro, {:.1} ms de logica)", n, el.as_secs_f64(), n as f64 / el.as_secs_f64(), el.as_secs_f64() * 1000.0 / n as f64, self.cpu_ms / (30 + n) as f64);
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint();
            return;
        }
        if let Ok(path) = std::env::var("MR_TEST_SHOT") {
            self.frames += 1;
            if self.frames == 20 {
                if std::env::var("MR_TEST_EDITOR").is_ok() {
                    self.open_editor();
                } else if std::env::var("MR_TEST_CLIP").is_ok() {
                    self.start_preview();
                } else if std::env::var("MR_TEST_EXPORT").is_ok() {
                    self.start_export();
                } else if std::env::var("MR_TEST_PREVIEW").is_ok() {
                    self.start_preview();
                } else {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
                }
            }
            if self.frames > 20
                && self.preview.is_some()
                && std::env::var("MR_TEST_PREVIEW").is_ok()
                && self.frames < 100000
            {
                self.frames = 100000;
            }
            if self.frames > 20
                && self.frames < 100000
                && std::env::var("MR_TEST_EXPORT").is_ok()
                && !self.busy()
            {
                self.frames = 100000;
            }
            if self.frames > 20
                && self.frames < 100000
                && std::env::var("MR_TEST_EDITOR").is_ok()
                && self
                    .editor
                    .as_ref()
                    .map(|e| e.tex.is_some())
                    .unwrap_or(false)
            {
                if let Some(o) = self.overlays.iter_mut().find(|o| o.id == "nowplaying") {
                    o.enabled = true;
                }
                if let Some(ed) = self.editor.as_mut() {
                    ed.selected = 1;
                    if ed.dirty || ed.rendering {
                    } else if self.frames < 99000 {
                        self.frames = 99000;
                        self.request_editor_frame();
                    }
                }
            }
            if self.frames > 99000
                && self.frames < 100000
                && self.editor.as_ref().map(|e| !e.rendering).unwrap_or(false)
            {
                self.frames = 100000;
            }
            if self.frames > 20
                && self.frames < 100000
                && std::env::var("MR_TEST_CLIP").is_ok()
                && self
                    .clip
                    .as_ref()
                    .map(|c| c.position() > 2.5)
                    .unwrap_or(false)
            {
                if std::env::var("MR_TEST_CLOSE").is_ok() {
                    self.close_clip();
                    self.frames = 50000;
                } else {
                    self.frames = 100000;
                }
            }
            if self.frames > 50005 && self.frames < 50010 {
                self.frames = 100000;
            }
            if self.frames == 100003 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
            }
            let shot = ctx.input(|i| {
                i.events.iter().find_map(|e| {
                    if let egui::Event::Screenshot { image, .. } = e {
                        Some(image.clone())
                    } else {
                        None
                    }
                })
            });
            if let Some(image) = shot {
                let [w, h] = image.size;
                let bytes: Vec<u8> = image
                    .pixels
                    .iter()
                    .flat_map(|c| [c.r(), c.g(), c.b(), 255])
                    .collect();
                let _ =
                    image::save_buffer(&path, &bytes, w as u32, h as u32, image::ColorType::Rgba8);
                self.allow_close = true;
                crate::settings::save_overlays(&self.overlays);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint();
            return;
        }
        let dt = ctx.input(|i| i.unstable_dt);
        if self.continuous_frames > 10 && dt > 0.0 {
            self.dt_avg = self.dt_avg * 0.9 + dt as f64 * 0.1;
            if self.dt_avg > 0.055 {
                self.slow_renderer = true;
            }
        }
        if self.busy() {
            self.continuous_frames = 0;
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        } else if !self.slow_renderer {
            self.continuous_frames += 1;
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
        }
        if std::env::var("MR_TEST_IDLE").is_ok() {
            ctx.request_repaint_after(std::time::Duration::from_millis(if self.slow_renderer {
                1000
            } else {
                33
            }));
        }
    }
}
