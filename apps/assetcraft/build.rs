//! Windows only: embed the app icon and version info (VERSIONINFO) into `assetcraft.exe`, so it
//! shows in Explorer, the taskbar, the Start menu and Alt-Tab. Same approach as FilmCraft
//! (`filmcraft/apps/filmcraft/build.rs`).
//!
//! On every other target this does nothing (`winresource` is only a build-dependency on Windows
//! hosts). A missing resource compiler (`rc.exe` from the Windows SDK, or MinGW `windres`) is a
//! warning, unless `ASSETCRAFT_REQUIRE_WINRES=1` (for release builds) makes it an error.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/app-icon/assetcraft.ico");
    println!("cargo:rerun-if-env-changed=ASSETCRAFT_REQUIRE_WINRES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    windows::embed_resources();
}

#[cfg(windows)]
mod windows {
    pub fn embed_resources() {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/app-icon/assetcraft.ico")
            .set("ProductName", "AssetCraft")
            .set("FileDescription", "AssetCraft asset browser")
            .set("LegalCopyright", "Copyright (c) the AssetCraft contributors. MIT OR Apache-2.0.")
            .set("OriginalFilename", "assetcraft.exe")
            .set("InternalName", "assetcraft");
        if let Err(e) = res.compile() {
            if std::env::var_os("ASSETCRAFT_REQUIRE_WINRES").is_some() {
                // No panic (house rule): report and fail the build.
                eprintln!("error: embedding Windows resources failed: {e}");
                std::process::exit(1);
            }
            println!("cargo:warning=assetcraft.exe built without icon/version resources: {e}");
        }
    }
}

/// Cross-compiling for Windows from macOS or Linux: `winresource` is not available on this host.
#[cfg(not(windows))]
mod windows {
    pub fn embed_resources() {
        println!(
            "cargo:warning=assetcraft.exe built without icon/version resources (cross-compiled from a non-Windows host)"
        );
    }
}
