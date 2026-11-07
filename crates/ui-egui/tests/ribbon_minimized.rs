//! A minimized ribbon opens again from any tab, not only from the tab that minimized it: a click
//! on another tab shows that tab's panels.

use egui::accesskit::Role;
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use rustdraft_engine::Session;
use rustdraft_ui_egui::{CadApp, Services};

fn harness() -> Harness<'static, CadApp> {
    let mut app = CadApp::new(Session::default(), Services::default());
    app.run("new", serde_json::json!({})).unwrap();
    app.ui.start_tab = false;
    let mut h = Harness::builder().with_size(vec2(1400.0, 800.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    h
}

fn click_tab(h: &mut Harness<'static, CadApp>, name: &str) {
    h.get_by_role_and_label(Role::Tab, name).click();
    h.run_steps(3);
}

#[test]
fn a_minimized_ribbon_opens_from_any_tab() {
    let mut h = harness();
    assert_eq!((h.state().ui.ribbon_tab.as_str(), h.state().ui.ribbon_minimized), ("Home", false));
    // A click on the open tab minimizes the ribbon.
    click_tab(&mut h, "Home");
    assert!(h.state().ui.ribbon_minimized);
    // Another tab opens it again, on that tab.
    click_tab(&mut h, "Annotate");
    assert_eq!((h.state().ui.ribbon_tab.as_str(), h.state().ui.ribbon_minimized), ("Annotate", false));
    // Minimized from Annotate, it opens again from Home too.
    click_tab(&mut h, "Annotate");
    assert!(h.state().ui.ribbon_minimized);
    click_tab(&mut h, "Home");
    assert_eq!((h.state().ui.ribbon_tab.as_str(), h.state().ui.ribbon_minimized), ("Home", false));
    // While it is open, another tab only switches.
    click_tab(&mut h, "View");
    assert_eq!((h.state().ui.ribbon_tab.as_str(), h.state().ui.ribbon_minimized), ("View", false));
}
