//! The writer against AutoCAD's own files: AutoCAD's DXF of a drawing, encoded record by
//! record, must give the objects of AutoCAD's DWG of the same drawing.

use super::*;
use draftcraft_dwg::file::DwgFile;

fn mi201(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/mi201").join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

const PAIRS: [(&str, &str, V); 6] = [
    ("mi201-r2000.dxf", "mi201-r2000.dwg", V::R2000),
    ("mi201-r2004.dxf", "mi201-r2004.dwg", V::R2004),
    ("mi201-r2007.dxf", "mi201-r2007.dwg", V::R2007),
    ("mi201-r2010.dxf", "mi201-r2010.dwg", V::R2010),
    ("mi201-r2013.dxf", "mi201-r2013.dwg", V::R2013),
    ("mi201-r2018.dxf", "mi201-r2018.dwg", V::R2018),
];

/// How each type of record compares with AutoCAD's object of the same handle: (same bytes,
/// different bytes, not in AutoCAD's file), and the handles that differ.
type Tally = BTreeMap<String, (usize, usize, usize, Vec<u64>)>;

fn compare(dxf: &str, dwg: &str, ver: V) -> (Tally, Unwritten, Vec<String>) {
    let tags = draftcraft_dxf::parse(&mi201(dxf)).expect("AutoCAD's DXF");
    let doc = Doc::parse(&tags);
    let ours = encode(&doc, ver);
    let bytes = mi201(dwg);
    let file = DwgFile::open(&bytes).expect("AutoCAD's DWG");
    let theirs: HashMap<u64, Vec<u8>> = file.objects().filter_map(|o| Some((o.handle, file.object_bytes(&o)?.to_vec()))).collect();
    let mut tally = Tally::new();
    let mut details = Vec::new();
    for o in &ours.objects {
        let e = tally.entry(o.kind.clone()).or_default();
        match theirs.get(&o.handle) {
            Some(t) if *t == o.bytes => e.0 += 1,
            Some(t) => {
                e.1 += 1;
                e.3.push(o.handle);
                if e.1 <= 2 {
                    details.push(format!("{} {:X}\n  ours   {}\n  theirs {}", o.kind, o.handle, hex_of(&o.bytes), hex_of(t)));
                }
            }
            None => e.2 += 1,
        }
    }
    (tally, ours.unwritten, details)
}

fn hex_of(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Prints the comparison for every version: `cargo test -p draftcraft-io --lib dwg::write::tests::probe -- --ignored --nocapture`.
/// `DWG_PROBE=KIND` prints the differing objects of one type.
#[test]
#[ignore]
fn probe() {
    let only = std::env::var("DWG_PROBE").unwrap_or_default();
    for (dxf, dwg, ver) in PAIRS {
        let (tally, unwritten, details) = compare(dxf, dwg, ver);
        println!("== {dwg}");
        for (kind, (same, different, missing, _)) in &tally {
            println!("  {kind:<22} same {same:>4}  different {different:>4}  not in file {missing:>3}");
        }
        println!("  unwritten: {unwritten:?}");
        for d in details.iter().filter(|d| !only.is_empty() && d.starts_with(&format!("{only} "))) {
            println!("{d}");
        }
    }
}
