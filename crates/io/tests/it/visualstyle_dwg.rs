//! Visual styles go from any of their three layouts (up to AutoCAD 2007, 2010, from 2013) to the
//! layout of the version a drawing is saved in: AutoCAD's own styles, read from its DXF of one
//! version and saved as a DWG of another, read back as the styles AutoCAD wrote in its DXF of
//! that version. (Its DWG of 2004 holds one value its DXF doesn't: the face lighting quality of
//! Realistic, 2 where the DXF has 3.)

use draftcraft_io::{DwgReader, Version, read_dwg_with, read_dxf};

const DXF_2004: &[u8] = include_bytes!("../data/mi201/mi201-r2004.dxf");
const DXF_2010: &[u8] = include_bytes!("../data/mi201/mi201-r2010.dxf");
const DXF_2018: &[u8] = include_bytes!("../data/mi201/mi201-r2018.dxf");

/// A value as text, or as a number when it reads as one.
#[derive(Debug)]
enum Value {
    Number(f64),
    Text(String),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Number(a), Value::Number(b)) => (a - b).abs() <= 1e-9 * a.abs().max(1.0),
            (Value::Text(a), Value::Text(b)) => a == b,
            _ => false,
        }
    }
}

/// The visual styles of a drawing, by name: their groups from the subclass marker to the
/// extended data.
fn styles(d: &draftcraft_doc::Drawing) -> std::collections::BTreeMap<String, Vec<(i32, Value)>> {
    d.foreign
        .objects
        .iter()
        .filter(|o| o.kind.eq_ignore_ascii_case("VISUALSTYLE"))
        .map(|o| {
            let from = o.tags.iter().position(|t| t.code == 100).unwrap_or(o.tags.len());
            let groups: Vec<(i32, Value)> = o.tags[from..]
                .iter()
                .take_while(|t| t.code < 1000)
                .map(|t| (t.code, t.value.trim().parse::<f64>().map_or_else(|_| Value::Text(t.value.clone()), Value::Number)))
                .collect();
            let name = o.tags.iter().find(|t| t.code == 2).map(|t| t.value.clone()).unwrap_or_default();
            (name, groups)
        })
        .collect()
}

#[test]
fn visual_styles_take_the_layout_of_the_version_saved() {
    for (target, version, reference) in [("2004", Version::R2004, DXF_2004), ("2010", Version::R2010, DXF_2010), ("2018", Version::R2018, DXF_2018)] {
        let expected = styles(&read_dxf(reference).unwrap());
        assert_eq!(expected.len(), 24, "{target}");
        for (source, dxf) in [("2004", DXF_2004), ("2010", DXF_2010), ("2018", DXF_2018)] {
            let d = read_dxf(dxf).unwrap();
            assert_eq!(styles(&d).len(), 24, "{source}");
            let (dwg, unwritten) = draftcraft_io::write_dwg_own(&d, version).unwrap();
            assert!(!unwritten.contains_key("VISUALSTYLE"), "{source} as {target}: {unwritten:?}");
            let back = styles(&read_dwg_with(&dwg, DwgReader::Direct).unwrap());
            assert_eq!(back.len(), 24, "{source} as {target}");
            for (name, groups) in &expected {
                assert_eq!(back.get(name), Some(groups), "{source} as {target}: {name}");
            }
        }
    }
}
