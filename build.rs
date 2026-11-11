//! Embeds the app icon in hudlight.exe, so Explorer, shortcuts and the Start menu show it.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "HudLight");
        res.set("FileDescription", "HudLight");
        if let Err(e) = res.compile() {
            println!("cargo:warning=couldn't embed the icon: {e}");
        }
    }
}
