//! 构建脚本：只做一件事 —— 给 Windows 的 exe 塞图标和版本信息。
//!
//! 判断用的是 cargo 传给构建脚本的 `TARGET` 环境变量，而不是 `#[cfg(windows)]`：
//! 构建脚本本身是给**宿主**编译的，从 macOS 交叉编译到 Windows 时
//! `#[cfg(windows)]` 是 false，图标就悄悄丢掉了。

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");

    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("windows") {
        return;
    }

    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/icon.ico");
    res.set("ProductName", "FloatClock");
    res.set("FileDescription", "FloatClock 悬浮 T± 倒计时");
    res.set("CompanyName", "bananaxiao2333");
    res.set("OriginalFilename", "float-clock.exe");
    res.set("LegalCopyright", "MIT License");
    res.set("FileVersion", &version);
    res.set("ProductVersion", &version);

    // 交叉编译时找不到资源编译器不算致命：图标没有，程序照常能跑
    if let Err(error) = res.compile() {
        println!("cargo:warning=写入 Windows 图标/版本信息失败：{error}");
    }
}
