//! 字体查找、排版与栅格化。
//!
//! 自己栅格化（而不是交给 UI 框架）是为了让三个平台的观感完全一致，
//! 也为了能实现「绿块 + 镂空字」——把已经画上去的像素重新抠成透明。

use std::path::Path;

use ab_glyph::{Font, FontVec, Glyph, GlyphId, PxScale, ScaleFont};

use crate::pixmap::{Color, Pixmap};

/// 各平台的等宽字体候选（顺序即优先级）。
pub const MONO_CANDIDATES: &[&str] = &[
    "Menlo",
    "SF Mono",
    "Monaco",
    "JetBrains Mono",
    "Fira Code",
    "Cascadia Mono",
    "Consolas",
    "DejaVu Sans Mono",
    "Liberation Mono",
    "Noto Sans Mono",
    "Ubuntu Mono",
    "Courier New",
];

/// 等宽字体没有的字（中文、`·` 之类）交给这些字体兜底。
pub const FALLBACK_CANDIDATES: &[&str] = &[
    "PingFang SC",
    "Hiragino Sans GB",
    "Heiti SC",
    "Songti SC",
    "Microsoft YaHei",
    "SimHei",
    "Noto Sans CJK SC",
    "Noto Sans SC",
    "Source Han Sans SC",
    "WenQuanYi Micro Hei",
    "DejaVu Sans",
    "Arial Unicode MS",
    "Segoe UI",
];

/// 画文字时的两种「墨」：正常上色，或者把像素抠掉。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ink {
    Paint(Color),
    Erase,
}

pub struct Fonts {
    primary: FontVec,
    pub primary_name: String,
    fallback: Option<FontVec>,
    pub fallback_name: Option<String>,
}

impl std::fmt::Debug for Fonts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fonts")
            .field("primary", &self.primary_name)
            .field("fallback", &self.fallback_name)
            .finish()
    }
}

impl Fonts {
    /// 按配置的字体族挑字体；留空则自动找系统等宽字体。
    pub fn load(family: &str) -> Result<Self, String> {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();

        let requested = family.trim();
        let (id, name) = if !requested.is_empty() {
            match query_family(&db, requested) {
                Some(id) => (id, requested.to_string()),
                None => return Err(format!("找不到字体 {requested:?}，请检查 font_family 拼写")),
            }
        } else {
            let mut found = None;
            for candidate in MONO_CANDIDATES {
                if let Some(id) = query_family(&db, candidate) {
                    found = Some((id, (*candidate).to_string()));
                    break;
                }
            }
            match found.or_else(|| {
                db.query(&fontdb::Query {
                    families: &[fontdb::Family::Monospace],
                    ..Default::default()
                })
                .map(|id| (id, "monospace".to_string()))
            }) {
                Some(pair) => pair,
                None => return Err("系统里找不到任何等宽字体，请在 font_family 里指定一个".into()),
            }
        };

        let primary =
            load_face(&db, id).ok_or_else(|| format!("字体 {name:?} 无法解析为可用字形数据"))?;

        let mut fallback = None;
        let mut fallback_name = None;
        for candidate in FALLBACK_CANDIDATES {
            if let Some(id) = query_family(&db, candidate) {
                if let Some(face) = load_face(&db, id) {
                    fallback = Some(face);
                    fallback_name = Some((*candidate).to_string());
                    break;
                }
            }
        }

        Ok(Self {
            primary,
            primary_name: name,
            fallback,
            fallback_name,
        })
    }

    /// 直接从字体文件加载（测试与 `--font-file` 用）。
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let data = std::fs::read(path).map_err(|e| format!("读取字体失败：{e}"))?;
        let index = 0u32;
        let primary = FontVec::try_from_vec_and_index(data, index)
            .map_err(|e| format!("解析字体失败：{e}"))?;
        Ok(Self {
            primary,
            primary_name: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "font".into()),
            fallback: None,
            fallback_name: None,
        })
    }

    /// 一行的高度（ascent - descent）。
    pub fn line_height(&self, px: f32) -> f32 {
        let scaled = self.primary.as_scaled(PxScale::from(px));
        scaled.height()
    }

    pub fn ascent(&self, px: f32) -> f32 {
        self.primary.as_scaled(PxScale::from(px)).ascent()
    }

    /// 找出能画这个字符的字体槽位。
    fn slot_for(&self, ch: char) -> Option<bool> {
        if self.primary.glyph_id(ch).0 != 0 {
            return Some(false);
        }
        match &self.fallback {
            Some(face) if face.glyph_id(ch).0 != 0 => Some(true),
            _ => None,
        }
    }

    /// 简单逐字排版。等宽字体下每个字符推进量相同，够用了。
    pub fn run(&self, text: &str, px: f32) -> GlyphRun {
        let scale = PxScale::from(px);
        let primary = self.primary.as_scaled(scale);
        let fallback = self.fallback.as_ref().map(|face| face.as_scaled(scale));
        let mut pen_x = 0.0f32;
        let mut glyphs = Vec::with_capacity(text.chars().count());
        for ch in text.chars() {
            let Some(use_fallback) = self.slot_for(ch) else {
                // 两个字体都没有这个字：跳过，免得画出 .notdef 方块
                if ch == ' ' {
                    pen_x += primary.h_advance(primary.glyph_id(' '));
                }
                continue;
            };
            if use_fallback {
                let scaled = fallback.as_ref().expect("fallback 已确认存在");
                let id = scaled.glyph_id(ch);
                glyphs.push(Placed {
                    fallback: true,
                    id,
                    pen_x,
                });
                pen_x += scaled.h_advance(id);
            } else {
                let id = primary.glyph_id(ch);
                glyphs.push(Placed {
                    fallback: false,
                    id,
                    pen_x,
                });
                pen_x += primary.h_advance(id);
            }
        }
        GlyphRun {
            glyphs,
            width: pen_x,
        }
    }

    /// 把一行字画到画布上。`baseline` 是基线在画布里的 y。
    pub fn draw(
        &self,
        pixmap: &mut Pixmap,
        run: &GlyphRun,
        px: f32,
        origin_x: f32,
        baseline: f32,
        ink: Ink,
    ) {
        let scale = PxScale::from(px);
        let primary = self.primary.as_scaled(scale);
        let fallback = self.fallback.as_ref().map(|face| face.as_scaled(scale));
        for placed in &run.glyphs {
            let scaled = if placed.fallback {
                match fallback.as_ref() {
                    Some(scaled) => scaled,
                    None => continue,
                }
            } else {
                &primary
            };
            let glyph = Glyph {
                id: placed.id,
                scale,
                position: ab_glyph::point(origin_x + placed.pen_x, baseline),
            };
            let Some(outlined) = scaled.outline_glyph(glyph) else {
                continue;
            };
            let bounds = outlined.px_bounds();
            let (left, top) = (bounds.min.x, bounds.min.y);
            outlined.draw(|x, y, coverage| {
                let px = left as i64 + x as i64;
                let py = top as i64 + y as i64;
                match ink {
                    Ink::Paint(color) => pixmap.blend(px, py, color, coverage),
                    Ink::Erase => pixmap.erase(px, py, coverage),
                }
            });
        }
    }

    /// 一行文字的紧致墨迹范围（左上角相对基线原点的偏移 + 尺寸）。
    pub fn ink_bounds(&self, run: &GlyphRun, px: f32) -> Option<(f32, f32, f32, f32)> {
        let scale = PxScale::from(px);
        let primary = self.primary.as_scaled(scale);
        let fallback = self.fallback.as_ref().map(|face| face.as_scaled(scale));
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for placed in &run.glyphs {
            let scaled = if placed.fallback {
                match fallback.as_ref() {
                    Some(scaled) => scaled,
                    None => continue,
                }
            } else {
                &primary
            };
            let glyph = Glyph {
                id: placed.id,
                scale,
                position: ab_glyph::point(placed.pen_x, 0.0),
            };
            if let Some(outlined) = scaled.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                min_x = min_x.min(bounds.min.x);
                min_y = min_y.min(bounds.min.y);
                max_x = max_x.max(bounds.max.x);
                max_y = max_y.max(bounds.max.y);
            }
        }
        if min_x > max_x {
            return None;
        }
        Some((min_x, min_y, max_x - min_x, max_y - min_y))
    }
}

fn query_family(db: &fontdb::Database, name: &str) -> Option<fontdb::ID> {
    let families = [fontdb::Family::Name(name)];
    for weight in [fontdb::Weight::BOLD, fontdb::Weight::NORMAL] {
        if let Some(id) = db.query(&fontdb::Query {
            families: &families,
            weight,
            stretch: fontdb::Stretch::Normal,
            style: fontdb::Style::Normal,
        }) {
            return Some(id);
        }
    }
    None
}

fn load_face(db: &fontdb::Database, id: fontdb::ID) -> Option<FontVec> {
    db.with_face_data(id, |data, index| {
        FontVec::try_from_vec_and_index(data.to_vec(), index).ok()
    })
    .flatten()
}

/// 已排版好的一个字形。
#[derive(Debug, Clone, Copy)]
pub struct Placed {
    pub fallback: bool,
    pub id: GlyphId,
    pub pen_x: f32,
}

/// 一行的排版结果。
#[derive(Debug, Clone, Default)]
pub struct GlyphRun {
    pub glyphs: Vec<Placed>,
    pub width: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fonts() -> Fonts {
        Fonts::load("").expect("系统里应该有等宽字体")
    }

    #[test]
    fn picks_a_monospace_font() {
        let fonts = fonts();
        assert!(!fonts.primary_name.is_empty());
    }

    #[test]
    fn monospace_digits_all_share_one_advance() {
        let fonts = fonts();
        let run = fonts.run("0123456789", 20.0);
        let advances: Vec<f32> = run
            .glyphs
            .windows(2)
            .map(|pair| pair[1].pen_x - pair[0].pen_x)
            .collect();
        let first = advances[0];
        for advance in &advances {
            assert!(
                (advance - first).abs() < 0.01,
                "等宽字体每位数推进量应该一致：{advance} vs {first}"
            );
        }
        assert!((run.width - first * 10.0).abs() < 0.5);
    }

    #[test]
    fn wider_size_means_wider_text() {
        let fonts = fonts();
        let small = fonts.run("T-00:00:00", 12.0).width;
        let large = fonts.run("T-00:00:00", 46.0).width;
        assert!(large > small * 3.0);
    }

    #[test]
    fn painting_writes_green_pixels() {
        let fonts = fonts();
        let run = fonts.run("8", 40.0);
        let mut pixmap = Pixmap::new(60, 60);
        let baseline = fonts.ascent(40.0) + 10.0;
        fonts.draw(
            &mut pixmap,
            &run,
            40.0,
            10.0,
            baseline,
            Ink::Paint(Color::rgb(0, 255, 102)),
        );
        let painted = pixmap
            .data
            .chunks_exact(4)
            .filter(|pixel| pixel[3] > 0)
            .count();
        assert!(painted > 50, "应该画出实心数字，实际 {painted} 像素");
        let opaque = pixmap
            .data
            .chunks_exact(4)
            .filter(|pixel| pixel[3] > 200)
            .count();
        assert!(opaque > 20, "应该有实心部分，实际 {opaque} 像素");
    }

    #[test]
    fn erasing_leaves_holes_where_the_glyph_is() {
        let fonts = fonts();
        let run = fonts.run("8", 40.0);
        let mut pixmap = Pixmap::new(60, 60);
        let baseline = fonts.ascent(40.0) + 10.0;
        pixmap.fill(Color::rgb(0, 255, 102));
        fonts.draw(&mut pixmap, &run, 40.0, 10.0, baseline, Ink::Erase);
        let holes = pixmap
            .data
            .chunks_exact(4)
            .filter(|pixel| pixel[3] == 0)
            .count();
        assert!(holes > 20, "镂空应该抠出洞，实际 {holes} 像素");
        assert!(pixmap.get(0, 0)[3] > 0, "角落应该还保留绿色");
    }

    #[test]
    fn unknown_family_is_an_error() {
        assert!(Fonts::load("绝对不存在的字体名").is_err());
    }
}
