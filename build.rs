//! Build script. It does exactly one thing: put the icon and the version block
//! into the Windows .exe.
//!
//! The condition is the `TARGET` environment variable cargo passes to build
//! scripts, not `#[cfg(windows)]`. A build script is compiled for the *host*,
//! so when cross-compiling from macOS to Windows `#[cfg(windows)]` is false and
//! the icon would quietly go missing.

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
    res.set(
        "FileDescription",
        "FloatClock - a borderless floating T+/- countdown overlay",
    );
    res.set("CompanyName", "bananaxiao2333");
    res.set("OriginalFilename", "float-clock.exe");
    res.set("LegalCopyright", "MIT License");
    res.set("FileVersion", &version);
    res.set("ProductVersion", &version);

    // A missing resource compiler during a cross-compile is not fatal: you lose
    // the icon, the program still runs.
    if let Err(error) = res.compile() {
        println!("cargo:warning=could not write the Windows icon / version info: {error}");
    }
}
