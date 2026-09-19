//! 极简 RGBA 画布：直通（非预乘）alpha，足够的混合与擦除操作。
//!
//! 悬浮窗需要「绿块 + 镂空字」，也就是要能把已经画上去的像素重新抠成透明，
//! 所以这里除了覆盖混合还提供 `erase`。

/// 颜色。`a` 用 0.0~1.0 的浮点，方便做抗锯齿覆盖度运算。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f32,
}

impl Color {
    pub const TRANSPARENT: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 0.0,
    };

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// 解析 `#RRGGBB` / `#RGB` / `RRGGBB`；带 alpha 的 `#RRGGBBAA` 也认。
    pub fn parse(text: &str) -> Result<Self, String> {
        let hex = text.trim().trim_start_matches('#');
        let digits: Vec<u8> = hex
            .chars()
            .map(|c| c.to_digit(16).map(|v| v as u8))
            .collect::<Option<Vec<u8>>>()
            .ok_or_else(|| format!("颜色只能由 16 进制字符组成：{text:?}"))?;
        match digits.len() {
            3 => Ok(Color::rgb(digits[0] * 17, digits[1] * 17, digits[2] * 17)),
            6 => Ok(Color::rgb(
                digits[0] * 16 + digits[1],
                digits[2] * 16 + digits[3],
                digits[4] * 16 + digits[5],
            )),
            8 => Ok(Color {
                r: digits[0] * 16 + digits[1],
                g: digits[2] * 16 + digits[3],
                b: digits[4] * 16 + digits[5],
                a: (digits[6] * 16 + digits[7]) as f32 / 255.0,
            }),
            _ => Err(format!("颜色需要 3、6 或 8 位 16 进制：{text:?}")),
        }
    }

    pub fn with_alpha(self, a: f32) -> Self {
        Self {
            a: a.clamp(0.0, 1.0),
            ..self
        }
    }

    pub fn to_rgba8(self) -> [u8; 4] {
        [
            self.r,
            self.g,
            self.b,
            (self.a.clamp(0.0, 1.0) * 255.0).round() as u8,
        ]
    }
}

/// 一块 RGBA 像素，行优先。
#[derive(Debug, Clone)]
pub struct Pixmap {
    pub width: u32,
    pub height: u32,
    /// 长度 = width * height * 4，顺序 R G B A
    pub data: Vec<u8>,
}

impl Pixmap {
    pub fn new(width: u32, height: u32) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        Self {
            width,
            height,
            data: vec![0; (width as usize) * (height as usize) * 4],
        }
    }

    pub fn fill(&mut self, color: Color) {
        let rgba = color.to_rgba8();
        for chunk in self.data.chunks_exact_mut(4) {
            chunk.copy_from_slice(&rgba);
        }
    }

    #[inline]
    fn index(&self, x: u32, y: u32) -> usize {
        ((y as usize) * (self.width as usize) + (x as usize)) * 4
    }

    #[inline]
    pub fn get(&self, x: u32, y: u32) -> [u8; 4] {
        if x >= self.width || y >= self.height {
            return [0, 0, 0, 0];
        }
        let index = self.index(x, y);
        [
            self.data[index],
            self.data[index + 1],
            self.data[index + 2],
            self.data[index + 3],
        ]
    }

    /// 把一个带覆盖度的前景色覆盖混合到 (x, y)。
    pub fn blend(&mut self, x: i64, y: i64, color: Color, coverage: f32) {
        if coverage <= 0.0 || x < 0 || y < 0 {
            return;
        }
        let (x, y) = (x as u32, y as u32);
        if x >= self.width || y >= self.height {
            return;
        }
        let sa = (color.a * coverage).clamp(0.0, 1.0);
        if sa <= 0.0 {
            return;
        }
        let index = self.index(x, y);
        let da = self.data[index + 3] as f32 / 255.0;
        let out_a = sa + da * (1.0 - sa);
        if out_a <= 0.0 {
            self.data[index] = 0;
            self.data[index + 1] = 0;
            self.data[index + 2] = 0;
            self.data[index + 3] = 0;
            return;
        }
        for (offset, src) in [(0usize, color.r), (1, color.g), (2, color.b)] {
            let dst = self.data[index + offset] as f32 / 255.0;
            let src = src as f32 / 255.0;
            let out = (src * sa + dst * da * (1.0 - sa)) / out_a;
            self.data[index + offset] = (out.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
        self.data[index + 3] = (out_a.clamp(0.0, 1.0) * 255.0).round() as u8;
    }

    /// 按覆盖度擦除：覆盖度 1 的像素直接变成全透明（镂空字的做法）。
    pub fn erase(&mut self, x: i64, y: i64, coverage: f32) {
        if coverage <= 0.0 || x < 0 || y < 0 {
            return;
        }
        let (x, y) = (x as u32, y as u32);
        if x >= self.width || y >= self.height {
            return;
        }
        let index = self.index(x, y);
        let alpha = self.data[index + 3] as f32 / 255.0;
        let out = alpha * (1.0 - coverage.clamp(0.0, 1.0));
        self.data[index + 3] = (out * 255.0).round() as u8;
    }

    pub fn rect(&mut self, x: i64, y: i64, width: i64, height: i64, color: Color) {
        for row in y..(y + height) {
            for column in x..(x + width) {
                self.blend(column, row, color, 1.0);
            }
        }
    }

    /// 描一个矩形边框；`dash` 为 `Some((实线长, 空白长))` 时画虚线。
    // 参数多，但每一个都是独立的几何量，包成结构体反而更难读
    #[allow(clippy::too_many_arguments)]
    pub fn border_rect(
        &mut self,
        x: i64,
        y: i64,
        width: i64,
        height: i64,
        thickness: i64,
        color: Color,
        dash: Option<(i64, i64)>,
    ) {
        let thickness = thickness.max(1);
        let (right, bottom) = (x + width, y + height);
        let on = |position: i64| match dash {
            None => true,
            Some((solid, gap)) => {
                let period = (solid + gap).max(1);
                (position.rem_euclid(period)) < solid
            }
        };
        for offset in 0..thickness {
            for column in x..right {
                if on(column - x) {
                    self.blend(column, y + offset, color, 1.0);
                    self.blend(column, bottom - 1 - offset, color, 1.0);
                }
            }
            for row in y..bottom {
                if on(row - y) {
                    self.blend(x + offset, row, color, 1.0);
                    self.blend(right - 1 - offset, row, color, 1.0);
                }
            }
        }
    }

    /// 编码成 PNG（`--render-png` 自检用）。
    pub fn to_png(&self) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, self.width, self.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder
                .write_header()
                .map_err(|e| format!("PNG 头写入失败：{e}"))?;
            writer
                .write_image_data(&self.data)
                .map_err(|e| format!("PNG 数据写入失败：{e}"))?;
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_colors() {
        assert_eq!(Color::parse("#00FF66").unwrap(), Color::rgb(0, 255, 102));
        assert_eq!(Color::parse("0f6").unwrap(), Color::rgb(0, 255, 102));
        assert_eq!(Color::parse("#00000000").unwrap().a, 0.0);
        assert!(Color::parse("#12345").is_err());
        assert!(Color::parse("zzzzzz").is_err());
    }

    #[test]
    fn new_pixmap_is_fully_transparent() {
        let pixmap = Pixmap::new(4, 3);
        assert_eq!(pixmap.data.len(), 4 * 3 * 4);
        assert!(pixmap.data.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn blend_opaque_writes_exact_color() {
        let mut pixmap = Pixmap::new(2, 2);
        pixmap.blend(1, 1, Color::rgb(0, 255, 102), 1.0);
        assert_eq!(pixmap.get(1, 1), [0, 255, 102, 255]);
        assert_eq!(pixmap.get(0, 0), [0, 0, 0, 0]);
    }

    #[test]
    fn blend_half_coverage_is_semi_transparent() {
        let mut pixmap = Pixmap::new(1, 1);
        pixmap.blend(0, 0, Color::rgb(0, 255, 102), 0.5);
        let [r, g, b, a] = pixmap.get(0, 0);
        assert_eq!((r, g, b), (0, 255, 102));
        assert!(
            (a as i32 - 128).abs() <= 2,
            "alpha 应该约等于 128，实际 {a}"
        );
    }

    #[test]
    fn erase_punches_holes() {
        let mut pixmap = Pixmap::new(3, 1);
        pixmap.fill(Color::rgb(0, 255, 102));
        pixmap.erase(0, 0, 1.0);
        pixmap.erase(1, 0, 0.5);
        assert_eq!(pixmap.get(0, 0)[3], 0, "覆盖度 1 应该完全镂空");
        assert!((pixmap.get(1, 0)[3] as i32 - 128).abs() <= 2);
        assert_eq!(pixmap.get(2, 0)[3], 255);
    }

    #[test]
    fn drawing_outside_bounds_is_ignored() {
        let mut pixmap = Pixmap::new(2, 2);
        pixmap.blend(-1, 0, Color::rgb(255, 0, 0), 1.0);
        pixmap.blend(9, 9, Color::rgb(255, 0, 0), 1.0);
        pixmap.erase(-3, -3, 1.0);
        assert!(pixmap.data.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn solid_border_has_no_gaps() {
        let mut pixmap = Pixmap::new(10, 10);
        pixmap.border_rect(0, 0, 10, 10, 1, Color::rgb(255, 0, 0), None);
        for index in 0..10 {
            assert_eq!(pixmap.get(index, 0)[3], 255, "顶边第 {index} 列不该有缺口");
            assert_eq!(pixmap.get(0, index)[3], 255, "左边第 {index} 行不该有缺口");
        }
        assert_eq!(pixmap.get(5, 5)[3], 0, "中间应该是空的");
    }

    #[test]
    fn dashed_border_has_gaps() {
        let mut pixmap = Pixmap::new(40, 10);
        pixmap.border_rect(0, 0, 40, 10, 1, Color::rgb(255, 0, 0), Some((4, 3)));
        let opaque = (0..40).filter(|x| pixmap.get(*x, 0)[3] == 255).count();
        let holes = (0..40).filter(|x| pixmap.get(*x, 0)[3] == 0).count();
        assert!(opaque > 10, "虚线也要有足够的实线段：{opaque}");
        assert!(holes > 10, "虚线必须有缺口：{holes}");
    }

    #[test]
    fn png_round_trip_has_ihdr() {
        let mut pixmap = Pixmap::new(2, 2);
        pixmap.fill(Color::rgb(0, 255, 102));
        let bytes = pixmap.to_png().unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&bytes[12..16], b"IHDR");
    }
}
