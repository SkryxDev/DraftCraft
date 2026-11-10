//! A block made dynamic in the Block Editor (BPARAMETER, BACTION) keeps its parameters and
//! actions through a DXF save, and its references stretch after reopening.
//!
//! A DWG save doesn't keep them yet: the DWG writer (acadrust) writes another program's objects
//! only from their original DWG data, which objects made here don't have.

use draftcraft_engine::Session;
use draftcraft_engine::cmd::file::{IoHooks, set_io};
use draftcraft_engine::doc::EntityKind;
use serde_json::json;

fn hooks() {
    set_io(IoHooks {
        read: |b, name| draftcraft_io::read(b, name).map_err(|e| e.to_string()),
        write: |d, name| draftcraft_io::write(d, name).map_err(|e| e.to_string()),
        plot: None,
    });
}

#[test]
fn an_authored_dynamic_block_survives_save_and_reopen() {
    hooks();
    let dir = std::env::temp_dir().join(format!("draftcraft-dynauthor-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for ext in ["dxf"] {
        let mut s = Session::new();
        let mut hs = Vec::new();
        for (a, b) in [([0, 0], [10, 0]), ([10, 0], [10, 5]), ([10, 5], [0, 5]), ([0, 5], [0, 0])] {
            let r = s.execute("line", &json!({ "points": [a, b] })).unwrap();
            hs.push(r["handles"][0].as_str().or_else(|| r["handle"].as_str()).unwrap().to_string());
        }
        s.execute("block", &json!({ "name": "PANEL", "base": [0, 0], "handles": hs, "retain": false })).unwrap();
        s.execute("insert", &json!({ "name": "PANEL", "at": [100, 0] })).unwrap();
        s.execute("bedit", &json!({ "name": "PANEL" })).unwrap();
        let hs: Vec<String> = s.doc().unwrap().model.iter().map(|e| e.handle.hex()).collect();
        s.execute("bparameter", &json!({ "type": "linear", "base": [0, 0], "end": [10, 0], "label": "Width" })).unwrap();
        s.execute("baction", &json!({ "type": "stretch", "parameter": "Width", "frame": [[8, -1], [12, 6]], "handles": hs })).unwrap();
        s.execute("bclose", &json!({ "save": true })).unwrap();
        let path = dir.join(format!("panel.{ext}"));
        s.execute("saveas", &json!({ "path": path.to_string_lossy() })).unwrap();

        let mut s = Session::new();
        s.execute("open", &json!({ "path": path.to_string_lossy() })).unwrap();
        let m = draftcraft_engine::doc::dynblock::dynamic_block(s.doc().unwrap(), "PANEL").unwrap_or_else(|| panic!("{ext}: not dynamic"));
        assert_eq!((m.parameters.len(), m.actions.len()), (1, 1), "{ext}");
        assert_eq!(m.actions[0].selection.len(), 4, "{ext}: the action keeps its objects");
        let ins = s.doc().unwrap().model.iter().find(|e| matches!(e.kind, EntityKind::Insert(_))).unwrap().handle;
        let d = s.doc_mut().unwrap();
        draftcraft_engine::doc::dynblock::set_property(d, ins, "Width", "25").unwrap();
        let EntityKind::Insert(i) = &d.entity(ins).unwrap().kind else { panic!("insert") };
        let xs: Vec<f64> = d
            .block(&i.block)
            .unwrap()
            .entities
            .iter()
            .filter_map(|e| if let EntityKind::Line(l) = &e.kind { Some(l.a.x.max(l.b.x)) } else { None })
            .collect();
        assert!(xs.iter().any(|x| (x - 25.0).abs() < 1e-9), "{ext}: {xs:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
