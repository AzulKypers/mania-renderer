use fontdue::{Font, FontSettings, Metrics};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tiny_skia::Pixmap;

pub static UBUNTU: &[u8] = include_bytes!("../../assets/fonts/Ubuntu-Regular.ttf");
pub static NOTO_JP: &[u8] = include_bytes!("../../assets/fonts/NotoSansJP-Regular.ttf");
pub static NOTO_KR: &[u8] = include_bytes!("../../assets/fonts/NotoSansKR-Regular.ttf");

#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Center,
    Right,
}

type Glyph = Arc<(Metrics, Vec<u8>)>;

pub struct TextRenderer {
    fonts: Vec<Font>,
    cache: RwLock<HashMap<(char, u32, bool), Glyph>>,
}

impl TextRenderer {
    pub fn new() -> TextRenderer {
        let fonts = [UBUNTU, NOTO_JP, NOTO_KR]
            .iter()
            .filter_map(|b| Font::from_bytes(*b, FontSettings::default()).ok())
            .collect();
        TextRenderer {
            fonts,
            cache: RwLock::new(HashMap::new()),
        }
    }

    fn font_for(&self, c: char) -> &Font {
        self.fonts
            .iter()
            .find(|f| f.lookup_glyph_index(c) != 0)
            .unwrap_or(&self.fonts[0])
    }

    fn glyph(&self, c: char, size: f32, bold: bool) -> Glyph {
        let key = (c, (size * 4.0).round() as u32, bold);
        if let Some(g) = self.cache.read().unwrap().get(&key) {
            return g.clone();
        }
        let (metrics, mut bitmap) = self.font_for(c).rasterize(c, size);
        if bold && metrics.width > 0 {
            let w = metrics.width;
            let src = bitmap.clone();
            for y in 0..metrics.height {
                for x in 1..w {
                    let i = y * w + x;
                    bitmap[i] = bitmap[i].max(src[i - 1]);
                }
            }
        }
        let g = Arc::new((metrics, bitmap));
        self.cache.write().unwrap().insert(key, g.clone());
        g
    }

    pub fn measure(&self, text: &str, size: f32, bold: bool) -> f32 {
        text.chars()
            .map(|c| self.glyph(c, size, bold).0.advance_width)
            .sum()
    }

    pub fn draw(
        &self,
        dst: &mut Pixmap,
        text: &str,
        x: f32,
        y_top: f32,
        size: f32,
        color: [u8; 4],
        align: Align,
        bold: bool,
    ) {
        let width = self.measure(text, size, bold);
        let mut pen = match align {
            Align::Left => x,
            Align::Center => x - width / 2.0,
            Align::Right => x - width,
        };
        let baseline = y_top + size * 0.8;
        let (dw, dh) = (dst.width() as i32, dst.height() as i32);
        let stride = dst.width() as usize * 4;
        let data = dst.data_mut();
        for c in text.chars() {
            let g = self.glyph(c, size, bold);
            let (m, bitmap) = (&g.0, &g.1);
            let gx = (pen + m.xmin as f32).round() as i32;
            let gy = (baseline - m.height as f32 - m.ymin as f32).round() as i32;
            for row in 0..m.height as i32 {
                let py = gy + row;
                if py < 0 || py >= dh {
                    continue;
                }
                for col in 0..m.width as i32 {
                    let px = gx + col;
                    if px < 0 || px >= dw {
                        continue;
                    }
                    let cov = bitmap[(row * m.width as i32 + col) as usize] as u32;
                    if cov == 0 {
                        continue;
                    }
                    let a = cov * color[3] as u32 / 255;
                    let i = py as usize * stride + px as usize * 4;
                    for k in 0..3 {
                        let s = color[k] as u32 * a / 255;
                        data[i + k] = (s + data[i + k] as u32 * (255 - a) / 255) as u8;
                    }
                    data[i + 3] = (a + data[i + 3] as u32 * (255 - a) / 255) as u8;
                }
            }
            pen += m.advance_width;
        }
    }

    pub fn draw_fit(
        &self,
        dst: &mut Pixmap,
        text: &str,
        x: f32,
        y_top: f32,
        max_width: f32,
        size: f32,
        color: [u8; 4],
        align: Align,
        bold: bool,
    ) {
        let width = self.measure(text, size, bold);
        let size = if width > max_width && width > 0.0 {
            (size * max_width / width).max(size * 0.55)
        } else {
            size
        };
        self.draw(dst, text, x, y_top + (size * 0.0), size, color, align, bold);
    }
}
