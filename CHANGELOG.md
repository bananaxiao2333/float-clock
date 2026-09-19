# 更新日志

## v0.2.1 — 2026-09-19

修掉「下载下来双击打不开」这件事，并把图标补上。

### 修复

- **macOS 下载后双击打不开**。原来 Release 里只有裸二进制，浏览器下载会抹掉可执行位，
  Finder 于是把它当文本文件丢给「文本编辑」，弹出
  「无法打开文件，文字编码 Unicode (UTF-8) 不适用」。
  现在多给一个 `float-clock-macos-universal.zip`，里面是带图标的 `FloatClock.app`，
  解压双击即可（裸二进制仍然保留，给脚本用，README 里写了 `chmod +x`）。
- **双击打开时配置没地方存**。`.app` 的工作目录是 `/`，相对路径的 `config.toml` 既找不到
  也写不进去。现在按 `--config` → `FLOAT_CLOCK_CONFIG` → 当前目录 → 用户配置目录
  （macOS `~/Library/Application Support/FloatClock/`、Windows `%APPDATA%\FloatClock\`、
  Linux `~/.config/float-clock/`）的顺序找，找不到就自动生成一份。
- **Linux 下载后同样丢可执行位**。多给一个 `float-clock-linux-x86_64.tar.gz`。

### 新增

- **图标**：近黑圆角方块 + 亮绿倒计时表盘 + 等宽 `T−`，配色和浮窗本身一致。
  由 `tools/make_icons.py` 生成（PNG / ICO / ICNS 都进仓库）。
- macOS 打包成 `.app`（`tools/make_app.sh`），带 `Info.plist`、图标，并做 ad-hoc 签名。
- Windows 的 exe 里写进图标和版本信息（`build.rs` + `winresource`）。
- CI 的冒烟测试补上 Linux / Windows 上真跑 `--print` 和 `--render-png`。
- `--print` 打印「接下来的提醒」时按时间点分组，每个时刻各显示最靠前的几条，
  不会被其中一个时刻占满。

## v0.2.0 — 2026-09-19

Rust 重写，第一个能用的版本。

- 一个代码库交叉编译出 macOS / Linux / Windows 单文件可执行程序
- 三行结构：主标题 = 距偏移时刻剩余，副标题 = 距目标时间点已过，第三行 = 绿底镂空
- T± 语义、补零等宽、超过一天 `DD:HH:MM:SS`
- 左键拖动 / 右键锁定 / 双击设置，锁定状态用实线·虚线外框区分
- 临近两个时间点的系统通知：阈值、去重、不补发启动前的提醒
- 配置热重载 + 注释保留回写
- 59 个单元测试
