# FloatClock

**无背景悬浮 T± 倒计时**——窗口里只有绿色粗体等宽文字，可以拖动、可以锁定，临近设定时刻会弹系统通知。

[![CI](https://github.com/bananaxiao2333/float-clock/actions/workflows/ci.yml/badge.svg)](https://github.com/bananaxiao2333/float-clock/actions/workflows/ci.yml)
[![Release](https://github.com/bananaxiao2333/float-clock/actions/workflows/release.yml/badge.svg)](https://github.com/bananaxiao2333/float-clock/actions/workflows/release.yml)
[![Release 下载](https://img.shields.io/github/v/release/bananaxiao2333/float-clock?label=%E4%B8%8B%E8%BD%BD&sort=semver)](https://github.com/bananaxiao2333/float-clock/releases/latest)
[![下载量](https://img.shields.io/github/downloads/bananaxiao2333/float-clock/total)](https://github.com/bananaxiao2333/float-clock/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![更新日志](https://img.shields.io/badge/%E6%9B%B4%E6%96%B0%E6%97%A5%E5%BF%97-CHANGELOG-informational)](CHANGELOG.md)

[![Rust](https://img.shields.io/badge/Rust-1.80%2B-000000?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![平台](https://img.shields.io/badge/%E5%B9%B3%E5%8F%B0-macOS%20%7C%20Linux%20%7C%20Windows-2ea44f)](#%E5%B9%B3%E5%8F%B0%E5%B7%AE%E5%BC%82)
[![单文件](https://img.shields.io/badge/%E5%8D%95%E6%96%87%E4%BB%B6-%E6%97%A0%E8%BF%90%E8%A1%8C%E6%97%B6-6f42c1)](#%E5%BF%AB%E9%80%9F%E5%BC%80%E5%A7%8B)
[![测试](https://img.shields.io/badge/test-61%20passed-success)](#%E4%BB%8E%E6%BA%90%E7%A0%81%E6%9E%84%E5%BB%BA)
[![无背景](https://img.shields.io/badge/%E8%83%8C%E6%99%AF-%E7%9C%9F%E9%80%8F%E6%98%8E-00FF66)](#%E5%B9%B3%E5%8F%B0%E5%B7%AE%E5%BC%82)

用 Rust 写的，同一个代码库交叉编译出 **macOS / Linux / Windows** 三个平台的**单文件**可执行程序，
双击就能跑，不需要装 Python、不需要运行时。

[Python 版](https://github.com/bananaxiao2333/float-clock/tree/python)在 `python` 分支上（最早用 uv + Tk 的实现）。

```
T-02:29:00          ← 主标题：距离「偏移时刻」还有多久
  T-00:29:00        ← 副标题：距离「目标时间点」过去了多久
▐20:29:00 · +02:00:00▌  ← 第三行：目标时间点 + 偏移（绿底镂空字）
```

---

## 快速开始

### 1. 下载

到 [**Releases**](https://github.com/bananaxiao2333/float-clock/releases/latest) 下对应平台的那个文件。

| 平台 | 下载这个 | 怎么跑 |
| --- | --- | --- |
| **macOS** | `float-clock-macos-universal.zip` | 解压 → 双击 `FloatClock.app`（Intel / Apple Silicon 通用） |
| **Linux** | `float-clock-linux-x86_64.tar.gz` | `tar -xzf` → `./float-clock-linux-x86_64` |
| **Windows** | `float-clock-windows-x86_64.exe` | 双击 |

> **macOS 别下裸二进制。** Release 里那个不带 `.zip` 的 `float-clock-macos-universal`
> 是给脚本/终端用的：浏览器下载会把可执行位抹掉，Finder 于是把它当文本文件丢给「文本编辑」，
> 弹出一句 **「无法打开文件，文字编码 Unicode (UTF-8) 不适用」**。
> 想手动用的话先 `chmod +x float-clock-macos-universal`。

> **macOS 首次打开**如果提示「无法验证开发者」，右键 →「打开」放行一次即可（没做公证，不是毒）。
> 也可以用终端绕过：`xattr -dr com.apple.quarantine FloatClock.app`。

同一个 Release 里还有 `SHA256SUMS`，想核对的话：

```bash
shasum -a 256 -c SHA256SUMS        # macOS / Linux
```

Linux 上还可以把图标装上（可选）：

```bash
mkdir -p ~/.local/share/icons/hicolor/512x512/apps
curl -L -o ~/.local/share/icons/hicolor/512x512/apps/float-clock.png \
  https://raw.githubusercontent.com/bananaxiao2333/float-clock/main/assets/icon-512.png
```

### 2. 跑起来

双击打开也行，命令行也行：

```bash
# 1. 生成配置
float-clock --init-config

# 2. 改 config.toml 里的 target / offset，然后跑起来
float-clock
```

命令行也能临时覆盖：

```bash
float-clock --target 2026-09-19T20:29:00 --offset +120m
```

**配置文件放哪**（都找不到时会自动生成一份）：

1. `--config <路径>` 指定的
2. 环境变量 `FLOAT_CLOCK_CONFIG`
3. 当前目录下的 `config.toml`
4. 用户配置目录 —— macOS `~/Library/Application Support/FloatClock/`，
   Windows `%APPDATA%\FloatClock\`，Linux `~/.config/float-clock/`

双击打开时工作目录是 `/`，走的是第 4 条。所以「下载 → 双击 → 关掉 → 再打开」
位置和设置都会记得。

### 交互

| 操作 | 效果 |
| --- | --- |
| 左键拖动 | 移动浮窗（松开后坐标自动写回 `config.toml`） |
| 右键 | 锁定 / 解锁 |
| 双击 | 打开设置窗口 |
| `Ctrl/⌘ + L` | 锁定 / 解锁 |
| `Ctrl/⌘ + ,` | 设置 |
| `Ctrl/⌘ + R` | 重载配置 |
| `Ctrl/⌘ + Q` | 退出 |

锁定状态不用文字提示，看外框线：**实线 = 锁定**，**虚线 = 可拖动**。

---

## T± 是什么意思

程序里有两个时刻：

* **目标时间点 `T`** —— 副标题倒计时指向它，过了之后显示已经过去多久
* **偏移时刻 `M = T + offset`** —— 主标题倒计时指向它

| 显示 | 含义 |
| --- | --- |
| `T-00:05:00` | 距离该时刻还有 5 分 00 秒 |
| `T+00:00:12` | 该时刻已经过去 12 秒 |
| `T-00:00:00` | 正点（该时刻所在的这一秒） |

所有字段都补 0 对齐；超过一天自动变成 `DD:HH:MM:SS`。

### 例子：20:29 泄露，紧急处理窗口期 120 分钟

```toml
[time]
target = "2026-09-19T20:29:00"   # 泄露发生的那一刻
offset = "+120m"                  # 紧急处理窗口期 120 分钟
```

* 主标题 `T-…` 指向 **22:29**（窗口期截止）
* 副标题 `T±` 指向 **20:29**（泄露时刻），过了 20:29 就翻成 `T+…` 显示已过多久
* 第三行 `20:29:00 · +02:00:00` 把两个时刻一起摆出来

想每天重复的日程直接用时间写法，已过会自动顺延到明天：

```toml
target = "20:29"
```

---

## 配置

`config.toml` 改完保存即可，程序约 1 秒内自动重载，不用重启。

```toml
[window]
x = 80               # 浮窗坐标（拖动后自动写回）
y = 80
borderless = true    # 无边框
topmost = true       # 永远置顶
locked = false       # 锁定后不能拖
opacity = 1.0

[display]
font_family = ""     # 留空自动挑系统等宽字体
main_size = 46.0     # 主标题字号（pt）
sub_size = 18.0
color = "#00FF66"    # 绿色
sub_color = ""       # 留空 = 跟主标题同色
bold = true
show_days = true     # 超过一天显示 DD:HH:MM:SS
gap = 2              # 行间距（像素）

info_template = "{time} · {delta}"   # 第三行，设置成 "" 就整行不显示
info_size = 14.0
info_color = ""                      # 留空 = 跟主标题同色
info_style = "auto"                  # auto/knockout = 绿底镂空；text = 普通绿字
background = "#101010"               # 不支持透明的平台上用的底色

interval_ms = 200
lock_indicator = "border"            # border = 用外框线表示锁定；none = 不显示
border_color = ""                    # 留空 = 跟文字同色
border_width = 2
solid_when_locked = true             # true：锁定时实线、解锁时虚线
max_scale = 2.0                      # 文字渲染倍率上限（Retina 自动跟随）

main_template = "T{sign}{clock}"
sub_template = "T{sign}{clock}"

[time]
target = "2026-09-19T20:29:00"
offset = "+120m"

[notify]
enabled = true
before = [3600, 1800, 900, 600, 300, 180, 120, 60, 30, 10, 5, 3, 2, 1]
after = [1, 5, 30, 60, 300]
at_moment = true
sound = true
title_template = "[T{sign}{clock}] {label}"
body_before = "距离{label}还有 {human}"
body_at = "{label}已到 · {time}"
body_after = "{label}已过去 {human}"
```

### 第三行的占位符

| 占位符 | 内容 |
| --- | --- |
| `{date}` `{time}` `{datetime}` | 目标时间点 `T` |
| `{mark}` `{mark_datetime}` | 偏移时刻 `M = T + offset` |
| `{delta}` | 偏移的补零写法，如 `+02:00:00` |
| `{delta_human}` | 偏移的中文写法，如 `+2 小时` |

认不出的占位符会原样留在画面上。

### 时间写法

`target` 支持：

```
2026-09-19T20:29:00   2026-09-19 20:29   2026-09-19
2026/09/19 20:29      2026.09.19 20:29   09-19 20:29
20:29                 20:29:00           ← 今天该时刻，已过顺延到明天
+1h30m                -10m               ← 相对现在
```

`offset` 支持：

```
0    -300    90s     5m     1h30m    1d2h
00:05:00   5:00   -00:05:00   +120m
```

### 通知文案

通知里**不带 emoji**，装饰统一用 `[]` 这类括号。可用占位符：

| 占位符 | 内容 |
| --- | --- |
| `{label}` | 时间点名称（`目标时间点` / `偏移时刻`） |
| `{sign}` `{clock}` | `-`/`+` 与补零时长 |
| `{human}` | 人话时长，如 `2 小时` |
| `{time}` `{datetime}` | 该时刻 |

通知的**图标由系统按「发送通知的程序」决定**，三个平台的后端都没有自定义图标的接口，
这是系统限制，程序不做任何平台特有的绕行。

---

## 命令行

```
float-clock [选项]

--config <路径>        指定配置文件（默认 ./config.toml）
--init-config          生成默认配置文件后退出
--force                配合 --init-config，覆盖已存在的文件
--target <时间点>      临时覆盖 [time] target
--offset <时长>        临时覆盖 [time] offset
--print                打印当前状态与接下来的提醒后退出（不开窗口）
--render-png <路径>    把浮窗渲染成 PNG 后退出（不开窗口）
--scale <倍数>         配合 --render-png，渲染倍率（默认 2）
--now <时间点>         配合 --print/--render-png/--diagnose，指定「现在」
--diagnose             打印环境与渲染诊断信息后退出
--test-notify          发一条测试通知后退出
--settings             启动时直接打开设置窗口
--no-transparent       强制不透明背景（排障用）
--probe <秒数>         窗口起来后从 GPU 回读真实像素并打印体检报告
--probe-png <路径>     配合 --probe，把回读到的像素写成 PNG
-h, --help / -V, --version
```

不开窗口就能自检：

```bash
# 画面长什么样，直接出图
float-clock --target 2026-09-19T20:29:00 --offset +120m \
            --now 2026-09-19T20:00:00 --render-png /tmp/overlay.png

# 状态、接下来的提醒
float-clock --print --now 2026-09-19T20:00:00

# 字体、渲染尺寸、平台能力
float-clock --diagnose

# 窗口是不是真的透明（从 GPU 回读这一帧的真实像素）
float-clock --probe 2 --probe-png /tmp/window.png
```

---

## 平台差异

| | macOS | Windows | Linux |
| --- | --- | --- | --- |
| 无边框 + 置顶 | ✅ | ✅ | ✅ |
| 真透明背景 | ✅ 窗口级 alpha | ✅ 窗口级 alpha | ✅ 需要合成器（compositor） |
| 绿底镂空第三行 | ✅ | ✅ | ✅ |
| 系统通知 | `osascript`（有 `terminal-notifier` 就用它） | PowerShell WinRT Toast | `notify-send` |
| 双击启动时的控制台 | 无 | 自动隐藏（进程仍是控制台子系统，`--print` 之类的输出照常） | 取决于桌面环境 |
| 默认等宽字体 | Menlo | Consolas | DejaVu Sans Mono |
| 默认中文兜底字体 | PingFang SC | Microsoft YaHei | Noto Sans CJK SC |
| 图标 | `FloatClock.app` 里的 `.icns` | 写进 exe 的资源段（含版本信息） | `assets/icon-512.png` |

不支持的机器上程序会退回 `background` 配置的实心底色——文字和倒计时照常工作，只是背后有个色块。

macOS 上还会顺手做两件事：关掉系统给透明窗口加的那圈阴影，并把应用设成
`Accessory`（有窗口但不占 Dock 图标）。

Windows 上发通知时会带 `CREATE_NO_WINDOW` 启动 PowerShell，否则每弹一次通知都会闪一个黑框。

### 这些平台到底验到什么程度

说清楚比较要紧：

| 平台 | 交叉编译 | 二进制格式 / 依赖检查 | 无窗口的命令行路径 | 真机开窗口 |
| --- | --- | --- | --- | --- |
| macOS arm64 | ✅ | ✅ | ✅ | ✅ 全流程（窗口透明、GPU 像素回读、通知、设置窗口、.app 双击） |
| macOS x86_64 | ✅ | ✅ | ✅ 同一个通用二进制 | ⬜ 没有 Intel 机器 |
| Linux x86_64 | ✅ | ✅ 最高只要求 glibc 2.28，动态依赖只有 libc/libm/libpthread/libdl | ✅ CI 里真跑了 `--print` / `--render-png` | ⬜ 没有 Linux 桌面 |
| Windows x86_64 | ✅ | ✅ 仅依赖系统 DLL（无 mingw 运行时依赖） | ✅ CI 里真跑了 `--print` / `--render-png` | ⬜ 没有 Windows 桌面 |

「命令行路径」那列是 GitHub Actions 在真机（真 Windows / 真 Linux 容器）上跑的：
`--version` / `--init-config` / `--print` / `--render-png` 都验证过。
**没验证的只有窗口系统那一层**——透明、置顶、拖拽、字体名，这部分交给 `eframe`/`winit`。

「长什么样」这件事本身也是平台无关的：整张浮窗图由 `render` 自己逐像素合成，
`cargo test` 里已经逐像素对过，三个平台跑的是同一段代码。

---

## 图标

![icon](assets/icon-512.png)

近黑圆角方块 + 亮绿倒计时表盘，缺口留在右上角，中间是等宽的 `T−`（主标题的前缀）。
和浮窗本身同一套配色（`#00FF66` / `#101010`）。

图标是脚本画出来的，改完重新生成：

```bash
python3 tools/make_icons.py     # 需要 Pillow；在 macOS 上还会顺手出 .icns
```

产物 `assets/icon.png` / `icon-512.png` / `icon.ico` / `icon.icns` 都直接进仓库，
所以 `build.sh` 和 CI 不需要额外装东西。

---

## 从源码构建

需要 Rust 1.80+（本项目在 1.98 上开发验证）。

```bash
cargo build --release            # 本机平台
cargo test                       # 单元测试
```

### 交叉编译三平台单文件

`build.sh` 把三条链路都串好了：

```bash
./build.sh            # 默认出 macOS(通用 + .app) + Linux x86_64 + Windows x86_64
./build.sh macos      # 只出某一个
./build.sh linux windows
```

产物在 `dist/`：

| 文件 | 目标 | 说明 |
| --- | --- | --- |
| `float-clock-macos-universal` | `aarch64-apple-darwin` + `x86_64-apple-darwin` | `lipo` 合成的通用二进制 |
| `float-clock-macos-universal.zip` | 同上 | 里面是 `FloatClock.app`，带图标、可执行位不丢 |
| `float-clock-linux-x86_64[.tar.gz]` | `x86_64-unknown-linux-gnu` | 需要 `zig` + `cargo-zigbuild` |
| `float-clock-windows-x86_64.exe` | `x86_64-pc-windows-gnu` | 需要 `mingw-w64`；图标由 `build.rs` 写进去 |
| `SHA256SUMS` | | 上面所有文件的校验和 |


准备工作：

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin \
                  x86_64-unknown-linux-gnu x86_64-pc-windows-gnu

# Linux：用 zig 当链接器，不用开虚拟机 / 容器
brew install zig && cargo install cargo-zigbuild

# Windows：mingw-w64
brew install mingw-w64
```

Windows 也可以用 MSVC 工具链（`cargo install cargo-xwin` + `--target x86_64-pc-windows-msvc`），
但那样要下整个 MSVC SDK，mingw 这条轻得多。

---

## 代码结构

分层的目的是**让画面本身能脱离窗口系统单独验证**：

| 模块 | 职责 | 能否纯单元测试 |
| --- | --- | --- |
| `timefmt` | 时间解析、T± 格式化 | ✅ |
| `config` | TOML 读取 / 保留注释的回写 | ✅ |
| `notifier` | 提醒排程、去重、不补发 | ✅ |
| `notify` | 三平台系统通知（走系统自带命令） | ✅（编码部分） |
| `pixmap` | RGBA 画布、覆盖混合、**擦除**（镂空靠它） | ✅ 逐像素 |
| `text` | 字体查找、逐字排版、`ab_glyph` 栅格化 | ✅ 逐像素 |
| `render` | 把三行合成一张 RGBA 图 | ✅ 逐像素 |
| `app` | 窗口、输入、通知调度 | 需要真实窗口 |
| `macos` | 窗口体检 / 阴影与 Dock 调整 | macOS 专用 |
| `build.rs` | 给 Windows 的 exe 写图标和版本信息 | 构建期 |

目录里还有几个不参与编译的东西：

| 路径 | 作用 |
| --- | --- |
| `assets/` | 图标（`tools/make_icons.py` 生成，直接进仓库） |
| `tools/make_icons.py` | 画图标 |
| `tools/make_app.sh` | 把 macOS 二进制包成 `.app` |
| `build.sh` | 一条命令出三平台产物 |
| `config.example.toml` | 默认配置的样例 |

`app` 只负责「把已经算好的那张图贴上去 + 收鼠标键盘」，
所以「长什么样」这件事完全由前面几层决定，而它们都在 `cargo test` 里逐像素对过。

```bash
cargo test        # 61 个测试

cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

CI（[`ci.yml`](.github/workflows/ci.yml)）跑的就是上面这三条；
[`release.yml`](.github/workflows/release.yml) 在打 `v*` 标签时把三平台产物编出来挂到 Release 上。

---

## 分支说明

| 分支 | 内容 |
| --- | --- |
| **`main`** | **Rust 版**（主力，跨平台单文件） |
| [`python`](https://github.com/bananaxiao2333/float-clock/tree/python) | 最早用 uv + Tk 写的 Python 版（保留存档） |

两个分支是两套独立的实现，文件不重叠，各自 clone 下来都能单独跑。
Python 版当初是为了验证「浮窗 + T± + 镂空 + 通知」这套交互可行，
Rust 版是为了解决它的两个硬伤：需要装 Python 运行时，以及三平台观感不一致。

行为是对齐的：

* 三行结构、T± 语义、补零规则、模板占位符一致
* 锁定状态的实线/虚线外框线一致
* 通知阈值、去重、不补发启动前的提醒一致
* 配置键名基本一致（`x11_background` → `background`，新增 `max_scale`）

渲染方式不同：Python 版在 macOS 上要靠 Tk + 一个 Objective-C 叠层才能做出镂空效果，
而且被 Tk 9.0 的透明回归坑过；Rust 版直接把整张图自己栅格化，
三个平台走同一段代码，所以三边长得一模一样。

