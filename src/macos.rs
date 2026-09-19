//! macOS only: read the pixels an NSWindow actually drew, to prove that the
//! background really is transparent.
//!
//! Taking a screenshot needs the Screen Recording permission, which a command
//! line process rarely has. Rendering the window into a bitmap with
//! `cacheDisplayInRect:toBitmapImageRep:` and then reading the alpha channel is
//! the only way to get at the ground truth on the same machine.
#![cfg(target_os = "macos")]

use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSBitmapImageRep};

/// The outcome of one window pixel health check.
#[derive(Debug, Clone)]
pub struct WindowProbe {
    pub title: String,
    pub is_opaque: bool,
    pub background_alpha: f64,
    pub level: i64,
    pub has_shadow: bool,
    pub style_mask: u64,
    /// Bitmap size; on a Retina display this is twice the size in points
    pub width: usize,
    pub height: usize,
    /// Share of pixels with alpha <= 8
    pub transparent_ratio: f64,
    /// The most common alpha value - which should be 0 for the overlay background
    pub dominant_alpha: u8,
    /// Opaque pixels that are green (G clearly above R and B)
    pub green_pixels: usize,
    /// Number of fully transparent pixels
    pub holes: usize,
}

impl WindowProbe {
    pub fn report(&self) -> String {
        format!(
            "window title      : {}\n\
             window is opaque  : {}  (true means the system treats the whole window as opaque)\n\
             window bg alpha   : {:.3}\n\
             window level      : {}  (3 = floats above every other window)\n\
             has shadow        : {}\n\
             style mask        : {:#x}  (lowest bit is 0 when borderless)\n\
             content bitmap    : {} x {} pixels\n\
             dominant bg alpha : {}\n\
             transparent share : {:.1}%\n\
             green pixels      : {}\n\
             fully transparent : {}",
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

/// Find the window whose title contains `title_contains`, render it into a
/// bitmap and measure its alpha channel.
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

/// The overlay should be nothing but text: turn the system shadow off, and do
/// not take up an icon in the Dock.
pub fn tune_window() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    // Accessory: may own windows, but stays out of the Dock and does not take
    // over the menu bar. The tray icon is an NSStatusItem and works fine here.
    if app.activationPolicy() != NSApplicationActivationPolicy::Accessory {
        let _ = app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    }
    for window in app.windows().iter() {
        // The system derives the shadow from the window alpha, which puts a
        // grey halo around a transparent overlay
        if window.hasShadow() {
            window.setHasShadow(false);
        }
    }
}
