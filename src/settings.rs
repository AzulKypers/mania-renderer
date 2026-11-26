use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    pub lang: String,
    #[serde(rename = "osuDir")]
    pub osu_dir: String,
    #[serde(rename = "songsDir")]
    pub songs_dir: String,
    pub skin: String,
    #[serde(rename = "outDir")]
    pub out_dir: String,
    pub teclas: Vec<String>,
    pub scroll: String,
    #[serde(rename = "scrollManual")]
    pub scroll_manual: bool,
    pub color: String,
    pub kps: bool,
    pub panel: bool,
    #[serde(rename = "useGpu")]
    pub use_gpu: bool,
    pub gpu: String,
    pub height: u32,
    #[serde(rename = "videoFps")]
    pub video_fps: u32,
    pub story: bool,
    #[serde(rename = "bgVideo")]
    pub bg_video: bool,
    pub smooth: bool,
    #[serde(rename = "smoothLevel")]
    pub smooth_level: usize,
    pub auto: bool,
    #[serde(rename = "skinHd")]
    pub skin_hd: bool,
    #[serde(rename = "bgDim")]
    pub bg_dim: i32,
    #[serde(rename = "showResults")]
    pub show_results: bool,
    pub performance: u8,
    #[serde(rename = "showScore")]
    pub show_score: bool,
    #[serde(rename = "mapHitsoundVolume")]
    pub map_hs_volume: u32,
    #[serde(rename = "skinHitsoundVolume")]
    pub skin_hs_volume: u32,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            lang: "en".into(),
            osu_dir: String::new(),
            songs_dir: String::new(),
            skin: String::new(),
            out_dir: crate::job::data_dir()
                .parent()
                .map(|p| p.join("videos"))
                .unwrap_or_else(|| PathBuf::from("videos"))
                .to_string_lossy()
                .into_owned(),
            teclas: vec!["A".into(), "S".into(), "K".into(), "L".into()],
            scroll: String::new(),
            scroll_manual: false,
            color: "#ffffff".into(),
            kps: true,
            panel: true,
            use_gpu: false,
            gpu: String::new(),
            height: 1080,
            video_fps: 60,
            story: true,
            bg_video: true,
            smooth: true,
            smooth_level: 0,
            auto: false,
            skin_hd: true,
            bg_dim: -1,
            show_results: true,
            performance: 1,
            show_score: true,
            map_hs_volume: 100,
            skin_hs_volume: 100,
        }
    }
}

pub fn path() -> PathBuf {
    crate::job::data_dir().join("ajustes.json")
}

pub fn db_path() -> PathBuf {
    crate::job::data_dir().join("config.sqlite")
}

fn open_db() -> Option<rusqlite::Connection> {
    let _ = std::fs::create_dir_all(crate::job::data_dir());
    let conn = rusqlite::Connection::open(db_path()).ok()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS overlays (id TEXT PRIMARY KEY, enabled INTEGER NOT NULL, x REAL NOT NULL, y REAL NOT NULL, scale REAL NOT NULL, options TEXT NOT NULL);",
    )
    .ok()?;
    Some(conn)
}

pub fn load_overlays() -> Vec<crate::overlays::catalog::OverlayConfig> {
    let mut saved = Vec::new();
    if let Some(conn) = open_db() {
        if let Ok(mut stmt) = conn.prepare("SELECT id, enabled, x, y, scale, options FROM overlays")
        {
            let rows = stmt.query_map([], |r| {
                Ok(crate::overlays::catalog::OverlayConfig {
                    id: r.get(0)?,
                    enabled: r.get::<_, i64>(1)? != 0,
                    x: r.get::<_, f64>(2)? as f32,
                    y: r.get::<_, f64>(3)? as f32,
                    scale: r.get::<_, f64>(4)? as f32,
                    options: serde_json::from_str(&r.get::<_, String>(5)?).unwrap_or_default(),
                })
            });
            if let Ok(rows) = rows {
                saved.extend(rows.flatten());
            }
        }
    }
    crate::overlays::catalog::merge(saved)
}

pub fn save_overlays(list: &[crate::overlays::catalog::OverlayConfig]) {
    let Some(mut conn) = open_db() else { return };
    let Ok(tx) = conn.transaction() else { return };
    for o in list {
        let _ = tx.execute(
            "INSERT INTO overlays (id, enabled, x, y, scale, options) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET enabled = excluded.enabled, x = excluded.x, y = excluded.y, scale = excluded.scale, options = excluded.options",
            rusqlite::params![o.id, o.enabled as i64, o.x as f64, o.y as f64, o.scale as f64, serde_json::to_string(&o.options).unwrap_or_default()],
        );
    }
    let _ = tx.commit();
}

fn foreign_path(value: &str) -> bool {
    if cfg!(windows) {
        value.starts_with('/')
    } else {
        value.len() >= 2 && value.as_bytes()[1] == b':' || value.starts_with("\\\\")
    }
}

impl Settings {
    pub fn load() -> Settings {
        let mut map = serde_json::Map::new();
        if let Some(conn) = open_db() {
            if let Ok(mut stmt) = conn.prepare("SELECT key, value FROM settings") {
                if let Ok(rows) =
                    stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                {
                    for (k, v) in rows.flatten() {
                        if let Ok(value) = serde_json::from_str(&v) {
                            map.insert(k, value);
                        }
                    }
                }
            }
        }
        let mut s: Settings = if map.is_empty() {
            let legacy = std::fs::read(path())
                .ok()
                .and_then(|b| serde_json::from_slice::<Settings>(&b).ok());
            match legacy {
                Some(old) => {
                    old.save();
                    let _ = std::fs::rename(path(), path().with_extension("json.importado"));
                    old
                }
                None => Settings::default(),
            }
        } else {
            serde_json::from_value(serde_json::Value::Object(map)).unwrap_or_default()
        };
        for v in [
            &mut s.osu_dir,
            &mut s.songs_dir,
            &mut s.skin,
            &mut s.out_dir,
        ] {
            if foreign_path(v) {
                v.clear();
            }
        }
        if s.out_dir.is_empty() {
            s.out_dir = Settings::default().out_dir;
        }
        if s.lang != "es" {
            s.lang = "en".into();
        }
        s
    }

    pub fn save(&self) {
        let Some(mut conn) = open_db() else { return };
        let Ok(serde_json::Value::Object(map)) = serde_json::to_value(self) else {
            return;
        };
        let Ok(tx) = conn.transaction() else { return };
        for (k, v) in map {
            let _ = tx.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                rusqlite::params![k, v.to_string()],
            );
        }
        let _ = tx.commit();
    }
}

pub struct Strings {
    table: HashMap<String, HashMap<String, String>>,
}

impl Strings {
    pub fn load() -> Strings {
        let raw: HashMap<String, HashMap<String, Value>> =
            serde_json::from_str(include_str!("../assets/strings.json")).unwrap_or_default();
        let table = raw
            .into_iter()
            .map(|(lang, m)| {
                (
                    lang,
                    m.into_iter()
                        .map(|(k, v)| (k, v.as_str().unwrap_or_default().to_string()))
                        .collect(),
                )
            })
            .collect();
        Strings { table }
    }

    pub fn get(&self, lang: &str, key: &str) -> String {
        self.table
            .get(lang)
            .and_then(|m| m.get(key))
            .or_else(|| self.table.get("en").and_then(|m| m.get(key)))
            .cloned()
            .unwrap_or_else(|| key.to_string())
    }

    pub fn fmt(&self, lang: &str, key: &str, args: &[String]) -> String {
        let mut text = self.get(lang, key);
        for (i, a) in args.iter().enumerate() {
            text = text.replace(&format!("{{{i}}}"), a);
        }
        text
    }
}
