use crate::game::Judgement;
use crate::osu::beatmap::find_case_insensitive;
use crate::osu::replay::{MOD_DOUBLE_TIME, MOD_HALF_TIME, MOD_NIGHTCORE};
use crate::osu::skin::SkinFiles;
use crate::render::Scene;
use anyhow::{bail, Context, Result};
use rayon::prelude::*;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

const RATE: u32 = 44100;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Encoder {
    X264,
    Nvenc,
    Amf,
    Qsv,
    OpenH264,
    Mpeg4,
}

impl Encoder {
    pub fn codec(self) -> &'static str {
        match self {
            Encoder::X264 => "libx264",
            Encoder::Nvenc => "h264_nvenc",
            Encoder::Amf => "h264_amf",
            Encoder::Qsv => "h264_qsv",
            Encoder::OpenH264 => "libopenh264",
            Encoder::Mpeg4 => "mpeg4",
        }
    }
}

pub struct ExportOptions {
    pub encoder: Encoder,
    pub frame_samples: u32,
    pub mods: u32,
    pub skin: Option<PathBuf>,
    pub profile: u8,
}

pub fn profile_limits(profile: u8) -> (usize, u64, usize) {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    match profile {
        0 => (cores.min(2), cores.min(2) as u64, 1),
        1 => {
            let t = (cores * 3 / 4).max(2).min(cores);
            (t, (t * 2) as u64, 1)
        }
        _ => (cores, (cores * 3).max(4) as u64, 2),
    }
}

pub fn ffmpeg_path() -> std::ffi::OsString {
    let local = crate::ffmpeg_setup::local_binary();
    if local.is_file() {
        local.into_os_string()
    } else {
        "ffmpeg".into()
    }
}

pub fn ffmpeg_major() -> u32 {
    let out = ffmpeg_command()
        .arg("-version")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    let text = out
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let rest = text.split("version ").nth(1).unwrap_or("");
    let digits: String = rest
        .trim_start_matches('n')
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().unwrap_or(99)
}

pub fn usable_encoder(wanted: Encoder) -> Encoder {
    if wanted != Encoder::X264 || encoder_works(Encoder::X264) {
        return wanted;
    }
    if encoder_works(Encoder::OpenH264) {
        Encoder::OpenH264
    } else {
        Encoder::Mpeg4
    }
}

pub fn ffmpeg_command() -> Command {
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut cmd = Command::new(ffmpeg_path());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

pub fn ffmpeg_available() -> bool {
    ffmpeg_command()
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn encoder_works(encoder: Encoder) -> bool {
    ffmpeg_command()
        .args([
            "-hide_banner",
            "-v",
            "quiet",
            "-f",
            "lavfi",
            "-i",
            "color=black:s=1280x720:d=0.1:r=30",
            "-pix_fmt",
            "nv12",
            "-c:v",
            encoder.codec(),
            "-f",
            "null",
            "-",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn decode_audio(path: &Path, filter: Option<&str>) -> Result<Vec<f32>> {
    let mut cmd = ffmpeg_command();
    cmd.args(["-v", "error", "-i"]).arg(path);
    if let Some(f) = filter {
        cmd.args(["-af", f]);
    }
    let output = cmd
        .args(["-f", "f32le", "-ac", "2", "-ar", &RATE.to_string(), "-"])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .context("ffmpeg could not be started")?;
    if !output.status.success() {
        bail!(
            "ffmpeg could not read {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output
        .stdout
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect())
}

fn sample_set_name(set: u8) -> &'static str {
    match set {
        2 => "soft",
        3 => "drum",
        _ => "normal",
    }
}

fn hitsound_files(
    scene: &Scene,
    note_index: usize,
    skin: &SkinFiles,
    fallback: &SkinFiles,
) -> Vec<(PathBuf, f32)> {
    let map_k = scene.opts.map_hs_volume;
    let skin_k = scene.opts.skin_hs_volume;
    let map = &scene.map;
    let note = &map.notes[note_index];
    let point = map.timing_at(note.time as f64);
    let point_set = point
        .map(|p| p.sample_set)
        .filter(|s| *s != 0)
        .unwrap_or(map.sample_set);
    let normal_set = if note.normal_set != 0 {
        note.normal_set
    } else {
        point_set
    };
    let addition_set = if note.addition_set != 0 {
        note.addition_set
    } else {
        normal_set
    };
    let index = if note.index != 0 {
        note.index
    } else {
        point.map(|p| p.sample_index).unwrap_or(0)
    };
    let volume = if note.volume != 0 {
        note.volume
    } else {
        point.map(|p| p.volume).unwrap_or(100)
    } as f32
        / 100.0;
    let mut out = Vec::new();
    if !note.filename.is_empty() {
        let p = find_case_insensitive(&map.folder, &note.filename);
        if p.is_file() {
            out.push((p, volume * map_k));
            return out;
        }
    }
    let mut names = vec![format!("{}-hitnormal", sample_set_name(normal_set))];
    for (bit, name) in [(2u8, "whistle"), (4, "finish"), (8, "clap")] {
        if note.hit_sound & bit != 0 {
            names.push(format!("{}-hit{}", sample_set_name(addition_set), name));
        }
    }
    for name in names {
        let mut found = None;
        let mut from_map = false;
        if index > 0 {
            let custom = if index == 1 {
                name.clone()
            } else {
                format!("{name}{index}")
            };
            for ext in ["wav", "ogg", "mp3"] {
                let p = find_case_insensitive(&map.folder, &format!("{custom}.{ext}"));
                if p.is_file() {
                    found = Some(p);
                    from_map = true;
                    break;
                }
            }
        }
        if found.is_none() {
            found = skin.sound(&name);
        }
        if found.is_none() {
            found = fallback.sound(&name);
        }
        if let Some(p) = found {
            out.push((p, volume * if from_map { map_k } else { skin_k }));
        }
    }
    out
}

pub fn build_audio(
    scene: &Scene,
    mods: u32,
    skin_dir: Option<&Path>,
    out_wav: &Path,
) -> Result<()> {
    let total = (scene.output_seconds() * RATE as f64).ceil() as usize * 2;
    let mut mix = vec![0.0f32; total];
    let filter = if mods & MOD_NIGHTCORE != 0 {
        Some(format!("asetrate={},aresample={}", RATE as f64 * 1.5, RATE))
    } else if mods & MOD_DOUBLE_TIME != 0 {
        Some("atempo=1.5".to_string())
    } else if mods & MOD_HALF_TIME != 0 {
        Some("atempo=0.75".to_string())
    } else {
        None
    };
    let audio_path = scene.map.audio_path();
    if audio_path.is_file() {
        let music = decode_audio(&audio_path, filter.as_deref())?;
        let shift = (-scene.start / scene.rate / 1000.0 * RATE as f64).round() as i64 * 2;
        for (i, out) in mix.iter_mut().enumerate() {
            let src = i as i64 - shift;
            if src >= 0 && (src as usize) < music.len() {
                *out += music[src as usize] * 0.8;
            }
        }
    }
    let default_dir = crate::default_skin::path();
    let skin = SkinFiles::scan(Some(skin_dir.unwrap_or(&default_dir)));
    let fallback = SkinFiles::scan(Some(&default_dir));
    let mut cache: HashMap<PathBuf, Option<Vec<f32>>> = HashMap::new();
    for j in &scene.sim.judgements {
        if j.kind == Judgement::Miss {
            continue;
        }
        let at = scene.sim.notes[j.note].head_hit.unwrap_or(j.time);
        let start = (((at - scene.start) / scene.rate / 1000.0) * RATE as f64).round() as i64 * 2;
        for (path, volume) in hitsound_files(scene, j.note, &skin, &fallback) {
            let entry = cache
                .entry(path.clone())
                .or_insert_with(|| decode_audio(&path, None).ok());
            let Some(samples) = entry else { continue };
            for (k, v) in samples.iter().enumerate() {
                let idx = start + k as i64;
                if idx >= 0 && (idx as usize) < mix.len() {
                    mix[idx as usize] += v * volume * 0.6;
                }
            }
        }
    }
    for sample in scene.storyboard_samples() {
        let start =
            (((sample.time - scene.start) / scene.rate / 1000.0) * RATE as f64).round() as i64 * 2;
        let entry = cache
            .entry(sample.path.clone())
            .or_insert_with(|| decode_audio(&sample.path, None).ok());
        let Some(samples) = entry else { continue };
        for (k, v) in samples.iter().enumerate() {
            let idx = start + k as i64;
            if idx >= 0 && (idx as usize) < mix.len() {
                mix[idx as usize] += v * sample.volume * 0.6;
            }
        }
    }
    let peak = mix.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let gain = if peak > 0.98 { 0.98 / peak } else { 1.0 };
    let mut bytes = Vec::with_capacity(44 + mix.len() * 2);
    let data_len = (mix.len() * 2) as u32;
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&RATE.to_le_bytes());
    bytes.extend_from_slice(&(RATE * 4).to_le_bytes());
    bytes.extend_from_slice(&4u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for v in mix {
        bytes.extend_from_slice(&(((v * gain).clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(out_wav, bytes)?;
    Ok(())
}

pub struct VideoReader {
    child: std::process::Child,
    stdout: std::process::ChildStdout,
    first_index: u64,
    frame_bytes: usize,
    done: bool,
}

impl VideoReader {
    pub fn open(scene: &Scene) -> Option<VideoReader> {
        let (path, offset) = scene.video_file()?;
        let (w, h, fps) = (scene.opts.width, scene.opts.height, scene.opts.fps);
        let frame_ms = 1000.0 / fps as f64 * scene.rate;
        let lead = offset - scene.start;
        let (seek, first_index) = if lead > 0.0 {
            (0.0, (lead / frame_ms).ceil() as u64)
        } else {
            (-lead / 1000.0, 0)
        };
        let video_at_first = scene.time_of_frame(first_index as f64) - offset;
        let seek = if lead > 0.0 {
            (video_at_first / 1000.0).max(0.0)
        } else {
            seek
        };
        let filter = format!("setpts=(PTS-STARTPTS)/{rate},fps={fps},scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h},format=rgba", rate = scene.rate);
        let mut cmd = ffmpeg_command();
        cmd.args(["-v", "error", "-ss", &format!("{seek:.3}"), "-i"])
            .arg(&path);
        cmd.args([
            "-an", "-vf", &filter, "-f", "rawvideo", "-pix_fmt", "rgba", "-",
        ]);
        let mut child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let stdout = child.stdout.take()?;
        Some(VideoReader {
            child,
            stdout,
            first_index,
            frame_bytes: (w * h * 4) as usize,
            done: false,
        })
    }

    pub fn frame_for(&mut self, index: u64) -> Option<Vec<u8>> {
        if index < self.first_index || self.done {
            return None;
        }
        let mut buf = vec![0u8; self.frame_bytes];
        if self.stdout.read_exact(&mut buf).is_err() {
            self.done = true;
            return None;
        }
        Some(buf)
    }
}

impl Drop for VideoReader {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn video_frame_at(scene: &Scene, map_time: f64) -> Option<Vec<u8>> {
    let (path, offset) = scene.video_file()?;
    let (w, h) = (scene.opts.width, scene.opts.height);
    let at = (map_time - offset) / 1000.0;
    if at < 0.0 {
        return None;
    }
    let filter =
        format!("scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h},format=rgba");
    let output = ffmpeg_command()
        .args(["-v", "error", "-ss", &format!("{at:.3}"), "-i"])
        .arg(&path)
        .args([
            "-an",
            "-frames:v",
            "1",
            "-vf",
            &filter,
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    (output.stdout.len() == (w * h * 4) as usize).then_some(output.stdout)
}

fn render_frame(scene: &Scene, index: u64, samples: u32, video: Option<&[u8]>) -> Vec<u8> {
    if samples <= 1 {
        return scene
            .render_with_video(scene.time_of_frame(index as f64), video)
            .take();
    }
    let mut sum: Vec<u16> = Vec::new();
    for s in 0..samples {
        let offset = ((s as f64 + 0.5) / samples as f64 - 0.5) * 0.5;
        let frame =
            scene.render_with_video(scene.time_of_frame((index as f64 + offset).max(0.0)), video);
        let data = frame.data();
        if sum.is_empty() {
            sum = data.iter().map(|&v| v as u16).collect();
        } else {
            for (a, b) in sum.iter_mut().zip(data) {
                *a += *b as u16;
            }
        }
    }
    sum.into_iter()
        .map(|v| ((v as u32 + samples / 2) / samples) as u8)
        .collect()
}

pub struct ExportProgress {
    pub done: u64,
    pub total: u64,
    pub fps: f64,
}

pub fn export(
    scene: &Scene,
    out: &Path,
    opts: &ExportOptions,
    cancel: &AtomicBool,
    progress: &(dyn Fn(ExportProgress) + Sync),
) -> Result<()> {
    let tmp_dir = crate::job::data_dir().join("tmp");
    std::fs::create_dir_all(&tmp_dir)?;
    let wav = tmp_dir.join(format!("audio-{}.wav", std::process::id()));
    build_audio(scene, opts.mods, opts.skin.as_deref(), &wav)?;
    let partial = out.with_extension("partial.mp4");
    let (w, h, fps) = (scene.opts.width, scene.opts.height, scene.opts.fps);
    let mut cmd = ffmpeg_command();
    cmd.args([
        "-y",
        "-v",
        "error",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgba",
        "-s",
        &format!("{w}x{h}"),
        "-framerate",
        &fps.to_string(),
        "-i",
        "-",
        "-i",
    ])
    .arg(&wav);
    let encoder = usable_encoder(opts.encoder);
    let major = ffmpeg_major();
    cmd.args([
        "-map",
        "0:v",
        "-map",
        "1:a",
        "-c:v",
        encoder.codec(),
        "-pix_fmt",
        "yuv420p",
        "-r",
        &fps.to_string(),
        "-g",
        &(fps * 2).to_string(),
    ]);
    match encoder {
        Encoder::X264 => cmd.args(["-preset", "veryfast", "-crf", "18", "-profile:v", "high"]),
        Encoder::Nvenc => cmd.args([
            "-preset",
            if major >= 5 { "p5" } else { "slow" },
            "-rc",
            "vbr",
            "-cq",
            "19",
            "-b:v",
            "0",
        ]),
        Encoder::OpenH264 => cmd.args(["-b:v", if h >= 1080 { "16M" } else { "8M" }]),
        Encoder::Mpeg4 => cmd.args(["-q:v", "2"]),
        Encoder::Amf => cmd.args([
            "-quality", "quality", "-rc", "cqp", "-qp_i", "18", "-qp_p", "18", "-qp_b", "18",
        ]),
        Encoder::Qsv => cmd.args(["-preset", "medium", "-global_quality", "20"]),
    };
    if matches!(encoder, Encoder::X264 | Encoder::Nvenc | Encoder::Amf | Encoder::Qsv) {
        let max = ((w as f64 * h as f64 * fps as f64 * 0.19) as u64).clamp(4_000_000, 60_000_000);
        cmd.args(["-maxrate", &max.to_string(), "-bufsize", &(max * 2).to_string()]);
    }
    if major >= 5 {
        cmd.args(["-fps_mode", "cfr"]);
    } else {
        cmd.args(["-vsync", "cfr"]);
    }
    cmd.args([
        "-color_primaries",
        "bt709",
        "-color_trc",
        "bt709",
        "-colorspace",
        "bt709",
        "-c:a",
        "aac",
        "-strict",
        "-2",
        "-b:a",
        "192k",
        "-ar",
        "48000",
        "-shortest",
        "-movflags",
        "+faststart",
        "-f",
        "mp4",
    ])
    .arg(&partial);
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("ffmpeg could not be started")?;
    let mut stdin = child.stdin.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    let total = scene.total_frames;
    let mut reader = if scene.has_layers() {
        VideoReader::open(scene)
    } else {
        None
    };
    let (threads, batch, queue) = profile_limits(opts.profile);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()?;
    let (tx, rx) = mpsc::sync_channel::<Vec<Vec<u8>>>(queue);
    let started = std::time::Instant::now();
    let write_result = std::thread::scope(|scope| {
        let writer = scope.spawn(move || -> std::io::Result<()> {
            for frames in rx {
                for f in frames {
                    stdin.write_all(&f)?;
                }
            }
            Ok(())
        });
        let mut next = 0u64;
        while next < total && !cancel.load(Ordering::Relaxed) {
            let end = (next + batch).min(total);
            let videos: Vec<Option<Vec<u8>>> = (next..end)
                .map(|i| reader.as_mut().and_then(|r| r.frame_for(i)))
                .collect();
            let frames: Vec<Vec<u8>> = pool.install(|| {
                (next..end)
                    .into_par_iter()
                    .map(|i| {
                        render_frame(
                            scene,
                            i,
                            opts.frame_samples,
                            videos[(i - next) as usize].as_deref(),
                        )
                    })
                    .collect()
            });
            if tx.send(frames).is_err() {
                break;
            }
            next = end;
            let secs = started.elapsed().as_secs_f64().max(0.001);
            progress(ExportProgress {
                done: next,
                total,
                fps: next as f64 / secs,
            });
        }
        drop(tx);
        writer.join().unwrap()
    });
    let status = child.wait()?;
    let errors = err_reader.join().unwrap_or_default();
    let _ = std::fs::remove_file(&wav);
    if cancel.load(Ordering::Relaxed) {
        let _ = std::fs::remove_file(&partial);
        bail!("cancelled");
    }
    if let Err(e) = write_result {
        let _ = std::fs::remove_file(&partial);
        bail!("ffmpeg stopped receiving frames ({e}): {}", errors.trim());
    }
    if !status.success() {
        let _ = std::fs::remove_file(&partial);
        bail!("ffmpeg failed: {}", errors.trim());
    }
    if out.exists() {
        let _ = std::fs::remove_file(out);
    }
    std::fs::rename(&partial, out)?;
    Ok(())
}
