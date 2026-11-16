//! Embeds the Corvus icon and version block into CorvusSetup.exe.
//! The icon comes from `branding/generated/karasu.ico`, produced by `cargo run -p make-icons`.
//! If it isn't there the build still succeeds (with a warning) and the exe gets no icon.

use std::path::Path;

fn main() {
    let ico = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../branding/generated/karasu.ico");
    println!("cargo:rerun-if-changed={}", ico.display());
    println!("cargo:rerun-if-changed=build.rs");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    match ico.canonicalize() {
        Ok(p) if p.is_file() => {
            res.set_icon(p.to_string_lossy().trim_start_matches("\\\\?\\"));
        }
        _ => println!(
            "cargo:warning=no icon at {}; run `cargo run -p make-icons` first",
            ico.display()
        ),
    }
    res.set("ProductName", "Corvus Setup")
        .set(
            "FileDescription",
            "Corvus Setup - builds and updates a Corvus drive",
        )
        .set("CompanyName", "Aiden")
        .set("LegalCopyright", "MIT licence");
    if let Err(e) = res.compile() {
        println!("cargo:warning=windows resource compile failed: {e}");
    }
}
