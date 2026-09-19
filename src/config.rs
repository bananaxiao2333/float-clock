//! Configuration: TOML loading, default merging, comment-preserving write-back
//! and generation of the default config file.

use std::path::{Path, PathBuf};

use chrono::{Duration, Local, NaiveDateTime, Timelike};
use serde::Deserialize;

pub const DEFAULT_CONFIG_NAME: &str = "config.toml";

/// Directory name used when we have to fall back to a per-user config location.
const APP_DIR_NAME: &str = "FloatClock";

/// Per-user configuration directory, using each platform's own convention and
/// without depending on any crate:
///
/// * macOS   `~/Library/Application Support/FloatClock`
/// * Windows `%APPDATA%\FloatClock`
/// * Linux   `$XDG_CONFIG_HOME/float-clock` (defaults to `~/.config/float-clock`)
///
/// This is what makes "download it and double-click it" actually work: a
/// double-clicked `.app` runs with `/` as its working directory, so a relative
/// `./config.toml` can neither be found nor written.
pub fn user_config_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);

    if cfg!(target_os = "macos") {
        return home.map(|home| home.join("Library/Application Support").join(APP_DIR_NAME));
    }
    if cfg!(target_os = "windows") {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .or_else(|| home.clone())?;
        return Some(base.join(APP_DIR_NAME));
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg).join("float-clock"));
        }
    }
    home.map(|home| home.join(".config/float-clock"))
}

/// Where the config file is looked for when `--config` was not given:
///
/// 1. the `FLOAT_CLOCK_CONFIG` environment variable
/// 2. `config.toml` in the current directory (handiest when running from a
///    terminal, and what earlier versions did)
/// 3. `config.toml` in the per-user config directory (this is the path a
///    double-click takes; the directory is created if it does not exist yet)
pub fn resolve_config_path() -> PathBuf {
    if let Some(explicit) = std::env::var_os("FLOAT_CLOCK_CONFIG") {
        if !explicit.is_empty() {
            return PathBuf::from(explicit);
        }
    }

    let local = PathBuf::from(DEFAULT_CONFIG_NAME);
    if local.exists() {
        return local;
    }

    match user_config_dir() {
        Some(dir) => dir.join(DEFAULT_CONFIG_NAME),
        None => local,
    }
}

fn default_x() -> i32 {
    80
}
fn default_y() -> i32 {
    80
}
fn default_true() -> bool {
    true
}
fn default_opacity() -> f64 {
    1.0
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct WindowConfig {
    /// Top-left corner of the overlay; written back here after every drag.
    pub x: i32,
    pub y: i32,
    pub borderless: bool,
    pub topmost: bool,
    pub locked: bool,
    /// Whole-window opacity, 0.05 .. 1.0
    pub opacity: f64,
    /// Show a tray / menu-bar icon. Ignored on platforms without a tray backend.
    pub tray: bool,
    /// Windows only: ask for administrator rights at startup so the overlay can
    /// be placed in the top window band (see `crate::windows`).
    ///
    /// Off means the overlay sits below anything in a higher band - Task
    /// Manager with "Always on top" ticked, for instance. Ignored elsewhere.
    pub ui_access: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            x: default_x(),
            y: default_y(),
            borderless: default_true(),
            topmost: default_true(),
            locked: false,
            opacity: default_opacity(),
            tray: default_true(),
            ui_access: default_true(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DisplayConfig {
    /// Monospace font family; empty means "pick the system's monospace font"
    pub font_family: String,
    pub main_size: f32,
    pub sub_size: f32,
    pub color: String,
    pub sub_color: String,
    pub bold: bool,
    pub show_days: bool,
    pub gap: i32,
    pub info_template: String,
    pub info_size: f32,
    pub info_color: String,
    /// auto | knockout | text
    pub info_style: String,
    pub background: String,
    pub interval_ms: u64,
    pub main_template: String,
    pub sub_template: String,
    /// border | none
    pub lock_indicator: String,
    pub border_color: String,
    pub border_width: i32,
    pub solid_when_locked: bool,
    /// Upper bound on the text rasterisation scale. On Retina / HiDPI the
    /// display's own scale is followed; this is only a ceiling.
    pub max_scale: f32,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            font_family: String::new(),
            main_size: 46.0,
            sub_size: 18.0,
            color: "#00FF66".into(),
            sub_color: String::new(),
            bold: true,
            show_days: true,
            gap: 2,
            info_template: "{time} | {delta}".into(),
            info_size: 14.0,
            info_color: String::new(),
            info_style: "auto".into(),
            background: "#101010".into(),
            interval_ms: 200,
            main_template: "T{sign}{clock}".into(),
            sub_template: "T{sign}{clock}".into(),
            lock_indicator: "border".into(),
            border_color: String::new(),
            border_width: 2,
            solid_when_locked: true,
            max_scale: 2.0,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct TimeConfig {
    pub target: String,
    pub offset: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct NotifyConfig {
    pub enabled: bool,
    pub before: Vec<i64>,
    pub after: Vec<i64>,
    pub at_moment: bool,
    pub sound: bool,
    pub sound_name: String,
    pub title_template: String,
    pub body_before: String,
    pub body_at: String,
    pub body_after: String,
}

impl Default for NotifyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            before: vec![3600, 1800, 900, 600, 300, 180, 120, 60, 30, 10, 5, 3, 2, 1],
            after: vec![1, 5, 30, 60, 300],
            at_moment: true,
            sound: true,
            sound_name: "Glass".into(),
            title_template: "[T{sign}{clock}] {label}".into(),
            body_before: "{human} until {label}".into(),
            body_at: "{label} reached at {time}".into(),
            body_after: "{label} passed {human} ago".into(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub window: WindowConfig,
    pub display: DisplayConfig,
    pub time: TimeConfig,
    pub notify: NotifyConfig,
    #[serde(skip)]
    pub path: PathBuf,
}

impl Config {
    /// Colour of the third line (falls back to the main colour when empty).
    pub fn info_color(&self) -> &str {
        if self.display.info_color.trim().is_empty() {
            &self.display.color
        } else {
            &self.display.info_color
        }
    }

    pub fn sub_color(&self) -> &str {
        if self.display.sub_color.trim().is_empty() {
            &self.display.color
        } else {
            &self.display.sub_color
        }
    }

    pub fn border_color(&self) -> &str {
        if self.display.border_color.trim().is_empty() {
            &self.display.color
        } else {
            &self.display.border_color
        }
    }

    pub fn interval_ms(&self) -> u64 {
        self.display.interval_ms.max(50)
    }
}

pub fn load_config(path: &Path) -> Result<Config, String> {
    let default = Config {
        path: path.to_path_buf(),
        ..Default::default()
    };
    if !path.exists() {
        return Ok(default);
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read config: {e}"))?;
    let mut config: Config =
        toml::from_str(&text).map_err(|e| format!("cannot parse config file: {e}"))?;
    // `path` does not take part in deserialisation (`#[serde(skip)]`), so put it back by hand
    config.path = default.path;
    Ok(config)
}

/// A value that can be written back into the TOML file.
#[derive(Debug, Clone)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    IntList(Vec<i64>),
}

impl Value {
    pub fn to_toml(&self) -> String {
        match self {
            Value::Bool(value) => if *value { "true" } else { "false" }.to_string(),
            Value::Int(value) => value.to_string(),
            Value::Float(value) => format_float(*value),
            Value::Str(value) => quote(value),
            Value::IntList(values) => {
                let items: Vec<String> = values.iter().map(|v| v.to_string()).collect();
                format!("[{}]", items.join(", "))
            }
        }
    }
}

fn format_float(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        format!("{value}")
    }
}

fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// Write `"section.key": value` pairs back into the TOML file in place,
/// preserving comments and the existing key order.
///
/// Missing keys are appended to the end of their section; a missing section is
/// created.
pub fn patch_toml(path: &Path, updates: &[(&str, Value)]) -> Result<(), String> {
    let lines: Vec<String> = match std::fs::read_to_string(path) {
        Ok(text) => text.lines().map(|line| line.to_string()).collect(),
        Err(_) => Vec::new(),
    };

    let mut pending: Vec<(String, Value)> = updates
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
    let mut out: Vec<String> = Vec::new();
    let mut section = String::new();
    let mut seen_sections: Vec<String> = Vec::new();

    for original in &lines {
        let stripped = original.trim();
        if stripped.starts_with('[') && stripped.ends_with(']') {
            section = stripped[1..stripped.len() - 1].trim().to_string();
            seen_sections.push(section.clone());
            out.push(original.clone());
            continue;
        }
        if !stripped.starts_with('#') {
            if let Some(key) = key_of(stripped) {
                let full = if section.is_empty() {
                    key.to_string()
                } else {
                    format!("{section}.{key}")
                };
                if let Some(index) = pending.iter().position(|(name, _)| *name == full) {
                    let (_, value) = pending.remove(index);
                    let indent: String =
                        original.chars().take_while(|c| c.is_whitespace()).collect();
                    out.push(format!("{indent}{key} = {}", value.to_toml()));
                    continue;
                }
            }
        }
        out.push(original.clone());
    }

    for (full_key, value) in pending {
        let (target_section, key) = match full_key.rsplit_once('.') {
            Some((section, key)) => (section.to_string(), key.to_string()),
            None => (String::new(), full_key.clone()),
        };
        let line = format!("{key} = {}", value.to_toml());
        if target_section.is_empty() {
            out.push(line);
            continue;
        }
        if !seen_sections.contains(&target_section) {
            if out.last().is_some_and(|last| !last.trim().is_empty()) {
                out.push(String::new());
            }
            out.push(format!("[{target_section}]"));
            out.push(line);
            seen_sections.push(target_section);
            continue;
        }
        // Section already exists: insert after the last key line of that section
        let header = format!("[{target_section}]");
        let mut insert_at = out.len();
        for (index, existing) in out.iter().enumerate() {
            if existing.trim() != header {
                continue;
            }
            insert_at = index + 1;
            while insert_at < out.len() {
                let next = out[insert_at].trim();
                if next.starts_with('[') && next.ends_with(']') {
                    break;
                }
                insert_at += 1;
            }
            break;
        }
        out.insert(insert_at, line);
    }

    let body = out.join("\n");
    let body = body.trim_end_matches('\n').to_string();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            // The per-user config directory may just have been cleaned out;
            // make sure it exists before writing.
            std::fs::create_dir_all(parent).map_err(|e| format!("cannot create directory: {e}"))?;
        }
    }
    std::fs::write(path, format!("{body}\n")).map_err(|e| format!("cannot write config: {e}"))
}

fn key_of(line: &str) -> Option<&str> {
    let (key, _) = line.split_once('=')?;
    let key = key.trim();
    if key.is_empty()
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }
    Some(key)
}

/// Default target time: the next full hour.
pub fn default_target(now: NaiveDateTime) -> String {
    let shifted = now + Duration::hours(1);
    let next = shifted
        .date()
        .and_hms_opt(shifted.hour(), 0, 0)
        .unwrap_or(shifted);
    next.format("%Y-%m-%dT%H:%M:%S").to_string()
}

const TEMPLATE: &str = r##"# FloatClock overlay countdown - configuration
#
# Save this file and the running overlay picks the change up within about a
# second; there is no need to restart it.
#
# Precedence: --config <path>  >  $FLOAT_CLOCK_CONFIG  >  ./config.toml  >  this file.
# The command-line flags --target / --offset override this file for one run.
#
# Every key below is optional: delete a line and the built-in default comes back.

[window]
# Top-left corner of the overlay. Written back here automatically after a drag.
x = @@X@@
y = @@Y@@
# Drop the title bar, so that combined with the transparent background the
# window really is "just the text".
borderless = true
# Keep the overlay above every other window.
topmost = true
# While locked the overlay cannot be dragged. Toggle with the right mouse button
# (or the tray menu, or Cmd/Ctrl+L); the state is written back here.
locked = false
# Whole-window opacity, 0.05 .. 1.0. The transparent background is usually
# enough, so this normally stays at 1.0.
opacity = 1.0
# Show a tray icon (macOS menu bar / Windows notification area) with a menu for
# show-hide, lock, settings, opening this file, reloading and quitting.
# Linux has no tray backend in this build; the setting is ignored there.
tray = true
# Windows only. From Windows 8 on, windows sit in bands and an ordinary process
# cannot rise above ZBID_DESKTOP, so anything in a higher band - Task Manager
# with "Always on top" ticked, the on-screen keyboard - covers the overlay no
# matter what. Reaching the top band needs UIAccess, which needs administrator
# rights, so this asks for them at startup: one UAC prompt per launch.
#
# Say no to the prompt and the overlay still runs, just below those windows.
# Set this to false to stop being asked.
ui_access = true

[display]
# Monospace font family. Leave empty to auto-pick the system monospace font
# (Menlo / SF Mono / Consolas / DejaVu Sans Mono / Liberation Mono ...).
font_family = ""
# Size of the main title (time left until the offset moment), in points.
main_size = 46.0
# Size of the subtitle (time elapsed since the target moment).
sub_size = 18.0
# Bold green.
color = "#00FF66"
# Subtitle colour; empty means "same as the main colour".
sub_color = ""
bold = true
# Show DD:HH:MM:SS once the remaining time passes a day, zero-padded so every
# digit column lines up.
show_days = true
# Gap between the title and the subtitle, in pixels.
gap = 2
# Third line: the target moment plus the offset, drawn as green text knocked out
# of a solid green bar. Set it to "" to hide the line entirely.
# Placeholders:
#   {date} {time} {datetime}     the target moment T
#   {mark} {mark_datetime}       the offset moment M = T + offset
#   {delta}                      the offset, zero-padded, e.g. +02:00:00
#   {delta_human}                the offset in words, e.g. +2 hours
info_template = "{time} | {delta}"
# Size of the third line.
info_size = 14.0
# Third line colour; empty means "same as the main colour" (green).
info_color = ""
# Third line style:
#   "auto"     = knockout when possible (green bar, glyphs punched out), and
#                plain green text when no usable font is available
#   "knockout" = force the knockout
#   "text"     = plain green text on a transparent background
info_style = "auto"
# Backing colour used on platforms that cannot do per-pixel transparency (old X11).
background = "#101010"
# Refresh interval, in milliseconds.
interval_ms = 200
# Lock indicator: "border" draws a frame (solid = locked, dashed = draggable),
# "none" draws nothing.
lock_indicator = "border"
# Frame colour; empty means "same as the text".
border_color = ""
# Frame width, in pixels.
border_width = 2
# true: solid while locked and dashed while unlocked; false swaps the two.
solid_when_locked = true
# Ceiling for the text rasterisation scale. Retina / HiDPI displays are followed
# automatically; this only caps how far that can go.
max_scale = 2.0
# Display templates. {sign} is + or -, {clock} is the zero-padded time.
main_template = "T{sign}{clock}"
sub_template = "T{sign}{clock}"

[time]
# The target moment. Accepted forms:
#   2026-01-01T09:30:00   2026-01-01 09:30   2026-01-01
#   09:30:00   (that time today, rolling over to tomorrow once it has passed -
#               use this for a daily recurring schedule)
#   +1h30m     (relative to now)
# Once an absolute moment is in the past the display flips to T+ on its own.
target = "@@TARGET@@"
# The offset. The main countdown points at  target + offset.
# Accepts -00:05:00 / 5:00 / 1h30m / -300 / 0 and friends.
offset = "@@OFFSET@@"

[notify]
# Master switch.
enabled = true
# Fire a notification this many seconds BEFORE each moment.
before = [3600, 1800, 900, 600, 300, 180, 120, 60, 30, 10, 5, 3, 2, 1]
# Fire a notification this many seconds AFTER each moment.
after = [1, 5, 30, 60, 300]
# Notify exactly on the moment itself.
at_moment = true
# Play a sound.
sound = true
# macOS alert sound: Glass / Ping / Pop / Funk / Basso / Blow / Bottle / Frog /
# Hero / Morse / Purr / Sosumi / Submarine / Tink
sound_name = "Glass"
# Notification templates (plain text, no emoji - use [ ] and friends if you want
# decoration). Placeholders:
#   {label}    name of the moment      {sign}     - or +
#   {clock}    zero-padded time        {human}    duration in words ("2m")
#   {time}     that moment as HH:MM:SS {datetime} that moment in full
title_template = "[T{sign}{clock}] {label}"
body_before = "{human} until {label}"
body_at = "{label} reached at {time}"
body_after = "{label} passed {human} ago"
"##;

pub fn write_default_config(
    path: &Path,
    x: i32,
    y: i32,
    target: &str,
    offset: &str,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("cannot create directory: {e}"))?;
        }
    }
    let target = if target.trim().is_empty() {
        default_target(Local::now().naive_local())
    } else {
        target.to_string()
    };
    let text = TEMPLATE
        .replace("@@X@@", &x.to_string())
        .replace("@@Y@@", &y.to_string())
        .replace("@@TARGET@@", &target)
        .replace("@@OFFSET@@", offset);
    std::fs::write(path, text).map_err(|e| format!("cannot write config: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("float-clock-test-{name}-{}", std::process::id()));
        path
    }

    #[test]
    fn user_config_dir_follows_platform_convention() {
        let dir = user_config_dir().expect("the test machine should have HOME");
        let text = dir.to_string_lossy();
        if cfg!(target_os = "macos") {
            assert!(
                text.ends_with("Library/Application Support/FloatClock"),
                "{text}"
            );
        } else if cfg!(target_os = "windows") {
            assert!(text.ends_with("FloatClock"), "{text}");
        } else {
            assert!(text.ends_with("float-clock"), "{text}");
        }
    }

    #[test]
    fn explicit_config_env_var_wins() {
        let path = temp_path("env");
        // Only this process sees the variable, and the tests share one process,
        // so restore whatever was there as soon as we are done.
        let before = std::env::var_os("FLOAT_CLOCK_CONFIG");
        std::env::set_var("FLOAT_CLOCK_CONFIG", &path);
        assert_eq!(resolve_config_path(), path);
        match before {
            Some(value) => std::env::set_var("FLOAT_CLOCK_CONFIG", value),
            None => std::env::remove_var("FLOAT_CLOCK_CONFIG"),
        }
    }

    #[test]
    fn defaults_are_sane() {
        let config = Config::default();
        assert_eq!(config.display.color, "#00FF66");
        assert!(config.display.bold);
        assert_eq!(config.window.x, 80);
        assert_eq!(config.info_color(), "#00FF66");
        assert_eq!(config.interval_ms(), 200);
        assert!(config.notify.before.contains(&120));
        assert!(config.window.tray);
        assert!(config.window.ui_access);
    }

    #[test]
    fn missing_file_gives_defaults() {
        let path = temp_path("missing");
        let _ = std::fs::remove_file(&path);
        let config = load_config(&path).unwrap();
        assert_eq!(config.display.main_size, 46.0);
        assert_eq!(config.path, path);
    }

    #[test]
    fn partial_file_merges_with_defaults() {
        let path = temp_path("partial");
        std::fs::write(&path, "[display]\ncolor = \"#FF0000\"\nmain_size = 60\n").unwrap();
        let config = load_config(&path).unwrap();
        assert_eq!(config.display.color, "#FF0000");
        assert_eq!(config.display.main_size, 60.0);
        // Keys that were not written still hold their defaults
        assert_eq!(config.display.sub_size, 18.0);
        assert!(config.notify.enabled);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn patch_keeps_comments_and_order() {
        let path = temp_path("patch");
        std::fs::write(
            &path,
            "# leading comment\n[window]\n# the coordinates\nx = 1\ny = 2\n\n[time]\ntarget = \"09:00\"\n",
        )
        .unwrap();
        patch_toml(
            &path,
            &[
                ("window.x", Value::Int(120)),
                ("window.locked", Value::Bool(true)),
                ("time.offset", Value::Str("-5m".into())),
            ],
        )
        .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("# leading comment"));
        assert!(text.contains("# the coordinates"));
        assert!(text.contains("x = 120"));
        assert!(text.contains("y = 2"));
        assert!(text.contains("locked = true"));
        assert!(text.contains("offset = \"-5m\""));
        // Read it back to confirm the result is still valid TOML
        let config = load_config(&path).unwrap();
        assert_eq!(config.window.x, 120);
        assert!(config.window.locked);
        assert_eq!(config.time.offset, "-5m");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn patch_creates_missing_section() {
        let path = temp_path("patch-new");
        std::fs::write(&path, "[window]\nx = 1\n").unwrap();
        patch_toml(&path, &[("notify.before", Value::IntList(vec![60, 10]))]).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("[notify]"));
        assert!(text.contains("before = [60, 10]"));
        let config = load_config(&path).unwrap();
        assert_eq!(config.notify.before, vec![60, 10]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn default_config_round_trips() {
        let path = temp_path("template");
        write_default_config(&path, 80, 80, "2026-09-19T20:29:00", "+120m").unwrap();
        let config = load_config(&path).unwrap();
        assert_eq!(config.time.target, "2026-09-19T20:29:00");
        assert_eq!(config.time.offset, "+120m");
        assert_eq!(config.display.info_template, "{time} | {delta}");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("{time} | {delta}"),
            "the braces in the template must survive the round trip"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn value_rendering_escapes() {
        assert_eq!(Value::Str("a\"b".into()).to_toml(), "\"a\\\"b\"");
        assert_eq!(Value::Bool(false).to_toml(), "false");
        assert_eq!(Value::IntList(vec![1, 2]).to_toml(), "[1, 2]");
        assert_eq!(Value::Float(1.0).to_toml(), "1.0");
    }

    /// `config.example.toml` is shipped inside every release archive and linked
    /// from the README, so it has to stay in step with the template that
    /// `--init-config` writes. Rather than reviewing two copies by hand, this
    /// pins them together: the example must be exactly the template with the
    /// placeholders filled in.
    #[test]
    fn the_shipped_example_matches_the_generated_template() {
        let expected = TEMPLATE
            .replace("@@X@@", "80")
            .replace("@@Y@@", "80")
            .replace("@@TARGET@@", "2026-09-19T20:29:00")
            .replace("@@OFFSET@@", "-00:05:00");
        let example = include_str!("../config.example.toml");
        assert_eq!(
            example, expected,
            "config.example.toml has drifted from the template in src/config.rs; \
             regenerate it with `float-clock --init-config` and fill the placeholders in"
        );
    }
}
