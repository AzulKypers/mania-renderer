#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#![allow(dead_code)]
mod default_skin;
mod ffmpeg_setup;
mod game;
mod job;
mod osu;
mod overlays;
mod render;
mod settings;
mod storyboard;
mod ui;
mod video;
mod worker;

use std::path::{Path, PathBuf};

fn test_settings(args: &[String]) -> job::JobSettings {
    let osu_dir = args.get(0).map(PathBuf::from);
    let songs_dir = osu_dir.as_ref().map(|d| osu::library::songs_dir_for(d));
    let config = osu_dir
        .as_ref()
        .map(|d| osu::library::read_osu_config(d))
        .unwrap_or_default();
    job::JobSettings {
        osu_dir,
        songs_dir,
        map_path: None,
        skin: args.get(1).filter(|s| !s.is_empty()).map(PathBuf::from),
        skin_hd: std::env::var("NOHD").is_err(),
        results: std::env::var("NORESULTS").is_err(),
        width: std::env::var("W")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1920),
        height: std::env::var("H")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1080),
        fps: 60,
        scroll: config.mania_speed.unwrap_or(30) as f64,
        dim: std::env::var("DIM")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(config.dim_level.map(|d| d as f32 / 100.0).unwrap_or(0.8)),
        bpm_scale: config.bpm_scale,
        decimal_comma: false,
        storyboard: std::env::var("NOSB").is_err(),
        video: std::env::var("NOVIDEO").is_err(),
        overlays: {
            let mut o = overlays::catalog::defaults();
            if let Ok(opts) = std::env::var("OVOPTS") {
                for pair in opts.split(',') {
                    if let Some((id_key, value)) = pair.split_once('=') {
                        if let Some((id, key)) = id_key.split_once('.') {
                            for c in o.iter_mut().filter(|c| c.id == id) {
                                c.options.insert(key.to_string(), value.to_string());
                            }
                        }
                    }
                }
            }
            if let Ok(opts) = std::env::var("OVOPTS") {
                for pair in opts.split(',') {
                    if let Some((id_key, value)) = pair.split_once('=') {
                        if let Some((id, key)) = id_key.split_once('.') {
                            for c in o.iter_mut().filter(|c| c.id == id) {
                                c.options.insert(key.to_string(), value.to_string());
                            }
                        }
                    }
                }
            }
            if let Ok(list) = std::env::var("OVERLAYS") {
                for c in &mut o {
                    c.enabled = list.split(',').any(|id| id == c.id);
                }
            }
            o
        },
        hud: render::HudOptions {
            kps: true,
            panel: true,
            labels: std::env::var("KEYS")
                .map(|v| v.split_whitespace().map(String::from).collect())
                .unwrap_or_else(|_| ["A", "S", "K", "L", "D", "F", "J", "SPC", "G", "H"].iter().map(|s| s.to_string()).collect()),
            bar_color: [255, 255, 255, 255],
            score: std::env::var("NOSCORE").is_err(),
        },
    }
}

fn source_of(arg: &str) -> job::Source {
    if arg.to_lowercase().ends_with(".osr") {
        job::Source::Replay(PathBuf::from(arg))
    } else {
        job::Source::Map(arg.to_string())
    }
}

fn main() {
    std::panic::set_hook(Box::new(|info| {
        let dir = job::data_dir();
        let _ = std::fs::create_dir_all(&dir);
        let bt = std::backtrace::Backtrace::force_capture();
        let _ = std::fs::write(
            dir.join("crash.log"),
            format!(
                "{info}

{bt}"
            ),
        );
        eprintln!("{info}");
    }));
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("--export-icon") if args.len() >= 3 => {
            let rgba = ui::export_icon(256);
            let _ = image::save_buffer(&args[2], &rgba, 256, 256, image::ColorType::Rgba8);
        }
        Some("stars") => {
            for f in &args[2..] {
                if let Ok(map) = osu::beatmap::parse_file(Path::new(f)) {
                    println!("{:.2}\t{}", game::stars::star_rating(&map, 1.0), f);
                }
            }
        }
        Some("lndebug") if args.len() >= 4 => {
            let settings = test_settings(&args[3..]);
            let scene = job::prepare(&source_of(&args[2]), &settings, &|_| {}).expect("prepare");
            if let Ok(r) = osu::replay::parse_file(std::path::Path::new(&args[2])) {
                println!(
                    "osu!: 320={} 300={} 200={} 100={} 50={} miss={} combo={} score={} mods={}",
                    r.count_geki,
                    r.count_300,
                    r.count_katu,
                    r.count_100,
                    r.count_50,
                    r.count_miss,
                    r.max_combo,
                    r.score,
                    r.mods
                );
            }
            if let (Ok(a), Ok(b), Ok(c)) = (
                std::env::var("COL").unwrap_or_default().parse::<usize>(),
                std::env::var("FROM").unwrap_or_default().parse::<i32>(),
                std::env::var("TO").unwrap_or_default().parse::<i32>(),
            ) {
                for (i, n) in scene
                    .map
                    .notes
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| n.column == a && n.time >= b && n.time <= c)
                {
                    println!("  nota {} t={} fin={:?}", i, n.time, n.end_time);
                }
                for e in scene
                    .sim
                    .key_events
                    .iter()
                    .filter(|e| e.column == a && e.time >= b && e.time <= c)
                {
                    println!("  tecla t={} down={}", e.time, e.down);
                }
                for j in scene
                    .sim
                    .judgements
                    .iter()
                    .filter(|j| j.column == a && j.time >= b as f64 && j.time <= c as f64)
                {
                    println!(
                        "  juicio t={:.0} {:?} off={:?} nota={}",
                        j.time, j.kind, j.offset, j.note
                    );
                }
            }
            let f = scene.sim.final_snapshot();
            println!(
                "nos:  {:?} combo={} score={:.0}",
                f.counts, f.max_combo, f.score
            );
            for j in &scene.sim.judgements {
                let st = &scene.sim.notes[j.note];
                let n = &scene.map.notes[j.note];
                if j.kind == game::Judgement::Miss || st.broken {
                    println!(
                        "t={:.0} col={} nota={}..{:?} {:?} head={:?} rel={:?} broken={}",
                        j.time,
                        j.column,
                        n.time,
                        n.end_time,
                        j.kind,
                        st.head_hit,
                        st.released,
                        st.broken
                    );
                }
            }
        }
        Some("scrolldebug") if args.len() >= 4 => {
            let settings = test_settings(&args[3..]);
            let scene = job::prepare(&source_of(&args[2]), &settings, &|_| {}).expect("prepare");
            let mut prev: Option<f64> = None;
            let mut prev_delta: Option<f64> = None;
            let mut jumps = 0;
            for i in 0..scene.total_frames {
                let t = scene.time_of_frame(i as f64);
                let d = scene.scroll_distance(t);
                if let Some(p) = prev {
                    let delta = d - p;
                    if let Some(pd) = prev_delta {
                        if (delta - pd).abs() > pd.abs() * 0.02 + 0.01 {
                            jumps += 1;
                            if jumps <= 40 {
                                println!("frame {i} t={t:.1} delta {pd:.3} -> {delta:.3}");
                            }
                        }
                    }
                    prev_delta = Some(delta);
                }
                prev = Some(d);
            }
            println!("cambios de velocidad: {jumps} de {} cuadros", scene.total_frames);
        }
        Some("findscore") if args.len() >= 5 => {
            let settings = test_settings(&args[4..]);
            let scene = job::prepare(&source_of(&args[2]), &settings, &|_| {}).expect("prepare");
            let target: u64 = args[3].parse().unwrap_or(0);
            for w in scene.sim.snapshots.windows(2) {
                if (w[0].score.round() as i64 - target as i64).abs() < 150 {
                    println!(
                        "desde {:.1} hasta {:.1} combo {}",
                        w[0].time, w[1].time, w[0].combo
                    );
                }
            }
        }
        Some("frames") if args.len() >= 7 => {
            let settings = test_settings(&args[6..]);
            let scene = job::prepare(&source_of(&args[2]), &settings, &|_| {}).expect("prepare");
            let first: f64 = args[4].parse().unwrap_or(0.0);
            let count: u32 = args[5].parse().unwrap_or(30);
            let base =
                ((first - scene.start) / scene.rate / (1000.0 / scene.opts.fps as f64)).round();
            for k in 0..count {
                let t = scene.time_of_frame(base + k as f64);
                scene
                    .render(t)
                    .save_png(format!("{}/f{:03}.png", args[3], k))
                    .unwrap();
            }
        }
        Some("skininfo") if args.len() >= 4 => {
            let files = osu::skin::SkinFiles::scan(Some(Path::new(&args[2])));
            for name in [
                "mania/judgments/extraordinary",
                "combo-0",
                "score-0",
                "mania-note1",
            ] {
                println!("{name}: {:?}", files.image(name));
            }
            let set = render::skin::build(&files, args[3].parse().unwrap_or(4), 1920, 1080);
            println!(
                "judgements {:?}",
                set.judgements.iter().map(|v| v.len()).collect::<Vec<_>>()
            );
            println!(
                "combo {} score {}",
                set.combo_digits.is_some(),
                set.score_digits.is_some()
            );
        }
        Some("export") if args.len() >= 5 => {
            let settings = test_settings(&args[4..]);
            let mut scene = match job::prepare(&source_of(&args[2]), &settings, &|_| {}) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("err: {e}");
                    std::process::exit(1);
                }
            };
            if let (Ok(from), Ok(to)) = (
                std::env::var("FROM").unwrap_or_default().parse::<f64>(),
                std::env::var("TO").unwrap_or_default().parse::<f64>(),
            ) {
                scene.clip(from, to);
            }
            let mods = if args[2].to_lowercase().ends_with(".osr") {
                osu::replay::parse_file(Path::new(&args[2]))
                    .map(|r| r.mods)
                    .unwrap_or(0)
            } else {
                0
            };
            let samples = std::env::var("SAMPLES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1);
            let opts = video::ExportOptions {
                encoder: video::Encoder::X264,
                frame_samples: samples,
                mods,
                skin: settings.skin.clone(),
                profile: std::env::var("PROFILE")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(2),
            };
            let cancel = std::sync::atomic::AtomicBool::new(false);
            let start = std::time::Instant::now();
            let r = video::export(&scene, Path::new(&args[3]), &opts, &cancel, &|p| {
                if p.done % 600 < 40 || p.done == p.total {
                    println!("{} / {} cuadros  {:.1} fps", p.done, p.total, p.fps);
                }
            });
            match r {
                Ok(()) => println!("ok en {:.1}s", start.elapsed().as_secs_f64()),
                Err(e) => println!("err: {e}"),
            }
        }
        Some("frame") if args.len() >= 6 => {
            let settings = test_settings(&args[5..]);
            let scene = match job::prepare(&source_of(&args[2]), &settings, &|_| {}) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("err: {e}");
                    std::process::exit(1);
                }
            };
            let t: f64 = args[4].parse().unwrap_or(30000.0);
            if let Ok(n) = std::env::var("BENCH").unwrap_or_default().parse::<u32>() {
                let start = std::time::Instant::now();
                for i in 0..n {
                    let _ = scene.render(t + i as f64 * 16.667);
                }
                println!("promedio {:?} por cuadro", start.elapsed() / n);
            }
            let start = std::time::Instant::now();
            let frame = scene.render(t);
            println!(
                "frame in {:?} (video {:.0}..{:.0})",
                start.elapsed(),
                scene.start,
                scene.end
            );
            frame.save_png(&args[3]).unwrap();
        }
        _ => {
            if let Err(e) = ui::run() {
                eprintln!("err: {e}");
            }
        }
    }
}
