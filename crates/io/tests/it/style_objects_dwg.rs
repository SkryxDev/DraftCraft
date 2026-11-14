//! The style objects AutoCAD puts in every drawing (visual styles, materials, section and
//! detail view styles) through DWG saves: AutoCAD's own objects, read from its DXF of one
//! version and saved as a DWG of another, read back as AutoCAD's DXF of that version gives
//! them. Each type changed layout over the versions (visual styles twice, in 2010 and 2013; the
//! view styles in 2018), and colours AutoCAD's DXF before 2004 leaves out are its defaults.
//!
//! A drawing from a 2000 file has no true colours, only their indexes. AutoCAD's DWG of 2000 to
//! 2007 holds one value its DXF doesn't: the face lighting quality of
//! the Realistic visual style, 2 where the DXF has 3.

use draftcraft_io::{DwgReader, Version, read_dwg_with, read_dxf};

const DXF: [(&str, Version, &[u8]); 5] = [
    ("2000", Version::R2000, include_bytes!("../data/mi201/mi201-r2000.dxf")),
    ("2004", Version::R2004, include_bytes!("../data/mi201/mi201-r2004.dxf")),
    ("2010", Version::R2010, include_bytes!("../data/mi201/mi201-r2010.dxf")),
    ("2013", Version::R2013, include_bytes!("../data/mi201/mi201-r2013.dxf")),
    ("2018", Version::R2018, include_bytes!("../data/mi201/mi201-r2018.dxf")),
];

const KINDS: [(&str, usize); 4] = [("VISUALSTYLE", 24), ("MATERIAL", 3), ("ACDBSECTIONVIEWSTYLE", 1), ("ACDBDETAILVIEWSTYLE", 1)];

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

type Objects = std::collections::BTreeMap<String, Vec<(i32, Value)>>;

/// The objects of type `kind`, by name (group 1, 2 or 3): their groups from the subclass marker
/// to the extended data, references aside (a save numbers the objects anew).
fn objects(d: &draftcraft_doc::Drawing, kind: &str) -> Objects {
    d.foreign
        .objects
        .iter()
        .filter(|o| o.kind.eq_ignore_ascii_case(kind))
        .map(|o| {
            let from = o.tags.iter().position(|t| t.code == 100).unwrap_or(o.tags.len());
            let groups = o.tags[from..]
                .iter()
                .take_while(|t| t.code < 1000)
                .filter(|t| !(330..=369).contains(&t.code))
                .map(|t| (t.code, t.value.trim().parse::<f64>().map_or_else(|_| Value::Text(t.value.clone()), Value::Number)))
                .collect();
            let name = o.tags.iter().find(|t| matches!(t.code, 1..=3)).map(|t| t.value.clone()).unwrap_or_default();
            (name, groups)
        })
        .collect()
}

/// `got` and `want` alike: names the first object and group that differ.
fn same(got: &Objects, want: &Objects, context: &str) {
    for (name, groups) in want {
        let Some(g) = got.get(name) else { panic!("{context}: no {name} among {:?}", got.keys().collect::<Vec<_>>()) };
        if let Some(i) = (0..groups.len().max(g.len())).find(|i| g.get(*i) != groups.get(*i)) {
            panic!("{context}: {name}, group {i}: {:?} where AutoCAD has {:?}", g.get(i), groups.get(i));
        }
    }
    assert_eq!(got.len(), want.len(), "{context}: {:?}", got.keys().collect::<Vec<_>>());
}

#[test]
fn style_objects_take_the_layout_of_the_version_saved() {
    let expected: Vec<_> = DXF.iter().map(|(_, _, dxf)| read_dxf(dxf).unwrap()).collect();
    for (source, _, dxf) in DXF {
        let d = read_dxf(dxf).unwrap();
        for ((target, version, _), reference) in DXF.iter().zip(&expected) {
            let (dwg, unwritten) = draftcraft_io::write_dwg_own(&d, *version).unwrap();
            let back = read_dwg_with(&dwg, DwgReader::Direct).unwrap();
            for (kind, count) in KINDS {
                let context = format!("{source} saved as {target}: {kind}");
                assert!(!unwritten.contains_key(kind), "{context}: {unwritten:?}");
                let mut want = objects(reference, kind);
                assert_eq!(want.len(), count, "{context}");
                let mut got = objects(&back, kind);
                // A 2000 file has no true colours: the colours it gives are their indexes.
                if source == "2000" {
                    for groups in want.values_mut().chain(got.values_mut()) {
                        groups.retain(|(code, _)| !(420..=427).contains(code));
                    }
                }
                same(&got, &want, &context);
            }
        }
    }
}

/// AutoCAD's own DWG files read to the groups its DXF of the same version gives; the cell
/// style map, which its DXF of every version has, is in its DWG only up to 2007.
#[test]
fn autocads_style_objects_read_as_its_dxf_gives_them() {
    for (name, dwg, dxf) in [
        ("2000", include_bytes!("../data/mi201/mi201-r2000.dwg").as_slice(), DXF[0].2),
        ("2004", include_bytes!("../data/mi201/mi201-r2004.dwg").as_slice(), DXF[1].2),
        ("2007", include_bytes!("../data/mi201/mi201-r2007.dwg").as_slice(), include_bytes!("../data/mi201/mi201-r2007.dxf").as_slice()),
        ("2010", include_bytes!("../data/mi201/mi201-r2010.dwg").as_slice(), DXF[2].2),
        ("2013", include_bytes!("../data/mi201/mi201-r2013.dwg").as_slice(), DXF[3].2),
        ("2018", include_bytes!("../data/mi201/mi201-r2018.dwg").as_slice(), DXF[4].2),
    ] {
        let theirs = read_dwg_with(dwg, DwgReader::Direct).unwrap();
        let tags = draftcraft_dxf::parse(dxf).unwrap();
        let d = read_dxf(dxf).unwrap();
        for (kind, _) in KINDS {
            let mut want = objects(&d, kind);
            if kind == "VISUALSTYLE" && matches!(name, "2000" | "2004" | "2007") {
                want.remove("Realistic");
                let mut got = objects(&theirs, kind);
                got.remove("Realistic");
                same(&got, &want, &format!("{name}: {kind}"));
                continue;
            }
            same(&objects(&theirs, kind), &want, &format!("{name}: {kind}"));
        }
        // The readers model the table style and leave its map out (nothing is lost): the codec's
        // groups of AutoCAD's map, against its DXF's.
        let maps = cell_style_maps(dwg);
        if matches!(name, "2000" | "2004" | "2007") {
            assert_eq!(maps.len(), 1, "{name}");
            let want: Objects = [("map".to_string(), cell_style_map(&tags))].into_iter().collect();
            let got: Objects = maps.into_iter().map(|m| ("map".to_string(), m)).collect();
            same(&got, &want, &format!("{name}: CELLSTYLEMAP"));
        } else {
            assert!(maps.is_empty(), "{name}");
        }
        assert!(!theirs.read_notes.iter().any(|n| n.contains("CELLSTYLEMAP")), "{name}: {:?}", theirs.read_notes);
    }
}

/// The cell style maps of a DWG file, as the DWG codec decodes them, references aside.
fn cell_style_maps(dwg: &[u8]) -> Vec<Vec<(i32, Value)>> {
    use draftcraft_dwg::file::{DwgFile, NonGraphic, XrValue};
    let file = DwgFile::open(dwg).unwrap();
    let objects: Vec<_> = file.objects().filter(|o| o.type_name == "CELLSTYLEMAP").collect();
    objects
        .iter()
        .map(|o| {
            let mut r = file.read_object(o).unwrap();
            let Some(Ok((NonGraphic::Groups(g), _))) = r.non_graphic(&[]) else { panic!("CELLSTYLEMAP {:X} not decoded", o.handle) };
            g.items
                .iter()
                .filter(|(code, _)| !(330..=369).contains(code))
                .map(|(code, v)| {
                    let value = match v {
                        XrValue::Str(s) => Value::Text(s.clone()),
                        XrValue::Real(f) => Value::Number(*f),
                        XrValue::Int8(i) => Value::Number(f64::from(*i)),
                        XrValue::Int16(i) => Value::Number(f64::from(*i)),
                        XrValue::Int32(i) => Value::Number(f64::from(*i)),
                        XrValue::Int64(i) => Value::Number(*i as f64),
                        XrValue::Bool(b) => Value::Number(f64::from(u8::from(*b))),
                        other => Value::Text(format!("{other:?}")),
                    };
                    (i32::from(*code), value)
                })
                .collect()
        })
        .collect()
}

/// The groups of the cell style map in a DXF file's groups, as [`objects`] gives them.
fn cell_style_map(tags: &[draftcraft_dxf::Tag]) -> Vec<(i32, Value)> {
    let is = |t: &draftcraft_dxf::Tag, s: &str| matches!(&t.value, draftcraft_dxf::Value::Str(v) if v == s);
    let start = tags.iter().position(|t| t.code == 0 && is(t, "CELLSTYLEMAP")).expect("a cell style map");
    let body: Vec<_> = tags[start + 1..].iter().take_while(|t| t.code != 0).collect();
    let from = body.iter().position(|t| t.code == 100).unwrap_or(body.len());
    body[from..]
        .iter()
        .take_while(|t| t.code < 1000)
        .filter(|t| !(330..=369).contains(&t.code))
        .map(|t| {
            let value = match &t.value {
                draftcraft_dxf::Value::Str(s) => s.trim().parse::<f64>().map_or_else(|_| Value::Text(s.clone()), Value::Number),
                draftcraft_dxf::Value::Real(f) => Value::Number(*f),
                draftcraft_dxf::Value::Int(i) => Value::Number(*i as f64),
                draftcraft_dxf::Value::Bool(b) => Value::Number(f64::from(u8::from(*b))),
                draftcraft_dxf::Value::Hex(h) => Value::Text(h.clone()),
            };
            (t.code, value)
        })
        .collect()
}
