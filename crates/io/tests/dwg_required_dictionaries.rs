//! #1813: AutoCAD always writes an `AcDbVariableDictionary` in the named object dictionary;
//! ezdxf, and DraftCraft's own DXF writer, do not. Vectric Aspire 10.5 reads an AC1018 DWG without
//! it as empty ("No data was imported..."), though it reads the same drawing as DXF (and as an
//! AC1032 DWG) fine. The DWG writer adds the dictionary AutoCAD writes, with its usual variables,
//! when the drawing has none.
#![cfg(not(target_arch = "wasm32"))]

use draftcraft_doc::{Common, Drawing, EntityKind, LwPolyline, Space};
use draftcraft_geom::{PolyVertex, Vec2};
use draftcraft_io::{SaveOptions, Version, write_with};

/// ezdxf's rectangle: one closed LWPOLYLINE in model space.
fn rectangle() -> Drawing {
    let mut d = Drawing::new_metric();
    let corners = [(0.0, 0.0), (4.0, 0.0), (4.0, 2.0), (0.0, 2.0)];
    let pl = LwPolyline {
        vertices: corners.iter().map(|(x, y)| PolyVertex::new(Vec2::new(*x, *y))).collect(),
        closed: true,
        const_width: 0.0,
        elevation: 0.0,
        plinegen: false,
    };
    d.add(&Space::Model, Common::default(), EntityKind::LwPolyline(pl)).expect("add the rectangle");
    d
}

/// The drawing variables in the named object dictionary, with their values, or nothing when the
/// dictionary is missing. Dictionary keys are case-insensitive in AutoCAD; its own spelling is
/// `AcDbVariableDictionary` (the DXF Reference calls it the ACDBVARIABLEDICTIONARY).
fn variable_dictionary(doc: &draftcraft_dwg::records::Drawing) -> Option<Vec<(String, String)>> {
    use draftcraft_dwg::records::model::{NonGraphic, ObjectKind};
    let dictionary = |h| match doc.objects.get(h).map(|o| &o.kind) {
        Some(ObjectKind::NonGraphic(NonGraphic::Dictionary(d))) => Some(d),
        _ => None,
    };
    let root = dictionary(&doc.header.named_objects_dict)?;
    let vars = root.entries.iter().find(|(k, _)| k.eq_ignore_ascii_case("AcDbVariableDictionary")).and_then(|(_, h)| dictionary(h))?;
    Some(
        vars.entries
            .iter()
            .filter_map(|(name, handle)| match doc.objects.get(handle).map(|o| &o.kind) {
                Some(ObjectKind::NonGraphic(NonGraphic::DictionaryVar { value, .. })) => Some((name.clone(), value.clone())),
                _ => None,
            })
            .collect(),
    )
}

/// Read a DWG back as its object records, so the assertions inspect the objects the file holds
/// rather than the drawing DraftCraft builds from them.
fn read_dwg(dwg: &[u8]) -> draftcraft_dwg::records::Drawing {
    draftcraft_dwg::read_bytes(dwg).expect("read the DWG back")
}

/// An integer header variable of the DWG, by its name.
fn header_int(dwg: &[u8], name: &str) -> Option<i64> {
    let file = draftcraft_dwg::file::DwgFile::open(dwg).ok()?;
    file.header().ok()?.vars.into_iter().find(|(n, _)| *n == name).and_then(|(_, v)| match v {
        draftcraft_dwg::file::HeaderValue::Int(i) => Some(i),
        _ => None,
    })
}

/// An AutoCAD 2004 DWG of the drawing in `dxf`.
fn ac1018(dxf: &[u8]) -> Vec<u8> {
    let d = draftcraft_io::read_dxf(dxf).expect("read the DXF");
    write_with(&d, "case.dwg", &SaveOptions { version: Some(Version::R2004), binary: false }).expect("write the DWG")
}

/// The value of the variable called `name`, matched case-insensitively as AutoCAD matches
/// dictionary keys, or nothing when the dictionary has no such entry.
fn value<'a>(vars: &'a [(String, String)], name: &str) -> Option<&'a str> {
    vars.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

/// #1813's regression: a DXF written without an `AcDbVariableDictionary` (as ezdxf and DraftCraft
/// write them) must gain AutoCAD's dictionary when exported as an AC1018 DWG, so readers that
/// require it see the drawing's contents; the added objects must not break DraftCraft's own read.
#[test]
fn an_ac1018_dwg_gets_the_variable_dictionary_autocad_always_writes() {
    let d = rectangle();
    let dxf = write_with(&d, "rect.dxf", &SaveOptions { version: Some(Version::R2000), binary: false }).expect("write the DXF");
    let text = String::from_utf8_lossy(&dxf).to_ascii_lowercase();
    assert!(!text.contains("acdbvariabledictionary"), "this DXF is one without the dictionary, like ezdxf's");

    let dwg = write_with(&d, "rect.dwg", &SaveOptions { version: Some(Version::R2004), binary: false }).expect("write the DWG");
    assert!(dwg.starts_with(b"AC1018"), "the DWG is AutoCAD 2004");
    let doc = read_dwg(&dwg);
    let vars = variable_dictionary(&doc).expect("AC1018 DWG: the named object dictionary has an AcDbVariableDictionary");

    // The variables AutoCAD keeps there, by name (Aspire needs at least the dictionary; these
    // are the ones the reference application writes).
    for (name, want) in [("CMLEADERSTYLE", "Standard"), ("CTABLESTYLE", "Standard"), ("CANNOSCALE", "1:1"), ("DIMASSOC", "2"), ("HIDETEXT", "1")] {
        assert_eq!(value(&vars, name), Some(want), "{name} in {vars:?}");
    }
    assert!(value(&vars, "LAYEREVAL").is_some() && value(&vars, "LAYERNOTIFY").is_some(), "{vars:?}");

    // The added objects don't disturb DraftCraft's own read of the file: the rectangle is there.
    let back = draftcraft_io::read(&dwg, "rect.dwg").expect("DraftCraft reads its own 2004 DWG");
    assert_eq!(back.model.len(), 1);
}

/// The dictionary must carry the drawing's own `$DIMASSOC` and `$HIDETEXT`, not AutoCAD's
/// defaults: a drawing that turned text hiding off or stores dimensions non-associatively keeps
/// those settings for every consumer that reads them from the dictionary.
#[test]
fn the_dictionary_keeps_the_drawings_dimassoc_and_hidetext() {
    let dxf = "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1015\n9\n$DIMASSOC\n70\n1\n9\n$HIDETEXT\n70\n0\n0\nENDSEC\n0\nEOF\n";
    let dwg = ac1018(dxf.as_bytes());
    let doc = read_dwg(&dwg);
    let vars = variable_dictionary(&doc).expect("AC1018 DWG: the named object dictionary has an AcDbVariableDictionary");
    assert_eq!(value(&vars, "DIMASSOC"), Some("1"), "DIMASSOC in {vars:?}");
    assert_eq!(value(&vars, "HIDETEXT"), Some("0"), "HIDETEXT in {vars:?}");
    // The DWG's own header settings match the dictionary, so both kinds of consumer agree.
    assert_eq!(header_int(&dwg, "DIMASSOC"), Some(1));
    assert_eq!(header_int(&dwg, "HIDETEXT"), Some(0));
}

/// A header variable holding a value the variable doesn't accept is written with AutoCAD's
/// default, as a hostile file can put any integer there.
#[test]
fn impossible_header_values_fall_back_to_the_defaults() {
    let dxf = "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1015\n9\n$DIMASSOC\n70\n9\n9\n$HIDETEXT\n70\n7\n0\nENDSEC\n0\nEOF\n";
    let dwg = ac1018(dxf.as_bytes());
    let doc = read_dwg(&dwg);
    let vars = variable_dictionary(&doc).expect("AC1018 DWG: the named object dictionary has an AcDbVariableDictionary");
    assert_eq!(value(&vars, "DIMASSOC"), Some("2"), "DIMASSOC in {vars:?}");
    assert_eq!(value(&vars, "HIDETEXT"), Some("1"), "HIDETEXT in {vars:?}");
    assert_eq!((header_int(&dwg, "DIMASSOC"), header_int(&dwg, "HIDETEXT")), (Some(2), Some(1)), "the header too");
}

/// AutoCAD writes the dictionary in every version, and so does DraftCraft's writer: an AC1032
/// (AutoCAD 2018) DWG has it too.
#[test]
fn an_ac1032_dwg_has_it_too() {
    let d = rectangle();
    let dwg = write_with(&d, "rect.dwg", &SaveOptions { version: Some(Version::R2018), binary: false }).expect("write the DWG");
    assert!(dwg.starts_with(b"AC1032"));
    let doc = read_dwg(&dwg);
    let vars = variable_dictionary(&doc).expect("AC1032 DWG: the named object dictionary has an AcDbVariableDictionary");
    assert_eq!(value(&vars, "CTABLESTYLE"), Some("Standard"), "{vars:?}");
}

/// A drawing that already has the dictionary must keep its own entries and values: conversion
/// only supplies AutoCAD's dictionary when it is absent, rather than merging defaults into one
/// that a producer such as ezdxf's `add_dict_var` already wrote.
#[test]
fn a_variable_dictionary_the_drawing_has_is_left_alone() {
    // A drawing that already has the dictionary keeps its own values and entries (here only
    // CMLEADERSTYLE, as ezdxf's add_dict_var("CMLEADERSTYLE", "Standard") writes it): a DWG
    // export must not add defaults next to them.
    let dxf = "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1015\n0\nENDSEC\n0\nSECTION\n2\nOBJECTS\n\
        0\nDICTIONARY\n5\nC\n330\n0\n100\nAcDbDictionary\n281\n1\n3\nAcDbVariableDictionary\n350\nD0\n\
        0\nDICTIONARY\n5\nD0\n330\nC\n100\nAcDbDictionary\n281\n1\n3\nCMLEADERSTYLE\n350\nD1\n\
        0\nDICTIONARYVAR\n5\nD1\n330\nD0\n100\nDictionaryVariables\n280\n0\n1\nNotes\n0\nENDSEC\n0\nEOF\n";
    let dwg = ac1018(dxf.as_bytes());
    let doc = read_dwg(&dwg);
    let vars = variable_dictionary(&doc).expect("the drawing's dictionary is kept");
    assert_eq!(vars.len(), 1, "{vars:?}");
    assert_eq!(value(&vars, "CMLEADERSTYLE"), Some("Notes"), "{vars:?}");
}
