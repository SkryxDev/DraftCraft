//! Contextual ribbon tabs: a tab of its own appears after the others, in the accent colour, while
//! its context holds, and is selected when the context begins, as on Windows: Text Editor while
//! MTEXT's editor is open, Block Editor in the Block Editor, and Hatch Editor, Array, Image or
//! PDF Underlay while the selection is only hatches, one associative array, raster images or PDF
//! underlays. When the context ends the ribbon goes back to the tab it was on.
//!
//! The Text Editor tab's buttons are UI commands (`ui.mtext.*`) the editor applies to its
//! selection on the next frame ([`crate::mtext_editor`]); the other tabs start ordinary commands.

use draftcraft_engine::doc::EntityKind;
use serde_json::Value;

use super::Col::{Large, Small};
use super::{RibbonPanel, RibbonTab, b};
use crate::CadApp;
use crate::icons::Icon;

pub(crate) const TEXT_EDITOR: &str = "Text Editor";
pub(crate) const BLOCK_EDITOR: &str = "Block Editor";
pub(crate) const HATCH_EDITOR: &str = "Hatch Editor";
pub(crate) const ARRAY: &str = "Array";
pub(crate) const IMAGE: &str = "Image";
pub(crate) const PDF_UNDERLAY: &str = "PDF Underlay";

/// Close button of the tabs that a selection opens: it clears the selection.
const CLOSE_SELECTION: super::Btn = b(Icon::CloseTab, "ui.context.close", "Close");

pub(crate) const CONTEXT_TABS: &[RibbonTab] = &[
    RibbonTab {
        name: TEXT_EDITOR,
        panels: &[
            RibbonPanel {
                title: "Formatting",
                cols: &[
                    Small(&[b(Icon::Bold, "ui.mtext.bold", "Bold"), b(Icon::Italic, "ui.mtext.italic", "Italic")]),
                    Small(&[b(Icon::Underline, "ui.mtext.underline", "Underline"), b(Icon::Overline, "ui.mtext.overline", "Overline")]),
                ],
            },
            RibbonPanel {
                title: "Paragraph",
                cols: &[
                    Small(&[
                        b(Icon::AlignLeft, "ui.mtext.left", "Left"),
                        b(Icon::AlignCenter, "ui.mtext.center", "Center"),
                        b(Icon::AlignRight, "ui.mtext.right", "Right"),
                    ]),
                    Small(&[
                        b(Icon::AlignJustify, "ui.mtext.justify", "Justify"),
                        b(Icon::Bullets, "ui.mtext.bullets", "Bulleted list"),
                        b(Icon::Numbering, "ui.mtext.numbers", "Numbered list"),
                    ]),
                ],
            },
            RibbonPanel {
                title: "Insert",
                cols: &[
                    Small(&[
                        b(Icon::Symbol, "ui.mtext.degree", "Degree"),
                        b(Icon::Symbol, "ui.mtext.plusminus", "Plus/minus"),
                        b(Icon::DimDiameter, "ui.mtext.diameter", "Diameter"),
                    ]),
                    Small(&[b(Icon::AlignJustify, "ui.mtext.tab", "Insert tab")]),
                ],
            },
            RibbonPanel { title: "Close", cols: &[Large(b(Icon::CloseTab, "ui.mtext.ok", "Close Text Editor"))] },
        ],
    },
    RibbonTab {
        name: BLOCK_EDITOR,
        panels: &[
            RibbonPanel {
                title: "Open/Save",
                cols: &[
                    Large(b(Icon::BlockEdit, "bedit", "Edit Block")),
                    Small(&[b(Icon::Save, "bsave", "Save Block"), b(Icon::SaveAs, "bsaveas", "Save Block As")]),
                ],
            },
            RibbonPanel {
                title: "Authoring",
                cols: &[Large(b(Icon::Parameters, "bparameter", "Parameter")), Large(b(Icon::Stretch, "baction", "Action"))],
            },
            RibbonPanel {
                title: "Geometry",
                cols: &[Small(&[b(Icon::AttDef, "attdef", "Define Attributes"), b(Icon::Constraint, "autoconstrain", "Auto Constrain")])],
            },
            RibbonPanel { title: "Close", cols: &[Large(b(Icon::CloseTab, "bclose", "Close Block Editor"))] },
        ],
    },
    RibbonTab {
        name: HATCH_EDITOR,
        panels: &[
            RibbonPanel { title: "Pattern", cols: &[Large(b(Icon::Hatch, "hatchedit", "Hatch Edit"))] },
            RibbonPanel {
                title: "Properties",
                cols: &[Small(&[b(Icon::Properties, "properties", "Properties"), b(Icon::MatchProp, "matchprop", "Match Properties")])],
            },
            RibbonPanel {
                title: "Options",
                cols: &[Small(&[b(Icon::MoveBottom, "hatchtoback", "Send Hatches to Back"), b(Icon::Boundary, "boundary", "Boundary")])],
            },
            RibbonPanel { title: "Close", cols: &[Large(CLOSE_SELECTION)] },
        ],
    },
    RibbonTab {
        name: ARRAY,
        panels: &[
            RibbonPanel {
                title: "Options",
                cols: &[Large(b(Icon::ArrayRect, "arrayedit", "Edit Array")), Small(&[b(Icon::Explode, "explode", "Explode")])],
            },
            RibbonPanel { title: "Properties", cols: &[Small(&[b(Icon::Properties, "properties", "Properties")])] },
            RibbonPanel { title: "Close", cols: &[Large(CLOSE_SELECTION)] },
        ],
    },
    RibbonTab {
        name: IMAGE,
        panels: &[
            RibbonPanel { title: "Adjust", cols: &[Large(b(Icon::Adjust, "imageadjust", "Adjust"))] },
            RibbonPanel { title: "Clipping", cols: &[Large(b(Icon::ClipFrame, "imageclip", "Create Clipping Boundary"))] },
            RibbonPanel {
                title: "Options",
                cols: &[Small(&[b(Icon::List, "image", "External References"), b(Icon::MoveBottom, "draworder", "Draw Order")])],
            },
            RibbonPanel { title: "Close", cols: &[Large(CLOSE_SELECTION)] },
        ],
    },
    RibbonTab {
        name: PDF_UNDERLAY,
        panels: &[
            RibbonPanel { title: "Adjust", cols: &[Large(b(Icon::Adjust, "pdfadjust", "Adjust"))] },
            RibbonPanel { title: "Clipping", cols: &[Large(b(Icon::ClipFrame, "pdfclip", "Create Clipping Boundary"))] },
            RibbonPanel {
                title: "Options",
                cols: &[Large(b(Icon::Pdf, "pdfimport", "PDF Import")), Small(&[b(Icon::List, "ulayers", "Underlay Layers")])],
            },
            RibbonPanel { title: "Close", cols: &[Large(CLOSE_SELECTION)] },
        ],
    },
];

/// Which contextual tab is up and the tab the ribbon was on before it.
#[derive(Default)]
pub struct State {
    shown: Option<&'static str>,
    before: Option<String>,
    /// A Text Editor tab button for the MTEXT editor to apply on its next frame.
    pub(crate) text_action: Option<&'static str>,
}

fn tab(name: &str) -> Option<&'static RibbonTab> {
    CONTEXT_TABS.iter().find(|t| t.name == name)
}

pub(crate) fn is_contextual(name: &str) -> bool {
    tab(name).is_some()
}

/// The contextual tab the app's state calls for now.
pub(crate) fn current(app: &CadApp) -> Option<&'static RibbonTab> {
    if crate::mtext_editor::is_open(app) {
        return tab(TEXT_EDITOR);
    }
    let st = app.session.state().ok()?;
    if st.block_editor().is_some() {
        return tab(BLOCK_EDITOR);
    }
    let d = &st.doc;
    let kinds: Vec<&'static str> = st
        .selection
        .iter()
        .take(10_000)
        .map(|h| match d.entity(*h) {
            Some(e) if d.array_params(e).is_some() => ARRAY,
            Some(e) => match &e.kind {
                EntityKind::Hatch(_) => HATCH_EDITOR,
                EntityKind::Image(i) if i.underlay.is_some() => PDF_UNDERLAY,
                EntityKind::Image(_) => IMAGE,
                _ => "",
            },
            None => "",
        })
        .collect();
    let first = *kinds.first()?;
    // One array at a time (its tab edits one); the other kinds may be many of the same.
    let same = kinds.iter().all(|k| *k == first) && (first != ARRAY || kinds.len() == 1);
    if first.is_empty() || !same {
        return None;
    }
    tab(first)
}

/// Bring the ribbon to the contextual tab when a context begins and back when it ends.
pub(crate) fn update(app: &mut CadApp) {
    let now = current(app).map(|t| t.name);
    if now == app.ribbon_context.shown {
        return;
    }
    match now {
        Some(name) => {
            if app.ribbon_context.shown.is_none() || !is_contextual(&app.ui.ribbon_tab) {
                app.ribbon_context.before = Some(app.ui.ribbon_tab.clone());
            }
            app.ui.ribbon_tab = name.to_string();
        }
        None => {
            if let Some(before) = app.ribbon_context.before.take()
                && is_contextual(&app.ui.ribbon_tab)
            {
                app.ui.ribbon_tab = before;
            }
        }
    }
    app.ribbon_context.shown = now;
}

/// The Text Editor buttons and the Close button of the selection tabs.
pub(crate) fn route(app: &mut CadApp, id: &str, _params: &Value) -> Option<Result<Value, String>> {
    if id == "ui.context.close" {
        app.session.set_selection(Vec::new());
        return Some(Ok(Value::Null));
    }
    let action = id.strip_prefix("ui.mtext.")?;
    let known = crate::mtext_editor::ACTIONS.iter().find(|a| **a == action)?;
    if !crate::mtext_editor::is_open(app) {
        return Some(Err("The text editor is not open.".into()));
    }
    app.ribbon_context.text_action = Some(known);
    Some(Ok(Value::Null))
}

#[cfg(test)]
mod tests {
    use draftcraft_engine::Session;
    use draftcraft_engine::doc::{EntityKind, Image, Underlay};
    use egui::{Rect, pos2, vec2};
    use serde_json::json;

    use super::*;
    use crate::Services;

    fn image(app: &mut CadApp, underlay: bool) -> draftcraft_engine::doc::Handle {
        let i = Image { underlay: underlay.then(|| Underlay { page: "1".into(), ..Default::default() }), ..Image::default() };
        app.session.add_entity(EntityKind::Image(i)).unwrap()
    }

    #[test]
    fn a_selection_brings_its_tab_and_clearing_it_goes_back() {
        let mut app = CadApp::new(Session::new(), Services::default());
        app.ui.ribbon_tab = "Insert".into();
        let (img, pdf) = (image(&mut app, false), image(&mut app, true));
        update(&mut app);
        assert_eq!(app.ui.ribbon_tab, "Insert");
        app.session.set_selection(vec![img]);
        update(&mut app);
        assert_eq!(app.ui.ribbon_tab, IMAGE);
        // From one contextual tab to another, the tab to go back to stays the first one.
        app.session.set_selection(vec![pdf]);
        update(&mut app);
        assert_eq!(app.ui.ribbon_tab, PDF_UNDERLAY);
        // A mixed selection has no contextual tab.
        app.session.set_selection(vec![img, pdf]);
        update(&mut app);
        assert_eq!(app.ui.ribbon_tab, "Insert");
        app.session.set_selection(vec![pdf]);
        update(&mut app);
        app.start("ui.context.close");
        assert!(app.session.selection().is_empty());
        update(&mut app);
        assert_eq!(app.ui.ribbon_tab, "Insert");
        // A tab the user picked meanwhile is kept when the context ends.
        app.session.set_selection(vec![img]);
        update(&mut app);
        app.ui.ribbon_tab = "View".into();
        app.session.set_selection(Vec::new());
        update(&mut app);
        assert_eq!(app.ui.ribbon_tab, "View");
    }

    #[test]
    fn the_text_editor_tab_formats_the_editor_text() {
        let ctx = egui::Context::default();
        let mut app = CadApp::new(Session::new(), Services::default());
        let frame = |app: &mut CadApp| {
            let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1200.0, 800.0))), ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| crate::mtext_editor::show(app, ui.ctx()));
            out.textures_delta.clear();
        };
        assert!(app.run("ui.mtext.bold", json!(null)).is_err(), "no editor open");
        app.start("mtext");
        app.session.input(draftcraft_engine::Input::Point(draftcraft_geom::Vec2::new(1.0, 1.0))).unwrap();
        app.session.input(draftcraft_engine::Input::Point(draftcraft_geom::Vec2::new(6.0, 0.0))).unwrap();
        frame(&mut app);
        update(&mut app);
        assert_eq!(app.ui.ribbon_tab, TEXT_EDITOR);
        // Bold, then the degree sign typed in bold, then Close Text Editor.
        app.start("ui.mtext.bold");
        frame(&mut app);
        app.start("ui.mtext.degree");
        frame(&mut app);
        app.start("ui.mtext.ok");
        frame(&mut app);
        let d = app.session.doc().unwrap();
        let text = d.model.iter().find_map(|e| if let EntityKind::MText(m) = &e.kind { Some(m.contents.clone()) } else { None });
        let text = text.expect("the MTEXT was made");
        assert!(text.contains("|b1") && text.contains("°"), "{text}");
        update(&mut app);
        assert_ne!(app.ui.ribbon_tab, TEXT_EDITOR);
    }
}
