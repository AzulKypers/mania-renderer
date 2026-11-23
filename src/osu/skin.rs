use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default)]
pub struct ManiaConfig {
    pub keys: usize,
    pub values: HashMap<String, String>,
}

#[derive(Clone, Debug, Default)]
pub struct SkinIni {
    pub general: HashMap<String, String>,
    pub fonts: HashMap<String, String>,
    pub colours: HashMap<String, String>,
    pub mania: Vec<ManiaConfig>,
}

pub fn parse_ini(text: &str) -> SkinIni {
    let mut ini = SkinIni::default();
    let mut section = String::new();
    for raw in text.lines() {
        let line = raw.split("//").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_lowercase();
            if section == "mania" {
                ini.mania.push(ManiaConfig::default());
            }
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let (key, value) = (key.trim().to_string(), value.trim().to_string());
        match section.as_str() {
            "general" => {
                ini.general.insert(key, value);
            }
            "fonts" => {
                ini.fonts.insert(key, value);
            }
            "colours" => {
                ini.colours.insert(key, value);
            }
            "mania" => {
                if let Some(cfg) = ini.mania.last_mut() {
                    if key.eq_ignore_ascii_case("Keys") {
                        cfg.keys = value.parse().unwrap_or(0);
                    } else {
                        cfg.values.insert(key, value);
                    }
                }
            }
            _ => {}
        }
    }
    ini
}

pub struct SkinFiles {
    pub root: Option<PathBuf>,
    hd: bool,
    index: HashMap<String, PathBuf>,
}

impl SkinFiles {
    pub fn scan(root: Option<&Path>) -> SkinFiles {
        let mut index = HashMap::new();
        if let Some(root) = root {
            let mut stack = vec![root.to_path_buf()];
            while let Some(dir) = stack.pop() {
                let Ok(entries) = std::fs::read_dir(&dir) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        stack.push(path);
                    } else if let Ok(rel) = path.strip_prefix(root) {
                        let key = rel.to_string_lossy().replace('\\', "/").to_lowercase();
                        index.insert(key, path);
                    }
                }
            }
        }
        SkinFiles {
            root: root.map(Path::to_path_buf),
            hd: true,
            index,
        }
    }

    pub fn with_hd(mut self, hd: bool) -> SkinFiles {
        self.hd = hd;
        self
    }

    pub fn ini(&self) -> SkinIni {
        self.index
            .get("skin.ini")
            .and_then(|p| std::fs::read(p).ok())
            .map(|b| parse_ini(&String::from_utf8_lossy(&b)))
            .unwrap_or_default()
    }

    pub fn image(&self, name: &str) -> Option<(PathBuf, f32)> {
        let base = name.replace('\\', "/").to_lowercase();
        let base = base.trim_end_matches(".png").trim_end_matches(".jpg");
        let order = if self.hd {
            [("@2x", 2.0), ("", 1.0)]
        } else {
            [("", 1.0), ("@2x", 2.0)]
        };
        for (suffix, density) in order {
            for ext in ["png", "jpg", "jpeg"] {
                if let Some(path) = self.index.get(&format!("{base}{suffix}.{ext}")) {
                    return Some((path.clone(), density));
                }
            }
        }
        None
    }

    pub fn sound(&self, name: &str) -> Option<PathBuf> {
        let base = name.to_lowercase();
        for ext in ["wav", "ogg", "mp3"] {
            if let Some(path) = self.index.get(&format!("{base}.{ext}")) {
                return Some(path.clone());
            }
        }
        None
    }
}

pub fn parse_colour(value: &str) -> Option<[u8; 4]> {
    let parts: Vec<u8> = value
        .split(',')
        .filter_map(|v| v.trim().parse::<i32>().ok())
        .map(|v| v.clamp(0, 255) as u8)
        .collect();
    match parts.len() {
        3 => Some([parts[0], parts[1], parts[2], 255]),
        n if n >= 4 => Some([parts[0], parts[1], parts[2], parts[3]]),
        _ => None,
    }
}

pub fn parse_list(value: &str) -> Vec<f32> {
    value
        .split(',')
        .filter_map(|v| v.trim().parse::<f32>().ok())
        .collect()
}

pub fn list_skins(skins_dir: &Path) -> Vec<(String, PathBuf)> {
    let mut found: Vec<(String, PathBuf)> = std::fs::read_dir(skins_dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
                .collect()
        })
        .unwrap_or_default();
    found.sort_by_key(|(name, _)| name.to_lowercase());
    found
}
