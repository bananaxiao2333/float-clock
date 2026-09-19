//! 窗口层：唯一依赖窗口系统的模块。
//!
//! 用 eframe/egui 当「透明的、置顶的、无边框的画板 + 输入源」，
//! 内容还是我们自己栅格化的那张 RGBA 图，所以三个平台长得一模一样。

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

#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    pub open_settings: bool,
    /// 排障用：强制不透明背景
    pub force_opaque: bool,
    /// 排障用：窗口起来 N 秒后读一次真实像素，打印报告并退出
    pub probe_after: Option<Duration>,
    /// 排障用：把回读到的像素写成 PNG
    pub probe_png: Option<std::path::PathBuf>,
}

const MOMENT_MAIN: &str = "偏移时刻";
const MOMENT_SUB: &str = "目标时间点";

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
    .map_err(|e| format!("启动窗口失败：{e}"))
}

fn moments(config: &Config, now: NaiveDateTime) -> Result<(NaiveDateTime, NaiveDateTime), String> {
    let target = parse_target(&config.time.target, now)?;
    let offset = parse_duration(&config.time.offset)?;
    Ok((
        target,
        target + ChronoDuration::milliseconds((offset * 1000.0).round() as i64),
    ))
}

/// 按当前文字算出来的窗口逻辑尺寸（点）。
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

struct SettingsDraft {
    target: String,
    offset: String,
    color: String,
    main_size: String,
    sub_size: String,
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
    notifier: MomentNotifier,
    position: egui::Pos2,
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
            notifier,
            position,
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

    fn report_error(&mut self, message: String) {
        eprintln!("[float-clock] {message}");
        if self.last_error.as_deref() != Some(message.as_str()) {
            self.last_error = Some(message.clone());
            crate::notify::send_async("FloatClock 出错", &message, None);
        }
    }

    fn current_scale(&self, ctx: &egui::Context) -> f32 {
        let ppp = ctx.pixels_per_point().max(0.5);
        ppp.min(self.config.display.max_scale.max(1.0))
    }

    /// 重算浮窗图片；内容没变就直接复用。
    fn refresh_image(&mut self, ctx: &egui::Context, now: NaiveDateTime) {
        let scale = self.current_scale(ctx);
        // 只看「秒」：毫秒级变化不需要重画
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

    fn toggle_lock(&mut self) {
        self.locked = !self.locked;
        self.config.window.locked = self.locked;
        self.persist(&[("window.locked", Value::Bool(self.locked))]);
    }

    fn reload(&mut self, now: NaiveDateTime) {
        let Ok(mut fresh) = config::load_config(&self.config.path) else {
            return;
        };
        // 位置/锁定如果还是我们自己写回去的值，就以内存里的为准
        fresh.window.locked = self.locked;
        match moments(&fresh, now) {
            Ok((target, mark)) => {
                self.target = target;
                self.mark = mark;
            }
            Err(message) => {
                self.report_error(format!("配置有误：{message}"));
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
        self.draft = SettingsDraft::from_config(&self.config);
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

    fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let builder = egui::ViewportBuilder::default()
            .with_title("FloatClock 设置")
            .with_inner_size([380.0, 300.0])
            .with_resizable(false)
            .with_always_on_top();
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("float-clock-settings"),
            builder,
            |ctx, _class| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    egui::Grid::new("float-clock-settings-grid")
                        .num_columns(2)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("目标时间点");
                            ui.text_edit_singleline(&mut self.draft.target);
                            ui.end_row();
                            ui.label("偏移");
                            ui.text_edit_singleline(&mut self.draft.offset);
                            ui.end_row();
                            ui.label("绿色");
                            ui.text_edit_singleline(&mut self.draft.color);
                            ui.end_row();
                            ui.label("主标题字号");
                            ui.text_edit_singleline(&mut self.draft.main_size);
                            ui.end_row();
                            ui.label("副标题字号");
                            ui.text_edit_singleline(&mut self.draft.sub_size);
                            ui.end_row();
                        });
                    if let Some(error) = &self.draft.error {
                        ui.colored_label(egui::Color32::from_rgb(255, 120, 120), error);
                    }
                    ui.label(
                        egui::RichText::new("改动会写回 config.toml（保留注释），立刻生效")
                            .small()
                            .weak(),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("应用").clicked() {
                            apply = true;
                        }
                        if ui.button("取消").clicked() {
                            cancel = true;
                        }
                    });
                });
                if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
                if ctx.input(|i| i.viewport().close_requested()) {
                    cancel = true;
                }
            },
        );
        if cancel {
            self.settings_open = false;
            self.draft = SettingsDraft::from_config(&self.config);
        }
        if apply {
            match self.apply_settings() {
                Ok(()) => {
                    self.settings_open = false;
                    open = false;
                }
                Err(message) => self.draft.error = Some(message),
            }
        }
        let _ = open;
    }

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
            .map_err(|_| "主标题字号需要是一个数字".to_string())?;
        let sub_size: f32 = self
            .draft
            .sub_size
            .trim()
            .parse()
            .map_err(|_| "副标题字号需要是一个数字".to_string())?;
        let color = if self.draft.color.trim().is_empty() {
            "#00FF66".to_string()
        } else {
            crate::pixmap::Color::parse(&self.draft.color)?;
            self.draft.color.trim().to_string()
        };

        self.persist(&[
            ("time.target", Value::Str(target.clone())),
            ("time.offset", Value::Str(offset.clone())),
            ("display.color", Value::Str(color.clone())),
            ("display.main_size", Value::Float(main_size as f64)),
            ("display.sub_size", Value::Float(sub_size as f64)),
        ]);
        self.config.time.target = target;
        self.config.time.offset = offset;
        self.config.display.color = color;
        self.config.display.main_size = main_size;
        self.config.display.sub_size = sub_size;
        let (target, mark) = moments(&self.config, now)?;
        self.target = target;
        self.mark = mark;
        self.revision += 1;
        self.draft.error = None;
        self.arm(now);
        Ok(())
    }
}

impl eframe::App for OverlayApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // 全透明：桌面直接从窗口的 alpha 通道透出来
        [0.0, 0.0, 0.0, 0.0]
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = Local::now().naive_local();

        // 窗口一存在就把 macOS 上的阴影 / Dock 图标收拾掉
        if !self.tuned {
            self.tuned = true;
            #[cfg(target_os = "macos")]
            crate::macos::tune_window();
        }

        // 配置文件热重载：每 5 个 tick 检查一次
        self.ticks += 1;
        if self.ticks % 5 == 0 && mtime_of(&self.config.path) != self.mtime {
            self.reload(now);
        }

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

        // 窗口系统可能把窗口放到了别处（Wayland 就常忽略初始坐标），
        // 没在拖动时以系统给的位置为准，免得下次保存写回一个假坐标
        if !response.dragged() {
            if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
                if (rect.min - self.position).length() > 0.5 {
                    self.position = rect.min;
                }
            }
        }
        if response.dragged() && !self.locked {
            self.position += response.drag_delta();
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(self.position));
        }
        if response.drag_stopped() {
            let position = self.position;
            self.save_position(position);
        }
        if response.clicked_by(egui::PointerButton::Secondary) {
            self.toggle_lock();
        }
        if response.double_clicked() {
            self.settings_open = true;
        }

        let (lock_key, reload_key, quit_key, settings_key) = ctx.input(|i| {
            (
                i.modifiers.command && i.key_pressed(egui::Key::L),
                i.modifiers.command && i.key_pressed(egui::Key::R),
                i.modifiers.command && i.key_pressed(egui::Key::Q),
                i.modifiers.command && i.key_pressed(egui::Key::Comma),
            )
        });
        if lock_key {
            self.toggle_lock();
        }
        if reload_key {
            self.reload(now);
        }
        if settings_key {
            self.settings_open = true;
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

/// 在绘制阶段最后插一个回调：等 egui 把这一帧画完，再从默认帧缓冲回读像素。
/// `App::ui` 只是「录制」绘制指令，真正的 GL 绘制发生在之后，所以必须这样拿。
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
    // glReadPixels 是自下而上的，翻过来再统计 / 存图
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

    println!("{}", "─".repeat(56));
    println!("GPU 回读      : {w} × {h} 像素（{total} 个）");
    println!("背景 alpha 众数: {dominant}  （0 = 桌面能直接透出来）");
    println!(
        "全透明像素    : {fully_transparent}  （{:.1}%）",
        fully_transparent as f64 / total as f64 * 100.0
    );
    println!("绿色像素      : {green}");
    if green == 0 {
        println!("⚠️ 一个绿色像素都没有，说明窗口里其实什么都没画出来");
    }

    #[cfg(target_os = "macos")]
    match crate::macos::probe("FloatClock") {
        Some(probe) => println!("{}", probe.report()),
        None => println!("找不到 FloatClock 窗口，跳过窗口属性体检"),
    }

    if let Some(path) = &options.probe_png {
        let pixmap = Pixmap {
            width: w as u32,
            height: h as u32,
            data: flipped,
        };
        match pixmap.to_png() {
            Ok(bytes) => match std::fs::write(path, bytes) {
                Ok(()) => println!("GPU 回读已写入 {}", path.display()),
                Err(error) => println!("写入 PNG 失败：{error}"),
            },
            Err(error) => println!("编码 PNG 失败：{error}"),
        }
    }
}
