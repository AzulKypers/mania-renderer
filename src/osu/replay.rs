use anyhow::{bail, Context, Result};
use std::io::Cursor;
use std::path::Path;

pub const MOD_EASY: u32 = 1 << 1;
pub const MOD_HARD_ROCK: u32 = 1 << 4;
pub const MOD_DOUBLE_TIME: u32 = 1 << 6;
pub const MOD_HALF_TIME: u32 = 1 << 8;
pub const MOD_NIGHTCORE: u32 = 1 << 9;
pub const MOD_AUTO: u32 = 1 << 11;
pub const MOD_MIRROR: u32 = 1 << 30;

#[derive(Clone, Debug, Default)]
pub struct Replay {
    pub mode: u8,
    pub beatmap_md5: String,
    pub player: String,
    pub count_300: u16,
    pub count_100: u16,
    pub count_50: u16,
    pub count_geki: u16,
    pub count_katu: u16,
    pub count_miss: u16,
    pub score: u32,
    pub max_combo: u16,
    pub mods: u32,
    pub timestamp: i64,
    pub frames: Vec<(i32, u32)>,
}

impl Replay {
    pub fn rate(&self) -> f64 {
        rate_for_mods(self.mods)
    }
}

pub fn rate_for_mods(mods: u32) -> f64 {
    if mods & (MOD_DOUBLE_TIME | MOD_NIGHTCORE) != 0 {
        1.5
    } else if mods & MOD_HALF_TIME != 0 {
        0.75
    } else {
        1.0
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.pos + n > self.data.len() {
            bail!("the replay file is cut short");
        }
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into()?))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into()?))
    }
    fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into()?))
    }
    fn uleb(&mut self) -> Result<usize> {
        let mut value = 0usize;
        let mut shift = 0;
        loop {
            let byte = self.u8()?;
            value |= ((byte & 0x7f) as usize) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
            shift += 7;
        }
    }
    fn string(&mut self) -> Result<String> {
        if self.u8()? != 0x0b {
            return Ok(String::new());
        }
        let len = self.uleb()?;
        Ok(String::from_utf8_lossy(self.take(len)?).into_owned())
    }
}

pub fn read_md5_only(path: &Path) -> Result<String> {
    let data = std::fs::read(path)?;
    let mut r = Reader {
        data: &data,
        pos: 0,
    };
    r.u8()?;
    r.u32()?;
    Ok(r.string()?.to_lowercase())
}

pub fn parse_file(path: &Path) -> Result<Replay> {
    let data = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let mut r = Reader {
        data: &data,
        pos: 0,
    };
    let mut replay = Replay {
        mode: r.u8()?,
        ..Default::default()
    };
    r.u32()?;
    replay.beatmap_md5 = r.string()?.to_lowercase();
    replay.player = r.string()?;
    r.string()?;
    replay.count_300 = r.u16()?;
    replay.count_100 = r.u16()?;
    replay.count_50 = r.u16()?;
    replay.count_geki = r.u16()?;
    replay.count_katu = r.u16()?;
    replay.count_miss = r.u16()?;
    replay.score = r.u32()?;
    replay.max_combo = r.u16()?;
    r.u8()?;
    replay.mods = r.u32()?;
    r.string()?;
    replay.timestamp = r.i64()?;
    let len = r.u32()? as usize;
    let compressed = r.take(len)?;
    let mut raw = Vec::new();
    lzma_rs::lzma_decompress(&mut Cursor::new(compressed), &mut raw)
        .context("cannot decompress the replay data")?;
    replay.frames = decode_frames(&String::from_utf8_lossy(&raw));
    if replay.mode != 3 {
        bail!("this replay is not osu!mania (mode {})", replay.mode);
    }
    Ok(replay)
}

fn decode_frames(text: &str) -> Vec<(i32, u32)> {
    let mut frames = Vec::new();
    let mut time: i64 = 0;
    for segment in text.split(',') {
        let parts: Vec<&str> = segment.trim().split('|').collect();
        if parts.len() < 4 {
            continue;
        }
        let delta = parts[0].parse::<f64>().unwrap_or(0.0).round() as i64;
        if delta == -12345 {
            continue;
        }
        time += delta;
        let x = parts[1].parse::<f64>().unwrap_or(0.0);
        let keys = if (0.0..=1_048_575.0).contains(&x) {
            x as u32
        } else {
            0
        };
        frames.push((time as i32, keys));
    }
    frames
}

#[derive(Clone, Copy, Debug)]
pub struct KeyEvent {
    pub time: i32,
    pub column: usize,
    pub down: bool,
}

pub fn key_events(frames: &[(i32, u32)], keys: usize, mirror: bool) -> Vec<KeyEvent> {
    let mut events = Vec::new();
    let mut last = 0u32;
    for &(time, mask) in frames {
        let changed = mask ^ last;
        if changed != 0 {
            for bit in 0..keys.min(32) {
                if changed & (1 << bit) != 0 {
                    let column = if mirror { keys - 1 - bit } else { bit };
                    events.push(KeyEvent {
                        time,
                        column,
                        down: mask & (1 << bit) != 0,
                    });
                }
            }
        }
        last = mask;
    }
    events.sort_by_key(|e| (e.time, !e.down));
    events
}
