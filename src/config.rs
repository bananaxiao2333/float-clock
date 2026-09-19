//! 配置：TOML 读取、默认值合并、保留注释的回写、默认配置生成。

use std::path::{Path, PathBuf};

use chrono::{Duration, Local, NaiveDateTime, Timelike};
use serde::Deserialize;

pub const DEFAULT_CONFIG_NAME: &str = "config.toml";

/// 找不到配置文件时，按用户目录放一份的目录名。
const APP_DIR_NAME: &str = "FloatClock";

/// 每用户配置目录（不依赖任何 crate，三平台各按自己的惯例来）：
///
/// * macOS   `~/Library/Application Support/FloatClock`
/// * Windows `%APPDATA%\FloatClock`
/// * Linux   `$XDG_CONFIG_HOME/float-clock`（默认 `~/.config/float-clock`）
///
/// 有它才能让「下载下来双击打开」这件事成立：.app 双击时工作目录是 `/`，
/// 相对路径的 `./config.toml` 既找不到也写不进去。
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

/// 没显式指定 `--config` 时，按这个顺序找配置文件：
///
/// 1. 环境变量 `FLOAT_CLOCK_CONFIG`
/// 2. 当前目录下的 `config.toml`（在终端里跑的时候最顺手，也和旧版行为一致）
/// 3. 每用户配置目录里的 `config.toml`（双击打开走的是这条，目录不存在就创建）
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
    /// 浮窗左上角坐标（拖动后自动写回）
    pub x: i32,
    pub y: i32,
    pub borderless: bool,
    pub topmost: bool,
    pub locked: bool,
    /// 整体不透明度 0.05 ~ 1.0
    pub opacity: f64,
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
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DisplayConfig {
    /// 等宽字体族名；留空自动挑系统等宽字体
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
    /// 字体渲染倍率上限（Retina 上跟随显示器缩放，此项只是上限）
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
            info_template: "{time} · {delta}".into(),
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
            body_before: "距离{label}还有 {human}".into(),
            body_at: "{label}已到 · {time}".into(),
            body_after: "{label}已过去 {human}".into(),
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
    /// 目标时间点的显示颜色（第三行留空时跟随主色）。
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
    let text = std::fs::read_to_string(path).map_err(|e| format!("读取配置失败：{e}"))?;
    let mut config: Config = toml::from_str(&text).map_err(|e| format!("解析配置文件失败：{e}"))?;
    // path 不参与反序列化（`#[serde(skip)]`），手工补回来
    config.path = default.path;
    Ok(config)
}

/// 可以写回 TOML 的值。
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

/// 就地把 `"节.键": 值` 写回 TOML，保留注释与原有顺序。
///
/// 缺失的键会追加到对应小节末尾；小节不存在则新建。
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
        // 小节已存在：插到该节最后一行键的后面
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
            // 用户配置目录可能刚被清理掉，写之前确保它在
            std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败：{e}"))?;
        }
    }
    std::fs::write(path, format!("{body}\n")).map_err(|e| format!("写入配置失败：{e}"))
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

/// 默认目标时间点：下一个整点。
pub fn default_target(now: NaiveDateTime) -> String {
    let shifted = now + Duration::hours(1);
    let next = shifted
        .date()
        .and_hms_opt(shifted.hour(), 0, 0)
        .unwrap_or(shifted);
    next.format("%Y-%m-%dT%H:%M:%S").to_string()
}

const TEMPLATE: &str = r##"# FloatClock 悬浮倒计时配置
# 改完保存即可，程序约 1 秒内自动重载（无需重启）。
# 命令行 --target / --offset 可临时覆盖本文件。

[window]
# 浮窗左上角坐标（拖动后会自动写回这里）
x = @@X@@
y = @@Y@@
# 去掉标题栏，配合透明背景实现「只有文字」的悬浮效果
borderless = true
# 永远置顶
topmost = true
# 锁定后不能拖动（右键随时切换；锁定状态也会写回这里）
locked = false
# 整体不透明度 0.05~1.0（透明背景已够用，一般保持 1.0）
opacity = 1.0

[display]
# 等宽字体；留空自动挑选系统等宽字体（Menlo / Consolas / DejaVu Sans Mono ...）
font_family = ""
# 主标题（距离偏移还有多久）字号，单位 pt
main_size = 46.0
# 副标题（距离目标时间点过去了多久）字号
sub_size = 18.0
# 绿色粗体
color = "#00FF66"
# 副标题颜色，留空表示跟主标题一致
sub_color = ""
bold = true
# 超过一天时显示 DD:HH:MM:SS（所有位都补 0 对齐）
show_days = true
# 主副标题之间的间距（像素）
gap = 2
# 第三行：目标时间点 + 偏移（绿底镂空字）。设成空字符串 "" 则整行不显示。
# 可用占位符：
#   {date} {time} {datetime}     目标时间点 T
#   {mark} {mark_datetime}       偏移时刻 M = T + offset
#   {delta}                      偏移的补零写法，如 +02:00:00
#   {delta_human}                偏移的中文写法，如 +2 小时
info_template = "{time} · {delta}"
# 第三行字号
info_size = 14.0
# 第三行颜色，留空 = 跟主标题一致（绿色）
info_color = ""
# 第三行样式：
#   "auto"     = 强制镂空（绿块 + 抠掉字），拿不到字体时退回普通绿字
#   "knockout" = 同上
#   "text"     = 普通绿字、透明底
info_style = "auto"
# 不支持透明背景的平台（老 X11）上用的底色
background = "#101010"
# 刷新间隔（毫秒）
interval_ms = 200
# 锁定状态指示：border = 用外框线表示（实线=锁定，虚线=可拖动），none = 不显示
lock_indicator = "border"
# 外框线颜色，留空 = 跟文字同色
border_color = ""
# 外框线宽（像素）
border_width = 2
# true：锁定时实线、解锁时虚线；false 反过来
solid_when_locked = true
# 文字渲染倍率上限（Retina / HiDPI 上自动跟随显示器缩放，此项只是上限）
max_scale = 2.0
# 显示模板：{sign} 就是 + / -，{clock} 是补零时间
main_template = "T{sign}{clock}"
sub_template = "T{sign}{clock}"

[time]
# 目标时间点。支持：
#   2026-01-01T09:30:00   2026-01-01 09:30   2026-01-01
#   09:30:00（今天该时刻，已过则自动顺延到明天 —— 每天重复的日程用这种写法）
#   +1h30m（相对现在）
# 绝对时间点过去之后，显示会自然翻转成 T+…
target = "@@TARGET@@"
# 偏移。主标题倒计时指向的时刻 = 目标时间点 + 偏移。
# 支持 -00:05:00 / 5:00 / 1h30m / -300 / 0 等写法
offset = "@@OFFSET@@"

[notify]
# 总开关
enabled = true
# 在每个时间点【之前】这些秒数各提醒一次
before = [3600, 1800, 900, 600, 300, 180, 120, 60, 30, 10, 5, 3, 2, 1]
# 在每个时间点【之后】这些秒数各提醒一次
after = [1, 5, 30, 60, 300]
# 正点提醒
at_moment = true
# 通知声音
sound = true
# macOS 提示音：Glass / Ping / Pop / Funk / Basso / Blow / Bottle / Frog / Hero / Morse / Purr / Sosumi / Submarine / Tink
sound_name = "Glass"
# 通知文案模板（纯文本，不带 emoji，用 [] 这类符号做装饰）。占位符：
#   {label} 时间点名称   {sign} - 或 +   {clock} 补零时间   {human} 人话时长
#   {time} 该时刻 HH:MM:SS   {datetime} 该时刻完整时间
title_template = "[T{sign}{clock}] {label}"
body_before = "距离{label}还有 {human}"
body_at = "{label}已到 · {time}"
body_after = "{label}已过去 {human}"
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
            std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败：{e}"))?;
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
    std::fs::write(path, text).map_err(|e| format!("写入配置失败：{e}"))
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
        let dir = user_config_dir().expect("测试机上应该有 HOME");
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
        // 这个变量只在本进程里改，测试是串行跑同一进程，所以用完立刻还原
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
        // 没写的键仍然是默认值
        assert_eq!(config.display.sub_size, 18.0);
        assert!(config.notify.enabled);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn patch_keeps_comments_and_order() {
        let path = temp_path("patch");
        std::fs::write(
            &path,
            "# 顶部注释\n[window]\n# 坐标\nx = 1\ny = 2\n\n[time]\ntarget = \"09:00\"\n",
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
        assert!(text.contains("# 顶部注释"));
        assert!(text.contains("# 坐标"));
        assert!(text.contains("x = 120"));
        assert!(text.contains("y = 2"));
        assert!(text.contains("locked = true"));
        assert!(text.contains("offset = \"-5m\""));
        // 重新读回来确认是合法 TOML
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
        assert_eq!(config.display.info_template, "{time} · {delta}");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("{time} · {delta}"),
            "模板里的花括号不能被吃掉"
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
}
