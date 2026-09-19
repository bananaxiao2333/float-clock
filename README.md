# FloatClock（Python 版）

> **这是 `python` 分支** —— 最早用 uv + Tk 写的实现，现在作为存档保留。
> 主力版本是 [`main` 分支的 Rust 版](https://github.com/bananaxiao2333/float-clock)：
> 一个代码库交叉编译出三平台单文件，不用装 Python 运行时，三平台观感完全一致。

[![Python CI](https://github.com/bananaxiao2333/float-clock/actions/workflows/python.yml/badge.svg?branch=python)](https://github.com/bananaxiao2333/float-clock/actions/workflows/python.yml)
[![Python](https://img.shields.io/badge/Python-3.11%2B-3776AB?logo=python&logoColor=white)](https://www.python.org/)
[![uv](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/astral-sh/uv/main/assets/badge/v0.json)](https://github.com/astral-sh/uv)
[![Tk](https://img.shields.io/badge/Tk-8.6%20%E5%BF%85%E9%9C%80-orange)](#%E5%B8%B8%E8%A7%81%E9%97%AE%E9%A2%98tk-90-%E7%9A%84%E9%80%8F%E6%98%8E%E6%9C%89-bug)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![平台](https://img.shields.io/badge/%E5%B9%B3%E5%8F%B0-macOS%20%7C%20Linux%20%7C%20Windows-2ea44f)](#%E8%B7%A8%E5%B9%B3%E5%8F%B0%E8%AF%B4%E6%98%8E)
[![测试](https://img.shields.io/badge/test-47%20passed-success)](#%E6%B5%8B%E8%AF%95)

一个**只有文字、没有背景**的悬浮 T± 倒计时浮窗：

- 绿色粗体、等宽字体（Menlo / Consolas / DejaVu Sans Mono，自动挑选）
- 所有数字**补零对齐**（`00:05:03`，超过一天为 `01:02:03:04`），秒数跳动时宽度不抖
- 左键**拖动**，右键**锁定 / 解锁**
- 主标题 = 距离「偏移时刻」还有多久，副标题 = 距离「目标时间点」过去了多久
- 第三行 = 目标时间点 + 偏移，做成**实心绿底 + 字镂空**（笔画里透出桌面）
- 临近各个时间点自动弹**系统通知**
- 用 uv 管理；依赖只有 Pillow（只为第三行的镂空位图），其余全是标准库

```
T-00:04:32      ← 主标题：距离 目标时间点+偏移 还有 4 分 32 秒
T+00:00:28      ← 副标题：距离 目标时间点 已经过去 28 秒
▛20:29:00 · +02:00:00▟  ← 第三行：绿底镂空（字里透出桌面）
```

## 测试

```bash
uv run python -m unittest discover -s tests -v      # 47 个测试
```

其中 `tests/test_overlay_smoke.py` 会真的开一个 Tk 窗口，**需要真实显示器**
（CI 上只跑纯逻辑的 `test_float_clock.py`）。

---

## 跨平台说明

核心功能（浮窗、拖动、锁定、T± 计时、系统通知）在三个平台上都是同一套代码，
没有任何平台特有的依赖：

| 能力 | macOS | Windows | Linux |
| --- | --- | --- | --- |
| 无边框置顶浮窗 | ✅ `overrideredirect` | ✅ | ✅ |
| **无背景（只有文字）** | ✅ `-transparent` + `systemTransparent`，**需 Tk 8.6** | ✅ `-transparentcolor` 抠色 | ⚠️ X11 不支持真透明，退化为 `x11_background` 底色 |
| 拖动 / 右键锁定 / 双击设置 | ✅ | ✅ | ✅ |
| T± 计时、补零等宽、主副标题 | ✅ | ✅ | ✅ |
| 系统通知 | ✅ `osascript`（装了 terminal-notifier 就用它） | ✅ PowerShell WinRT Toast | ✅ `notify-send` |
| 第三行「绿底镂空」 | ✅ AppKit 叠层 | 自动退回普通绿字 | 自动退回普通绿字 |

**只有两处是平台相关的**，而且都做了优雅降级、不会让程序跑不起来：

1. **透明背景的画法**（`overlay._configure_window`）——三个分支各写各的，Linux 用底色兜底。
2. **第三行的镂空**（`knockout.py` + `macos_overlay.py`）——镂空位图需要往窗口上贴一张带
   alpha 的图，而 Tk 在透明窗口上画不出图片（见下面「实现说明」）。macOS 上走 AppKit 子视图，
   其它平台 `info_style` 自动按 `text` 处理，就是普通绿字。
   不想要这块也可以直接把 `info_style = "text"` 写死。

通知图标方面：**三个后端都不提供自定义图标的接口**，图标由系统按「发送通知的程序」决定
（macOS 上就是脚本编辑器）。这是系统限制，不做平台特有的绕行。

---

## 怎么设置

三种方式，改完立刻生效，**不用重启**：

1. **双击浮窗** → 弹出设置窗口，改目标时间点 / 偏移 / 颜色 / 字号，点「应用」
2. **直接编辑 `config.toml`** → 保存即可，程序每秒检查一次文件改动，约 1 秒内生效（注释不会丢）
3. **命令行临时覆盖** → `uv run float-clock --target "2026-01-01 09:30" --offset -00:05:00`

操作方式：**拖动 = 左键，锁定/解锁 = 右键，设置 = 双击，菜单 = 中键**。
浮窗上只有三行字，不会显示任何操作提示；出错走系统通知 + 终端 stderr。

还可以随时不开窗口检查当前状态：

```bash
uv run float-clock --print
```

---

## 快速开始

```bash
cd float-clock

uv run float-clock --init-config        # 生成默认 config.toml（目标时间点 = 下一个整点）
uv run float-clock                      # 启动浮窗
```

不想先写配置也行，首次运行会自动生成 `config.toml`。

临时指定时间点：

```bash
uv run float-clock --target "2026-01-01 09:30" --offset -00:05:00
```

不开窗口，只检查当前状态和接下来的提醒计划：

```bash
uv run float-clock --print
```

> **环境要求**：Python ≥ 3.11。
> `.python-version` 钉在 **3.12.7** 是**为了 macOS**——那是 uv 最后一个自带 Tk 8.6 的构建，
> 而 Tk 9.0 在 macOS 上会把透明背景画成黑底（原因见「常见问题：背景还是黑的」）。
> Windows / Linux 不受这个限制，想用别的解释器直接 `uv run --python 3.13 float-clock` 即可，
> 仓库里的这份钉版对它们只是保守默认值。

---

## T± 语义（重点）

全程序统一用火箭倒计时那套写法：

| 显示 | 含义 |
| --- | --- |
| `T-00:05:00` | 距离该时刻**还有** 5 分钟 |
| `T-00:00:00` | 正点那一秒 |
| `T+00:00:12` | 该时刻**已经过去** 12 秒 |

程序里有两个时刻：

```
目标时间点  T  = config.toml 里的 [time] target
偏移时刻    M  = T + offset
```

- **主标题**：倒计时到 `M`（「距离偏移还有多久」）
- **副标题**：以 `T` 为基准（「距离时间点过去了多久」）

例：`target = "09:30:00"`、`offset = "-00:05:00"` → `M = 09:25:00`。
09:20 时主标题是 `T-00:05:00`（距离 09:25 还有 5 分钟），副标题是 `T-00:10:00`（距离 09:30 还有 10 分钟）；
09:26 时主标题变成 `T+00:01:00`（09:25 已过去 1 分钟），副标题是 `T-00:04:00`。

`offset` 为 `0` 时主副标题都指向同一个时刻；`offset` 为正是 `T` 之后，为负是 `T` 之前。

---

## 实战示例：20:29 泄露，紧急处置窗口 120 分钟

把「事件发生时刻」放进 `target`，把「窗口期」放进 `offset`，两个数就出来了：

```toml
[time]
# 泄露发生时刻（当天 20:29）
target = "2026-09-19T20:29:00"
# 紧急处置窗口 120 分钟 → 截止时刻 = 20:29 + 120min = 22:29
offset = "+120m"
```

于是：

- **主标题** `= 距离截止 22:29 还有多久` ← 盯这个决定还有多少时间
- **副标题** `= 泄露已经过去多久` ← 汇报口径

21:45:38 时它长这样：

```
T-00:43:22            ← 距离 22:29 处置截止还有 43 分 22 秒
T+01:16:38            ← 泄露已过去 1 小时 16 分 38 秒
20:29:00 · +02:00:00  ← 泄露时刻 · 处置窗口 120 分钟
```

整条时间线上主/副标题的变化：

| 时刻 | 主标题 | 副标题 | 含义 |
| --- | --- | --- | --- |
| 19:29 | `T-03:00:00` | `T-01:00:00` | 距离截止 3 小时，距离泄露还有 1 小时 |
| 20:29 | `T-02:00:00` | `T-00:00:00` | **泄露发生** |
| 21:29 | `T-01:00:00` | `T+01:00:00` | 窗口过半 |
| 22:24 | `T-00:05:00` | `T+01:55:00` | 距离截止 5 分钟 |
| 22:29 | `T-00:00:00` | `T+02:00:00` | **处置窗口到期** |
| 22:35 | `T+00:06:00` | `T+02:06:00` | 已超时 6 分钟 |

窗口内的提醒建议（120 分钟的窗口，`before` 里加上 `7200`/`3600` 两档）：

```toml
[notify]
before = [7200, 3600, 1800, 900, 600, 300, 180, 120, 60, 30, 10, 5, 3, 2, 1]
after  = [1, 5, 30, 60, 300, 1800]
```

这样会在这几个点弹系统通知：**截止前 2 小时 / 1 小时 / 30 分 / 15 分 / 10 分 / 5 分 /
3 分 / 2 分 / 1 分 / 30 秒 / 10 秒 / 5 秒 / 3 / 2 / 1 秒**，正点再弹一次，
超时后 1 秒 / 5 秒 / 30 秒 / 1 分 / 5 分 / 30 分各补一次。
（`7200` 那一档正好落在泄露发生那一刻。）

一行命令先看效果，不用开窗口：

```bash
uv run float-clock --print --target "2026-09-19T20:29:00" --offset +120m
```

第三行也可以按需换写法：

```toml
[display]
info_template = "{time} · {delta}"                  # 20:29:00 · +02:00:00（默认）
# info_template = "{datetime} ({delta_human})"      # 2026-09-19 20:29:00 (+2 小时)
# info_template = "{time} → {mark}"                 # 20:29:00 → 22:29:00
# info_template = ""                                # 不要第三行
# info_style = "text"                               # 第三行改成普通绿字（不镂空）
```

> 只想每天同一时间复用，`target` 可以直接写 `"20:29:00"`——当天已经过了就自动顺延到明天。
> 真实事故请写完整的年月日，避免第二天变成「明天的 20:29」。

---

## 操作方式

| 操作 | 效果 |
| --- | --- |
| 左键拖动 | 移动浮窗（松手后坐标自动写回 `config.toml`，下次从这里开始） |
| **双击** | 打开设置窗口 |
| **右键单击** | **锁定 / 解锁**，看外框线就知道状态：**实线 = 锁定（拖不动）**，**虚线 = 可拖动** |
| 中键 / Ctrl+右键 / ⌘+右键 | 弹出菜单（锁定、设置、重载配置、退出） |
| Ctrl+L | 锁定 / 解锁 |
| Ctrl+, | 打开设置窗口 |
| Ctrl+R | 手动重载配置 |
| Ctrl+Q | 退出 |

锁定状态**不再用文字提示**，改成外框线：实线表示已固定，虚线表示可移动，一眼就能看出来。
不想要外框就把 `[display] lock_indicator` 设成 `"none"`，或者改成锁定虚线、解锁实线
（`solid_when_locked = false`）。

窗口无边框、永久置顶，不占任务栏/程序坞。

> 快捷键需要窗口拿到键盘焦点；无边框窗口在 macOS 上有时拿不到焦点，
> **右键 / 双击 / 中键始终可用**，所以这些操作都不依赖快捷键。

### 设置窗口

右键菜单 →「设置…」可以直接改目标时间点、偏移、颜色、字号，点「应用」后
写回 `config.toml`（**保留原有注释**）并立即生效。

也可以直接编辑 `config.toml`：程序每秒检查一次文件修改时间，**保存即生效，不用重启**。

---

## 配置说明（config.toml）

```toml
[window]
x = 80                 # 浮窗左上角坐标（拖动后自动更新）
y = 80
borderless = true      # 无边框，只留文字
topmost = true         # 置顶
locked = false         # 右键切换，会写回这里
opacity = 1.0          # 整体不透明度

[display]
font_family = ""       # 留空 = 自动挑系统等宽字体；也可写 "Menlo"
main_size = 46         # 主标题字号
sub_size = 18          # 副标题字号
color = "#00FF66"      # 绿色
sub_color = ""         # 副标题颜色，留空跟主标题一致
bold = true            # 粗体
show_days = true       # 超过一天显示 DD:HH:MM:SS
gap = 2
x11_background = "#101010"   # Linux 等不支持透明背景时的底色
interval_ms = 200      # 刷新间隔
lock_indicator = "border"    # 外框线指示锁定状态；"none" 关闭
border_color = ""            # 外框线颜色，留空 = 跟文字同色
border_width = 2             # 外框线宽
solid_when_locked = true     # true: 锁定=实线/解锁=虚线；false 反过来
info_template = "{time} · {delta}"   # 第三行内容，设成 "" 则整行不显示
info_size = 14               # 第三行字号
info_color = ""              # 第三行颜色（镂空模式下是"底色"），留空 = 绿色
info_style = "auto"          # auto = 能镂空就镂空；knockout / text 强制指定
main_template = "T{sign}{clock}"
sub_template = "T{sign}{clock}"

[time]
target = "2026-01-01T09:30:00"
offset = "-00:05:00"

[notify]
enabled = true
before = [3600, 1800, 900, 600, 300, 180, 120, 60, 30, 10, 5, 3, 2, 1]
after = [1, 5, 30, 60, 300]
at_moment = true
sound = true
sound_name = "Glass"
```

### 时间点写法

`target` 支持：

- `2026-01-01T09:30:00`、`2026-01-01 09:30`、`2026-01-01`（ISO / 常见格式）
- `2026/02/03 08:05`、`02-03 08:05`
- `09:30` / `09:30:00` —— 今天该时刻，**已经过了就顺延到明天**（适合每天固定的日程）
- `+1h30m` —— 相对现在

`offset` 支持：

- `-00:05:00`、`5:00`、`+00:00:30`、`01:02:03:04`（天:时:分:秒）
- `1h30m`、`90m`、`2d`、`90s`
- `-300` / `300` —— 纯数字按**秒**算

---

## 系统通知

对**两个时间点**（目标时间点 `T`、偏移时刻 `M`）分别排提醒：

- `before` 里的每个秒数：提前提醒一次，标题形如 `⏳ 目标时间点 T-00:05:00`，正文写「距离目标时间点还有 5 分」
- `at_moment = true`：正点提醒 `🔔 目标时间点 已到`
- `after` 里的每个秒数：过后提醒一次，标题形如 `✅ 目标时间点 T+00:00:05`

规则细节：

- **不会重复弹**：同一条提醒只发一次（即使你拖动窗口导致配置重载）。
- **启动时不补发历史**：程序启动时已经过去的提醒不会补弹，只安排往后的。
- 修改 `before` / `after` 会重新排提醒队列。
- 发送走系统命令，不阻塞界面（后台线程）：
  - macOS：优先 `terminal-notifier`，否则 `osascript -e 'display notification …'`
  - Windows：PowerShell 的 WinRT Toast
  - Linux：`notify-send`

**macOS 第一次不弹通知？** 去「系统设置 → 通知」把 **脚本编辑器 / Script Editor**
（或终端、terminal-notifier）的通知权限打开，并关掉「专注模式」。用 osascript
发通知时，系统把发送者认成 Script Editor。

---

## 实现说明

- 透明背景：macOS 用 `wm attributes -transparent` + `systemTransparent` 颜色
  （**必须 Tk 8.6**，Tk 9.0 有回归），Windows 用 `-transparentcolor` 抠色，
  Linux 退化为深色底（可配 `x11_background`）。这三个分支互不影响，其它平台只是少个特效。
- **窗口样式必须在映射前设置**：`withdraw()` → 样式 → 控件 → `geometry()` → `deiconify()`，
  否则 macOS 会重新套上标题栏、透明失效。`--diagnose` 可验证。
- 锁定指示用画布上的外框线（实线 / 虚线），不需要任何文字提示。
- 第三行是内容而不是提示：用 `info_template` 组合，占位符有
  `{date} {time} {datetime}`、`{mark} {mark_datetime}`、`{delta} {delta_human}`。
- **第三行的「绿底镂空」为什么绕了一圈**：Tk 8.6 在 `-transparent` 窗口上**不绘制任何图片**
  （`tk.Label(image=...)` 和 `canvas.create_image(...)` 实测都是整片透明，同一张带 alpha 的 PNG
  在不透明窗口里完全正常），而且 `fg=systemTransparent` 也挖不出洞（实测像素数与 `fg=黑色`
  逐点相同，等于空操作）。所以这一行的位图由 Pillow 生成（`knockout.py`），再通过一个
  AppKit 子视图（`macos_overlay.py`，纯 ctypes 调 ObjC 运行时，不引入 PyObjC）盖在窗口上。
  子视图重写了 `hitTest:` 返回 nil，鼠标事件照常穿透给 Tk，不影响拖动。
- 补零等宽：`format_hms()` 只产出 `HH:MM:SS` / `DD:HH:MM:SS`，配合等宽字体保证逐秒跳动不位移。
- 剩余时间向上取整、已过时间向下取整，所以正点那一秒恰好显示 `T-00:00:00`。
- 配置回写用「按行定位 `节.键`」的方式，注释和顺序都不会丢。

### 项目结构

```
float-clock/
├── pyproject.toml            # uv 项目定义 + 入口脚本 float-clock
├── .python-version           # 3.12.7（uv 托管解释器：自带 Tk 8.6，透明才有效）
├── config.toml               # 运行时生成/编辑
├── src/float_clock/
│   ├── __main__.py           # 命令行入口
│   ├── overlay.py            # 悬浮窗：透明背景、拖动、锁定、渲染
│   ├── tclenv.py             # 修 venv 里 Tcl/Tk 数据目录的搜索路径
│   ├── knockout.py           # 第三行镂空位图（Pillow，跨平台，缺了就退化成绿字）
│   ├── macos_overlay.py      # macOS 专用：把镂空位图贴到窗口上（非 macOS 自动空转）
│   ├── notifier.py           # 临近时间点通知调度（去重）
│   ├── notify.py             # 跨平台系统通知
│   ├── config.py             # TOML 读取 / 保留注释回写 / 默认配置
│   └── timefmt.py            # T± 格式化与时间点解析
└── tests/
    ├── test_float_clock.py       # 纯逻辑
    ├── test_overlay_smoke.py     # 真窗口：无边框 / 透明像素 / 拖动 / 锁定 / 热重载
    └── macos_probe.py            # 读 NSWindow 渲染位图（无需屏幕录制权限）
```

### 测试

```bash
uv run python -m unittest discover -s tests -v
uv run float-clock --print           # 不开窗口，打印 T± 与提醒计划
uv run float-clock --diagnose        # 开窗 1 秒后报告「无边框 / 透明是否真的生效」
uv run float-clock --selftest 5      # 开窗 5 秒自检
```

### 常见问题：窗口带了标题栏

macOS 上 `overrideredirect`（去标题栏）和 `-transparent`（透明背景）**必须在窗口映射之前**
设置好，最后再用 `deiconify()` 一次性显示；中途被系统重新映射的话，标题栏会回来、
`geometry` 也会被忽略。

本项目已经改成：`Tk()` → `withdraw()` → 配置样式 → 建控件 → `geometry()` → `deiconify()`
→ 再确认一次样式，并用「内容原点 − 窗口框架原点 = 标题栏高度」做回归测试
（`test_no_title_bar`）。

### 常见问题：背景还是黑的 + 文字拖影

**这是 Tk 9.0 的 macOS 回归 bug，不是配置问题。**

Tk 9.0 的窗口后备缓冲是不透明的：它把 `systemTransparent` 当成「全透明色」往缓冲里填，
等于什么都没擦，于是留下一整块不透明黑，旧字形也不被清除（这就是拖影）。实测同一段代码：

| Tk 版本 | 背景像素 | 透明像素占比 |
| --- | --- | --- |
| **Tk 9.0** | `RGBA(0,0,0,255)` 不透明黑 | 0% |
| **Tk 8.6** | `RGBA(0,0,0,0)` 全透明 | 87%（只有绿色字形不透明） |

所以项目把 `.python-version` 钉在 **3.12.7**（uv 的最后一个自带 Tk 8.6 的构建）。
检查当前用的是哪一代 Tk：

```bash
uv run python -c "import tkinter; print('Tk', tkinter.TkVersion)"
uv run float-clock --diagnose      # 顺便报告无边框/透明是否生效
```

如果显示 Tk 9，且你确实要留在 Tk 9 上，程序启动时会直接提示「⚠︎ Tk 9 的透明有 bug」。
`tests/test_overlay_smoke.py::test_background_pixels_are_really_transparent`
会用位图读像素的方式守住这个回归（在 Tk 9 上会失败）。

### 常见问题：第三行不是镂空的

- 只有 macOS 能镂空。其它平台（或 Pillow / 字体文件缺失、ObjC 挂载失败时）会自动回退成
  普通绿字，并把原因打到 stderr。**这是设计好的降级，不是故障。**
- 想手动切回普通绿字：`[display] info_style = "text"`。
- 自检：`uv run float-clock --diagnose` 的「第三行」会打印这一行实际内容。

### 常见问题：`Can't find a usable init.tcl`

python-build-standalone 把 `tcl8.6` / `tk8.6` 数据目录放在解释器基础前缀的 `lib/` 下，
从 venv 里启动时 Tcl 找不到。程序在导入时自动设置 `TCL_LIBRARY` / `TK_LIBRARY`
（见 `src/float_clock/tclenv.py`），不用手动配环境变量。
