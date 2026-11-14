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
type Tally = BTreeMap<String, (usize, usize, usize, Vec<u64>, usize)>;

fn compare(dxf: &str, dwg: &str, ver: V) -> (Tally, Unwritten, Vec<String>) {
    let tags = draftcraft_dxf::parse(&mi201(dxf)).expect("AutoCAD's DXF");
    let doc = Doc::parse(&tags);
    let ours = encode(&doc, ver);
    let bytes = mi201(dwg);
    let file = DwgFile::open(&bytes).expect("AutoCAD's DWG");
    let theirs: HashMap<u64, Vec<u8>> = file.objects().filter_map(|o| Some((o.handle, file.object_bytes(&o)?.to_vec()))).collect();
    let mut records: HashMap<u64, &Rec<'_>> = HashMap::new();
    for t in &doc.tables {
        records.insert(t.head.handle, &t.head);
        records.extend(t.records.iter().map(|r| (r.handle, r)));
    }
    for b in &doc.blocks {
        records.insert(b.begin.handle, &b.begin);
        records.extend(b.entities.iter().map(|r| (r.handle, r)));
        records.extend(b.end.iter().map(|r| (r.handle, r)));
    }
    records.extend(doc.entities.iter().chain(&doc.objects).map(|r| (r.handle, r)));
    let groups = |h: u64| {
        records.get(&h).map_or(String::new(), |r| r.tags.iter().chain(r.xdata).map(|t| format!("{}={}", t.code, t.str())).collect::<Vec<_>>().join(" "))
    };
    let mut tally = Tally::new();
    let mut details = Vec::new();
    for o in &ours.objects {
        let e = tally.entry(o.kind.clone()).or_default();
        match theirs.get(&o.handle) {
            Some(t) if *t == o.bytes => e.0 += 1,
            // The DXF holds 16 digits of each number: an object of the same size whose bytes
            // differ in a few places differs in the last bits of its numbers.
            Some(t) if near(&o.bytes, t) => {
                e.4 += 1;
                if e.4 <= 2 {
                    details.push(format!("{} ~{:X}\n  {}\n  ours   {}\n  theirs {}", o.kind, o.handle, groups(o.handle), hex_of(&o.bytes), hex_of(t)));
                }
            }
            Some(t) => {
                e.1 += 1;
                e.3.push(o.handle);
                if e.1 <= 3 {
                    let bit = o.bytes.iter().zip(t).position(|(a, b)| a != b).map_or(o.bytes.len().min(t.len()) * 8, |i| {
                        i * 8 + (o.bytes[i] ^ t[i]).leading_zeros() as usize
                    });
                    details.push(format!(
                        "{} {:X} first different bit {bit} (byte {})\n  {}\n  ours   {}\n  theirs {}\n  ours   {}\n  theirs {}",
                        o.kind,
                        o.handle,
                        bit / 8,
                        groups(o.handle),
                        hex_of(&o.bytes),
                        hex_of(t),
                        bits_of(&o.bytes, bit),
                        bits_of(t, bit)
                    ));
                }
            }
            None => e.2 += 1,
        }
    }
    (tally, ours.unwritten, details)
}

fn near(a: &[u8], b: &[u8]) -> bool {
    let body = a.len().saturating_sub(2);
    a.len() == b.len() && a.iter().zip(b).take(body).filter(|(x, y)| x != y).count() * 6 <= body
}

/// The bits around `at`: 24 before, a bar, 64 after.
fn bits_of(b: &[u8], at: usize) -> String {
    let bit = |i: usize| b.get(i / 8).map_or('-', |x| if x >> (7 - i % 8) & 1 == 1 { '1' } else { '0' });
    let from = at.saturating_sub(24);
    (from..at).map(bit).chain(['|']).chain((at..at + 64).map(bit)).collect()
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
    let only_ver = std::env::var("DWG_PROBE_VER").unwrap_or_default();
    for (dxf, dwg, ver) in PAIRS {
        if !only_ver.is_empty() && !dwg.contains(&only_ver) {
            continue;
        }
        let (tally, unwritten, details) = compare(dxf, dwg, ver);
        println!("== {dwg}");
        for (kind, (same, different, missing, _, near)) in &tally {
            println!("  {kind:<22} same {same:>4}  last digits {near:>4}  different {different:>4}  not in file {missing:>3}");
        }
        println!("  unwritten: {unwritten:?}");
        for d in details.iter().filter(|d| only.split(',').any(|k| !k.is_empty() && d.starts_with(&format!("{k} ")))) {
            println!("{d}");
        }
    }
}

/// A drawing saved by the writer and read again by the reader, in every version it writes.
#[test]
fn a_drawing_reads_back() {
    use draftcraft_doc::{Drawing, EntityKind, Line};
    use draftcraft_geom::Vec3;
    let mut d = Drawing::new_metric();
    d.add(&draftcraft_doc::Space::Model, draftcraft_doc::Common::default(), EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 0.0), b: Vec3::new(10.0, 5.0, 0.0) })).expect("add");
    for version in [draftcraft_dxf::Version::R2000, draftcraft_dxf::Version::R2004, draftcraft_dxf::Version::R2010, draftcraft_dxf::Version::R2013, draftcraft_dxf::Version::R2018] {
        let (bytes, unwritten) = crate::write_dwg_own(&d, version).unwrap_or_else(|e| panic!("{version:?}: {e}"));
        println!("{version:?}: {} bytes, unwritten {unwritten:?}", bytes.len());
        let (back, unread) = crate::dwg::read(&bytes).unwrap_or_else(|e| panic!("{version:?}: {e}"));
        println!("  unread {:?} notes {:?}", unread.by_type, back.read_notes);
        assert_eq!(back.model.len(), 1, "{version:?}");
        assert!(unread.is_empty(), "{version:?}: {}", unread.summary());
    }
}
