//! The direct DWG reader (`draftcraft_io::dwg`) on the MI-201 test drawings: one part saved by
//! AutoCAD in every DWG version, each with AutoCAD's own DXF of the same drawing as the reference.
//!
//! The reader is built category by category. These tests hold it to its account of the file: every
//! object is either in the drawing or counted as unread, and a damaged file is an error or a
//! drawing, never a panic.

use draftcraft_dwg::file::DwgFile;
use draftcraft_io::dwg;

/// The MI-201 DWG files the reader opens, with their version codes.
const DWGS: &[(&str, &str, &[u8])] = &[
    ("mi201-r14.dwg", "AC1014", include_bytes!("../data/mi201/mi201-r14.dwg")),
    ("mi201-r2000.dwg", "AC1015", include_bytes!("../data/mi201/mi201-r2000.dwg")),
    ("mi201-r2004.dwg", "AC1018", include_bytes!("../data/mi201/mi201-r2004.dwg")),
    ("mi201-r2010.dwg", "AC1024", include_bytes!("../data/mi201/mi201-r2010.dwg")),
    ("mi201-r2013.dwg", "AC1027", include_bytes!("../data/mi201/mi201-r2013.dwg")),
    ("mi201-r2018.dwg", "AC1032", include_bytes!("../data/mi201/mi201-r2018.dwg")),
    ("mi201-r2018-from-ascii-dxf.dwg", "AC1032", include_bytes!("../data/mi201/mi201-r2018-from-ascii-dxf.dwg")),
    ("mi201-r2018-from-binary-dxf.dwg", "AC1032", include_bytes!("../data/mi201/mi201-r2018-from-binary-dxf.dwg")),
];

/// The 2007 file: its container is not read yet.
const DWG_2007: &[u8] = include_bytes!("../data/mi201/mi201-r2007.dwg");

/// AutoCAD's DXF of each version: the references the reader is compared with as it grows.
const DXFS: &[(&str, &[u8])] = &[
    ("mi201-r12.dxf", include_bytes!("../data/mi201/mi201-r12.dxf")),
    ("mi201-r2000.dxf", include_bytes!("../data/mi201/mi201-r2000.dxf")),
    ("mi201-r2004.dxf", include_bytes!("../data/mi201/mi201-r2004.dxf")),
    ("mi201-r2007.dxf", include_bytes!("../data/mi201/mi201-r2007.dxf")),
    ("mi201-r2010.dxf", include_bytes!("../data/mi201/mi201-r2010.dxf")),
    ("mi201-r2013.dxf", include_bytes!("../data/mi201/mi201-r2013.dxf")),
    ("mi201-r2018.dxf", include_bytes!("../data/mi201/mi201-r2018.dxf")),
];

#[test]
fn every_object_of_every_version_is_accounted_for() {
    for (name, code, bytes) in DWGS {
        let file = DwgFile::open(bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(file.version_code(), *code, "{name}");
        assert!(file.object_count() > 100, "{name}: {} objects", file.object_count());
        let (d, unread) = dwg::read(bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(d.header.str("ACADVER", ""), *code, "{name}");
        // Every object is in the drawing or counted as unread: the categories read so far leave
        // none of their own behind, and nothing else is dropped.
        // A key with a space is a part of an object that waits for a later category (a header
        // variable's name, a layer's plot style), not an object.
        let objects: usize = unread.by_type.iter().filter(|(k, _)| !k.contains(' ')).map(|(_, n)| n).sum();
        let tables = file.objects().filter(|o| READ_SO_FAR.contains(&o.type_name.as_str())).count();
        assert_eq!(objects + tables, file.object_count(), "{name}: {}", unread.summary());
        assert!(!unread.by_type.contains_key("?"), "{name}: {}", unread.summary());
        for t in READ_SO_FAR {
            assert!(!unread.by_type.keys().any(|k| k == t || *k == format!("{t} (not decoded)")), "{name}: {t} left unread: {}", unread.summary());
        }
        // A type of a later step is still counted, not dropped.
        assert!(unread.by_type.contains_key("DICTIONARY"), "{name}: no DICTIONARY among {}", unread.summary());
    }
}

/// The object types the reader puts in the drawing so far.
const READ_SO_FAR: &[&str] = &[
    "DIMENSION_LINEAR",
    "DIMASSOC",
    "LINE",
    "CIRCLE",
    "ARC",
    "TEXT",
    "POINT",
    "SOLID",
    "MTEXT",
    "ELLIPSE",
    "LWPOLYLINE",
    "TRACE",
    "3DFACE",
    "XLINE",
    "RAY",
    "APPID",
    "STYLE",
    "LTYPE",
    "LAYER",
    "DIMSTYLE",
    "UCS",
    "VIEW",
    "VPORT",
    "BLOCK_HEADER",
    "BLOCK",
    "ENDBLK",
    "LAYOUT",
    "BLOCK_CONTROL",
    "LAYER_CONTROL",
    "STYLE_CONTROL",
    "LTYPE_CONTROL",
    "VIEW_CONTROL",
    "UCS_CONTROL",
    "VPORT_CONTROL",
    "APPID_CONTROL",
    "DIMSTYLE_CONTROL",
    "VX_CONTROL",
];

#[test]
fn the_2007_container_is_refused_by_name() {
    let err = dwg::read(DWG_2007).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(err.contains("AC1021"), "{err}");
}

#[test]
fn the_references_read() {
    for (name, bytes) in DXFS {
        let d = draftcraft_io::read(bytes, name).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(d.model.len(), 34, "{name}: model-space entities");
    }
}

#[test]
fn damaged_files_never_panic() {
    for (_, _, bytes) in DWGS {
        for cut in [0, 5, 6, 64, 128, 1024, bytes.len() / 2, bytes.len() - 1] {
            let _ = dwg::read(&bytes[..cut.min(bytes.len())]);
        }
        let mut b = bytes.to_vec();
        for i in (0..b.len()).step_by(b.len() / 251 + 1) {
            b[i] ^= 0x5A;
            let _ = dwg::read(&b);
            b[i] ^= 0x5A;
        }
    }
}

/// Each DWG with AutoCAD's DXF of the same version (R14's is the R12 DXF).
fn pairs() -> Vec<(&'static str, &'static [u8], &'static [u8])> {
    let dxf = |name: &str| DXFS.iter().find(|(n, _)| *n == name).map(|(_, b)| *b).unwrap();
    DWGS.iter()
        .map(|(name, _, bytes)| {
            let reference = match *name {
                "mi201-r14.dwg" => "mi201-r12.dxf",
                "mi201-r2000.dwg" => "mi201-r2000.dxf",
                "mi201-r2004.dwg" => "mi201-r2004.dxf",
                "mi201-r2010.dwg" => "mi201-r2010.dxf",
                "mi201-r2013.dwg" => "mi201-r2013.dxf",
                _ => "mi201-r2018.dxf",
            };
            (*name, *bytes, dxf(reference))
        })
        .collect()
}

/// Header variables that belong to one save of a file, not to the drawing: AutoCAD wrote the DWG
/// and its DXF at different moments, so these differ between the two.
const OF_THE_SAVE: &[&str] = &["ACADVER", "ACADMAINTVER", "HANDSEED", "TDUPDATE", "TDUUPDATE", "TDINDWG", "TDUSRTIMER", "VERSIONGUID", "DWGCODEPAGE"];

/// The two dates in local time: from AutoCAD 2000 on a DWG stores them in universal time only
/// (TDUCREATE, TDUUPDATE), and the offset AutoCAD used is not in the file.
const LOCAL_TIMES: &[&str] = &["TDCREATE", "TDUPDATE"];

/// The default visual styles of interference checking: AutoCAD's DXF names them, its DWG holds
/// null references.
const NULL_IN_DWG: &[&str] = &["INTERFEREOBJVS", "INTERFEREVPVS"];

/// Variables that name a table record, a block or a UCS. They are set when those are read; until
/// then the reader must count each as unread and leave it out of the header.
const NAMES: &[&str] = &[
    "CLAYER",
    "CELTYPE",
    "TEXTSTYLE",
    "DIMSTYLE",
    "CMLSTYLE",
    "DIMTXSTY",
    "DIMBLK",
    "DIMBLK1",
    "DIMBLK2",
    "DIMLDRBLK",
    "DIMLTYPE",
    "DIMLTEX1",
    "DIMLTEX2",
    "UCSNAME",
    "UCSBASE",
    "UCSORTHOREF",
    "PUCSNAME",
    "PUCSBASE",
    "PUCSORTHOREF",
];

/// Where the R14 DWG and the R12 DXF, the nearest reference AutoCAD writes, are different files
/// of different formats: R12 has a variable for whether handles are on (a later DWG always has
/// them), and AutoCAD wrote 64 as the R12 file's $MAXACTVP where the R14 file holds 48.
const R12_DIFFERS: &[&str] = &["HANDLING", "MAXACTVP"];

fn same(a: &draftcraft_doc::HVal, b: &draftcraft_doc::HVal) -> bool {
    use draftcraft_doc::HVal::*;
    match (a, b) {
        (Real(x), Real(y)) => (x - y).abs() <= 1e-9 * (1.0 + y.abs()),
        (Int(i), Real(r)) | (Real(r), Int(i)) => (*i as f64 - r).abs() < 1e-9,
        (Point(p), Point(q)) => (*p - *q).len() < 1e-9,
        _ => a == b,
    }
}

#[test]
fn header_variables_match_autocads_dxf() {
    for (name, dwg_bytes, dxf_bytes) in pairs() {
        let (d, unread) = dwg::read(dwg_bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        let reference = draftcraft_io::read(dxf_bytes, "reference.dxf").unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!unread.by_type.contains_key("header"), "{name}: the header was not read");
        let before_2000 = name.contains("r14");
        let mut wrong = Vec::new();
        let mut compared = 0;
        for (var, want) in &reference.header.vars {
            let v = var.as_str();
            if OF_THE_SAVE.contains(&v)
                || NULL_IN_DWG.contains(&v)
                || (!before_2000 && LOCAL_TIMES.contains(&v))
                || (before_2000 && R12_DIFFERS.contains(&v))
            {
                continue;
            }
            // A name the file gives as a reference waits for its table; one it gives as text
            // (R14's $DIMBLK) is compared like any other variable.
            if NAMES.contains(&v) && unread.by_type.contains_key(&format!("header ${v}")) {
                assert!(d.header.get(v).is_none(), "{name}: ${v} is set before its table is read");
                continue;
            }
            // The two DWGs AutoCAD made from DXFs have handles of their own, so a variable that
            // holds a handle differs from the reference of the original drawing.
            if v == "CMATERIAL" && name.contains("-from-") {
                assert!(d.header.get(v).is_some(), "{name}");
                continue;
            }
            // A 2004 DXF holds $XCLIPFRAME as a yes/no and keeps the drawing's real value, 2, in a
            // dictionary variable; the 2004 DWG's header holds the 2 itself, as later DXFs do.
            if v == "XCLIPFRAME" && name.contains("r2004") {
                assert_eq!(d.header.i64(v, -1), 2, "{name}");
                continue;
            }
            compared += 1;
            match d.header.get(v) {
                Some(got) if same(got, want) => {}
                got => wrong.push(format!("${v}: read {got:?}, AutoCAD's DXF has {want:?}")),
            }
        }
        assert!(wrong.is_empty(), "{name}: {} of {compared} header variables differ:\n{}", wrong.len(), wrong.join("\n"));
        assert!(compared > 60, "{name}: only {compared} variables compared");
    }
}

#[test]
fn layers_linetypes_and_text_styles_match_autocads_dxf() {
    for (name, dwg_bytes, dxf_bytes) in pairs() {
        let (d, _) = dwg::read(dwg_bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        let reference = draftcraft_io::read(dxf_bytes, "reference.dxf").unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(reference.layers.len() >= 2 && reference.linetypes.len() >= 3 && !reference.text_styles.is_empty(), "{name}");
        assert_eq!(d.layers, reference.layers, "{name}: layers");
        assert_eq!(d.linetypes, reference.linetypes, "{name}: linetypes");
        assert_eq!(d.text_styles, reference.text_styles, "{name}: text styles");
        for var in ["CLAYER", "CELTYPE", "TEXTSTYLE", "DIMTXSTY"] {
            // The R12 DXF, R14's reference, has no $DIMTXSTY.
            if let Some(want) = reference.header.get(var) {
                assert_eq!(d.header.get(var), Some(want), "{name}: ${var}");
            }
        }
    }
}

#[test]
fn dimension_styles_views_ucss_and_viewports_match_autocads_dxf() {
    for (name, dwg_bytes, dxf_bytes) in pairs() {
        let (d, _) = dwg::read(dwg_bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        let reference = draftcraft_io::read(dxf_bytes, "reference.dxf").unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!reference.dim_styles.is_empty(), "{name}");
        assert_eq!(d.dim_styles.len(), reference.dim_styles.len(), "{name}: dimension styles");
        for (got, want) in d.dim_styles.iter().zip(&reference.dim_styles) {
            let (g, w) = (serde_json::to_value(got).unwrap(), serde_json::to_value(want).unwrap());
            let diff: Vec<String> = w
                .as_object()
                .unwrap()
                .iter()
                .filter(|(k, v)| g.get(k.as_str()) != Some(*v))
                .map(|(k, v)| format!("{k}: read {:?}, AutoCAD's DXF has {v:?}", g.get(k.as_str())))
                .collect();
            assert!(diff.is_empty(), "{name}: dimension style {}:\n{}", want.name, diff.join("\n"));
        }
        assert_eq!(d.views, reference.views, "{name}: views");
        assert_eq!(d.ucss, reference.ucss, "{name}: UCSs");
        assert_eq!(d.vports, reference.vports, "{name}: viewport configurations");
        for var in ["DIMSTYLE", "UCSNAME", "PUCSNAME", "DIMBLK", "DIMBLK1", "DIMBLK2", "DIMLDRBLK", "SNAPUNIT", "GRIDUNIT", "SNAPBASE", "SNAPANG"] {
            if let Some(want) = reference.header.get(var) {
                assert!(
                    d.header.get(var).is_some_and(|got| same(got, want)),
                    "{name}: ${var}: read {:?}, AutoCAD's DXF has {want:?}",
                    d.header.get(var)
                );
            }
        }
    }
}

#[test]
fn blocks_and_layouts_match_autocads_dxf() {
    for (name, dwg_bytes, dxf_bytes) in pairs() {
        let (d, _) = dwg::read(dwg_bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        let reference = draftcraft_io::read(dxf_bytes, "reference.dxf").unwrap_or_else(|e| panic!("{name}: {e}"));
        // Layouts: the tabs in order, each with its page setup, limits and saved view. Their
        // entities are read with the entities.
        let tabs = |d: &draftcraft_doc::Drawing| {
            d.layouts.iter().map(|l| (l.name.clone(), l.tab_order, l.page.clone(), l.limits, l.view)).collect::<Vec<_>>()
        };
        if name.contains("r14") {
            // An R12 DXF has no layouts, so its reading has the default page setup; the R14 DWG
            // holds the layout with its own. Only the tabs can be compared.
            let names = |d: &draftcraft_doc::Drawing| d.layouts.iter().map(|l| (l.name.clone(), l.tab_order)).collect::<Vec<_>>();
            assert_eq!(names(&d), names(&reference), "{name}: layouts");
        } else {
            assert_eq!(tabs(&d), tabs(&reference), "{name}: layouts");
        }
        assert_eq!(d.active_layout, reference.active_layout, "{name}: active layout");
        // Block definitions: the same blocks, each with its own properties. An R12 DXF, R14's
        // reference, also has the two spaces as blocks ($MODEL_SPACE, $PAPER_SPACE) and the
        // blocks AutoCAD made of what R12 cannot hold (*U2, *U3), which the R14 DWG does not.
        assert!(!reference.blocks.is_empty(), "{name}");
        if !name.contains("r14") {
            assert_eq!(d.blocks.keys().collect::<Vec<_>>(), reference.blocks.keys().collect::<Vec<_>>(), "{name}: blocks");
        }
        for (block, got) in &d.blocks {
            let want = reference.blocks.get(block).unwrap_or_else(|| panic!("{name}: block {block} is not in the reference"));
            let props = |b: &draftcraft_doc::Block| {
                (b.base, b.description.clone(), b.anonymous, b.xref_path.clone(), b.xref_overlay, b.explodable, b.units, b.annotative)
            };
            assert_eq!(props(got), props(want), "{name}: block {block}");
        }
    }
}

/// The entity types the reader puts in the drawing so far.
fn read_so_far(k: &draftcraft_doc::EntityKind) -> bool {
    use draftcraft_doc::EntityKind::*;
    match k {
        Line(_) | Point(_) | Circle(_) | Arc(_) | Ellipse(_) | LwPolyline(_) | Text(_) | MText(_) | Solid(_) | Trace(_) | Face3d(_) | XLine(_)
        | Ray(_) | Dimension(_) => true,
        Insert(i) => i.attribs.is_empty(),
        _ => false,
    }
}

/// Every entity with where it lives, in order: `model`, `layout NAME` or `block NAME`.
fn placed(d: &draftcraft_doc::Drawing) -> Vec<(String, std::sync::Arc<draftcraft_doc::Entity>)> {
    let mut v: Vec<_> = d.model.iter().map(|e| ("model".to_string(), e.clone())).collect();
    for l in &d.layouts {
        v.extend(l.entities.iter().map(|e| (format!("layout {}", l.name), e.clone())));
    }
    for (n, b) in &d.blocks {
        v.extend(b.entities.iter().map(|e| (format!("block {n}"), e.clone())));
    }
    v
}

/// Two values equal but for the last digits of their numbers: a DXF holds a number as text of
/// 16 digits, a DWG holds the exact double. With `any_case`, names are compared without case too
/// (an R12 DXF leaves out a text's style when it is the standard one, which then reads as
/// `Standard`).
fn close(a: &serde_json::Value, b: &serde_json::Value, any_case: bool) -> bool {
    use serde_json::Value::*;
    match (a, b) {
        (Number(x), Number(y)) => match (x.as_f64(), y.as_f64()) {
            (Some(x), Some(y)) => (x - y).abs() <= 1e-9 * (1.0 + y.abs()),
            _ => x == y,
        },
        (String(x), String(y)) if any_case => x.eq_ignore_ascii_case(y),
        (Array(x), Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| close(p, q, any_case)),
        (Object(x), Object(y)) => x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| close(v, w, any_case))),
        _ => a == b,
    }
}

#[test]
fn simple_entities_match_autocads_dxf() {
    for (name, dwg_bytes, dxf_bytes) in pairs() {
        // R14's reference is an R12 DXF, where AutoCAD turned what R12 cannot hold into other
        // entities; and the two DWGs made from DXFs have other handles. Those are compared by
        // content and order, the others by handle too.
        let by_handle = !name.contains("r14") && !name.contains("-from-");
        let (d, _) = dwg::read(dwg_bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        let reference = draftcraft_io::read(dxf_bytes, "reference.dxf").unwrap_or_else(|e| panic!("{name}: {e}"));
        let r14 = name.contains("r14");
        // R12 has no MTEXT: AutoCAD wrote each one into the R12 DXF as an anonymous block (*U2,
        // *U3) and an insert of it. For R14 the MTEXTs read and those inserts are set aside.
        let converted = |e: &draftcraft_doc::Entity| matches!(&e.kind, draftcraft_doc::EntityKind::Insert(i) if i.block.starts_with("*U"));
        let is_mtext = |e: &draftcraft_doc::Entity| matches!(e.kind, draftcraft_doc::EntityKind::MText(_));
        let got: Vec<_> = placed(&d).into_iter().filter(|(_, e)| !(r14 && is_mtext(e))).collect();
        let want: Vec<_> = placed(&reference)
            .into_iter()
            .filter(|(place, e)| read_so_far(&e.kind) && !place.starts_with("block $") && !place.starts_with("block *U") && !(r14 && converted(e)))
            .collect();
        if r14 {
            let mtexts = placed(&d).iter().filter(|(_, e)| is_mtext(e)).count();
            let inserts = placed(&reference).iter().filter(|(_, e)| converted(e)).count();
            assert_eq!(mtexts, inserts, "{name}: one MTEXT for each converted block");
        }
        assert!(want.len() >= 30, "{name}: {} reference entities", want.len());
        let show = |v: &[(String, std::sync::Arc<draftcraft_doc::Entity>)]| {
            v.iter().map(|(p, e)| format!("{p} {:X} {}", e.handle.0, e.kind.type_name())).collect::<Vec<_>>()
        };
        assert_eq!(got.len(), want.len(), "{name}:\nread {:?}\nreference {:?}", show(&got), show(&want));
        for ((gp, g), (wp, w)) in got.iter().zip(&want) {
            assert_eq!(gp, wp, "{name}: place of {:X}", w.handle.0);
            if by_handle {
                assert_eq!(g.handle, w.handle, "{name}: handle in {wp}");
            }
            let (mut g, mut w) = (serde_json::to_value(g.as_ref()).unwrap(), serde_json::to_value(w.as_ref()).unwrap());
            // The R14 DWG also carries extended data that R12 has no place for (AutoCAD keeps an
            // R14 entity's lineweight there): compared without it.
            if r14
                && w["common"].get("xdata").is_none()
                && let Some(c) = g["common"].as_object_mut()
            {
                c.remove("xdata");
            }
            // So do a dimension's style override in it (AutoCAD keeps an R14 dimension's text
            // style there) and its associativity, which R12 has no DIMASSOC object for.
            // The two DWGs AutoCAD made from DXFs hold no DIMASSOC objects (the DXFs they were
            // made from had none), while their reference is the original drawing's DXF.
            if name.contains("-from-") {
                assert!(g["kind"].get("assoc").is_none(), "{name}: associativity read from a file that holds none");
                if let Some(k) = w.get("kind").cloned().as_mut().and_then(|k| k.as_object_mut()) {
                    k.remove("assoc");
                    w["kind"] = serde_json::Value::Object(k.clone());
                }
            }
            for key in ["overrides", "assoc"] {
                if r14
                    && w["kind"].get(key).is_none()
                    && let Some(k) = g["kind"].as_object_mut()
                {
                    k.remove(key);
                }
            }
            assert!(close(&g["kind"], &w["kind"], r14), "{name}: {wp}\nread {}\nreference {}", g["kind"], w["kind"]);
            assert!(close(&g["common"], &w["common"], r14), "{name}: properties in {wp}\nread {}\nreference {}", g["common"], w["common"]);
        }
    }
}

/// A drawing with attributes in every mode: block TAG with five definitions (plain, invisible,
/// constant, every other mode, and a centred one with a prompt), and two references to it, one
/// plain and one mirrored, scaled and rotated, whose attributes carry their own layer, colour,
/// linetype and lineweight.
fn attributes_drawing() -> draftcraft_doc::Drawing {
    use draftcraft_doc::*;
    use draftcraft_geom::Vec3;
    let text = |value: &str, x: f64, halign: HAlign| Text {
        insert: Vec3::new(x, 2.0, 0.0),
        align_pt: (halign != HAlign::Left).then(|| Vec3::new(x + 3.0, 2.0, 0.0)),
        height: 0.25,
        value: value.into(),
        rotation: 0.0,
        width_factor: 1.0,
        oblique: 0.0,
        style: "Standard".into(),
        halign,
        valign: VAlign::Baseline,
    };
    let attrib = |tag: &str, x: f64| Attrib {
        tag: tag.into(),
        text: text(tag, x, HAlign::Left),
        invisible: false,
        constant: false,
        prompt: String::new(),
        props: AttribProps::default(),
        multiline: None,
        modes: AttribModes::default(),
    };
    let defs = vec![
        attrib("PLAIN", 0.0),
        Attrib { invisible: true, ..attrib("HIDDEN", 1.0) },
        Attrib { constant: true, ..attrib("FIXED", 2.0) },
        Attrib { modes: AttribModes { verify: true, preset: true, lock_position: true }, ..attrib("MODES", 3.0) },
        Attrib { prompt: "Drawing number?".into(), text: text("NUMBER", 4.0, HAlign::Center), ..attrib("NUMBER", 4.0) },
    ];
    let mut d = Drawing::new_imperial();
    d.ensure_layer("TITLE");
    d.ensure_layer("NOTES");
    let mut b = Block::new("TAG");
    for a in &defs {
        let h = d.new_handle();
        b.entities.push(Entity::new(h, EntityKind::AttDef(a.clone())));
    }
    d.blocks.insert("TAG".into(), std::sync::Arc::new(b));
    let values = |own: bool| -> Vec<Attrib> {
        defs.iter()
            .filter(|a| !a.constant)
            .map(|a| Attrib {
                prompt: String::new(),
                text: Text { value: format!("{} value", a.tag), ..a.text.clone() },
                props: if own {
                    AttribProps {
                        layer: Some("NOTES".into()),
                        color: Some(draftcraft_color::Color::Index(1)),
                        linetype: Some("Continuous".into()),
                        lineweight: Some(Lineweight::from_dxf(35)),
                    }
                } else {
                    AttribProps::default()
                },
                ..a.clone()
            })
            .collect()
    };
    for (at, own, scale, rotation) in [(Vec3::new(10.0, 10.0, 0.0), false, 1.0, 0.0), (Vec3::new(30.0, 5.0, 0.0), true, 2.0, 0.5)] {
        let ins = Insert {
            block: "TAG".into(),
            insert: at,
            scale: Vec3::new(scale, scale, 1.0),
            rotation,
            attribs: values(own),
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
            clip: None,
        };
        let common = Common { layer: "TITLE".into(), ..Common::default() };
        d.add(&Space::Model, common, EntityKind::Insert(ins)).expect("add");
    }
    d
}

/// The attribute definitions of every block and the attributes of every block reference, with
/// their places, for comparing two readings.
fn attributes_of(d: &draftcraft_doc::Drawing) -> Vec<(String, serde_json::Value)> {
    use draftcraft_doc::EntityKind;
    placed(d)
        .into_iter()
        .filter_map(|(place, e)| match &e.kind {
            EntityKind::AttDef(a) => Some((place, serde_json::to_value(a).unwrap())),
            EntityKind::Insert(i) if !i.attribs.is_empty() => {
                Some((place, serde_json::json!({ "block": i.block, "common": e.common, "attribs": i.attribs })))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn attributes_read_as_the_acadrust_reader_reads_them() {
    use draftcraft_io::{DwgReader, SaveOptions, Version, read_dwg_with, write_with};
    let source = attributes_drawing();
    // The 2007 container is not read yet.
    for version in [Version::R2000, Version::R2004, Version::R2010, Version::R2013, Version::R2018] {
        let bytes = write_with(&source, "t.dwg", &SaveOptions { version: Some(version), binary: false }).unwrap();
        let (d, unread) = dwg::read(&bytes).unwrap_or_else(|e| panic!("{version:?}: {e}"));
        let reference = read_dwg_with(&bytes, DwgReader::Acadrust).unwrap();
        let want = attributes_of(&reference);
        assert_eq!(want.len(), 7, "{version:?}: the reference has the five definitions and the two references: {want:#?}");
        let got = attributes_of(&d);
        assert_eq!(got.len(), want.len(), "{version:?}: unread {}", unread.summary());
        for ((gp, g), (wp, w)) in got.iter().zip(&want) {
            assert_eq!(gp, wp, "{version:?}");
            assert!(close(g, w, false), "{version:?}: {wp}\nread      {g}\nreference {w}");
        }
        for t in ["ATTRIB", "ATTDEF", "SEQEND", "INSERT"] {
            assert!(!unread.by_type.keys().any(|k| k.starts_with(t)), "{version:?}: {t} left unread: {}", unread.summary());
        }
    }
}

/// The multiline attributes fixture of `multiline_attributes.rs` (a block with a multiline
/// definition, and a reference with its own multiline text), as DXF.
fn multiline_fixture() -> String {
    let src = include_str!("multiline_attributes.rs");
    let start = src.find("const FIXTURE: &str = \"").map(|i| i + "const FIXTURE: &str = \"".len()).unwrap();
    let end = start + src[start..].find("\";").unwrap();
    let mut dxf = String::new();
    for line in src[start..end].lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (code, value) = line.split_once(' ').unwrap_or((line, ""));
        dxf.push_str(&format!("{code}\n{value}\n"));
    }
    dxf
}

/// Multiline attributes (AutoCAD 2018 DWG) embed MTEXT fields whose layout no AutoCAD-saved file
/// here confirms, so the reader does not decode them yet. It must count them, and leave out the
/// block reference that owns one whole, rather than lose any part silently.
#[test]
fn multiline_attributes_are_counted_not_dropped() {
    use draftcraft_io::{SaveOptions, Version, write_with};
    let d = draftcraft_io::read(multiline_fixture().as_bytes(), "m.dxf").unwrap();
    let bytes = write_with(&d, "t.dwg", &SaveOptions { version: Some(Version::R2018), binary: false }).unwrap();
    let (read, unread) = dwg::read(&bytes).unwrap();
    assert!(attributes_of(&read).is_empty(), "nothing of a multiline attribute's block or reference is read in part");
    for what in ["ATTDEF (not decoded)", "ATTRIB (not decoded)", "INSERT whose attributes were not all read"] {
        assert_eq!(unread.by_type.get(what), Some(&1), "{what}: {}", unread.summary());
    }
}

/// A DWG of `version` holding every kind of polyline and both kinds of spline, written with
/// acadrust's own API: DraftCraft writes 2D polylines and polygon meshes in other forms, so its
/// save path can't make them.
fn curves_dwg(version: acadrust::types::DxfVersion) -> Vec<u8> {
    use acadrust::entities::*;
    use acadrust::types::Vector3;
    let mut doc = acadrust::CadDocument::with_version(version);
    // 2D polylines: open with bulges and widths at an elevation, closed, and mirrored.
    for (normal_z, closed, y) in [(1.0, false, 0.0), (1.0, true, 20.0), (-1.0, false, 40.0)] {
        let mut p = Polyline2D::new();
        p.elevation = 1.5;
        p.normal = Vector3::new(0.0, 0.0, normal_z);
        p.start_width = 0.25;
        p.end_width = 0.25;
        p.add_vertex(Vertex2D::new(Vector3::new(0.0, y, 0.0)).with_bulge(0.5).with_width(0.1, 0.3));
        p.add_vertex(Vertex2D::new(Vector3::new(10.0, y, 0.0)).with_width(0.3, 0.1));
        p.add_vertex(Vertex2D::new(Vector3::new(10.0, y + 10.0, 0.0)).with_bulge(-1.0));
        p.add_vertex(Vertex2D::new(Vector3::new(0.0, y + 10.0, 0.0)));
        if closed {
            p.close();
        }
        doc.add_entity(EntityType::Polyline2D(p)).unwrap();
    }
    // A closed 3D polyline.
    let mut p3 = Polyline3D::from_points(vec![
        Vector3::new(0.0, 0.0, 0.0),
        Vector3::new(5.0, 0.0, 2.0),
        Vector3::new(5.0, 5.0, 4.0),
        Vector3::new(0.0, 5.0, 6.0),
    ]);
    p3.flags.closed = true;
    doc.add_entity(EntityType::Polyline3D(p3)).unwrap();
    // A polyface mesh: a quad and a triangle over five vertices.
    let mut pf = PolyfaceMesh::new();
    for (x, y, z) in [(0.0, 0.0, 0.0), (4.0, 0.0, 0.0), (4.0, 4.0, 1.0), (0.0, 4.0, 1.0), (2.0, 6.0, 2.0)] {
        pf.add_vertex_xyz(x + 50.0, y, z);
    }
    pf.add_face(PolyfaceFace::quad(1, 2, 3, 4));
    pf.add_face(PolyfaceFace::triangle(3, 5, 4));
    doc.add_entity(EntityType::PolyfaceMesh(pf)).unwrap();
    // A 3 × 4 polygon mesh, closed in N.
    let mut pm = PolygonMeshEntity::new();
    pm.m_vertex_count = 3;
    pm.n_vertex_count = 4;
    pm.flags = PolygonMeshFlags::POLYGON_MESH | PolygonMeshFlags::CLOSED_N;
    for i in 0..3 {
        for j in 0..4 {
            pm.vertices.push(PolygonMeshVertex::at(Vector3::new(70.0 + f64::from(i) * 3.0, f64::from(j) * 2.0, f64::from(i * j))));
        }
    }
    doc.add_entity(EntityType::PolygonMesh(pm)).unwrap();
    // A rational spline by its control points, and a spline by its fit points with end tangents.
    let control = vec![Vector3::new(0.0, 60.0, 0.0), Vector3::new(5.0, 70.0, 0.0), Vector3::new(10.0, 55.0, 0.0), Vector3::new(15.0, 65.0, 0.0)];
    let mut sc = Spline::from_control_points(3, control);
    sc.weights = vec![1.0, 2.0, 0.5, 1.0];
    sc.flags.rational = true;
    doc.add_entity(EntityType::Spline(sc)).unwrap();
    let fit = vec![Vector3::new(30.0, 60.0, 0.0), Vector3::new(35.0, 66.0, 0.0), Vector3::new(42.0, 58.0, 0.0), Vector3::new(48.0, 64.0, 0.0)];
    let mut sf = Spline::from_fit_points(fit);
    sf.begin_tangent = Vector3::new(1.0, 1.0, 0.0);
    sf.end_tangent = Vector3::new(1.0, -1.0, 0.0);
    doc.add_entity(EntityType::Spline(sf)).unwrap();
    acadrust::DwgWriter::write_to_vec(&doc).unwrap()
}

#[test]
fn polylines_and_splines_read_as_the_acadrust_reader_reads_them() {
    use acadrust::types::DxfVersion;
    use draftcraft_doc::EntityKind;
    use draftcraft_io::{DwgReader, read_dwg_with};
    let curve = |k: &EntityKind| {
        matches!(
            k,
            EntityKind::LwPolyline(_) | EntityKind::Polyline3d(_) | EntityKind::PolyfaceMesh(_) | EntityKind::Face3d(_) | EntityKind::Spline(_)
        )
    };
    // The 2007 container is not read yet.
    for version in [DxfVersion::AC1015, DxfVersion::AC1018, DxfVersion::AC1024, DxfVersion::AC1027, DxfVersion::AC1032] {
        let bytes = curves_dwg(version);
        let (d, unread) = dwg::read(&bytes).unwrap_or_else(|e| panic!("{version:?}: {e}"));
        let reference = read_dwg_with(&bytes, DwgReader::Acadrust).unwrap();
        let want: Vec<_> = reference.model.iter().filter(|e| curve(&e.kind)).collect();
        // Three 2D polylines, a 3D one, a polyface mesh, the 3 × 4 mesh closed in N as 2 × 4 faces,
        // two splines.
        assert_eq!(want.len(), 15, "{version:?}: {:?}", want.iter().map(|e| e.kind.type_name()).collect::<Vec<_>>());
        let got: Vec<_> = d.model.iter().filter(|e| curve(&e.kind)).collect();
        assert_eq!(got.len(), want.len(), "{version:?}: unread {}", unread.summary());
        for (g, w) in got.iter().zip(&want) {
            // acadrust's DXF gives a vertex stored with no width the polyline's default width and
            // leaves the default out, so 2D polylines are checked against what was written.
            if matches!(g.kind, EntityKind::LwPolyline(_)) {
                continue;
            }
            let (g, w) = (serde_json::to_value(g.as_ref()).unwrap(), serde_json::to_value(w.as_ref()).unwrap());
            assert!(close(&g["kind"], &w["kind"], false), "{version:?}\nread      {}\nreference {}", g["kind"], w["kind"]);
            assert!(close(&g["common"], &w["common"], false), "{version:?}: properties\nread      {}\nreference {}", g["common"], w["common"]);
        }
        // The 2D polylines as they were written: the vertices with their own widths and bulges,
        // the default width, the elevation, closed or not; the mirrored one with x and the bulges
        // turned round and its extrusion kept.
        let written = |closed: bool, y: f64, m: f64| {
            let v = |x: f64, y: f64, bulge: f64, sw: f64, ew: f64| draftcraft_geom::PolyVertex {
                p: draftcraft_geom::Vec2::new(m * x, y),
                bulge: m * bulge,
                start_width: sw,
                end_width: ew,
            };
            draftcraft_doc::LwPolyline {
                vertices: vec![
                    v(0.0, y, 0.5, 0.1, 0.3),
                    v(10.0, y, 0.0, 0.3, 0.1),
                    v(10.0, y + 10.0, -1.0, 0.0, 0.0),
                    v(0.0, y + 10.0, 0.0, 0.0, 0.0),
                ],
                closed,
                const_width: 0.25,
                elevation: 1.5,
                plinegen: false,
            }
        };
        let polylines: Vec<_> =
            d.model.iter().filter_map(|e| if let EntityKind::LwPolyline(p) = &e.kind { Some((p, e.common.extrusion.z)) } else { None }).collect();
        let expected = [(written(false, 0.0, 1.0), 1.0), (written(true, 20.0, 1.0), 1.0), (written(false, 40.0, -1.0), -1.0)];
        assert_eq!(polylines.len(), 3, "{version:?}");
        for ((got, z), (want, wz)) in polylines.iter().zip(&expected) {
            assert_eq!(*got, want, "{version:?}");
            assert_eq!(*z, *wz, "{version:?}: extrusion");
        }
        for t in ["POLYLINE", "VERTEX", "SEQEND", "SPLINE", "polyline", "vertex"] {
            assert!(!unread.by_type.keys().any(|k| k.starts_with(t)), "{version:?}: {t} left unread: {}", unread.summary());
        }
    }
}

/// A drawing with one dimension of every kind: linear (rotated, with oblique extension lines and an
/// override), aligned with text the user placed and wrote, both angular kinds, radius, diameter,
/// both ordinate kinds, and a mirrored linear one.
fn dimensions_drawing() -> draftcraft_doc::Drawing {
    use draftcraft_doc::*;
    use draftcraft_geom::Vec3;
    let p = |x: f64, y: f64| Vec3::new(x, y, 0.0);
    let dim = |kind: DimKind, defpt: Vec3, text_mid: Vec3, p13: Vec3, p14: Vec3, p15: Vec3, p16: Vec3| Dimension {
        kind,
        defpt,
        text_mid,
        p13,
        p14,
        p15,
        p16,
        text: String::new(),
        style: "Standard".into(),
        measurement: 0.0,
        text_rotation: 0.0,
        user_text_pos: false,
        block: None,
        insertion_point: Vec3::ZERO,
        overrides: Default::default(),
        assoc: Vec::new(),
        marks: Default::default(),
    };
    let mut d = Drawing::new_imperial();
    let mut linear = dim(DimKind::Linear { rotation: 0.3 }, p(10.0, 5.0), p(5.0, 5.0), p(0.0, 0.0), p(10.0, 2.0), Vec3::ZERO, Vec3::ZERO);
    linear.marks.oblique = Some(0.3 + 1.0);
    linear.overrides.insert("textHeight".into(), serde_json::json!(0.3));
    let mut aligned = dim(DimKind::Aligned, p(32.0, 6.0), p(28.0, 8.0), p(20.0, 0.0), p(30.0, 4.0), Vec3::ZERO, Vec3::ZERO);
    aligned.text = "<> typ.".into();
    aligned.user_text_pos = true;
    let kinds = vec![
        linear,
        aligned,
        dim(DimKind::Angular, p(45.0, 8.0), p(46.0, 9.0), p(40.0, 0.0), p(48.0, 0.0), p(40.0, 0.0), p(46.0, 6.0)),
        dim(DimKind::Angular3P, p(58.0, 8.0), p(58.0, 9.0), p(55.0, 0.0), p(62.0, 3.0), p(54.0, 2.0), Vec3::ZERO),
        dim(DimKind::Radius, p(70.0, 0.0), p(73.0, 2.0), Vec3::ZERO, Vec3::ZERO, p(74.0, 3.0), Vec3::ZERO),
        dim(DimKind::Diameter, p(80.0, 0.0), p(84.0, 3.0), Vec3::ZERO, Vec3::ZERO, p(86.0, 0.0), Vec3::ZERO),
        dim(DimKind::Ordinate { x_type: true }, p(0.0, 20.0), p(5.0, 30.0), p(5.0, 22.0), p(5.0, 28.0), Vec3::ZERO, Vec3::ZERO),
        dim(DimKind::Ordinate { x_type: false }, p(0.0, 20.0), p(15.0, 25.0), p(8.0, 25.0), p(14.0, 25.0), Vec3::ZERO, Vec3::ZERO),
    ];
    for k in kinds {
        d.add(&Space::Model, Common::default(), EntityKind::Dimension(k)).expect("add");
    }
    d
}

#[test]
fn every_kind_of_dimension_reads_as_the_acadrust_reader_reads_it() {
    use draftcraft_doc::EntityKind;
    use draftcraft_io::{DwgReader, SaveOptions, Version, read_dwg_with, write_with};
    let source = dimensions_drawing();
    for version in [Version::R2000, Version::R2004, Version::R2010, Version::R2013, Version::R2018] {
        let bytes = write_with(&source, "t.dwg", &SaveOptions { version: Some(version), binary: false }).unwrap();
        let (d, unread) = dwg::read(&bytes).unwrap_or_else(|e| panic!("{version:?}: {e}"));
        let reference = read_dwg_with(&bytes, DwgReader::Acadrust).unwrap();
        let dims = |d: &draftcraft_doc::Drawing| d.model.iter().filter(|e| matches!(e.kind, EntityKind::Dimension(_))).cloned().collect::<Vec<_>>();
        let (got, want) = (dims(&d), dims(&reference));
        assert_eq!(want.len(), 8, "{version:?}");
        assert_eq!(got.len(), want.len(), "{version:?}: unread {}", unread.summary());
        for (ge, we) in got.iter().zip(&want) {
            let (mut g, mut w) = (serde_json::to_value(ge.as_ref()).unwrap(), serde_json::to_value(we.as_ref()).unwrap());
            // DWG stores anonymous blocks unnumbered: this reader numbers them as AutoCAD does in a
            // DXF (*D0 first), acadrust from *D1. The dimension's block must hold the same.
            let block = |d: &draftcraft_doc::Drawing, v: &serde_json::Value| {
                let b = d.blocks.get(v["kind"]["block"].as_str().unwrap_or_default());
                b.map(|b| b.entities.iter().map(|e| serde_json::to_value(&e.kind).unwrap()).collect::<Vec<_>>())
            };
            let (gb, wb) = (block(&d, &g), block(&reference, &w));
            assert!(wb.is_some(), "{version:?}: the reference's dimension has a block");
            assert!(close(&serde_json::json!(gb), &serde_json::json!(wb), false), "{version:?}: the dimension's block");
            g["kind"]["block"] = serde_json::Value::Null;
            w["kind"]["block"] = serde_json::Value::Null;
            assert!(close(&g["kind"], &w["kind"], false), "{version:?}\nread      {}\nreference {}", g["kind"], w["kind"]);
            assert!(close(&g["common"], &w["common"], false), "{version:?}: properties\nread      {}\nreference {}", g["common"], w["common"]);
        }
        assert!(!unread.by_type.keys().any(|k| k.starts_with("DIMENSION")), "{version:?}: {}", unread.summary());
    }
}

/// A drawing with hatches of every kind of boundary and fill: a solid fill whose outer loop has a
/// bulge and an island, a scaled and rotated pattern with its own origin, a pattern the library
/// doesn't know (its definition lines kept), a gradient, a background colour, and an associative
/// hatch whose loop names the circle it was made from.
fn hatches_drawing() -> draftcraft_doc::Drawing {
    use draftcraft_doc::library::PatternLine;
    use draftcraft_doc::*;
    use draftcraft_geom::{PolyVertex, Vec2};
    let v = |x: f64, y: f64| PolyVertex::new(Vec2::new(x, y));
    let square = |x: f64, y: f64, s: f64| vec![v(x, y), v(x + s, y), v(x + s, y + s), v(x, y + s)];
    let hatch = |pattern: &str, loops: Vec<HatchLoop>| Hatch {
        pattern: pattern.into(),
        solid: pattern == "SOLID",
        loops,
        scale: 1.0,
        angle: 0.0,
        associative: false,
        style: 0,
        elevation: 0.0,
        gradient: None,
        origin: Vec2::ZERO,
        background: None,
        pattern_lines: Vec::new(),
    };
    let outer = |vertices: Vec<PolyVertex>| HatchLoop { vertices, outer: true, sources: Vec::new() };
    let mut d = Drawing::new_imperial();
    let circle = d
        .add(&Space::Model, Common::default(), EntityKind::Circle(Circle { center: draftcraft_geom::Vec3::new(60.0, 5.0, 0.0), radius: 4.0 }))
        .expect("circle");
    let mut solid_loop = square(0.0, 0.0, 10.0);
    solid_loop[1].bulge = 0.4;
    let mut solid = hatch("SOLID", vec![outer(solid_loop), HatchLoop { vertices: square(3.0, 3.0, 2.0), outer: false, sources: Vec::new() }]);
    solid.elevation = 1.5;
    let mut ansi = hatch("ANSI31", vec![outer(square(15.0, 0.0, 10.0))]);
    ansi.scale = 2.0;
    ansi.angle = 0.5;
    ansi.origin = Vec2::new(1.0, 2.0);
    ansi.style = 1;
    ansi.background = Some(color::Color::Index(3));
    let mut own = hatch("MYGRID", vec![outer(square(30.0, 0.0, 10.0))]);
    own.pattern_lines = vec![
        PatternLine { angle: 0.0, origin: (0.0, 0.0), delta: (0.0, 0.5), dashes: vec![0.25, -0.25] },
        PatternLine { angle: 90.0, origin: (0.1, 0.0), delta: (0.0, 0.5), dashes: Vec::new() },
    ];
    own.scale = 1.5;
    let mut gradient = hatch("SOLID", vec![outer(square(45.0, 0.0, 10.0))]);
    gradient.gradient = Some(Gradient {
        name: "CYLINDER".into(),
        color1: color::Color::Index(1),
        color2: color::Color::True(color::Rgb(10, 200, 30)),
        angle: 0.25,
        centered: false,
    });
    let mut associative = hatch("SOLID", vec![HatchLoop { vertices: Vec::new(), outer: true, sources: vec![circle] }]);
    associative.associative = true;
    if let Some(l) = associative.loops.first_mut() {
        l.vertices = vec![PolyVertex::with_bulge(Vec2::new(56.0, 5.0), 1.0), PolyVertex::with_bulge(Vec2::new(64.0, 5.0), 1.0)];
    }
    for h in [solid, ansi, own, gradient, associative] {
        d.add(&Space::Model, Common::default(), EntityKind::Hatch(h)).expect("add");
    }
    d
}

#[test]
fn hatches_read_as_the_acadrust_reader_reads_them() {
    use draftcraft_doc::EntityKind;
    use draftcraft_io::{DwgReader, SaveOptions, Version, read_dwg_with, write_with};
    let source = hatches_drawing();
    for version in [Version::R2000, Version::R2004, Version::R2010, Version::R2013, Version::R2018] {
        let bytes = write_with(&source, "t.dwg", &SaveOptions { version: Some(version), binary: false }).unwrap();
        let (d, unread) = dwg::read(&bytes).unwrap_or_else(|e| panic!("{version:?}: {e}"));
        let reference = read_dwg_with(&bytes, DwgReader::Acadrust).unwrap();
        let hatches = |d: &draftcraft_doc::Drawing| d.model.iter().filter(|e| matches!(e.kind, EntityKind::Hatch(_))).cloned().collect::<Vec<_>>();
        let (got, want) = (hatches(&d), hatches(&reference));
        assert_eq!(want.len(), 5, "{version:?}");
        assert_eq!(got.len(), want.len(), "{version:?}: unread {}", unread.summary());
        for (ge, we) in got.iter().zip(&want) {
            let (g, w) = (serde_json::to_value(ge.as_ref()).unwrap(), serde_json::to_value(we.as_ref()).unwrap());
            assert!(close(&g["kind"], &w["kind"], false), "{version:?}\nread      {}\nreference {}", g["kind"], w["kind"]);
            assert!(close(&g["common"], &w["common"], false), "{version:?}: properties\nread      {}\nreference {}", g["common"], w["common"]);
        }
        assert!(!unread.by_type.contains_key("HATCH"), "{version:?}: {}", unread.summary());
    }
}

/// Hatches whose loops are edges, written through acadrust's own API (DraftCraft writes polyline
/// loops): lines, a counter-clockwise and clockwise arcs, an elliptic arc ending on its axes, and
/// a spline, with seed points, a derived loop (pixel size) and a pattern's own definition lines.
fn hatch_edges_dwg(version: acadrust::types::DxfVersion) -> Vec<u8> {
    use acadrust::entities::*;
    use acadrust::types::Vector2;
    use std::f64::consts::{FRAC_PI_2, PI};
    let mut doc = acadrust::CadDocument::with_version(version);
    let p = |x: f64, y: f64| Vector2::new(x, y);
    let line = |a: Vector2, b: Vector2| BoundaryEdge::Line(LineEdge { start: a, end: b });
    let arc = |c: Vector2, r: f64, s: f64, e: f64, ccw: bool| {
        BoundaryEdge::CircularArc(CircularArcEdge { center: c, radius: r, start_angle: s, end_angle: e, counter_clockwise: ccw })
    };
    // Line, arc, line, half an ellipse (its major axis up, so it bows out to the left).
    let mut a = BoundaryPath::with_flags(BoundaryPathFlags::EXTERNAL);
    a.add_edge(line(p(100.0, 0.0), p(110.0, 0.0)));
    a.add_edge(arc(p(110.0, 5.0), 5.0, -FRAC_PI_2, FRAC_PI_2, true));
    a.add_edge(line(p(110.0, 10.0), p(100.0, 10.0)));
    a.add_edge(BoundaryEdge::EllipticArc(EllipticArcEdge {
        center: p(100.0, 5.0),
        major_axis_endpoint: p(0.0, 5.0),
        minor_axis_ratio: 0.4,
        start_angle: 0.0,
        end_angle: PI,
        counter_clockwise: true,
    }));
    // A spline and the line that closes it.
    let mut b = BoundaryPath::with_flags(BoundaryPathFlags::EXTERNAL);
    b.add_edge(BoundaryEdge::Spline(SplineEdge {
        degree: 3,
        rational: false,
        periodic: false,
        knots: vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
        control_points: vec![
            acadrust::types::Vector3::new(120.0, 0.0, 1.0),
            acadrust::types::Vector3::new(122.0, 5.0, 1.0),
            acadrust::types::Vector3::new(128.0, 5.0, 1.0),
            acadrust::types::Vector3::new(130.0, 0.0, 1.0),
        ],
        fit_points: Vec::new(),
        start_tangent: Vector2::ZERO,
        end_tangent: Vector2::ZERO,
    }));
    b.add_edge(line(p(130.0, 0.0), p(120.0, 0.0)));
    // A circle as two clockwise arcs, derived (a pixel size follows the loops).
    let mut c = BoundaryPath::with_flags(BoundaryPathFlags::from_bits(1 | 4));
    c.add_edge(arc(p(140.0, 5.0), 5.0, 0.0, PI, false));
    c.add_edge(arc(p(140.0, 5.0), 5.0, PI, 2.0 * PI, false));
    let mut solid = Hatch::solid();
    solid.add_path(a);
    solid.add_seed_point(p(105.0, 5.0));
    doc.add_entity(EntityType::Hatch(Box::new(solid))).unwrap();
    let mut spline = Hatch::solid();
    spline.add_path(b);
    doc.add_entity(EntityType::Hatch(Box::new(spline))).unwrap();
    let mut pattern = HatchPattern::new("STRIPES");
    pattern.add_line(HatchPatternLine { angle: 0.3, base_point: p(140.0, 5.0), offset: p(-0.2, 0.6), dash_lengths: vec![0.5, -0.25] });
    pattern.add_line(HatchPatternLine { angle: 0.3 + FRAC_PI_2, base_point: p(140.0, 5.0), offset: p(0.0, 1.0), dash_lengths: Vec::new() });
    let mut patterned = Hatch::with_pattern(pattern);
    patterned.is_solid = false;
    patterned.pattern_type = HatchPatternType::Custom;
    patterned.pattern_angle = 0.3;
    patterned.pattern_scale = 2.0;
    patterned.pixel_size = 0.05;
    patterned.add_path(c);
    patterned.add_seed_point(p(140.0, 5.0));
    patterned.add_seed_point(p(141.0, 5.0));
    doc.add_entity(EntityType::Hatch(Box::new(patterned))).unwrap();
    acadrust::DwgWriter::write_to_vec(&doc).unwrap()
}

#[test]
fn hatch_edges_read_as_the_acadrust_reader_reads_them() {
    use acadrust::types::DxfVersion as A;
    use draftcraft_doc::EntityKind;
    use draftcraft_io::{DwgReader, read_dwg_with};
    for version in [A::AC1015, A::AC1018, A::AC1024, A::AC1027, A::AC1032] {
        let bytes = hatch_edges_dwg(version);
        let (d, unread) = dwg::read(&bytes).unwrap_or_else(|e| panic!("{version:?}: {e}"));
        let reference = read_dwg_with(&bytes, DwgReader::Acadrust).unwrap();
        let hatches = |d: &draftcraft_doc::Drawing| d.model.iter().filter(|e| matches!(e.kind, EntityKind::Hatch(_))).cloned().collect::<Vec<_>>();
        let (got, want) = (hatches(&d), hatches(&reference));
        assert_eq!(want.len(), 3, "{version:?}");
        assert_eq!(got.len(), want.len(), "{version:?}: unread {}", unread.summary());
        for (ge, we) in got.iter().zip(&want) {
            let (g, w) = (serde_json::to_value(ge.as_ref()).unwrap(), serde_json::to_value(we.as_ref()).unwrap());
            assert!(close(&g["kind"], &w["kind"], false), "{version:?}\nread      {}\nreference {}", g["kind"], w["kind"]);
            let EntityKind::Hatch(h) = &ge.kind else { unreachable!() };
            assert!(h.loops.iter().all(|l| l.vertices.len() >= 2), "{version:?}: every edge loop has its vertices");
        }
        let EntityKind::Hatch(patterned) = &got[2].kind else { unreachable!() };
        assert_eq!(patterned.pattern_lines.len(), 2, "{version:?}");
        assert!(!unread.by_type.contains_key("HATCH"), "{version:?}: {}", unread.summary());
    }
}
