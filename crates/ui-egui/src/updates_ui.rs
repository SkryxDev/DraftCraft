//! The update check (desktop only; the web build is always the latest). Once at startup, and again
//! from Help ▸ Check for Updates…, a background thread asks GitHub for the newest release of
//! RustDraft (`SkryxDev/RustDraft`). When it is newer than this build, a window offers it:
//! Download opens the release page, Later closes the window until the next start, and Skip This
//! Version stops the startup check offering that version. Nothing is downloaded or installed here.
//! The asking and comparing are `craft_update::check_for_update`; `RUSTDRAFT_PRETEND_VERSION`
//! makes this build count as another version (to try the prompt).

use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use egui::{RichText, vec2};

use crate::CadApp;
use crate::theme::Tokens;

/// The version this build reports to the update check.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The file in the settings directory naming the version Skip This Version skipped.
const SKIP_FILE: &str = "update-skipped-version.txt";

/// Release notes shown in the window are cut to this many characters.
const NOTES_MAX: usize = 1500;

/// A newer release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    /// `1.1.0`.
    pub version: String,
    /// Its GitHub release page.
    pub url: String,
    /// Its release notes (Markdown, capped).
    pub notes: String,
}

/// Where a check gets the newest release (tests stand in for GitHub with one).
pub type Source = Arc<dyn Fn() -> Result<Option<Offer>, String> + Send + Sync>;

/// What the update check is doing between frames.
#[derive(Default)]
pub struct Runtime {
    /// The host asked for the startup check (see [`CadApp::enable_update_check`]).
    enabled: bool,
    /// The startup check has started.
    started: bool,
    /// Help ▸ Check for Updates… was chosen; the next frame starts it.
    check_requested: bool,
    /// The running check was asked for: its "up to date" or failure is reported too.
    requested: bool,
    check: Option<Receiver<Result<Option<Offer>, String>>>,
    /// The release being offered (the window is open).
    pub offer: Option<Offer>,
    /// The answer to a requested check that found nothing to offer (the window is open).
    pub message: Option<String>,
    /// The version Skip This Version skipped.
    skipped: Option<String>,
    source: Option<Source>,
}

impl CadApp {
    /// Turn on the startup update check (the desktop host does; tests and the web build do not).
    pub fn enable_update_check(&mut self) {
        self.updates_rt.enabled = true;
        self.updates_rt.skipped = read_skipped();
    }

    /// Check with `source` instead of GitHub (tests).
    pub fn set_update_source(&mut self, source: Source) {
        self.updates_rt.source = Some(source);
    }
}

/// Help ▸ Check for Updates…: check now and say what was found, even when nothing is newer.
pub fn check_now(app: &mut CadApp) {
    app.updates_rt.check_requested = true;
}

/// GitHub's newest release of RustDraft, when it is newer than this build.
fn from_github() -> Result<Option<Offer>, String> {
    let http = craft_update::Http::new(&format!("RustDraft/{VERSION}"))?;
    let release = craft_update::check_for_update(&crate::UPDATE_APP, &http, VERSION)?;
    Ok(release.map(|r| Offer { version: r.version_label(), url: r.html_url, notes: r.notes.chars().take(NOTES_MAX).collect() }))
}

/// The version this build counts as: [`VERSION`], or `RUSTDRAFT_PRETEND_VERSION` when it is set
/// (the version the check compares with).
fn running_version() -> String {
    craft_update::self_update::current_version(&crate::UPDATE_APP, VERSION).map_or_else(|| VERSION.to_string(), |v| v.to_string())
}

fn skip_path() -> Option<std::path::PathBuf> {
    rustdraft_engine::settings_dir::current().map(|d| d.join(SKIP_FILE))
}

fn read_skipped() -> Option<String> {
    let text = std::fs::read_to_string(skip_path()?).ok()?;
    Some(text.trim().to_string()).filter(|v| !v.is_empty() && v.len() <= 64)
}

fn write_skipped(version: &str) {
    let Some(path) = skip_path() else { return };
    if let Err(e) = std::fs::write(&path, version) {
        log::warn!("could not save the skipped update version to {}: {e}", path.display());
    }
}

/// Start a check on its own thread, unless one is running.
fn start(app: &mut CadApp, ctx: &egui::Context, requested: bool) {
    let rt = &mut app.updates_rt;
    rt.requested |= requested;
    if rt.check.is_some() {
        return;
    }
    let source = rt.source.clone().unwrap_or_else(|| Arc::new(from_github));
    let (tx, rx) = channel();
    let ctx = ctx.clone();
    let spawned = std::thread::Builder::new().name("update-check".into()).spawn(move || {
        let _ = tx.send(source());
        ctx.request_repaint();
    });
    match spawned {
        Ok(_) => rt.check = Some(rx),
        Err(e) => finish(app, Err(format!("the check could not start: {e}"))),
    }
}

/// Act on a check's answer.
fn finish(app: &mut CadApp, answer: Result<Option<Offer>, String>) {
    let rt = &mut app.updates_rt;
    let requested = std::mem::take(&mut rt.requested);
    let message = match answer {
        // The startup check doesn't offer a version the user skipped; a requested one does.
        Ok(Some(offer)) if requested || rt.skipped.as_deref() != Some(offer.version.as_str()) => {
            log::info!("DraftCraft {} is available ({})", offer.version, offer.url);
            rt.message = None;
            rt.offer = Some(offer);
            return;
        }
        Ok(_) => crate::tf!("DraftCraft is up to date (version {version}).", version = running_version()),
        Err(e) => {
            log::info!("update check: {e}");
            format!("{} {e}", crate::tl!("Could not check for updates."))
        }
    };
    if requested {
        app.session.echo(message.clone());
        rt.message = Some(message);
    }
}

/// Start the checks and collect their answers.
fn tick(app: &mut CadApp, ctx: &egui::Context) {
    if std::mem::take(&mut app.updates_rt.check_requested) {
        start(app, ctx, true);
    } else if app.updates_rt.enabled && !std::mem::replace(&mut app.updates_rt.started, true) {
        start(app, ctx, false);
    }
    let Some(rx) = &app.updates_rt.check else { return };
    let answer = match rx.try_recv() {
        Ok(answer) => answer,
        Err(TryRecvError::Empty) => return,
        Err(TryRecvError::Disconnected) => Err("the check stopped".to_string()),
    };
    app.updates_rt.check = None;
    finish(app, answer);
}

/// What the update window's buttons asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choice {
    None,
    Download,
    Later,
    Skip,
}

/// Run the checks, and draw the update window while there is something to say.
pub fn show(app: &mut CadApp, ctx: &egui::Context) {
    tick(app, ctx);
    if let Some(offer) = app.updates_rt.offer.clone() {
        match offer_window(ctx, &offer) {
            Choice::Download => {
                ctx.open_url(egui::OpenUrl::new_tab(&offer.url));
                app.updates_rt.offer = None;
            }
            Choice::Later => app.updates_rt.offer = None,
            Choice::Skip => {
                write_skipped(&offer.version);
                app.updates_rt.skipped = Some(offer.version);
                app.updates_rt.offer = None;
            }
            Choice::None => {}
        }
    } else if let Some(message) = app.updates_rt.message.clone() {
        let mut close = false;
        egui::Window::new(crate::tl!("Check for Updates"))
            .id(egui::Id::unique("rd_update_result"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, vec2(0.0, 120.0))
            .show(ctx, |ui| {
                ui.set_width(360.0);
                ui.label(&message);
                ui.add_space(6.0);
                ui.allocate_ui_with_layout(vec2(ui.available_width(), 0.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close = ui.button(crate::tl!("OK")).clicked();
                });
            });
        if close {
            app.updates_rt.message = None;
        }
    }
}

fn offer_window(ctx: &egui::Context, offer: &Offer) -> Choice {
    let t = Tokens::get();
    let mut choice = Choice::None;
    egui::Window::new(crate::tl!("Update Available"))
        .id(egui::Id::unique("rd_update_offer"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_TOP, vec2(0.0, 120.0))
        .show(ctx, |ui| {
            ui.set_width(420.0);
            ui.label(RichText::new(crate::tf!("DraftCraft {version} is available.", version = offer.version)).strong().size(15.0));
            ui.label(RichText::new(crate::tf!("You have version {version}.", version = running_version())).color(t.text_dim));
            ui.add_space(4.0);
            ui.label(crate::tl!("Do you want to download the update?"));
            if !offer.notes.trim().is_empty() {
                ui.add_space(4.0);
                ui.collapsing(crate::tl!("What's new"), |ui| {
                    egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                        ui.label(RichText::new(offer.notes.trim()).color(t.text_dim));
                    });
                });
            }
            ui.add_space(6.0);
            ui.allocate_ui_with_layout(vec2(ui.available_width(), 0.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(crate::tl!("Download")).clicked() {
                    choice = Choice::Download;
                }
                if ui.button(crate::tl!("Later")).clicked() {
                    choice = Choice::Later;
                }
                if ui.button(crate::tl!("Skip This Version")).clicked() {
                    choice = Choice::Skip;
                }
            });
        });
    choice
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer(v: &str) -> Offer {
        Offer { version: v.into(), url: format!("https://github.com/SkryxDev/RustDraft/releases/tag/v{v}"), notes: "Conic arcs".into() }
    }

    fn app_with(answer: Result<Option<Offer>, String>) -> CadApp {
        let mut app = CadApp::new(rustdraft_engine::Session::new(), crate::Services::default());
        app.set_update_source(Arc::new(move || answer.clone()));
        app
    }

    /// Runs frames until the check has answered.
    fn settle(app: &mut CadApp, ctx: &egui::Context) {
        for _ in 0..200 {
            tick(app, ctx);
            if app.updates_rt.check.is_none() && !app.updates_rt.check_requested {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the check never answered");
    }

    #[test]
    fn a_newer_release_is_offered_at_startup() {
        let ctx = egui::Context::default();
        let mut app = app_with(Ok(Some(offer("9.1.0"))));
        app.updates_rt.enabled = true;
        settle(&mut app, &ctx);
        assert_eq!(app.updates_rt.offer, Some(offer("9.1.0")));
        assert_eq!(app.updates_rt.message, None);
        // Once per start.
        app.updates_rt.offer = None;
        settle(&mut app, &ctx);
        assert_eq!(app.updates_rt.offer, None);
    }

    #[test]
    fn nothing_newer_is_silent_at_startup_and_said_on_request() {
        let ctx = egui::Context::default();
        let mut app = app_with(Ok(None));
        app.updates_rt.enabled = true;
        settle(&mut app, &ctx);
        assert_eq!((app.updates_rt.offer.clone(), app.updates_rt.message.clone()), (None, None));
        check_now(&mut app);
        settle(&mut app, &ctx);
        assert!(app.updates_rt.message.as_deref().is_some_and(|m| m.contains(VERSION)), "{:?}", app.updates_rt.message);

        // A failure is silent at startup and reported on request.
        let mut app = app_with(Err("offline".into()));
        app.updates_rt.enabled = true;
        settle(&mut app, &ctx);
        assert_eq!(app.updates_rt.message, None);
        check_now(&mut app);
        settle(&mut app, &ctx);
        assert!(app.updates_rt.message.as_deref().is_some_and(|m| m.ends_with("offline")));
    }

    #[test]
    fn a_skipped_version_is_only_offered_on_request() {
        let ctx = egui::Context::default();
        let mut app = app_with(Ok(Some(offer("9.1.0"))));
        app.updates_rt.enabled = true;
        app.updates_rt.skipped = Some("9.1.0".into());
        settle(&mut app, &ctx);
        assert_eq!(app.updates_rt.offer, None);
        check_now(&mut app);
        settle(&mut app, &ctx);
        assert_eq!(app.updates_rt.offer, Some(offer("9.1.0")));
    }

    #[test]
    fn no_check_unless_enabled_or_asked() {
        let ctx = egui::Context::default();
        let mut app = app_with(Ok(Some(offer("9.1.0"))));
        for _ in 0..5 {
            tick(&mut app, &ctx);
        }
        assert!(app.updates_rt.check.is_none() && app.updates_rt.offer.is_none());
    }
}
