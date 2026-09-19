//! The window layer: the only module that talks to the window system.
//!
//! eframe / egui is used purely as a transparent, always-on-top, undecorated
//! canvas plus an input source. The content is still the RGBA image this crate
//! rasterises itself, which is why all three platforms look identical.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use chrono::{Duration as ChronoDuration, Local, NaiveDateTime};
use eframe::egui;

use crate::config::{self, Config, Value};
use crate::notifier::MomentNotifier;
use crate::pixmap::Pixmap;
use crate::render::{self, RenderInput};
use crate::text::Fonts;
use crate::timefmt::{parse_duration, parse_target};
use crate::tray::{Tray, TrayCommand};

#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    pub open_settings: bool,
    /// Diagnostics: force an opaque background.
    pub force_opaque: bool,
    /// Diagnostics: after the window has been up for N seconds, read the real
    /// pixels back once, print a report and exit.
    pub probe_after: Option<Duration>,
    /// Diagnostics: write the read-back pixels to this PNG.
    pub probe_png: Option<std::path::PathBuf>,
}

const MOMENT_MAIN: &str = "Offset moment";
const MOMENT_SUB: &str = "Target time";

/// How long the window has to sit still before the new coordinates go to disk.
const SAVE_DEBOUNCE: Duration = Duration::from_millis(600);

pub fn run(config: Config, options: RunOptions) -> Result<(), String> {
    let fonts = Fonts::load(&config.display.font_family)?;
    let now = Local::now().naive_local();
    let (target, mark) = moments(&config, now)?;
    let size = logical_size(&config, &fonts, now, target, mark)?;

    let transparent = !options.force_opaque;
    let viewport = egui::ViewportBuilder::default()
        .with_title("FloatClock")
        .with_app_id("float-clock")
        .with_decorations(!config.window.borderless)
        .with_resizable(false)
        .with_transparent(transparent)
        .with_taskbar(false)
        .with_inner_size([size.0.max(16.0), size.1.max(16.0)])
        .with_position([config.window.x as f32, config.window.y as f32]);
    let viewport = if config.window.topmost {
        viewport.with_always_on_top()
    } else {
        viewport
    };

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "FloatClock",
        native_options,
        Box::new(move |cc| {
            Ok(Box::new(OverlayApp::new(
                cc, config, fonts, options, target, mark,
            )))
        }),
    )
    .map_err(|e| format!("could not open a window: {e}"))
}

fn moments(config: &Config, now: NaiveDateTime) -> Result<(NaiveDateTime, NaiveDateTime), String> {
    let target = parse_target(&config.time.target, now)?;
    let offset = parse_duration(&config.time.offset)?;
    Ok((
        target,
        target + ChronoDuration::milliseconds((offset * 1000.0).round() as i64),
    ))
}

/// Window size in logical points, computed from the text currently on screen.
fn logical_size(
    config: &Config,
    fonts: &Fonts,
    now: NaiveDateTime,
    target: NaiveDateTime,
    mark: NaiveDateTime,
) -> Result<(f32, f32), String> {
    let overlay = render::render(&RenderInput {
        config,
        fonts,
        target,
        mark,
        now,
        locked: config.window.locked,
        scale: 1.0,
        transparent: true,
    })?;
    Ok((overlay.layout.width, overlay.layout.height))
}

/// Where the pointer and the window were, in absolute monitor coordinates.
///
/// Monitor space is what makes manual dragging stable: if the window moves by
/// `d`, the pointer's *window-relative* position moves by `-d` in the same
/// instant, so the sum is invariant. Accumulating `pointer.delta()` instead
/// feeds our own window movement back into the next frame's delta and the
/// window visibly shakes.
#[derive(Debug, Clone, Copy, PartialEq)]
struct DragAnchor {
    pointer: egui::Pos2,
    window: egui::Pos2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragMode {
    /// Hand the drag to the window manager and keep out of the way.
    Native,
    /// Move the window ourselves, following the pointer.
    Manual,
}

/// A drag currently in progress.
#[derive(Debug, Clone, Copy)]
struct DragRun {
    anchor: DragAnchor,
    /// `None` while we are still working out whether the window manager took
    /// the drag over.
    mode: Option<DragMode>,
    started: Instant,
}

/// Decide how to move the window during a drag.
///
/// Returns `None` while the answer is still unknown: we give the window manager
/// a short grace period to start moving the window, and only fall back to doing
/// it ourselves if the pointer has clearly moved while the window did not.
fn drag_mode(
    anchor: DragAnchor,
    pointer: egui::Pos2,
    window: egui::Pos2,
    elapsed: Duration,
) -> Option<DragMode> {
    const WINDOW_MOVED: f32 = 1.5;
    const POINTER_MOVED: f32 = 4.0;
    const GRACE: Duration = Duration::from_millis(120);

    if (window - anchor.window).length() > WINDOW_MOVED {
        return Some(DragMode::Native);
    }
    if elapsed >= GRACE && (pointer - anchor.pointer).length() > POINTER_MOVED {
        return Some(DragMode::Manual);
    }
    None
}

/// Pointer and window origin, both in absolute monitor coordinates.
///
/// `None` on Wayland and Android, where the compositor refuses to say where the
/// window is. Manual dragging cannot work there at all; native dragging is a
/// no-op there too, so Wayland simply cannot be dragged and is documented as
/// such.
fn drag_state(ctx: &egui::Context) -> Option<DragAnchor> {
    ctx.input(|i| {
        let viewport = i.viewport();
        let rect = viewport.outer_rect.or(viewport.inner_rect)?;
        let pointer = i.pointer.latest_pos()?;
        Some(anchor_from(rect.min, pointer))
    })
}

/// Combine the two pieces egui gives us - the window origin, in monitor space,
/// and the pointer, relative to the window - into one comparable anchor.
///
/// The constant offset between the window's outer rect and its content area
/// cancels out here, which is all this needs.
fn anchor_from(window: egui::Pos2, pointer_inside_window: egui::Pos2) -> DragAnchor {
    DragAnchor {
        pointer: window + pointer_inside_window.to_vec2(),
        window,
    }
}

struct SettingsDraft {
    target: String,
    offset: String,
    color: String,
    main_size: String,
    sub_size: String,
    font_family: String,
    opacity: String,
    locked: bool,
    topmost: bool,
    notify_enabled: bool,
    error: Option<String>,
}

impl SettingsDraft {
    fn from_config(config: &Config) -> Self {
        Self {
            target: config.time.target.clone(),
            offset: config.time.offset.clone(),
            color: config.display.color.clone(),
            main_size: config.display.main_size.to_string(),
            sub_size: config.display.sub_size.to_string(),
            font_family: config.display.font_family.clone(),
            opacity: config.window.opacity.to_string(),
            locked: config.window.locked,
            topmost: config.window.topmost,
            notify_enabled: config.notify.enabled,
            error: None,
        }
    }
}

pub struct OverlayApp {
    config: Config,
    fonts: Fonts,
    options: RunOptions,
    target: NaiveDateTime,
    mark: NaiveDateTime,
    locked: bool,
    visible: bool,
    notifier: MomentNotifier,
    position: egui::Pos2,
    /// Set while the window has moved but the new coordinates are not on disk yet.
    position_dirty: Option<Instant>,
    drag: Option<DragRun>,
    texture: Option<egui::TextureHandle>,
    signature: String,
    drawn_size: egui::Vec2,
    mtime: Option<SystemTime>,
    revision: u64,
    ticks: u64,
    settings_open: bool,
    draft: SettingsDraft,
    last_error: Option<String>,
    started: Instant,
    tuned: bool,
    tray: Option<Tray>,
    capture_started: bool,
    capture: Arc<Mutex<Option<Vec<u8>>>>,
    capture_size: Arc<Mutex<(i32, i32)>>,
}

impl OverlayApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        config: Config,
        fonts: Fonts,
        options: RunOptions,
        target: NaiveDateTime,
        mark: NaiveDateTime,
    ) -> Self {
        let settings_open = options.open_settings;
        let draft = SettingsDraft::from_config(&config);
        let notifier = MomentNotifier::new(config.notify.clone());
        let position = egui::pos2(config.window.x as f32, config.window.y as f32);
        let mtime = mtime_of(&config.path);
        let _ = &cc;
        let mut app = Self {
            locked: config.window.locked,
            config,
            fonts,
            options,
            target,
            mark,
            visible: true,
            notifier,
            position,
            position_dirty: None,
            drag: None,
            texture: None,
            signature: String::new(),
            drawn_size: egui::Vec2::ZERO,
            mtime,
            revision: 0,
            ticks: 0,
            settings_open,
            draft,
            last_error: None,
            started: Instant::now(),
            tuned: false,
            tray: None,
            capture_started: false,
            capture: Arc::new(Mutex::new(None)),
            capture_size: Arc::new(Mutex::new((0, 0))),
        };
        app.arm(Local::now().naive_local());
        app
    }

    fn arm(&mut self, now: NaiveDateTime) {
        *self.notifier.config_mut() = self.config.notify.clone();
        self.notifier.arm(
            vec![
                (MOMENT_MAIN.to_string(), self.mark),
                (MOMENT_SUB.to_string(), self.target),
            ],
            now,
        );
    }

    /// Create the tray icon, once, the first time a frame is drawn. Doing it
    /// here rather than in `new` keeps it on the main thread, which macOS
    /// requires for a status item.
    fn install_tray(&mut self) {
        if self.tray.is_some()
            || !self.config.window.tray
            || crate::tray::unavailable_reason().is_some()
        {
            return;
        }
        match Tray::new(self.locked) {
            Ok(tray) => {
                if std::env::var_os("FLOAT_CLOCK_DEBUG").is_some() {
                    eprintln!("[float-clock] tray icon created");
                }
                self.tray = Some(tray);
            }
            Err(message) => self.report_error(format!("could not create the tray icon: {message}")),
        }
    }

    fn report_error(&mut self, message: String) {
        eprintln!("[float-clock] {message}");
        if self.last_error.as_deref() != Some(message.as_str()) {
            self.last_error = Some(message.clone());
            crate::notify::send_async("FloatClock error", &message, None);
        }
    }

    fn current_scale(&self, ctx: &egui::Context) -> f32 {
        let ppp = ctx.pixels_per_point().max(0.5);
        ppp.min(self.config.display.max_scale.max(1.0))
    }

    /// Rebuild the overlay image; reuse the old one when nothing changed.
    fn refresh_image(&mut self, ctx: &egui::Context, now: NaiveDateTime) {
        let scale = self.current_scale(ctx);
        // Only the second matters: sub-second changes need no repaint.
        let signature = format!(
            "{}|{}|{}|{}|{scale}|{}",
            self.target,
            self.mark,
            self.locked,
            self.revision,
            now.and_utc().timestamp()
        );
        if signature == self.signature && self.texture.is_some() {
            return;
        }

        let transparent = !self.options.force_opaque;
        let overlay = match render::render(&RenderInput {
            config: &self.config,
            fonts: &self.fonts,
            target: self.target,
            mark: self.mark,
            now,
            locked: self.locked,
            scale,
            transparent,
        }) {
            Ok(overlay) => overlay,
            Err(message) => {
                self.report_error(message);
                return;
            }
        };

        let image = color_image(&overlay.pixmap);
        let logical = egui::vec2(overlay.layout.width / scale, overlay.layout.height / scale);
        let options = egui::TextureOptions {
            magnification: egui::TextureFilter::Nearest,
            minification: egui::TextureFilter::Linear,
            ..Default::default()
        };
        match &mut self.texture {
            Some(handle) => handle.set(image, options),
            None => {
                self.texture = Some(ctx.load_texture("float-clock-overlay", image, options));
            }
        }

        if (logical - self.drawn_size).length() > 0.5 || self.drawn_size == egui::Vec2::ZERO {
            self.drawn_size = logical;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(logical));
        }
        self.signature = signature;
    }

    fn persist(&mut self, updates: &[(&str, Value)]) {
        if let Err(message) = config::patch_toml(&self.config.path, updates) {
            self.report_error(message);
            return;
        }
        self.mtime = mtime_of(&self.config.path);
    }

    fn toggle_lock(&mut self, ctx: &egui::Context) {
        self.locked = !self.locked;
        self.config.window.locked = self.locked;
        self.persist(&[("window.locked", Value::Bool(self.locked))]);
        if let Some(tray) = &self.tray {
            tray.set_locked(self.locked);
        }
        ctx.request_repaint();
    }

    /// Show or hide the overlay, keeping the tray label in step.
    fn set_visible(&mut self, ctx: &egui::Context, visible: bool) {
        if self.visible == visible {
            return;
        }
        self.visible = visible;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(visible));
        if let Some(tray) = &self.tray {
            tray.set_visible(visible);
        }
        ctx.request_repaint();
    }

    fn toggle_visible(&mut self, ctx: &egui::Context) {
        let visible = !self.visible;
        self.set_visible(ctx, visible);
    }

    fn open_settings(&mut self, ctx: &egui::Context) {
        // A settings window over a hidden overlay would look like a bug.
        self.set_visible(ctx, true);
        self.settings_open = true;
        self.draft = SettingsDraft::from_config(&self.config);
    }

    fn reload(&mut self, now: NaiveDateTime) {
        let Ok(mut fresh) = config::load_config(&self.config.path) else {
            return;
        };
        // If the position and lock state are still the ones we wrote back
        // ourselves, the in-memory values win.
        fresh.window.locked = self.locked;
        match moments(&fresh, now) {
            Ok((target, mark)) => {
                self.target = target;
                self.mark = mark;
            }
            Err(message) => {
                self.report_error(format!("bad configuration: {message}"));
                self.mtime = mtime_of(&self.config.path);
                return;
            }
        }
        let family_changed = fresh.display.font_family != self.config.display.font_family;
        let wanted_position = egui::pos2(fresh.window.x as f32, fresh.window.y as f32);
        if (wanted_position - self.position).length() > 0.5 {
            self.position = wanted_position;
        }
        self.config = fresh;
        if family_changed {
            match Fonts::load(&self.config.display.font_family) {
                Ok(fonts) => self.fonts = fonts,
                Err(message) => self.report_error(message),
            }
        }
        // Never clobber edits that are in progress in the settings window.
        if !self.settings_open {
            self.draft = SettingsDraft::from_config(&self.config);
        }
        self.revision += 1;
        self.last_error = None;
        self.arm(now);
    }

    fn save_position(&mut self, position: egui::Pos2) {
        let x = position.x.round() as i64;
        let y = position.y.round() as i64;
        if (self.config.window.x as i64, self.config.window.y as i64) == (x, y) {
            return;
        }
        self.config.window.x = x as i32;
        self.config.window.y = y as i32;
        self.persist(&[("window.x", Value::Int(x)), ("window.y", Value::Int(y))]);
    }

    /// Follow the window when something else moves it, and write the settled
    /// position back to disk. A debounce is used instead of reacting to
    /// "drag ended", because a native drag may never deliver that event.
    fn track_position(&mut self, ctx: &egui::Context) {
        let Some(rect) = ctx.input(|i| i.viewport().outer_rect) else {
            return;
        };
        if (rect.min - self.position).length() > 0.5 {
            self.position = rect.min;
            self.position_dirty = Some(Instant::now());
            return;
        }
        if let Some(since) = self.position_dirty {
            if since.elapsed() >= SAVE_DEBOUNCE {
                self.position_dirty = None;
                let position = self.position;
                self.save_position(position);
            }
        }
    }

    fn handle_tray(&mut self, ctx: &egui::Context, now: NaiveDateTime) {
        let Some(tray) = &self.tray else {
            return;
        };
        for command in tray.poll() {
            match command {
                TrayCommand::ToggleVisible => self.toggle_visible(ctx),
                TrayCommand::ToggleLock => self.toggle_lock(ctx),
                TrayCommand::OpenSettings => self.open_settings(ctx),
                TrayCommand::OpenConfig => match crate::shell::open(&self.config.path) {
                    Ok(()) => {}
                    Err(message) => {
                        self.report_error(format!("cannot open the config file: {message}"))
                    }
                },
                TrayCommand::RevealConfig => match crate::shell::reveal(&self.config.path) {
                    Ok(()) => {}
                    Err(message) => {
                        self.report_error(format!("cannot show the config file: {message}"))
                    }
                },
                TrayCommand::ReloadConfig => self.reload(now),
                TrayCommand::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            }
        }
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let builder = egui::ViewportBuilder::default()
            .with_title("FloatClock settings")
            .with_inner_size([440.0, 380.0])
            .with_resizable(false)
            .with_always_on_top();
        let mut apply = false;
        let mut cancel = false;
        let mut open_config = false;
        let mut reveal_config = false;
        let mut reload = false;
        let path = self.config.path.clone();

        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("float-clock-settings"),
            builder,
            |ctx, _class| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.label(
                        egui::RichText::new(
                            "Every change is written back into the config file below.",
                        )
                        .small()
                        .weak(),
                    );
                    ui.add_space(6.0);

                    egui::Grid::new("float-clock-settings-grid")
                        .num_columns(2)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Target time");
                            ui.text_edit_singleline(&mut self.draft.target);
                            ui.end_row();

                            ui.label("Offset");
                            ui.text_edit_singleline(&mut self.draft.offset);
                            ui.end_row();

                            ui.label("Colour");
                            ui.text_edit_singleline(&mut self.draft.color);
                            ui.end_row();

                            ui.label("Title size");
                            ui.text_edit_singleline(&mut self.draft.main_size);
                            ui.end_row();

                            ui.label("Subtitle size");
                            ui.text_edit_singleline(&mut self.draft.sub_size);
                            ui.end_row();

                            ui.label("Font family");
                            ui.text_edit_singleline(&mut self.draft.font_family);
                            ui.end_row();

                            ui.label("Opacity");
                            ui.text_edit_singleline(&mut self.draft.opacity);
                            ui.end_row();
                        });

                    ui.add_space(4.0);
                    ui.checkbox(&mut self.draft.locked, "Locked (cannot be dragged)");
                    ui.checkbox(&mut self.draft.topmost, "Always on top");
                    ui.checkbox(&mut self.draft.notify_enabled, "System notifications");

                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("Config file").small().weak());
                    ui.label(
                        egui::RichText::new(path.display().to_string())
                            .monospace()
                            .small(),
                    );
                    ui.horizontal(|ui| {
                        open_config = ui.button("Open file").clicked();
                        reveal_config = ui.button("Show in folder").clicked();
                        reload = ui.button("Reload from disk").clicked();
                    });

                    ui.add_space(4.0);
                    if let Some(error) = &self.draft.error {
                        ui.colored_label(egui::Color32::from_rgb(255, 120, 120), error);
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Apply").clicked() {
                            apply = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(
                            "Drag with the left button · right-click or Ctrl/Cmd+L to lock · \
                             double-click or Ctrl/Cmd+, for this window · Ctrl/Cmd+R to reload",
                        )
                        .small()
                        .weak(),
                    );
                });
                if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
                if ctx.input(|i| i.viewport().close_requested()) {
                    cancel = true;
                }
            },
        );

        if open_config {
            match crate::shell::open(&self.config.path) {
                Ok(()) => {}
                Err(message) => self.draft.error = Some(message),
            }
        }
        if reveal_config {
            match crate::shell::reveal(&self.config.path) {
                Ok(()) => {}
                Err(message) => self.draft.error = Some(message),
            }
        }
        if reload {
            let now = Local::now().naive_local();
            self.reload(now);
        }
        if cancel {
            self.settings_open = false;
            self.draft = SettingsDraft::from_config(&self.config);
        }
        if apply {
            match self.apply_settings() {
                Ok(()) => {
                    self.settings_open = false;
                }
                Err(message) => self.draft.error = Some(message),
            }
        }
    }

    /// Validate the draft, write it to the config file, and let `reload` bring
    /// the file back in.
    ///
    /// It would be tempting to assign every field from the draft as well, but
    /// that means three hand-kept-in-step lists - the draft fields, the write
    /// list below, and the in-memory assignments. Writing the file and then
    /// re-reading it leaves exactly one list.
    fn apply_settings(&mut self) -> Result<(), String> {
        let now = Local::now().naive_local();
        let target = self.draft.target.trim().to_string();
        let offset = self.draft.offset.trim().to_string();
        parse_target(&target, now)?;
        parse_duration(&offset)?;
        let main_size: f32 = self
            .draft
            .main_size
            .trim()
            .parse()
            .map_err(|_| "Title size has to be a number".to_string())?;
        let sub_size: f32 = self
            .draft
            .sub_size
            .trim()
            .parse()
            .map_err(|_| "Subtitle size has to be a number".to_string())?;
        let opacity: f64 = self
            .draft
            .opacity
            .trim()
            .parse()
            .map_err(|_| "Opacity has to be a number".to_string())?;
        if !(0.05..=1.0).contains(&opacity) {
            return Err("Opacity has to be between 0.05 and 1.0".to_string());
        }
        let color = if self.draft.color.trim().is_empty() {
            "#00FF66".to_string()
        } else {
            crate::pixmap::Color::parse(&self.draft.color)?;
            self.draft.color.trim().to_string()
        };
        let family = self.draft.font_family.trim().to_string();
        // Fail before writing anything if the font cannot be resolved.
        Fonts::load(&family)?;

        self.persist(&[
            ("time.target", Value::Str(target)),
            ("time.offset", Value::Str(offset)),
            ("display.color", Value::Str(color)),
            ("display.main_size", Value::Float(main_size as f64)),
            ("display.sub_size", Value::Float(sub_size as f64)),
            ("display.font_family", Value::Str(family)),
            ("window.opacity", Value::Float(opacity)),
            ("window.locked", Value::Bool(self.draft.locked)),
            ("window.topmost", Value::Bool(self.draft.topmost)),
            ("notify.enabled", Value::Bool(self.draft.notify_enabled)),
        ]);

        // `reload` deliberately lets the in-memory lock state win over the file,
        // so update that one first.
        self.locked = self.draft.locked;
        if let Some(tray) = &self.tray {
            tray.set_locked(self.locked);
        }
        self.reload(now);
        self.draft.error = None;
        Ok(())
    }
}

impl eframe::App for OverlayApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // Fully transparent: the desktop shows straight through the window.
        [0.0, 0.0, 0.0, 0.0]
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = Local::now().naive_local();

        self.install_tray();

        // Once the window exists, deal with the macOS shadow / Dock icon.
        if !self.tuned {
            self.tuned = true;
            #[cfg(target_os = "macos")]
            crate::macos::tune_window();
        }

        // Config hot reload: check every 5 ticks.
        self.ticks += 1;
        if self.ticks % 5 == 0 && mtime_of(&self.config.path) != self.mtime {
            self.reload(now);
        }

        self.handle_tray(&ctx, now);

        self.refresh_image(&ctx, now);
        self.notifier.tick(now);

        let rect = ui.max_rect();
        let response = ui.interact(rect, ui.id().with("overlay"), egui::Sense::click_and_drag());

        if let Some(texture) = &self.texture {
            ui.painter().image(
                texture.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }

        // The window system may have put the window somewhere else entirely
        // (Wayland likes to ignore the initial coordinates). While we are not
        // dragging, the system's answer is the truth, so we never write back a
        // made-up position.
        if !response.dragged() {
            self.drag = None;
            self.track_position(&ctx);
        }

        // Dragging. The drag is handed to the window manager, which is the only
        // arrangement that cannot shake: our code never sees the movement, so
        // there is no feedback loop. If the window manager does not pick the
        // drag up (X11 without focus, a compositor that refuses), `drag_mode`
        // notices within a few frames and we move the window ourselves from an
        // absolute anchor.
        if response.drag_started() && !self.locked {
            if let Some(anchor) = drag_state(&ctx) {
                self.drag = Some(DragRun {
                    anchor,
                    mode: None,
                    started: Instant::now(),
                });
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
        }
        if response.dragged() && !self.locked {
            if let (Some(mut run), Some(now)) = (self.drag, drag_state(&ctx)) {
                let mode = run.mode.or_else(|| {
                    drag_mode(run.anchor, now.pointer, now.window, run.started.elapsed())
                });
                if mode == Some(DragMode::Manual) && run.mode != Some(DragMode::Manual) {
                    // First frame of manual dragging: re-anchor here, otherwise
                    // the window jumps by however far the pointer travelled
                    // while we were waiting to see whether the window manager
                    // would take over.
                    run.anchor = now;
                }
                run.mode = mode;
                self.drag = Some(run);

                if mode == Some(DragMode::Manual) {
                    let target = run.anchor.window + (now.pointer - run.anchor.pointer);
                    if (target - self.position).length() > 0.1 {
                        self.position = target;
                        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(target));
                    }
                }
            }
        }

        if response.clicked_by(egui::PointerButton::Secondary) {
            self.toggle_lock(&ctx);
        }
        if response.double_clicked() {
            self.open_settings(&ctx);
        }

        let (lock_key, reload_key, quit_key, settings_key, hide_key) = ctx.input(|i| {
            (
                i.modifiers.command && i.key_pressed(egui::Key::L),
                i.modifiers.command && i.key_pressed(egui::Key::R),
                i.modifiers.command && i.key_pressed(egui::Key::Q),
                i.modifiers.command && i.key_pressed(egui::Key::Comma),
                i.modifiers.command && i.key_pressed(egui::Key::H),
            )
        });
        if lock_key {
            self.toggle_lock(&ctx);
        }
        if reload_key {
            self.reload(now);
        }
        if settings_key {
            self.open_settings(&ctx);
        }
        if hide_key {
            self.toggle_visible(&ctx);
        }
        if quit_key {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        self.settings_window(&ctx);

        if let Some(after) = self.options.probe_after {
            if !self.capture_started && self.started.elapsed() >= after {
                self.capture_started = true;
                install_capture(
                    ui,
                    Arc::clone(&self.capture),
                    Arc::clone(&self.capture_size),
                );
                ctx.request_repaint();
            }
            let captured = self.capture.lock().ok().and_then(|mut slot| slot.take());
            if let Some(buffer) = captured {
                let (width, height) = *self.capture_size.lock().unwrap();
                report_probe(&self.options, &buffer, width, height);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        ctx.request_repaint_after(Duration::from_millis(self.config.interval_ms()));
    }
}

fn color_image(pixmap: &Pixmap) -> egui::ColorImage {
    egui::ColorImage::from_rgba_unmultiplied(
        [pixmap.width as usize, pixmap.height as usize],
        &pixmap.data,
    )
}

fn mtime_of(path: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .ok()
        .and_then(|meta| meta.modified().ok())
}

/// Install a callback at the very end of the paint phase: egui records paint
/// commands during `ui` and only submits the real GL draw afterwards, so
/// reading the framebuffer has to happen from inside the draw.
fn install_capture(ui: &egui::Ui, slot: Arc<Mutex<Option<Vec<u8>>>>, size: Arc<Mutex<(i32, i32)>>) {
    let rect = ui.max_rect();
    ui.painter().add(egui::PaintCallback {
        rect,
        callback: Arc::new(egui_glow::CallbackFn::new(move |info, painter| {
            let viewport = info.viewport_in_pixels();
            let (width, height) = (viewport.width_px, viewport.height_px);
            if width <= 0 || height <= 0 {
                return;
            }
            let mut buffer = vec![0u8; (width as usize) * (height as usize) * 4];
            unsafe {
                use glow::HasContext;
                painter.gl().read_pixels(
                    viewport.left_px,
                    viewport.from_bottom_px,
                    width,
                    height,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelPackData::Slice(Some(&mut buffer)),
                );
            }
            *size.lock().unwrap() = (width, height);
            *slot.lock().unwrap() = Some(buffer);
        })),
    });
}

fn report_probe(options: &RunOptions, buffer: &[u8], width: i32, height: i32) {
    // glReadPixels hands back bottom-up rows; flip before measuring / saving.
    let (w, h) = (width as usize, height as usize);
    let mut flipped = vec![0u8; buffer.len()];
    for y in 0..h {
        let src = (h - 1 - y) * w * 4;
        flipped[y * w * 4..(y + 1) * w * 4].copy_from_slice(&buffer[src..src + w * 4]);
    }

    let mut histogram = [0usize; 256];
    let mut green = 0usize;
    let mut fully_transparent = 0usize;
    for pixel in flipped.chunks_exact(4) {
        histogram[pixel[3] as usize] += 1;
        if pixel[3] == 0 {
            fully_transparent += 1;
        } else if pixel[1] > pixel[0] && pixel[1] > pixel[2] {
            green += 1;
        }
    }
    let total = w * h;
    let dominant = histogram
        .iter()
        .enumerate()
        .max_by_key(|(_, count)| **count)
        .map(|(alpha, _)| alpha)
        .unwrap_or(255);

    println!("{}", "-".repeat(56));
    println!("GPU read-back     : {w} x {h} pixels ({total} total)");
    println!("dominant bg alpha : {dominant}  (0 means the desktop shows through)");
    println!(
        "fully transparent : {fully_transparent}  ({:.1}%)",
        fully_transparent as f64 / total as f64 * 100.0
    );
    println!("green pixels      : {green}");
    if green == 0 {
        println!("WARNING: not a single green pixel - nothing was actually drawn");
    }

    #[cfg(target_os = "macos")]
    match crate::macos::probe("FloatClock") {
        Some(probe) => println!("{}", probe.report()),
        None => println!("no FloatClock window found, skipping the window property check"),
    }

    if let Some(path) = &options.probe_png {
        let pixmap = Pixmap {
            width: w as u32,
            height: h as u32,
            data: flipped,
        };
        match pixmap.to_png() {
            Ok(bytes) => match std::fs::write(path, bytes) {
                Ok(()) => println!("wrote the GPU read-back to {}", path.display()),
                Err(error) => println!("could not write the PNG: {error}"),
            },
            Err(error) => println!("could not encode the PNG: {error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor(pointer: (f32, f32), window: (f32, f32)) -> DragAnchor {
        DragAnchor {
            pointer: egui::pos2(pointer.0, pointer.1),
            window: egui::pos2(window.0, window.1),
        }
    }

    #[test]
    fn a_moving_window_means_the_window_manager_took_over() {
        let start = anchor((500.0, 500.0), (100.0, 100.0));
        let mode = drag_mode(
            start,
            egui::pos2(520.0, 500.0),
            egui::pos2(120.0, 100.0),
            Duration::from_millis(10),
        );
        assert_eq!(mode, Some(DragMode::Native));
    }

    #[test]
    fn a_still_window_under_a_moving_pointer_falls_back_to_manual() {
        let start = anchor((500.0, 500.0), (100.0, 100.0));
        // Pointer moved 20px, window did not move at all.
        let mode = drag_mode(
            start,
            egui::pos2(520.0, 500.0),
            egui::pos2(100.0, 100.0),
            Duration::from_millis(200),
        );
        assert_eq!(mode, Some(DragMode::Manual));
    }

    #[test]
    fn the_grace_period_gives_the_window_manager_a_chance() {
        let start = anchor((500.0, 500.0), (100.0, 100.0));
        // Same situation as above, but the grace period has not elapsed yet,
        // so we are still undecided rather than grabbing the window ourselves.
        let mode = drag_mode(
            start,
            egui::pos2(520.0, 500.0),
            egui::pos2(100.0, 100.0),
            Duration::from_millis(10),
        );
        assert_eq!(mode, None);
    }

    #[test]
    fn a_stationary_pointer_never_starts_a_move() {
        // If the pointer has not moved, the window must not move either -
        // otherwise our own window movement would feed back into the next
        // frame and the overlay would drift away on its own.
        let start = anchor((500.0, 500.0), (100.0, 100.0));
        let mode = drag_mode(
            start,
            egui::pos2(500.0, 500.0),
            egui::pos2(100.0, 100.0),
            Duration::from_secs(5),
        );
        assert_eq!(mode, None);
    }

    #[test]
    fn manual_drag_target_is_invariant_to_our_own_window_movement() {
        // Grab the overlay with the pointer 30px into the window, the window
        // sitting at (100, 100).
        let grab = anchor_from(egui::pos2(100.0, 100.0), egui::pos2(30.0, 12.0));

        // The user drags 40px to the right; that is the position we command.
        let dragged_to = egui::pos2(grab.pointer.x + 40.0, grab.pointer.y);
        let target = grab.window + (dragged_to - grab.pointer);
        assert_eq!(target, egui::pos2(140.0, 100.0));

        // The window is now at 140 and the pointer is still 30px into it, which
        // in monitor space is exactly where the drag left it.
        let now = anchor_from(egui::pos2(140.0, 100.0), egui::pos2(30.0, 12.0));
        assert_eq!(now.pointer, dragged_to);

        // Which means the next frame asks for exactly the same position again
        // instead of bouncing back. That absence of feedback is what stops the
        // overlay from shaking while it is dragged.
        assert_eq!(grab.window + (now.pointer - grab.pointer), target);
    }
}
