//! The Parametric category of the Tool Sets offers the constraint tools that only Tools ›
//! Parametric used to reach: Fix, Symmetric, Delete Constraints and the rest (#1134).

use egui::accesskit::Role;
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use rustdraft_engine::Session;
use rustdraft_ui_egui::{CadApp, Services};

fn harness() -> Harness<'static, CadApp> {
    let mut app = CadApp::new(Session::default(), Services::default());
    // The Windows layout keeps the Tool Sets and the docked palettes closed; these tests use them.
    app.ui.show_toolsets = true;
    app.ui.show_palettes = true;
    app.ui.in_window_menu = false;
    app.integrated_titlebar = true;
    // Tall enough that the Parametric category, last in Drafting, is on screen.
    let mut harness = Harness::builder().with_size(vec2(1280.0, 1600.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    );
    harness.run_steps(5);
    harness
}

#[test]
fn the_parametric_category_lists_fix_symmetric_and_delete_constraints() {
    let parametric = rustdraft_ui_egui::palettes::DRAFTING.iter().find(|(name, _, _)| *name == "Parametric").unwrap();
    let ids: Vec<&str> = parametric.1.iter().chain(parametric.2).map(|(_, cmd, _)| *cmd).collect();
    for id in [
        "gcfix",
        "gcsymmetric",
        "gccollinear",
        "gcsmooth",
        "dclinear",
        "dcaligned",
        "dcangular",
        "dcradius",
        "dcdiameter",
        "delconstraint",
        "parameters",
    ] {
        assert!(ids.contains(&id), "{id} is missing from the Parametric tool set: {ids:?}");
    }

    let harness = harness();
    for label in ["Fix", "Symmetric", "Delete Constraints"] {
        let button = harness.get_by_role_and_label(Role::Button, label);
        assert!(!button.accesskit_node().is_disabled(), "{label} is a known command, so its button is enabled");
    }
}

#[test]
fn clicking_fix_in_the_tool_set_starts_gcfix() {
    let mut harness = harness();
    assert!(harness.state().session.running.is_none());
    harness.get_by_role_and_label(Role::Button, "Fix").click();
    harness.run_steps(5);
    assert_eq!(harness.state().session.running.as_ref().map(|r| r.id.as_str()), Some("gcfix"));
}
