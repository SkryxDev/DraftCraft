//! Windows only: embed the app icon and version info into `rustdraft.exe`. Elsewhere a no-op.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/app-icon/rustdraft.ico");
    println!("cargo:rerun-if-env-changed=RUSTDRAFT_REQUIRE_WINRES");
    println!("cargo:rerun-if-env-changed=RUSTDRAFT_BUILD_SHA");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    if std::path::Path::new("../../assets/app-icon/rustdraft.ico").exists() {
        res.set_icon("../../assets/app-icon/rustdraft.ico");
    }
    res.set("ProductName", "DraftCraft")
        .set("FileDescription", "DraftCraft computer-aided design")
        .set("CompanyName", "Skryx Dev")
        .set(
            "LegalCopyright",
            "Copyright (c) 2026 Skryx Dev; based on CADCraft, (c) 2026 ArtCraft Team and the CADCraft contributors. MIT OR Apache-2.0.",
        )
        .set("OriginalFilename", "rustdraft.exe")
        .set("InternalName", "rustdraft");
    if let Err(e) = res.compile() {
        if std::env::var_os("RUSTDRAFT_REQUIRE_WINRES").is_some() {
            eprintln!("embedding Windows resources failed: {e}");
            std::process::exit(1);
        }
        println!("cargo:warning=rustdraft.exe built without icon/version resources: {e}");
    }
}
