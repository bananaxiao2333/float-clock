//! Composite the three lines of content into a single RGBA image.
//!
//! This layer never touches the window system, so "what it looks like" can be
//! verified pixel by pixel in unit tests.

use chrono::NaiveDateTime;

use crate::config::Config;
use crate::pixmap::{Color, Pixmap};
use crate::text::{Fonts, GlyphRun, Ink};
use crate::timefmt::{format_hms, render_info, split_delta};

/// Where a line of content lands after it has been drawn (logical pixels).
#[derive(Debug, Clone, PartialEq)]
pub struct LineInfo {
    pub text: String,
    pub size: f32,
    pub x: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
    pub knockout: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub scale: f32,
    pub width: f32,
    pub height: f32,
    pub inset: f32,
    pub lines: Vec<LineInfo>,
    pub locked: bool,
    pub border_dashed: bool,
}

#[derive(Debug, Clone)]
pub struct Overlay {
    pub pixmap: Pixmap,
    pub layout: Layout,
}

impl Overlay {
    pub fn width(&self) -> u32 {
        self.pixmap.width
    }
    pub fn height(&self) -> u32 {
        self.pixmap.height
    }
}

pub struct RenderInput<'a> {
    pub config: &'a Config,
    pub fonts: &'a Fonts,
    pub target: NaiveDateTime,
    pub mark: NaiveDateTime,
    pub now: NaiveDateTime,
    pub locked: bool,
    /// Physical pixels per logical point
    pub scale: f32,
    /// Whether the platform supports true transparency; when it does not, a
    /// layer of config.display.background is painted underneath
    pub transparent: bool,
}

/// `{sign}` / `{clock}` interpolation for the main and sub titles.
pub fn compose(template: &str, sign: char, clock: &str) -> String {
    template
        .replace("{sign}", &sign.to_string())
        .replace("{clock}", clock)
}

struct Prepared {
    text: String,
    size: f32,
    run: GlyphRun,
    height: f32,
    ascent: f32,
    color: Color,
    knockout: bool,
}

pub fn render(input: &RenderInput<'_>) -> Result<Overlay, String> {
    let display = &input.config.display;
    let scale = input.scale.max(0.5);
    let physical = |points: f32| points * scale;

    let show_days = display.show_days;
    let (main_sign, main_secs) =
        split_delta((input.mark - input.now).num_milliseconds() as f64 / 1000.0);
    let (sub_sign, sub_secs) =
        split_delta((input.target - input.now).num_milliseconds() as f64 / 1000.0);

    let main_text = compose(
        &display.main_template,
        main_sign,
        &format_hms(main_secs, show_days),
    );
    let sub_text = compose(
        &display.sub_template,
        sub_sign,
        &format_hms(sub_secs, show_days),
    );
    let info_text = if display.info_template.trim().is_empty() {
        String::new()
    } else {
        render_info(
            &display.info_template,
            input.target,
            input.mark,
            (input.mark - input.target).num_milliseconds() as f64 / 1000.0,
            show_days,
        )
    };

    let main_color = Color::parse(&display.color)?;
    let sub_color = Color::parse(input.config.sub_color())?;
    let info_color = Color::parse(input.config.info_color())?;
    let border_color = Color::parse(input.config.border_color())?;

    let knockout_wanted = display.info_style != "text" && !info_text.trim().is_empty();

    let mut prepared: Vec<Prepared> = Vec::new();
    let specs: [(String, f32, Color, bool); 3] = [
        (main_text, display.main_size, main_color, false),
        (sub_text, display.sub_size, sub_color, false),
        (info_text, display.info_size, info_color, knockout_wanted),
    ];
    for (text, size, color, knockout) in specs {
        let size = size.max(1.0);
        let run = input.fonts.run(&text, physical(size));
        prepared.push(Prepared {
            height: input.fonts.line_height(physical(size)),
            ascent: input.fonts.ascent(physical(size)),
            text,
            size,
            run,
            color,
            knockout,
        });
    }

    // An empty third line must take up no height at all
    if prepared[2].text.trim().is_empty() {
        prepared.pop();
    }

    let border_on = display.lock_indicator == "border" && display.border_width > 0;
    let inset = if border_on {
        physical(display.border_width as f32 + 2.0)
    } else {
        0.0
    };
    let gap = physical(display.gap.max(0) as f32);

    let content_width = prepared
        .iter()
        .map(|line| line.run.width)
        .fold(0.0f32, f32::max);
    let content_height: f32 = prepared.iter().map(|line| line.height).sum::<f32>()
        + gap * (prepared.len().saturating_sub(1)) as f32;

    let width = (content_width + inset * 2.0).ceil().max(1.0);
    let height = (content_height + inset * 2.0).ceil().max(1.0);
    let mut pixmap = Pixmap::new(width as u32, height as u32);

    if !input.transparent {
        let background =
            Color::parse(&display.background).unwrap_or_else(|_| Color::rgb(16, 16, 16));
        pixmap.fill(background);
    }

    let mut lines: Vec<LineInfo> = Vec::with_capacity(prepared.len());
    let mut cursor = inset;
    for line in &prepared {
        let x = inset + (content_width - line.run.width) / 2.0;
        let baseline = cursor + line.ascent;
        if line.knockout {
            // Green block + knockout: the block spans the union of the layout
            // box and the ink bounds, so a glyph reaching outside its line box
            // does not leave a strip of residue that never got erased.
            // Note that the y of ink_bounds is relative to the **baseline**, so
            // baseline has to be added back in.
            let ink = input.fonts.ink_bounds(&line.run, line.size * scale);
            let (ink_left, ink_top, ink_right, ink_bottom) = match ink {
                Some((bx, by, bw, bh)) => (bx, baseline + by, bx + bw, baseline + by + bh),
                None => (0.0, cursor, line.run.width, cursor + line.height),
            };
            let left = ink_left.min(0.0);
            let right = ink_right.max(line.run.width);
            let top = ink_top.min(cursor);
            let bottom = ink_bottom.max(cursor + line.height);
            let (bx, by) = ((x + left).floor(), top.floor());
            let (bw, bh) = (
                (right - left).ceil().max(1.0),
                (bottom - top).ceil().max(1.0),
            );
            pixmap.rect(bx as i64, by as i64, bw as i64, bh as i64, line.color);
            input.fonts.draw(
                &mut pixmap,
                &line.run,
                line.size * scale,
                x,
                baseline,
                Ink::Erase,
            );
        } else {
            input.fonts.draw(
                &mut pixmap,
                &line.run,
                line.size * scale,
                x,
                baseline,
                Ink::Paint(line.color),
            );
        }
        lines.push(LineInfo {
            text: line.text.clone(),
            size: line.size,
            x: x / scale,
            top: cursor / scale,
            width: line.run.width / scale,
            height: line.height / scale,
            knockout: line.knockout,
        });
        cursor += line.height + gap;
    }

    let border_dashed = input.locked != display.solid_when_locked;
    if border_on {
        let thickness = (physical(display.border_width as f32).round() as i64).max(1);
        let dash = if border_dashed {
            Some((
                (4.0 * scale).round().max(1.0) as i64,
                (3.0 * scale).round().max(1.0) as i64,
            ))
        } else {
            None
        };
        pixmap.border_rect(
            0,
            0,
            width as i64,
            height as i64,
            thickness,
            border_color,
            dash,
        );
    }

    Ok(Overlay {
        pixmap,
        layout: Layout {
            scale,
            width,
            height,
            inset,
            lines,
            locked: input.locked,
            border_dashed,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::text::Fonts;
    use chrono::NaiveDate;

    fn dt(hh: u32, mm: u32, ss: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 9, 19)
            .unwrap()
            .and_hms_opt(hh, mm, ss)
            .unwrap()
    }

    fn scenario(config: &Config, now: NaiveDateTime, locked: bool) -> Overlay {
        let fonts = Fonts::load("").unwrap();
        let target = dt(20, 29, 0);
        let mark = dt(22, 29, 0);
        render(&RenderInput {
            config,
            fonts: &fonts,
            target,
            mark,
            now,
            locked,
            scale: 1.0,
            transparent: true,
        })
        .unwrap()
    }

    fn count(pixmap: &Pixmap, predicate: impl Fn([u8; 4]) -> bool) -> usize {
        pixmap
            .data
            .chunks_exact(4)
            .filter(|pixel| predicate([pixel[0], pixel[1], pixel[2], pixel[3]]))
            .count()
    }

    fn is_green(pixel: [u8; 4]) -> bool {
        pixel[3] > 0 && pixel[1] > pixel[0] && pixel[1] > pixel[2]
    }

    #[test]
    fn overlay_is_transparent_outside_the_text() {
        let mut config = Config::default();
        // The border already sits flush against the window edge, so turn it off
        // first before checking "is everything outside the text transparent"
        config.display.lock_indicator = "none".into();
        let overlay = scenario(&config, dt(20, 0, 0), false);
        assert!(overlay.width() > 10 && overlay.height() > 10);
        assert_eq!(
            overlay.pixmap.get(overlay.width() - 1, 0),
            [0, 0, 0, 0],
            "the top-right corner should be fully transparent"
        );
        assert_eq!(overlay.pixmap.get(0, overlay.height() - 1), [0, 0, 0, 0]);
        let transparent = count(&overlay.pixmap, |pixel| pixel[3] == 0);
        let total = (overlay.width() * overlay.height()) as usize;
        assert!(
            transparent * 2 > total,
            "most of the area outside the text should be transparent: {transparent}/{total}"
        );
    }

    #[test]
    fn main_title_shows_the_offset_countdown() {
        let config = Config::default();
        let overlay = scenario(&config, dt(20, 0, 0), false);
        // Mark 22:29, now 20:00 -> 2:29:00 to go
        assert_eq!(overlay.layout.lines[0].text, "T-02:29:00");
        // Target 20:29, now 20:00 -> 29:00 to go
        assert_eq!(overlay.layout.lines[1].text, "T-00:29:00");
        // Third line: target + offset
        assert_eq!(overlay.layout.lines[2].text, "20:29:00 | +02:00:00");
    }

    #[test]
    fn signs_flip_once_the_moment_passed() {
        let config = Config::default();
        let overlay = scenario(&config, dt(21, 0, 0), false);
        assert_eq!(overlay.layout.lines[0].text, "T-01:29:00");
        assert_eq!(overlay.layout.lines[1].text, "T+00:31:00");
    }

    #[test]
    fn every_field_is_zero_padded() {
        let config = Config::default();
        let overlay = scenario(&config, dt(22, 28, 59), false);
        assert_eq!(overlay.layout.lines[0].text, "T-00:00:01");
        assert_eq!(overlay.layout.lines[1].text, "T+01:59:59");
    }

    #[test]
    fn third_line_is_a_green_block_with_knocked_out_glyphs() {
        let config = Config::default();
        let overlay = scenario(&config, dt(20, 0, 0), false);
        assert!(overlay.layout.lines[2].knockout);
        let green = count(&overlay.pixmap, is_green);
        let holes = count(&overlay.pixmap, |pixel| pixel[3] == 0);
        assert!(
            green > 200,
            "there should be a green block, got {green} pixels"
        );
        assert!(
            holes > 200,
            "the knockout should erase many pixels, got {holes}"
        );
    }

    /// The longest run of "solid color" within one row: only the green block of
    /// the third line produces very long stretches of consecutive opaque pixels.
    fn longest_opaque_run(pixmap: &Pixmap, y: u32) -> u32 {
        let mut best = 0;
        let mut current = 0;
        for x in 0..pixmap.width {
            if pixmap.get(x, y)[3] == 255 {
                current += 1;
                best = best.max(current);
            } else {
                current = 0;
            }
        }
        best
    }

    #[test]
    fn the_knockout_block_does_not_creep_up_over_the_subtitle() {
        let config = Config::default();
        let overlay = scenario(&config, dt(20, 0, 0), false);
        let info_top = overlay.layout.lines[2].top.floor() as u32;
        let block_top = (0..overlay.height())
            .find(|y| longest_opaque_run(&overlay.pixmap, *y) >= 40)
            .expect("should find the green block of the third line");
        assert!(
            block_top >= info_top.saturating_sub(1),
            "the top of the green block ({block_top}) must not creep above the top of the third line ({info_top})"
        );
        // And the green block has to cover the height of the third line itself
        assert!((block_top as f32) <= info_top as f32 + 2.0);
    }

    #[test]
    fn info_style_text_disables_knockout() {
        let mut config = Config::default();
        config.display.info_style = "text".into();
        let overlay = scenario(&config, dt(20, 0, 0), false);
        assert!(!overlay.layout.lines[2].knockout);
    }

    #[test]
    fn empty_info_template_drops_the_third_line() {
        let mut config = Config::default();
        config.display.info_template = String::new();
        let fonts = Fonts::load("").unwrap();
        let overlay = render(&RenderInput {
            config: &config,
            fonts: &fonts,
            target: dt(20, 29, 0),
            mark: dt(22, 29, 0),
            now: dt(20, 0, 0),
            locked: false,
            scale: 1.0,
            transparent: true,
        })
        .unwrap();
        assert_eq!(overlay.layout.lines.len(), 2);
    }

    #[test]
    fn locked_draws_a_solid_border() {
        let config = Config::default();
        let overlay = scenario(&config, dt(20, 0, 0), true);
        assert!(
            !overlay.layout.border_dashed,
            "it should be solid while locked"
        );
        let top: Vec<u8> = (0..overlay.width())
            .map(|x| overlay.pixmap.get(x, 0)[3])
            .collect();
        assert!(
            top.iter().all(|alpha| *alpha > 200),
            "a solid border has no gaps"
        );
    }

    #[test]
    fn unlocked_draws_a_dashed_border() {
        let config = Config::default();
        let overlay = scenario(&config, dt(20, 0, 0), false);
        assert!(
            overlay.layout.border_dashed,
            "it should be dashed while unlocked"
        );
        let top: Vec<u8> = (0..overlay.width())
            .map(|x| overlay.pixmap.get(x, 0)[3])
            .collect();
        assert!(top.contains(&0), "a dashed border must have gaps");
        assert!(
            top.iter().any(|alpha| *alpha > 200),
            "dashes need solid parts too"
        );
    }

    #[test]
    fn lock_indicator_none_removes_the_border() {
        let mut config = Config::default();
        config.display.lock_indicator = "none".into();
        let overlay = scenario(&config, dt(20, 0, 0), true);
        let top: Vec<u8> = (0..overlay.width())
            .map(|x| overlay.pixmap.get(x, 0)[3])
            .collect();
        assert!(
            top.iter().all(|alpha| *alpha == 0),
            "there should be no border"
        );
    }

    #[test]
    fn opaque_mode_fills_the_background() {
        let config = Config::default();
        let fonts = Fonts::load("").unwrap();
        let overlay = render(&RenderInput {
            config: &config,
            fonts: &fonts,
            target: dt(20, 29, 0),
            mark: dt(22, 29, 0),
            now: dt(20, 0, 0),
            locked: false,
            scale: 1.0,
            transparent: false,
        })
        .unwrap();
        assert_eq!(overlay.pixmap.get(overlay.width() - 1, 0)[3], 255);
    }

    #[test]
    fn bigger_main_size_means_bigger_overlay() {
        let small = scenario(&Config::default(), dt(20, 0, 0), false);
        let mut config = Config::default();
        config.display.main_size = 92.0;
        let large = scenario(&config, dt(20, 0, 0), false);
        assert!(large.width() > small.width());
        assert!(large.height() > small.height());
    }

    #[test]
    fn scale_multiplies_the_pixel_size() {
        let config = Config::default();
        let fonts = Fonts::load("").unwrap();
        let render_at = |scale: f32| {
            render(&RenderInput {
                config: &config,
                fonts: &fonts,
                target: dt(20, 29, 0),
                mark: dt(22, 29, 0),
                now: dt(20, 0, 0),
                locked: false,
                scale,
                transparent: true,
            })
            .unwrap()
        };
        let normal = render_at(1.0);
        let retina = render_at(2.0);
        assert_eq!(retina.width(), normal.width() * 2);
        assert_eq!(retina.height(), normal.height() * 2);
    }

    #[test]
    fn bad_color_is_reported() {
        let mut config = Config::default();
        config.display.color = "not a color".into();
        let fonts = Fonts::load("").unwrap();
        let result = render(&RenderInput {
            config: &config,
            fonts: &fonts,
            target: dt(20, 29, 0),
            mark: dt(22, 29, 0),
            now: dt(20, 0, 0),
            locked: false,
            scale: 1.0,
            transparent: true,
        });
        assert!(result.is_err());
    }
}
