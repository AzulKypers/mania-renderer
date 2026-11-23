use anyhow::{Context, Result};
use md5::{Digest, Md5};
use rayon::prelude::*;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

pub fn md5_file(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Md5::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(format!("{:x}", hasher.finalize()))
}

#[derive(Default, Clone)]
pub struct OsuConfig {
    pub beatmap_dir: Option<String>,
    pub skin: Option<String>,
    pub mania_speed: Option<u32>,
    pub dim_level: Option<u32>,
    pub bpm_scale: bool,
}

pub fn read_osu_config(osu_dir: &Path) -> OsuConfig {
    let user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_default();
    let mut file = osu_dir.join(format!("osu!.{user}.cfg"));
    if !file.is_file() {
        let newest = std::fs::read_dir(osu_dir).ok().and_then(|entries| {
            entries
                .flatten()
                .filter(|e| {
                    let name = e.file_name().to_string_lossy().to_lowercase();
                    name.starts_with("osu!.") && name.ends_with(".cfg") && name != "osu!.cfg"
                })
                .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
                .map(|e| e.path())
        });
        match newest {
            Some(p) => file = p,
            None => return OsuConfig::default(),
        }
    }
    let mut config = OsuConfig::default();
    let text = std::fs::read(&file)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().to_string();
        match key.trim() {
            "BeatmapDirectory" => config.beatmap_dir = Some(value),
            "Skin" => config.skin = Some(value),
            "ManiaSpeed" => {
                config.mania_speed = value.parse().ok().filter(|v| (1..=40).contains(v))
            }
            "DimLevel" => config.dim_level = value.parse().ok(),
            "ManiaSpeedBPMScale" => config.bpm_scale = value == "1",
            _ => {}
        }
    }
    config
}

pub fn songs_dir_for(osu_dir: &Path) -> PathBuf {
    let config = read_osu_config(osu_dir);
    match config.beatmap_dir {
        Some(dir) if !dir.is_empty() => {
            let p = PathBuf::from(&dir);
            if p.is_absolute() {
                p
            } else {
                osu_dir.join(p)
            }
        }
        _ => osu_dir.join("Songs"),
    }
}

struct DbReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl DbReader<'_> {
    fn int(&mut self) -> Result<i32> {
        let v = i32::from_le_bytes(
            self.data
                .get(self.pos..self.pos + 4)
                .context("osu!.db cut")?
                .try_into()?,
        );
        self.pos += 4;
        Ok(v)
    }
    fn skip(&mut self, n: usize) {
        self.pos += n;
    }
    fn text(&mut self) -> Result<String> {
        let kind = *self.data.get(self.pos).context("osu!.db cut")?;
        self.pos += 1;
        if kind == 0 {
            return Ok(String::new());
        }
        anyhow::ensure!(kind == 0x0b, "bad string marker");
        let mut len = 0usize;
        let mut shift = 0;
        loop {
            let b = *self.data.get(self.pos).context("osu!.db cut")?;
            self.pos += 1;
            len |= ((b & 0x7f) as usize) << shift;
            if b & 0x80 == 0 {
                break;
            }
            shift += 7;
        }
        let s = String::from_utf8_lossy(
            self.data
                .get(self.pos..self.pos + len)
                .context("osu!.db cut")?,
        )
        .into_owned();
        self.pos += len;
        Ok(s)
    }
}

pub fn read_osu_db(path: &Path) -> Result<HashMap<String, PathBuf>> {
    let data = std::fs::read(path)?;
    let mut r = DbReader {
        data: &data,
        pos: 0,
    };
    let version = r.int()?;
    r.skip(4 + 1 + 8);
    r.text()?;
    let count = r.int()?.max(0) as usize;
    let mut result = HashMap::with_capacity(count);
    for _ in 0..count {
        if version < 20191106 {
            r.skip(4);
        }
        for _ in 0..7 {
            r.text()?;
        }
        let md5 = r.text()?;
        let file = r.text()?;
        r.skip(1 + 6 + 8);
        r.skip(if version < 20140609 { 4 } else { 16 });
        r.skip(8);
        if version >= 20140609 {
            let pair = if version >= 20250107 { 10 } else { 14 };
            for _ in 0..4 {
                let n = r.int()?.max(0) as usize;
                r.skip(n * pair);
            }
        }
        r.skip(12);
        let timing = r.int()?.max(0) as usize;
        r.skip(timing * 17);
        r.skip(12 + 4 + 2 + 4 + 1);
        r.text()?;
        r.text()?;
        r.skip(2);
        r.text()?;
        r.skip(1 + 8 + 1);
        let folder = r.text()?;
        r.skip(8 + 5);
        if version < 20140609 {
            r.skip(2);
        }
        r.skip(4 + 1);
        if !md5.is_empty() {
            result.insert(md5.to_lowercase(), PathBuf::from(folder.trim()).join(file));
        }
    }
    Ok(result)
}

fn modified_secs(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub enum FindResult {
    Found(PathBuf),
    NotInstalled,
}

pub fn find_beatmap(
    md5: &str,
    songs: &Path,
    osu_dir: Option<&Path>,
    index_path: &Path,
    difficulty_hint: &str,
    progress: &(dyn Fn(usize) + Sync),
) -> Result<FindResult> {
    if let Some(osu_dir) = osu_dir {
        let db_path = osu_dir.join("osu!.db");
        if db_path.is_file() {
            match read_osu_db(&db_path) {
                Ok(db) => {
                    if let Some(rel) = db.get(md5) {
                        let candidate = songs.join(rel);
                        if candidate.is_file() && md5_file(&candidate).as_deref() == Some(md5) {
                            return Ok(FindResult::Found(candidate));
                        }
                    } else if modified_secs(&db_path) >= modified_secs(songs) {
                        return Ok(FindResult::NotInstalled);
                    }
                }
                Err(e) => eprintln!("warn: osu!.db: {e}"),
            }
        }
    }
    scan_songs(md5, songs, index_path, difficulty_hint, progress)
}

fn scan_songs(
    md5: &str,
    songs: &Path,
    index_path: &Path,
    hint: &str,
    progress: &(dyn Fn(usize) + Sync),
) -> Result<FindResult> {
    let mut index: HashMap<String, (u64, u64, String)> = HashMap::new();
    if let Ok(text) = std::fs::read_to_string(index_path) {
        for line in text.lines() {
            let p: Vec<&str> = line.split('\t').collect();
            if p.len() == 4 {
                if let (Ok(size), Ok(time)) = (p[1].parse(), p[2].parse()) {
                    index.insert(p[0].to_string(), (size, time, p[3].to_string()));
                }
            }
        }
    }
    for (rel, (_, _, hash)) in &index {
        if hash == md5 {
            let known = songs.join(rel);
            if known.is_file() && md5_file(&known).as_deref() == Some(md5) {
                return Ok(FindResult::Found(known));
            }
        }
    }
    let folders: Vec<PathBuf> = std::fs::read_dir(songs)
        .with_context(|| format!("Songs folder not found: {}", songs.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    let mut files: Vec<PathBuf> = folders
        .par_iter()
        .flat_map_iter(|folder| {
            std::fs::read_dir(folder)
                .map(|entries| {
                    entries
                        .flatten()
                        .map(|e| e.path())
                        .filter(|p| {
                            p.extension()
                                .map(|e| e.eq_ignore_ascii_case("osu"))
                                .unwrap_or(false)
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
        .collect();
    let tag = format!("[{}]", hint.to_lowercase());
    files.sort_by_key(|f| {
        if !hint.is_empty() && f.to_string_lossy().to_lowercase().contains(&tag) {
            0
        } else {
            1
        }
    });
    let total = files.len().max(1);
    let done = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let found = Mutex::new(None);
    let shared = Mutex::new(index);
    files.par_iter().for_each(|file| {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let rel = file
            .strip_prefix(songs)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let size = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
        let time = modified_secs(file);
        let cached = shared
            .lock()
            .unwrap()
            .get(&rel)
            .filter(|e| e.0 == size && e.1 == time)
            .map(|e| e.2.clone());
        let hash = match cached {
            Some(h) => Some(h),
            None => {
                let h = md5_file(file);
                if let Some(h) = &h {
                    shared
                        .lock()
                        .unwrap()
                        .insert(rel.clone(), (size, time, h.clone()));
                }
                h
            }
        };
        if hash.as_deref() == Some(md5) {
            *found.lock().unwrap() = Some(file.clone());
            stop.store(true, Ordering::Relaxed);
        }
        let n = done.fetch_add(1, Ordering::Relaxed) + 1;
        if n % 200 == 0 {
            progress(n * 100 / total);
        }
    });
    let index = shared.into_inner().unwrap();
    if let Some(parent) = index_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let body: String = index
        .iter()
        .map(|(k, v)| format!("{k}\t{}\t{}\t{}\n", v.0, v.1, v.2))
        .collect();
    let partial = index_path.with_extension("partial");
    if std::fs::write(&partial, body).is_ok() {
        let _ = std::fs::rename(&partial, index_path);
    }
    Ok(match found.into_inner().unwrap() {
        Some(p) => FindResult::Found(p),
        None => FindResult::NotInstalled,
    })
}

pub fn find_in_folder(folder: &Path, md5: &str) -> Option<PathBuf> {
    std::fs::read_dir(folder)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.extension()
                .map(|e| e.eq_ignore_ascii_case("osu"))
                .unwrap_or(false)
                && md5_file(p).as_deref() == Some(md5)
        })
}

pub fn extract_osz(osz: &Path, cache_root: &Path) -> Result<PathBuf> {
    let meta = std::fs::metadata(osz)?;
    let tag = format!("{}-{}", meta.len(), modified_secs(osz));
    let target = cache_root.join(&tag);
    if target.is_dir() {
        return Ok(target);
    }
    if cache_root.exists() {
        let _ = std::fs::remove_dir_all(cache_root);
    }
    let partial = cache_root.join(format!("{tag}.partial"));
    std::fs::create_dir_all(&partial)?;
    let mut archive = zip::ZipArchive::new(std::fs::File::open(osz)?)?;
    archive.extract(&partial)?;
    std::fs::rename(&partial, &target)?;
    Ok(target)
}

pub fn osu_text_mode(text: &str) -> u8 {
    for line in text.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("Mode:") {
            return v.trim().parse().unwrap_or(0);
        }
        if line.starts_with("[Metadata]") {
            break;
        }
    }
    0
}

pub fn list_mania_maps(path: &Path) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let read_head = |p: &Path| {
        std::fs::read(p)
            .map(|b| String::from_utf8_lossy(&b[..b.len().min(8192)]).into_owned())
            .unwrap_or_default()
    };
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if path.is_dir() {
        let mut files: Vec<PathBuf> = std::fs::read_dir(path)
            .map(|e| e.flatten().map(|e| e.path()).collect())
            .unwrap_or_default();
        files.sort();
        for f in files {
            if f.extension()
                .map(|e| e.eq_ignore_ascii_case("osu"))
                .unwrap_or(false)
                && osu_text_mode(&read_head(&f)) == 3
            {
                found.push((stem(&f), f.to_string_lossy().into_owned()));
            }
        }
    } else if ext == "osu" {
        if osu_text_mode(&read_head(path)) == 3 {
            found.push((stem(path), path.to_string_lossy().into_owned()));
        }
    } else if ext == "osz" {
        if let Ok(mut archive) = std::fs::File::open(path)
            .map_err(anyhow::Error::from)
            .and_then(|f| zip::ZipArchive::new(f).map_err(Into::into))
        {
            for i in 0..archive.len() {
                let Ok(mut entry) = archive.by_index(i) else {
                    continue;
                };
                let name = entry.name().to_string();
                if !name.to_lowercase().ends_with(".osu") {
                    continue;
                }
                let mut head = vec![0u8; 8192];
                let n = entry.read(&mut head).unwrap_or(0);
                if osu_text_mode(&String::from_utf8_lossy(&head[..n])) == 3 {
                    found.push((
                        stem(Path::new(&name)),
                        format!("{}::{}", path.display(), name),
                    ));
                }
            }
        }
    }
    found
}

fn stem(p: &Path) -> String {
    p.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub fn resolve_beatmap_spec(spec: &str, cache_root: &Path) -> Result<PathBuf> {
    let (source, entry) = match spec.split_once("::") {
        Some((s, e)) => (s, Some(e)),
        None => (spec, None),
    };
    let mut base = PathBuf::from(source);
    if base
        .extension()
        .map(|e| e.eq_ignore_ascii_case("osz"))
        .unwrap_or(false)
    {
        base = extract_osz(&base, cache_root)?;
    }
    let osu = match entry {
        Some(e) => base.join(e),
        None => base,
    };
    anyhow::ensure!(osu.is_file(), "map not found: {spec}");
    Ok(osu)
}
