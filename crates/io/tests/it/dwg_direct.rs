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
        for t in ["LINE", "CIRCLE", "ARC", "TEXT", "DIMENSION_LINEAR", "BLOCK_HEADER", "DICTIONARY"] {
            assert!(unread.by_type.contains_key(t), "{name}: no {t} among {}", unread.summary());
        }
    }
}

/// The object types the reader puts in the drawing so far.
const READ_SO_FAR: &[&str] = &["APPID", "STYLE", "LTYPE", "LAYER", "DIMSTYLE", "UCS", "VIEW", "VPORT"];

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
