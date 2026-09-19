//! macOS 专用：直接读 NSWindow 的真实绘制像素，验证「背景是不是真的透明」。
//!
//! 屏幕截图要「屏幕录制」权限，命令行里经常拿不到；这里改用
//! `cacheDisplayInRect:toBitmapImageRep:` 把窗口内容渲染进位图再读 alpha，
//! 是同一台机器上唯一能拿到地面真相的办法。
#![cfg(target_os = "macos")]

use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSBitmapImageRep};

/// 一次窗口像素体检的结果。
#[derive(Debug, Clone)]
pub struct WindowProbe {
    pub title: String,
    pub is_opaque: bool,
    pub background_alpha: f64,
    pub level: i64,
    pub has_shadow: bool,
    pub style_mask: u64,
    /// 位图尺寸（Retina 上是「点」的两倍）
    pub width: usize,
    pub height: usize,
    /// alpha <= 8 的像素占比
    pub transparent_ratio: f64,
    /// 出现次数最多的 alpha —— 浮窗背景就该是 0
    pub dominant_alpha: u8,
    /// 绿色（G 明显大于 R/B）且不透明的像素数
    pub green_pixels: usize,
    /// 完全透明的像素数
    pub holes: usize,
}

impl WindowProbe {
    pub fn report(&self) -> String {
        format!(
            "窗口标题      : {}\n\
             窗口不透明标志: {}  （true 表示系统把整窗当成不透明，背景就会变黑）\n\
             窗口背景 alpha: {:.3}\n\
             窗口层级      : {}  （3 = 悬浮在所有窗口之上）\n\
             有阴影        : {}\n\
             样式掩码      : {:#x}  （无边框时最低位为 0）\n\
             内容位图      : {} × {} 像素\n\
             背景 alpha 众数: {}\n\
             透明像素占比  : {:.1}%\n\
             绿色像素      : {}\n\
             全透明像素    : {}",
            self.title,
            self.is_opaque,
            self.background_alpha,
            self.level,
            self.has_shadow,
            self.style_mask,
            self.width,
            self.height,
            self.dominant_alpha,
            self.transparent_ratio * 100.0,
            self.green_pixels,
            self.holes,
        )
    }
}

/// 找到标题里含 `title_contains` 的窗口，把它渲染进位图统计 alpha。
pub fn probe(title_contains: &str) -> Option<WindowProbe> {
    let mtm = MainThreadMarker::new()?;
    let app = NSApplication::sharedApplication(mtm);
    let windows = app.windows();
    for window in windows.iter() {
        let title = window.title().to_string();
        if !title.contains(title_contains) {
            continue;
        }
        let view = window.contentView()?;
        let bounds = view.bounds();
        let rep: objc2::rc::Retained<NSBitmapImageRep> =
            view.bitmapImageRepForCachingDisplayInRect(bounds)?;
        view.cacheDisplayInRect_toBitmapImageRep(bounds, &rep);

        let data = rep.bitmapData();
        let row_bytes = rep.bytesPerRow() as usize;
        let samples = rep.samplesPerPixel() as usize;
        let width = rep.pixelsWide() as usize;
        let height = rep.pixelsHigh() as usize;
        if data.is_null() || width == 0 || height == 0 || samples < 4 {
            return None;
        }

        let mut total = 0usize;
        let mut transparent = 0usize;
        let mut green = 0usize;
        let mut holes = 0usize;
        let mut histogram = [0usize; 256];
        for y in 0..height {
            for x in 0..width {
                let offset = y * row_bytes + x * samples;
                let pixel = unsafe { std::slice::from_raw_parts(data.add(offset), samples) };
                let (red, green_ch, blue, alpha) = (pixel[0], pixel[1], pixel[2], pixel[3]);
                total += 1;
                histogram[alpha as usize] += 1;
                if alpha <= 8 {
                    transparent += 1;
                }
                if alpha == 0 {
                    holes += 1;
                }
                if alpha > 8 && green_ch > red && green_ch > blue {
                    green += 1;
                }
            }
        }
        let dominant = histogram
            .iter()
            .enumerate()
            .max_by_key(|(_, count)| **count)
            .map(|(alpha, _)| alpha as u8)
            .unwrap_or(255);

        return Some(WindowProbe {
            title,
            is_opaque: window.isOpaque(),
            background_alpha: window.backgroundColor().alphaComponent(),
            level: window.level() as i64,
            has_shadow: window.hasShadow(),
            style_mask: window.styleMask().0 as u64,
            width,
            height,
            transparent_ratio: if total == 0 {
                0.0
            } else {
                transparent as f64 / total as f64
            },
            dominant_alpha: dominant,
            green_pixels: green,
            holes,
        });
    }
    None
}

/// 悬浮窗要的是「只有文字」：关掉系统阴影，并且不在 Dock 里占一个图标。
pub fn tune_window() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    // Accessory：可以有窗口，但不进 Dock、不抢菜单栏
    if app.activationPolicy() != NSApplicationActivationPolicy::Accessory {
        let _ = app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    }
    for window in app.windows().iter() {
        // 系统会按窗口 alpha 算阴影，透明浮窗外面会多出一圈灰边
        if window.hasShadow() {
            window.setHasShadow(false);
        }
    }
}
