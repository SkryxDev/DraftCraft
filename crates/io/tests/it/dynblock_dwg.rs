//! Every kind of dynamic block object DraftCraft's DWG writer encodes goes through a DWG save in
//! each version it writes and reads back, by DraftCraft's DWG reader, as the DXF groups it was
//! read from. acadrust's reader, whose layouts AutoCAD's files follow, reads each one too, to its
//! last field.
//!
//! The objects are synthetic, written for this test, in the order AutoCAD's DXF puts their
//! groups, added to the DOOR fixture's dynamic block. A value set's description, which DWG
//! doesn't hold, is left empty.

use draftcraft_doc::RawTag;
use draftcraft_io::{Version, read_dxf};

fn dxf(src: &str) -> String {
    let mut out = String::new();
    for line in src.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (code, value) = line.split_once(' ').unwrap_or((line, ""));
        out.push_str(&format!("{code}\n{value}\n"));
    }
    out
}

fn eval(node: i32) -> String {
    format!("100 AcDbEvalExpr\n90 {node}\n98 33\n99 232\n")
}
fn element(name: &str, node: i32) -> String {
    format!("{}100 AcDbBlockElement\n300 {name}\n98 33\n99 232\n1071 0\n", eval(node))
}
fn parameter(name: &str, node: i32) -> String {
    format!("{}100 AcDbBlockParameter\n280 1\n281 0\n", element(name, node))
}
fn one_point(name: &str, node: i32) -> String {
    format!("{}100 AcDbBlock1PtParameter\n1010 1.5\n1020 2.5\n1030 0\n93 3\n170 1\n91 2\n301 UpdatedX\n171 0\n", parameter(name, node))
}
fn two_point(name: &str, node: i32) -> String {
    format!(
        "{}100 AcDbBlock2PtParameter\n1010 0\n1020 -4\n1030 0\n1011 36\n1021 -4\n1031 0\n170 4\n91 0\n91 1\n91 2\n91 3\n\
         171 1\n92 1\n301 Distance\n172 0\n173 2\n94 2\n303 UpdatedBaseX\n94 3\n303 UpdatedBaseY\n174 0\n177 1\n",
        parameter(name, node)
    )
}
fn constraint(name: &str, node: i32) -> String {
    format!("{}100 AcDbBlockConstraintParameter\n330 0\n", two_point(name, node))
}
fn grip(name: &str, node: i32) -> String {
    format!("{}100 AcDbBlockGrip\n91 1\n92 2\n1010 3\n1020 4\n1030 0\n280 1\n93 5\n", element(name, node))
}
fn action(name: &str, node: i32) -> String {
    format!("{}100 AcDbBlockAction\n70 2\n91 7\n91 8\n71 1\n330 30\n1010 5\n1020 6\n1030 0\n", element(name, node))
}
fn value_set(description: i32, flags: i32, real: i32, count: i32) -> String {
    format!("{description}\n{flags} 2\n{real} 1\n{} 9\n{} 0.5\n{count} 2\n{} 3.25\n{} 4.75\n", real + 1, real + 2, real + 3, real + 3)
}

/// The objects: (handle, type, groups after the owner, a value acadrust's reading must hold).
fn objects() -> Vec<(&'static str, &'static str, String, &'static str)> {
    vec![
        ("80", "BLOCKALIGNMENTGRIP", format!("{}100 AcDbBlockAlignmentGrip\n140 0.25\n141 0.5\n142 0.875\n", grip("AlignGrip", 10)), "0.875"),
        ("81", "BLOCKLINEARGRIP", format!("{}100 AcDbBlockLinearGrip\n140 1\n141 0\n142 0.625\n", grip("LinearGrip", 11)), "0.625"),
        ("82", "BLOCKFLIPGRIP", format!("{}100 AcDbBlockFlipGrip\n140 0\n141 1\n142 0.375\n93 3\n", grip("FlipGrip", 12)), "0.375"),
        ("83", "BLOCKXYGRIP", format!("{}100 AcDbBlockXYGrip\n", grip("XYGrip", 13)), "XYGrip"),
        ("84", "BLOCKALIGNMENTPARAMETER", format!("{}100 AcDbBlockAlignmentParameter\n280 1\n", two_point("Alignment", 14)), "UpdatedBaseY"),
        (
            "85",
            "BLOCKBASEPOINTPARAMETER",
            format!("{}100 AcDbBlockBasepointParameter\n1011 7\n1021 8\n1031 0\n1012 9\n1022 10.125\n1032 0\n", one_point("Base", 15)),
            "10.125",
        ),
        (
            "86",
            "BLOCKFLIPPARAMETER",
            format!(
                "{}100 AcDbBlockFlipParameter\n305 Flip state\n306 Flips the door\n307 Not flipped\n308 Flipped\n\
                 1012 2\n1022 3\n1032 0\n309 FlipTip\n96 1\n",
                two_point("Flip", 16)
            ),
            "FlipTip",
        ),
        ("87", "BLOCKPOINTPARAMETER", format!("{}100 AcDbBlockPointParameter\n303 Position\n304 Where\n1011 4\n1021 5.875\n1031 0\n", one_point("Point", 17)), "5.875"),
        (
            "88",
            "BLOCKPOLARPARAMETER",
            format!(
                "{}100 AcDbBlockPolarParameter\n305 Distance1\n306 How far\n307 Angle1\n308 Which way\n140 0.125\n{}{}",
                two_point("Polar", 18),
                value_set(309, 96, 141, 175),
                value_set(410, 97, 145, 176)
            ),
            "Which way",
        ),
        (
            "89",
            "BLOCKROTATIONPARAMETER",
            format!("{}100 AcDbBlockRotationParameter\n305 Angle2\n306 Turn\n1011 3\n1021 1\n1031 0\n140 0.5\n{}", two_point("Rotation", 19), value_set(307, 96, 141, 175)),
            "Turn",
        ),
        (
            "8A",
            "BLOCKXYPARAMETER",
            format!(
                "{}100 AcDbBlockXYParameter\n305 Vertical\n306 Horizontal\n307 Up\n308 Across\n141 2.5\n140 3.5\n{}{}",
                two_point("XY", 20),
                value_set(309, 97, 146, 176),
                value_set(410, 96, 142, 175)
            ),
            "Across",
        ),
        (
            "8B",
            "BLOCKUSERPARAMETER",
            format!("{}100 AcDbBlockUserParameter\n90 2\n330 0\n301 d1*2\n70 40\n40 6.5\n170 1\n", one_point("User", 21)),
            "d1*2",
        ),
        (
            "8C",
            "BLOCKANGULARCONSTRAINTPARAMETER",
            format!(
                "{}100 AcDbBlockAngularConstraintParameter\n1011 1\n1021 1\n1031 0\n1012 2\n1022 2\n1032 0\n\
                 305 ang1\n306 Opening\n140 0.75\n280 1\n{}",
                constraint("Angular", 22),
                value_set(307, 96, 128, 175)
            ),
            "Opening",
        ),
        (
            "8D",
            "BLOCKHORIZONTALCONSTRAINTPARAMETER",
            format!(
                "{}100 AcDbBlockLinearConstraintParameter\n305 d1\n306 Across the leaf\n140 4.5\n{}100 AcDbBlockHorizontalConstraintParameter\n",
                constraint("Horizontal", 23),
                value_set(307, 96, 128, 175)
            ),
            "Across the leaf",
        ),
        (
            "8E",
            "BLOCKRADIALCONSTRAINTPARAMETER",
            format!("{}100 AcDbBlockRadialConstraintParameter\n305 rad1\n306 Round\n140 1.25\n{}", constraint("Radial", 24), value_set(307, 96, 128, 175)),
            "Round",
        ),
        (
            "8F",
            "BLOCKMOVEACTION",
            format!("{}100 AcDbBlockMoveAction\n92 2\n93 2\n301 UpdatedX\n302 UpdatedY\n140 1\n141 0.4375\n280 0\n", action("Move", 25)),
            "0.4375",
        ),
        (
            "90",
            "BLOCKFLIPACTION",
            format!("{}100 AcDbBlockFlipAction\n92 1\n93 1\n94 1\n95 1\n301 FlipBaseX\n302 FlipBaseY\n303 FlipEndX\n304 FlipEndY\n", action("FlipIt", 26)),
            "FlipEndY",
        ),
        (
            "91",
            "BLOCKROTATEACTION",
            format!(
                "{}100 AcDbBlockActionWithBasePt\n92 2\n93 2\n301 BaseX\n302 BaseY\n1011 1\n1021 2\n1031 0\n280 1\n1012 3\n1022 4\n1032 0\n\
                 100 AcDbBlockRotationAction\n94 2\n303 UpdatedAngle\n",
                action("Rotate", 27)
            ),
            "UpdatedAngle",
        ),
        (
            "92",
            "BLOCKARRAYACTION",
            format!("{}100 AcDbBlockArrayAction\n92 1\n93 1\n94 1\n95 1\n301 A1\n302 B1\n303 C1\n304 D1\n140 12.375\n141 6.625\n", action("Array", 28)),
            "6.625",
        ),
        (
            "93",
            "BLOCKPOLARSTRETCHACTION",
            format!(
                "{}100 AcDbBlockPolarStretchAction\n92 1\n93 1\n94 1\n95 1\n96 1\n97 1\n301 P1\n302 P2\n303 P3\n304 P4\n305 P5\n306 P6\n\
                 73 1\n1011 1\n1021 2\n72 1\n331 30\n74 1\n332 30\n75 1\n76 0\n78 1\n98 4\n79 1\n76 2\n141 1\n140 0.1875\n77 0\n",
                action("PolarStretch", 29)
            ),
            "0.1875",
        ),
        ("94", "BLOCKGRIPLOCATIONCOMPONENT", format!("{}100 AcDbBlockGripExpr\n91 1\n300 UpdatedGripX\n", eval(30)), "UpdatedGripX"),
        ("95", "ACDB_DYNAMICBLOCKPROXYNODE", "100 AcDbEvalExpr\n90 31\n98 33\n99 232\n1\n70 40\n140 2.75\n100 AcDbDynamicBlockProxyNode\n".to_string(), "2.75"),
        (
            "96",
            "ACDBBLOCKPARAMDEPENDENCYBODY",
            "100 AcDbAssocDependencyBody\n90 1\n100 AcDbImpAssocDimDependencyBodyBase\n90 2\n1 width1\n100 AcDbBlockParameterDependencyBody\n90 0\n".to_string(),
            "width1",
        ),
        ("97", "ACDB_BLOCKREPRESENTATION_DATA", "100 AcDbBlockRepresentationData\n70 1\n340 1D\n".to_string(), "1"),
    ]
}

/// The DOOR fixture with the objects above in its dynamic block.
fn drawing() -> draftcraft_doc::Drawing {
    let mut extra = String::new();
    for (handle, kind, groups, _) in objects() {
        extra.push_str(&format!("0 {kind}\n5 {handle}\n330 61\n{groups}"));
    }
    let src = include_str!("../data/dynblock_door.txt").replace("\r\n", "\n");
    let at = src.rfind("0 ENDSEC").unwrap();
    read_dxf(dxf(&format!("{}{extra}{}", &src[..at], &src[at..])).as_bytes()).unwrap()
}

/// An object's groups from its first subclass on, numbers compared by value.
fn groups(tags: &[RawTag]) -> Vec<(i32, String)> {
    let from = tags.iter().position(|t| t.code == 100).unwrap_or(tags.len());
    tags[from..]
        .iter()
        .map(|t| {
            let v = t.value.trim();
            let v = match v.parse::<f64>() {
                Ok(x) if !(300..=309).contains(&t.code) && !(1..=4).contains(&t.code) && t.code != 410 => format!("{x}"),
                _ => v.to_string(),
            };
            (t.code, v)
        })
        .collect()
}

/// The kept dynamic block objects of `d`, each its type and groups. A save may renumber them
/// (the dynamic block tests check the references follow): references are left out.
fn kept(d: &draftcraft_doc::Drawing) -> Vec<(String, Vec<(i32, String)>)> {
    let mut v: Vec<_> = d
        .foreign
        .objects
        .iter()
        .filter(|o| !matches!(o.kind.as_str(), "DICTIONARY" | "DICTIONARYVAR"))
        .map(|o| (o.kind.to_ascii_uppercase(), groups(&o.tags).into_iter().filter(|(c, _)| !matches!(c, 330..=369)).collect()))
        .collect();
    v.sort();
    v
}

#[test]
fn every_dynamic_block_object_survives_each_dwg_version() {
    let d = drawing();
    let before = kept(&d);
    assert_eq!(before.len(), 7 + objects().len());
    for version in [Version::R2000, Version::R2004, Version::R2010, Version::R2013, Version::R2018] {
        let (dwg, unwritten) = draftcraft_io::write_dwg_own(&d, version).unwrap();
        let lost: Vec<_> = before.iter().map(|(k, _)| k.as_str()).filter(|k| unwritten.contains_key(*k)).collect();
        assert!(lost.is_empty(), "{version:?} leaves out {lost:?}");
        let back = draftcraft_io::read_dwg_with(&dwg, draftcraft_io::DwgReader::Direct).unwrap();
        let after = kept(&back);
        // A record the DXF cuts short (the fixture's flip grip) comes back whole: the DWG
        // object holds every field, the missing ones at their defaults.
        for (b, a) in before.iter().zip(&after) {
            assert!(a.0 == b.0 && a.1.starts_with(&b.1), "{version:?}:\n{a:?}\n{b:?}");
        }
        assert_eq!(after.len(), before.len(), "{version:?}: {:?}", after.iter().map(|a| &a.0).collect::<Vec<_>>());
    }
}

/// acadrust reads each object to its end: a field out of place shows as a wrong last value.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn acadrust_reads_each_dynamic_block_object_to_its_last_field() {
    use acadrust::objects::{DynamicBlockData, DynamicBlockObject, ObjectType};
    let d = drawing();
    for version in [Version::R2000, Version::R2010, Version::R2018] {
        let (dwg, _) = draftcraft_io::write_dwg_own(&d, version).unwrap();
        let doc = acadrust::DwgReader::from_stream(std::io::Cursor::new(&dwg)).read().unwrap();
        let read: Vec<(&str, &DynamicBlockData)> = doc
            .objects
            .values()
            .filter_map(|o| match o {
                ObjectType::DynamicBlock(DynamicBlockObject { dxf_name, data, .. }) => Some((dxf_name.as_str(), data)),
                _ => None,
            })
            .collect();
        for (_, kind, _, last) in objects() {
            let of_kind: Vec<String> = read.iter().filter(|(k, _)| k.eq_ignore_ascii_case(kind)).map(|(_, data)| format!("{data:?}")).collect();
            assert!(!of_kind.is_empty(), "{version:?}: no {kind} read");
            assert!(of_kind.iter().any(|t| t.contains(last) && !t.starts_with("Unknown")), "{version:?}: {kind} lacks {last}: {of_kind:?}");
        }
    }
}

/// A damaged file with these objects is an error or a drawing, never a crash: the file with its
/// bytes flipped in turn, and cut short.
#[test]
fn damaged_dynamic_block_objects_do_not_panic() {
    let d = drawing();
    for version in [Version::R2000, Version::R2018] {
        let (dwg, _) = draftcraft_io::write_dwg_own(&d, version).unwrap();
        for at in (0..dwg.len()).step_by(11) {
            let mut b = dwg.clone();
            b[at] ^= 0x5A;
            let _ = draftcraft_io::read_dwg_with(&b, draftcraft_io::DwgReader::Direct);
        }
        for cut in (0..dwg.len()).step_by(211) {
            let _ = draftcraft_io::read_dwg_with(&dwg[..cut], draftcraft_io::DwgReader::Direct);
        }
    }
}
