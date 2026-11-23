use crate::job::{self, JobError, JobSettings, Progress, Source};
use crate::video::{self, Encoder, ExportOptions};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

#[derive(Clone)]
pub struct Item {
    pub name: String,
    pub source: Source,
}

impl Item {
    pub fn key(&self) -> String {
        match &self.source {
            Source::Replay(p) => p.to_string_lossy().into_owned(),
            Source::Map(s) => s.clone(),
        }
    }
    pub fn output_name(&self) -> String {
        match &self.source {
            Source::Replay(p) => p
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "video".into()),
            Source::Map(_) => format!("{} (Auto)", self.name),
        }
    }
}

pub enum Event {
    Started {
        index: usize,
        total: usize,
        name: String,
    },
    Searching {
        count: Option<usize>,
        percent: usize,
    },
    MapMismatch,
    Extracting,
    MapFound {
        title: String,
        stars: f64,
    },
    Progress {
        done: u64,
        total: u64,
        fps: f64,
    },
    Saved(PathBuf),
    Failed {
        key: &'static str,
        detail: String,
    },
    Finished {
        ok: usize,
        total: usize,
    },
    Cancelled,
    Preview(Option<(u32, u32, Vec<u8>)>),
    ClipProgress(usize),
    Clip {
        frames: Vec<Vec<u8>>,
        width: u32,
        height: u32,
        fps: u32,
        audio: Vec<u8>,
    },
    EditorScene(Arc<crate::render::Scene>, f64),
    EditorFrame(u32, u32, Vec<u8>),
    FfmpegProgress(i32),
    FfmpegDone(Result<(), String>),
}

pub struct ExportJob {
    pub items: Vec<Item>,
    pub settings: JobSettings,
    pub out_dir: PathBuf,
    pub encoder: Encoder,
    pub frame_samples: u32,
    pub profile: u8,
}

fn classify(err: &anyhow::Error, source: &Source) -> (&'static str, String) {
    if let Some(j) = err.downcast_ref::<JobError>() {
        return match j {
            JobError::NotInstalled => ("err_notfound", String::new()),
            JobError::Converted(name) => ("err_converted", name.clone()),
            JobError::MapPathMismatch => ("log_mismatch", String::new()),
        };
    }
    let text = format!("{err:#}");
    if text.contains("Songs folder") {
        return ("err_songs", text);
    }
    if matches!(source, Source::Replay(_))
        && (text.contains("replay") || text.contains("decompress"))
    {
        return ("err_replay", text);
    }
    if text.contains("map") && !text.contains("ffmpeg") {
        return ("err_nomap", text);
    }
    ("err_export", text)
}

fn progress_sender(tx: &Sender<Event>) -> impl Fn(Progress) + Sync + '_ {
    let tx = std::sync::Mutex::new(tx.clone());
    move |p| {
        let event = match p {
            Progress::Searching(pct) => Event::Searching {
                count: None,
                percent: pct,
            },
            Progress::MapFound { title, stars } => Event::MapFound { title, stars },
            Progress::MapPathMismatch => Event::MapMismatch,
            Progress::Extracting => Event::Extracting,
        };
        let _ = tx.lock().unwrap().send(event);
    }
}

pub fn run_export(job: ExportJob, cancel: Arc<AtomicBool>, tx: Sender<Event>) {
    std::thread::spawn(move || {
        let total = job.items.len();
        let mut ok = 0;
        for (index, item) in job.items.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let _ = tx.send(Event::Started {
                index,
                total,
                name: item.output_name(),
            });
            let report = progress_sender(&tx);
            let scene = match job::prepare(&item.source, &job.settings, &report) {
                Ok(s) => s,
                Err(e) => {
                    let (key, detail) = classify(&e, &item.source);
                    let _ = tx.send(Event::Failed { key, detail });
                    continue;
                }
            };
            let _ = std::fs::create_dir_all(&job.out_dir);
            let base = item.output_name();
            let mut out = job.out_dir.join(format!("{base}.mp4"));
            let mut copy = 2;
            while out.exists() {
                out = job.out_dir.join(format!("{base} ({copy}).mp4"));
                copy += 1;
            }
            let mods = match &item.source {
                Source::Replay(p) => crate::osu::replay::parse_file(p)
                    .map(|r| r.mods)
                    .unwrap_or(0),
                Source::Map(_) => 0,
            };
            let opts = ExportOptions {
                encoder: job.encoder,
                frame_samples: job.frame_samples,
                mods,
                skin: job.settings.skin.clone(),
                profile: job.profile,
            };
            let ptx = std::sync::Mutex::new(tx.clone());
            let result = video::export(&scene, &out, &opts, &cancel, &|p| {
                let _ = ptx.lock().unwrap().send(Event::Progress {
                    done: p.done,
                    total: p.total,
                    fps: p.fps,
                });
            });
            match result {
                Ok(()) => {
                    ok += 1;
                    let _ = tx.send(Event::Saved(out));
                }
                Err(_) if cancel.load(Ordering::Relaxed) => break,
                Err(e) => {
                    let (key, detail) = classify(&e, &item.source);
                    let _ = tx.send(Event::Failed { key, detail });
                }
            }
        }
        if cancel.load(Ordering::Relaxed) {
            let _ = tx.send(Event::Cancelled);
        } else {
            let _ = tx.send(Event::Finished { ok, total });
        }
    });
}

pub fn run_preview(item: Item, settings: JobSettings, tx: Sender<Event>) {
    std::thread::spawn(move || {
        let report = progress_sender(&tx);
        match job::prepare(&item.source, &settings, &report) {
            Ok(scene) => {
                let times: Vec<i32> = scene.map.notes.iter().map(|n| n.time).collect();
                let t = times.get(times.len() / 2).copied().unwrap_or(60000) as f64 + 40.0;
                let video = crate::video::video_frame_at(&scene, t);
                let frame = scene.render_with_video(t, video.as_deref());
                let (w, h) = (frame.width(), frame.height());
                let _ = tx.send(Event::Preview(Some((w, h, frame.take()))));
            }
            Err(e) => {
                let (key, detail) = classify(&e, &item.source);
                let _ = tx.send(Event::Failed { key, detail });
                let _ = tx.send(Event::Preview(None));
            }
        }
    });
}

pub fn detect_encoders(tx: Sender<Vec<(String, Encoder)>>) {
    std::thread::spawn(move || {
        let mut found = Vec::new();
        if video::ffmpeg_available() {
            for (name, enc) in [
                ("NVIDIA (nvenc)", Encoder::Nvenc),
                ("AMD (amf)", Encoder::Amf),
                ("Intel (qsv)", Encoder::Qsv),
            ] {
                if video::encoder_works(enc) {
                    found.push((name.to_string(), enc));
                }
            }
            let _ = tx.send(found);
        } else {
            let _ = tx.send(vec![("__noffmpeg__".into(), Encoder::X264)]);
        }
    });
}

fn middle_time(scene: &crate::render::Scene) -> f64 {
    let times: Vec<i32> = scene.map.notes.iter().map(|n| n.time).collect();
    times.get(times.len() / 2).copied().unwrap_or(60000) as f64 + 40.0
}

pub fn run_editor_scene(item: Item, settings: JobSettings, tx: Sender<Event>) {
    std::thread::spawn(move || {
        let report = progress_sender(&tx);
        match job::prepare(&item.source, &settings, &report) {
            Ok(mut scene) => {
                scene.no_fade = true;
                let t = middle_time(&scene);
                let _ = tx.send(Event::EditorScene(Arc::new(scene), t));
            }
            Err(e) => {
                let (key, detail) = classify(&e, &item.source);
                let _ = tx.send(Event::Failed { key, detail });
                let _ = tx.send(Event::Preview(None));
            }
        }
    });
}

pub fn render_editor_frame(
    scene: Arc<crate::render::Scene>,
    t: f64,
    overlays: Vec<crate::overlays::catalog::OverlayConfig>,
    tx: Sender<Event>,
) {
    std::thread::spawn(move || {
        let video = crate::video::video_frame_at(&scene, t);
        let frame = scene.render_full(t, video.as_deref(), &overlays);
        let (w, h) = (frame.width(), frame.height());
        let _ = tx.send(Event::EditorFrame(w, h, frame.take()));
    });
}

pub fn run_preview_clip(item: Item, settings: JobSettings, tx: Sender<Event>) {
    use rayon::prelude::*;
    std::thread::spawn(move || {
        let report = progress_sender(&tx);
        let mut scene = match job::prepare(&item.source, &settings, &report) {
            Ok(s) => s,
            Err(e) => {
                let (key, detail) = classify(&e, &item.source);
                let _ = tx.send(Event::Failed { key, detail });
                let _ = tx.send(Event::Preview(None));
                return;
            }
        };
        scene.no_fade = true;
        let mid = middle_time(&scene);
        scene.clip(mid - 5000.0 * scene.rate, mid + 5000.0 * scene.rate);
        let mods = match &item.source {
            Source::Replay(p) => crate::osu::replay::parse_file(p)
                .map(|r| r.mods)
                .unwrap_or(0),
            Source::Map(_) => 0,
        };
        let tmp = crate::job::data_dir().join("tmp");
        let _ = std::fs::create_dir_all(&tmp);
        let wav = tmp.join(format!("preview-{}.wav", std::process::id()));
        let audio = match crate::video::build_audio(&scene, mods, settings.skin.as_deref(), &wav) {
            Ok(()) => std::fs::read(&wav).unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        let _ = std::fs::remove_file(&wav);
        let total = scene.total_frames;
        let (w, h) = (scene.opts.width, scene.opts.height);
        let mut reader = if scene.has_layers() {
            crate::video::VideoReader::open(&scene)
        } else {
            None
        };
        let mut frames = Vec::with_capacity(total as usize);
        let batch = (rayon::current_num_threads() * 4) as u64;
        let mut next = 0u64;
        while next < total {
            let end = (next + batch).min(total);
            let videos: Vec<Option<Vec<u8>>> = (next..end)
                .map(|i| reader.as_mut().and_then(|r| r.frame_for(i)))
                .collect();
            let encoded: Vec<Vec<u8>> = (next..end)
                .into_par_iter()
                .map(|i| {
                    let frame = scene.render_with_video(
                        scene.time_of_frame(i as f64),
                        videos[(i - next) as usize].as_deref(),
                    );
                    let rgb: Vec<u8> = frame
                        .data()
                        .chunks_exact(4)
                        .flat_map(|p| [p[0], p[1], p[2]])
                        .collect();
                    let mut out = Vec::new();
                    let _ = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 88)
                        .encode(&rgb, w, h, image::ExtendedColorType::Rgb8);
                    out
                })
                .collect();
            frames.extend(encoded);
            next = end;
            let _ = tx.send(Event::ClipProgress((next * 100 / total.max(1)) as usize));
        }
        let _ = tx.send(Event::Clip {
            frames,
            width: w,
            height: h,
            fps: scene.opts.fps,
            audio,
        });
    });
}

pub fn run_ffmpeg_install(tx: Sender<Event>) {
    std::thread::spawn(move || {
        let progress_tx = std::sync::Mutex::new(tx.clone());
        let result = crate::ffmpeg_setup::install(&|stage| {
            let v = match stage {
                crate::ffmpeg_setup::Stage::Downloading(p) => p as i32,
                crate::ffmpeg_setup::Stage::Extracting => -1,
            };
            let _ = progress_tx.lock().unwrap().send(Event::FfmpegProgress(v));
        });
        let _ = tx.send(Event::FfmpegDone(
            result.map(|_| ()).map_err(|e| format!("{e:#}")),
        ));
    });
}
