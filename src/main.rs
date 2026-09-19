//! 命令行入口。

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use chrono::{Duration, Local, NaiveDateTime};

use float_clock::app::{self, RunOptions};
use float_clock::config::{self, Config};
use float_clock::notifier::MomentNotifier;
use float_clock::notify;
use float_clock::render::{self, RenderInput};
use float_clock::text::Fonts;
use float_clock::timefmt::{self, format_hms, parse_duration, parse_target};

const VERSION: &str = env!("CARGO_PKG_VERSION");

const USAGE: &str = "\
float-clock —— 无背景悬浮 T± 倒计时

用法:
    float-clock [选项]

选项:
    --config <路径>        指定配置文件（默认 ./config.toml）
    --init-config          生成默认配置文件后退出
    --force                配合 --init-config，覆盖已存在的文件
    --target <时间点>      临时覆盖 [time] target
    --offset <时长>        临时覆盖 [time] offset
    --print                打印当前状态与接下来的提醒后退出（不开窗口）
    --render-png <路径>    把悬浮窗渲染成 PNG 后退出（不开窗口）
    --scale <倍数>         配合 --render-png，渲染倍率（默认 2）
    --now <时间点>         配合 --print / --render-png，指定「现在」（便于复现）
    --diagnose             打印环境与渲染诊断信息后退出
    --test-notify          发一条测试通知后退出
    --settings             启动时直接打开设置窗口
    --no-transparent       强制不透明背景（排障用）
    --probe <秒数>         窗口起来后从 GPU 回读一次真实像素并打印体检报告（排障用）
    --probe-png <路径>     配合 --probe，把回读到的像素写成 PNG
    -h, --help             显示本帮助
    -V, --version          显示版本

交互:
    左键拖动移动 · 右键锁定/解锁（实线=锁定，虚线=可拖动）· 双击打开设置
    Ctrl+L 锁定 · Ctrl+R 重载配置 · Ctrl+Q 退出
";

/// Windows 上双击打开时，控制台子系统会先弹出一个黑框。
/// 这里保留控制台子系统（这样 `--print` / `--diagnose` 的输出在任何情况下都正常），
/// 只在「没有任何命令行参数」也就是双击启动的情况下，把那个黑框藏起来。
#[cfg(target_os = "windows")]
fn hide_console_for_double_click() {
    const SW_HIDE: i32 = 0;
    extern "system" {
        fn GetConsoleWindow() -> *mut core::ffi::c_void;
        fn ShowWindow(hwnd: *mut core::ffi::c_void, cmd: i32) -> i32;
    }
    unsafe {
        let hwnd = GetConsoleWindow();
        if !hwnd.is_null() {
            ShowWindow(hwnd, SW_HIDE);
        }
    }
}

#[cfg(not(target_os = "windows"))]
#[inline]
fn hide_console_for_double_click() {}

fn main() -> ExitCode {
    // 必须是第一件事，越早越好，黑框才来不及显示
    if std::env::args_os().len() <= 1 {
        hide_console_for_double_click();
    }
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("[float-clock] {message}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Default)]
struct Args {
    config: Option<PathBuf>,
    init_config: bool,
    force: bool,
    target: Option<String>,
    offset: Option<String>,
    print: bool,
    render_png: Option<PathBuf>,
    scale: Option<f32>,
    now: Option<String>,
    diagnose: bool,
    test_notify: bool,
    settings: bool,
    no_transparent: bool,
    probe: Option<f32>,
    probe_png: Option<PathBuf>,
}

fn run() -> Result<(), String> {
    let args = parse_args()?;

    let config_path = args
        .config
        .clone()
        .unwrap_or_else(|| PathBuf::from(config::DEFAULT_CONFIG_NAME));

    if args.init_config {
        if config_path.exists() && !args.force {
            return Err(format!(
                "{} 已经存在（要覆盖请加 --force）",
                config_path.display()
            ));
        }
        config::write_default_config(&config_path, 80, 80, "", "-00:05:00")?;
        println!("已生成 {}", config_path.display());
        return Ok(());
    }

    let mut config = config::load_config(&config_path)?;
    if let Some(target) = &args.target {
        config.time.target = target.clone();
    }
    if let Some(offset) = &args.offset {
        config.time.offset = offset.clone();
    }

    if args.test_notify {
        let sound = config
            .notify
            .sound
            .then(|| config.notify.sound_name.clone());
        let sent = notify::send(
            "[T-00:00:00] FloatClock 测试",
            "看到这条说明系统通知是通的",
            sound.as_deref(),
        );
        println!("通知后端：{}", notify::backend());
        return if sent {
            Ok(())
        } else {
            Err("发送通知失败".into())
        };
    }

    let now = match &args.now {
        Some(spec) => parse_target(spec, Local::now().naive_local())?,
        None => Local::now().naive_local(),
    };

    if args.diagnose {
        return print_diagnose(&config, &config_path, now);
    }

    if args.print {
        return print_status(&config, now);
    }

    if let Some(path) = &args.render_png {
        let overlay = build_overlay(&config, now, args.scale.unwrap_or(2.0), true)?;
        std::fs::write(path, overlay.pixmap.to_png()?)
            .map_err(|e| format!("写入 PNG 失败：{e}"))?;
        for line in &overlay.layout.lines {
            println!(
                "{:>7.1},{:<7.1} {:>5.1}pt {:>7.1}px  {}{}",
                line.x,
                line.top,
                line.size,
                line.width,
                line.text,
                if line.knockout { "   [镂空]" } else { "" }
            );
        }
        println!(
            "已写出 {}（{} × {} 像素，倍率 {}）",
            path.display(),
            overlay.width(),
            overlay.height(),
            overlay.layout.scale
        );
        return Ok(());
    }

    app::run(
        config,
        RunOptions {
            open_settings: args.settings,
            force_opaque: args.no_transparent,
            probe_after: args.probe.map(std::time::Duration::from_secs_f32),
            probe_png: args.probe_png.clone(),
        },
    )
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args::default();
    let mut argv = std::env::args().skip(1).peekable();
    while let Some(arg) = argv.next() {
        // 支持 --opt=value 与 --opt value 两种写法
        let (flag, inline) = match arg.split_once('=') {
            Some((flag, value)) => (flag.to_string(), Some(value.to_string())),
            None => (arg, None),
        };
        let mut take = |name: &str| -> Result<String, String> {
            match &inline {
                Some(value) => Ok(value.clone()),
                None => argv.next().ok_or_else(|| format!("{name} 后面需要一个值")),
            }
        };
        match flag.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("float-clock {VERSION}");
                std::process::exit(0);
            }
            "--config" => args.config = Some(PathBuf::from(take("--config")?)),
            "--init-config" => args.init_config = true,
            "--force" => args.force = true,
            "--target" => args.target = Some(take("--target")?),
            "--offset" => args.offset = Some(take("--offset")?),
            "--print" => args.print = true,
            "--render-png" => args.render_png = Some(PathBuf::from(take("--render-png")?)),
            "--scale" => {
                args.scale = Some(
                    take("--scale")?
                        .parse()
                        .map_err(|_| "--scale 需要一个数字".to_string())?,
                )
            }
            "--now" => args.now = Some(take("--now")?),
            "--diagnose" => args.diagnose = true,
            "--test-notify" => args.test_notify = true,
            "--settings" => args.settings = true,
            "--no-transparent" => args.no_transparent = true,
            "--probe-png" => args.probe_png = Some(PathBuf::from(take("--probe-png")?)),
            "--probe" => {
                args.probe = Some(
                    take("--probe")?
                        .parse()
                        .map_err(|_| "--probe 需要一个秒数".to_string())?,
                )
            }
            other => return Err(format!("无法识别的参数：{other}（用 --help 看看用法）")),
        }
    }
    Ok(args)
}

fn offset_duration(offset_seconds: f64) -> Duration {
    Duration::milliseconds((offset_seconds * 1000.0).round() as i64)
}

fn moment_text(
    config: &Config,
    now: NaiveDateTime,
) -> Result<(NaiveDateTime, NaiveDateTime, f64), String> {
    let target = parse_target(&config.time.target, now)?;
    let offset = parse_duration(&config.time.offset)?;
    let mark = target + offset_duration(offset);
    Ok((target, mark, offset))
}

fn build_overlay(
    config: &Config,
    now: NaiveDateTime,
    scale: f32,
    transparent: bool,
) -> Result<render::Overlay, String> {
    let fonts = Fonts::load(&config.display.font_family)?;
    let (target, mark, _) = moment_text(config, now)?;
    render::render(&RenderInput {
        config,
        fonts: &fonts,
        target,
        mark,
        now,
        locked: config.window.locked,
        scale,
        transparent,
    })
}

fn print_status(config: &Config, now: NaiveDateTime) -> Result<(), String> {
    let (target, mark, offset) = moment_text(config, now)?;
    let overlay = build_overlay(config, now, 1.0, true)?;

    println!("现在          : {}", timefmt::datetime_of(now));
    println!("目标时间点 T  : {}", timefmt::datetime_of(target));
    println!(
        "偏移时刻   M  : {}   （偏移 {}）",
        timefmt::datetime_of(mark),
        format_hms(offset.round() as i64, true)
    );
    println!("主标题        : {}", overlay.layout.lines[0].text);
    println!("副标题        : {}", overlay.layout.lines[1].text);
    if let Some(info) = overlay.layout.lines.get(2) {
        println!(
            "第三行        : {}{}",
            info.text,
            if info.knockout {
                "（绿块镂空）"
            } else {
                "（普通绿字）"
            }
        );
    }
    println!(
        "窗口尺寸      : {} × {} 像素",
        overlay.width(),
        overlay.height()
    );
    println!(
        "锁定状态      : {}",
        if config.window.locked {
            "锁定（实线框）"
        } else {
            "解锁（虚线框）"
        }
    );
    println!("通知后端      : {}", notify::backend());

    let mut notifier = MomentNotifier::new(config.notify.clone());
    notifier.arm(
        vec![
            ("偏移时刻".to_string(), mark),
            ("目标时间点".to_string(), target),
        ],
        now,
    );
    println!("接下来的提醒  :");
    // 两个时刻各自的提醒交错在一起，直接取前 N 条会被其中一个占满，
    // 所以按时刻分组，每个时刻只展示最靠前的几条。
    const PER_MOMENT: usize = 6;
    let mut seen: Vec<(String, usize)> = Vec::new();
    let mut shown = Vec::new();
    for (when, label, title) in notifier.upcoming(now, usize::MAX) {
        match seen.iter_mut().find(|(name, _)| *name == label) {
            Some((_, count)) if *count >= PER_MOMENT => continue,
            Some((_, count)) => *count += 1,
            None => seen.push((label, 1)),
        }
        shown.push((when, title));
    }
    for (when, title) in shown {
        let delta = (when - now).num_seconds().max(0);
        println!("    +{:<10} {}", format_hms(delta, true), title);
    }
    Ok(())
}

fn print_diagnose(config: &Config, path: &Path, now: NaiveDateTime) -> Result<(), String> {
    println!("float-clock   : {VERSION}");
    println!("平台          : {}", std::env::consts::OS);
    println!("配置文件      : {}", path.display());
    println!(
        "透明背景      : {}",
        if cfg!(target_os = "linux") {
            "需要合成器（compositor）才有效，否则铺 background 底色"
        } else {
            "窗口级 alpha，直接和桌面合成"
        }
    );
    println!("通知后端      : {}", notify::backend());

    let fonts = Fonts::load(&config.display.font_family)?;
    println!(
        "等宽字体      : {}{}",
        fonts.primary_name,
        match &fonts.fallback_name {
            Some(name) => format!("   兜底字体：{name}"),
            None => "   （没找到兜底字体）".to_string(),
        }
    );

    let overlay = build_overlay(config, now, 1.0, true)?;
    println!(
        "渲染          : {} × {} 像素，{} 行",
        overlay.width(),
        overlay.height(),
        overlay.layout.lines.len()
    );
    for line in &overlay.layout.lines {
        println!(
            "    {:<4} {:>5.1}pt  {:<26} x={:.1} y={:.1} w={:.1}",
            if line.knockout { "镂空" } else { "文字" },
            line.size,
            line.text,
            line.x,
            line.top,
            line.width
        );
    }
    Ok(())
}
