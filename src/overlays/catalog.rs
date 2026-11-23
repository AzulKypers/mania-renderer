use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct OverlayConfig {
    pub id: String,
    pub enabled: bool,
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub options: BTreeMap<String, String>,
}

pub struct OptionDef {
    pub key: &'static str,
    pub label: [&'static str; 2],
    pub choices: &'static [(&'static str, [&'static str; 2])],
    pub default: &'static str,
}

pub struct OverlayDef {
    pub id: &'static str,
    pub name: [&'static str; 2],
    pub size: (f32, f32),
    pub pos: (f32, f32),
    pub enabled: bool,
    pub options: &'static [OptionDef],
}

const COLORS: &[(&str, [&str; 2])] = &[
    ("orange", ["Orange", "Naranja"]),
    ("blue", ["Blue", "Azul"]),
    ("pink", ["Pink", "Rosa"]),
    ("green", ["Green", "Verde"]),
    ("purple", ["Purple", "Morado"]),
    ("red", ["Red", "Rojo"]),
    ("yellow", ["Yellow", "Amarillo"]),
];

const COLORS_RANDOM: &[(&str, [&str; 2])] = &[
    ("random", ["Random", "Al azar"]),
    ("red", ["Red", "Rojo"]),
    ("orange", ["Orange", "Naranja"]),
    ("yellow", ["Yellow", "Amarillo"]),
    ("green", ["Green", "Verde"]),
    ("blue", ["Blue", "Azul"]),
    ("purple", ["Purple", "Morado"]),
    ("pink", ["Pink", "Rosa"]),
];

pub const CATALOG: &[OverlayDef] = &[
    OverlayDef {
        id: "kps",
        name: ["KPS counter", "Contador KPS"],
        size: (475.0, 425.0),
        pos: (1387.0, 460.0),
        enabled: true,
        options: &[
            OptionDef {
                key: "style",
                label: ["Style", "Estilo"],
                choices: &[("jkps", ["JKPS", "JKPS"]), ("panel", ["Panel", "Panel"])],
                default: "jkps",
            },
            OptionDef {
                key: "space",
                label: ["Space bar color", "Color barra espacio"],
                choices: &[
                    ("same", ["Same as the others", "Igual a las demás"]),
                    ("orange", ["Orange", "Naranja"]),
                    ("blue", ["Blue", "Azul"]),
                    ("pink", ["Pink", "Rosa"]),
                    ("green", ["Green", "Verde"]),
                    ("yellow", ["Yellow", "Amarillo"]),
                    ("purple", ["Purple", "Morado"]),
                    ("red", ["Red", "Rojo"]),
                ],
                default: "same",
            },
        ],
    },
    OverlayDef {
        id: "panel",
        name: ["Simple overlay", "Panel simple"],
        size: (500.0, 220.0),
        pos: (1390.0, 100.0),
        enabled: true,
        options: &[
            OptionDef {
                key: "mode",
                label: ["Mode", "Modo"],
                choices: &[
                    ("normal", ["Normal", "Normal"]),
                    ("simple", ["Simple", "Simple"]),
                ],
                default: "normal",
            },
            OptionDef {
                key: "left",
                label: ["Left box", "Caja izquierda"],
                choices: &[
                    ("star", ["Star rating", "Estrellas"]),
                    ("acc", ["Accuracy", "Precisión"]),
                    ("combo", ["Max combo", "Combo máximo"]),
                ],
                default: "star",
            },
            OptionDef {
                key: "right",
                label: ["Right box", "Caja derecha"],
                choices: &[("ur", ["UR", "UR"]), ("pp", ["PP", "PP"])],
                default: "ur",
            },
            OptionDef {
                key: "background",
                label: ["Background", "Fondo"],
                choices: &[
                    ("gray", ["Gray", "Gris"]),
                    ("black", ["Black", "Negro"]),
                    ("white", ["White", "Blanco"]),
                ],
                default: "gray",
            },
            OptionDef {
                key: "subtitle",
                label: ["Subtitle", "Subtítulo"],
                choices: &[
                    ("mapper", ["Mapper", "Mapper"]),
                    ("difficulty", ["Difficulty", "Dificultad"]),
                ],
                default: "mapper",
            },
            OptionDef {
                key: "left_color",
                label: ["Left color", "Color izquierdo"],
                choices: COLORS,
                default: "orange",
            },
            OptionDef {
                key: "right_color",
                label: ["Right color", "Color derecho"],
                choices: COLORS,
                default: "blue",
            },
        ],
    },
    OverlayDef {
        id: "hiterror_osu",
        name: ["Hit error meter (osu!)", "Barra de error (osu!)"],
        size: (360.0, 52.0),
        pos: (780.0, 1028.0),
        enabled: true,
        options: &[],
    },
    OverlayDef {
        id: "hiterror_bar",
        name: ["Hit error bar", "Barra de error"],
        size: (350.0, 70.0),
        pos: (785.0, 950.0),
        enabled: false,
        options: &[],
    },
    OverlayDef {
        id: "hiterror_colorful",
        name: ["Hit error bar (colorful)", "Barra de error (colores)"],
        size: (350.0, 70.0),
        pos: (785.0, 950.0),
        enabled: false,
        options: &[],
    },
    OverlayDef {
        id: "hitcount_text",
        name: ["Hit count (text)", "Conteo de golpes (texto)"],
        size: (240.0, 360.0),
        pos: (60.0, 300.0),
        enabled: false,
        options: &[],
    },
    OverlayDef {
        id: "hitcount_simple",
        name: ["Simple hit count", "Conteo simple"],
        size: (200.0, 275.0),
        pos: (40.0, 380.0),
        enabled: false,
        options: &[],
    },
    OverlayDef {
        id: "nowplaying",
        name: ["Now playing", "Reproduciendo"],
        size: (540.0, 200.0),
        pos: (20.0, 20.0),
        enabled: false,
        options: &[],
    },
    OverlayDef {
        id: "nowplaying_disk",
        name: ["Now playing (disk)", "Reproduciendo (disco)"],
        size: (430.0, 550.0),
        pos: (20.0, 260.0),
        enabled: false,
        options: &[
            OptionDef {
                key: "sub",
                label: ["Bottom info", "Dato inferior"],
                choices: &[("pp", ["PP", "PP"]), ("ur", ["UR", "UR"])],
                default: "pp",
            },
            OptionDef {
                key: "anim",
                label: ["Entrance", "Entrada"],
                choices: &[
                    ("rs", ["From the right", "Desde la derecha"]),
                    ("ls", ["From the left", "Desde la izquierda"]),
                    ("us", ["From below", "Desde abajo"]),
                    ("bk", ["Fade in", "Aparecer"]),
                ],
                default: "rs",
            },
        ],
    },
    OverlayDef {
        id: "hitcount_bar",
        name: ["Hit count bar", "Barra de conteo"],
        size: (910.0, 150.0),
        pos: (505.0, 20.0),
        enabled: false,
        options: &[
            OptionDef {
                key: "left",
                label: ["Left info", "Dato izquierdo"],
                choices: &[("pp", ["PP", "PP"]), ("ur", ["UR", "UR"])],
                default: "pp",
            },
            OptionDef {
                key: "color",
                label: ["Color", "Color"],
                choices: COLORS_RANDOM,
                default: "random",
            },
        ],
    },
    OverlayDef {
        id: "osukps",
        name: ["osukps (keys + KPS)", "osukps (teclas + KPS)"],
        size: (480.0, 72.0),
        pos: (720.0, 930.0),
        enabled: false,
        options: &[
            OptionDef {
                key: "idle",
                label: ["Idle color", "Color en reposo"],
                choices: &[
                    ("blue", ["Blue", "Azul"]),
                    ("pink", ["Pink", "Rosa"]),
                    ("purple", ["Purple", "Morado"]),
                    ("green", ["Green", "Verde"]),
                    ("gray", ["Gray", "Gris"]),
                ],
                default: "blue",
            },
            OptionDef {
                key: "active",
                label: ["Pressed color", "Color al presionar"],
                choices: &[
                    ("white", ["White", "Blanco"]),
                    ("yellow", ["Yellow", "Amarillo"]),
                    ("cyan", ["Cyan", "Celeste"]),
                ],
                default: "white",
            },
            OptionDef {
                key: "background",
                label: ["Background", "Fondo"],
                choices: &[
                    ("black", ["Black", "Negro"]),
                    ("none", ["None", "Sin fondo"]),
                ],
                default: "black",
            },
            OptionDef {
                key: "labels",
                label: ["Key labels", "Letras de teclas"],
                choices: &[
                    ("on", ["Shown", "Visibles"]),
                    ("off", ["Hidden", "Ocultas"]),
                ],
                default: "on",
            },
        ],
    },
    OverlayDef {
        id: "mods",
        name: [
            "Mod icons (Mirror, DT...)",
            "Iconos de mods (Mirror, DT...)",
        ],
        size: (420.0, 130.0),
        pos: (1470.0, 325.0),
        enabled: true,
        options: &[
            OptionDef {
                key: "align",
                label: ["Align", "Alinear"],
                choices: &[
                    ("right", ["Right", "Derecha"]),
                    ("left", ["Left", "Izquierda"]),
                ],
                default: "right",
            },
            OptionDef {
                key: "spacing",
                label: ["Spacing", "Separación"],
                choices: &[
                    ("osu", ["Like an osu! replay", "Como replay de osu!"]),
                    ("play", ["Like an osu! match (5 s)", "Como partida de osu! (5 s)"]),
                    ("overlap", ["Overlapped", "Encimados"]),
                ],
                default: "osu",
            },
        ],
    },
    OverlayDef {
        id: "maniacat",
        name: ["Mania Cat", "Mania Cat"],
        size: (360.0, 240.0),
        pos: (40.0, 560.0),
        enabled: false,
        options: &[
            OptionDef {
                key: "color",
                label: ["Cat color", "Color del gato"],
                choices: &[
                    ("white", ["White", "Blanco"]),
                    ("gray", ["Gray", "Gris"]),
                    ("orange", ["Orange", "Naranja"]),
                    ("black", ["Black", "Negro"]),
                ],
                default: "white",
            },
            OptionDef {
                key: "background",
                label: ["Background", "Fondo"],
                choices: &[
                    ("none", ["None", "Sin fondo"]),
                    ("dark", ["Dark", "Oscuro"]),
                ],
                default: "none",
            },
            OptionDef {
                key: "keys",
                label: ["Keyboard", "Teclado"],
                choices: &[("on", ["Shown", "Visible"]), ("off", ["Hidden", "Oculto"])],
                default: "on",
            },
        ],
    },
    OverlayDef {
        id: "mania_ingame",
        name: ["Mania ingame (hit error + hit count)", "Mania ingame (error + conteo)"],
        size: (350.0, 440.0),
        pos: (60.0, 260.0),
        enabled: false,
        options: &[
            OptionDef {
                key: "hiterror",
                label: ["Hit error", "Error de golpe"],
                choices: &[("on", ["Shown", "Visible"]), ("off", ["Hidden", "Oculto"])],
                default: "on",
            },
            OptionDef {
                key: "hitcount",
                label: ["Hit count", "Conteo"],
                choices: &[("on", ["Shown", "Visible"]), ("off", ["Hidden", "Oculto"])],
                default: "on",
            },
        ],
    },
    OverlayDef {
        id: "nps_chart",
        name: ["NPS chart", "Gráfica de NPS"],
        size: (400.0, 400.0),
        pos: (20.0, 660.0),
        enabled: false,
        options: &[OptionDef {
            key: "style",
            label: ["Style", "Estilo"],
            choices: &[("line", ["Line (original)", "Línea (original)"]), ("bars", ["Bars", "Barras"])],
            default: "line",
        }],
    },
];

pub fn def(id: &str) -> Option<&'static OverlayDef> {
    CATALOG.iter().find(|d| d.id == id)
}

pub fn defaults() -> Vec<OverlayConfig> {
    CATALOG
        .iter()
        .map(|d| OverlayConfig {
            id: d.id.to_string(),
            enabled: d.enabled,
            x: d.pos.0,
            y: d.pos.1,
            scale: 1.0,
            options: d
                .options
                .iter()
                .map(|o| (o.key.to_string(), o.default.to_string()))
                .collect(),
        })
        .collect()
}

pub fn merge(saved: Vec<OverlayConfig>) -> Vec<OverlayConfig> {
    defaults()
        .into_iter()
        .map(|mut d| {
            if let Some(s) = saved.iter().find(|s| s.id == d.id) {
                d.enabled = s.enabled;
                d.x = s.x;
                d.y = s.y;
                d.scale = s.scale.clamp(0.3, 3.0);
                for (k, v) in &s.options {
                    if d.options.contains_key(k) {
                        d.options.insert(k.clone(), v.clone());
                    }
                }
            }
            d
        })
        .collect()
}

pub fn colour(name: &str) -> [u8; 4] {
    match name {
        "orange" => [255, 140, 0, 255],
        "blue" => [0, 191, 255, 255],
        "pink" => [255, 105, 180, 255],
        "green" => [60, 200, 90, 255],
        "purple" => [170, 120, 255, 255],
        "red" => [235, 70, 70, 255],
        "yellow" => [252, 220, 30, 255],
        _ => [255, 255, 255, 255],
    }
}

pub fn pastel(name: &str) -> [u8; 4] {
    match name {
        "red" => [255, 162, 162, 204],
        "orange" => [255, 204, 162, 204],
        "yellow" => [253, 255, 162, 204],
        "green" => [170, 255, 162, 204],
        "blue" => [162, 213, 255, 204],
        "purple" => [198, 162, 255, 204],
        "pink" => [255, 162, 250, 204],
        _ => [255, 255, 255, 204],
    }
}
