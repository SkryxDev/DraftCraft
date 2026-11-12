//! DraftCraft Viewer: the read-only phone build of DraftCraft for the web (web branch).
//!
//! Phones open drawings to look at them, not to draft: this binary reads DWG and DXF files, draws
//! them with the full app's GPU renderer (`draftcraft_ui_egui::gpu`) and adds touch navigation,
//! layer visibility, layouts, text search and measuring. It leaves out the command engine, the
//! ribbon, the panels, plotting and AutoLISP, so the phone downloads a fraction of the full app.
//! Build with `trunk build --release viewer.html` from `apps/draftcraft-web`; phones reach it
//! through `vercel.json` (or `phone.rs` in the full app on any other host).
//!
//! URL flags: `?sample` opens the sample drawing, `?webgl` forces WebGL2, `?cpu` draws with egui
//! shapes instead of the GPU canvas.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
// The modules below are the browser app's; natively only their tests build, and use part of them.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

#[cfg(any(target_arch = "wasm32", test))]
mod doc;
#[cfg(any(target_arch = "wasm32", test))]
mod gesture;
#[cfg(any(target_arch = "wasm32", test))]
mod measure;
#[cfg(any(target_arch = "wasm32", test))]
mod text;
#[cfg(any(target_arch = "wasm32", test))]
mod view;

#[cfg(target_arch = "wasm32")]
mod app;
#[cfg(target_arch = "wasm32")]
mod picker;
#[cfg(target_arch = "wasm32")]
mod scene;
#[cfg(target_arch = "wasm32")]
mod sheets;

#[cfg(target_arch = "wasm32")]
fn main() {
    app::start();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("draftcraft-viewer only runs in the browser: build it with `trunk build --release viewer.html` in apps/draftcraft-web");
}
