//! The writer against AutoCAD's own files: AutoCAD's DXF of a drawing, encoded record by
//! record, must give the objects of AutoCAD's DWG of the same drawing.

use super::*;
use draftcraft_dwg::file::DwgFile;

/// A file of the mi201 corpus, or any file by its absolute path.
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
    compare_tags(&draftcraft_dxf::parse(&mi201(dxf)).expect("AutoCAD's DXF"), dwg, ver)
}

/// [`compare`] for records already in hand: AutoCAD's DXF, or DraftCraft's records of a drawing.
fn compare_tags(tags: &[draftcraft_dxf::Tag], dwg: &str, ver: V) -> (Tally, Unwritten, Vec<String>) {
    let doc = Doc::parse(tags);
    let (dropped, counted) = left_out(&doc, ver);
    let pruned = if dropped.is_empty() { None } else { Some(doc.without(&dropped)) };
    let mut again = None;
    let ours = encode_pruned(pruned.as_ref().unwrap_or(&doc), ver, counted, &mut again);
    // `DWG_PROBE_HANDLE=HEX` prints the bytes written for that handle.
    if let Some(h) = std::env::var("DWG_PROBE_HANDLE").ok().and_then(|h| u64::from_str_radix(&h, 16).ok()) {
        for o in ours.objects.iter().filter(|o| o.handle == h) {
            println!("{} {h:X} ours {}", o.kind, hex_of(&o.bytes));
        }
    }
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
        records
            .get(&h)
            .map_or(String::new(), |r| r.tags.iter().chain(r.xdata).map(|t| format!("{}={}", t.code, t.str())).collect::<Vec<_>>().join(" "))
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
                    details.push(format!(
                        "{} ~{:X}\n  {}\n  ours   {}\n  theirs {}",
                        o.kind,
                        o.handle,
                        groups(o.handle),
                        hex_of(&o.bytes),
                        hex_of(t)
                    ));
                }
            }
            Some(t) => {
                e.1 += 1;
                e.3.push(o.handle);
                if e.1 <= 3 {
                    let bit = o
                        .bytes
                        .iter()
                        .zip(t)
                        .position(|(a, b)| a != b)
                        .map_or(o.bytes.len().min(t.len()) * 8, |i| i * 8 + (o.bytes[i] ^ t[i]).leading_zeros() as usize);
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
    // `DWG_PROBE_PAIR=/abs/file.dxf:/abs/file.dwg:r2018` compares another pair AutoCAD saved.
    let pair = std::env::var("DWG_PROBE_PAIR").unwrap_or_default();
    let mut pairs: Vec<(&str, &str, V)> = PAIRS.to_vec();
    if let [dxf, dwg, ver] = pair.split(':').collect::<Vec<_>>()[..] {
        let ver = match ver {
            "r2013" => V::R2013,
            "r2010" => V::R2010,
            "r2007" => V::R2007,
            "r2004" => V::R2004,
            "r2000" => V::R2000,
            _ => V::R2018,
        };
        pairs = vec![(dxf, dwg, ver)];
    }
    for (dxf, dwg, ver) in pairs {
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

/// [`probe`] along the path a save takes: AutoCAD's 2018 DXF read into a drawing, the drawing's
/// records for each version, compared with AutoCAD's DWG of that version. Records the drawing
/// keeps as AutoCAD's 2018 groups (an object DraftCraft doesn't model) show here when the older
/// version stores them otherwise.
/// `cargo test -p draftcraft-io --lib dwg::write::tests::probe_drawing -- --ignored --nocapture`
#[test]
#[ignore]
fn probe_drawing() {
    let only = std::env::var("DWG_PROBE").unwrap_or_default();
    let only_ver = std::env::var("DWG_PROBE_VER").unwrap_or_default();
    let d = crate::read_dxf(&mi201("mi201-r2018.dxf")).expect("AutoCAD's DXF");
    for (version, dwg, ver) in [
        (crate::Version::R2000, "mi201-r2000.dwg", V::R2000),
        (crate::Version::R2004, "mi201-r2004.dwg", V::R2004),
        (crate::Version::R2007, "mi201-r2007.dwg", V::R2007),
        (crate::Version::R2010, "mi201-r2010.dwg", V::R2010),
        (crate::Version::R2013, "mi201-r2013.dwg", V::R2013),
        (crate::Version::R2018, "mi201-r2018.dwg", V::R2018),
    ] {
        if !only_ver.is_empty() && !dwg.contains(&only_ver) {
            continue;
        }
        let tags = crate::dxf_write::dwg_records(&d, version);
        // `DWG_PROBE_DUMP=KIND` prints the first record of that type, as the drawing gives it.
        if let Ok(kind) = std::env::var("DWG_PROBE_DUMP") {
            let doc = Doc::parse(&tags);
            if let Some(r) = doc.objects.iter().chain(&doc.entities).find(|r| r.kind == kind) {
                println!(
                    "{kind} {:X}: {}",
                    r.handle,
                    r.tags.iter().chain(r.xdata).map(|t| format!("{}={}", t.code, t.str())).collect::<Vec<_>>().join(" ")
                );
            }
        }
        let (tally, unwritten, details) = compare_tags(&tags, dwg, ver);
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

/// The plain file header (its first 0x80 bytes) is AutoCAD's in every version, but for the
/// addresses, which depend on the drawing: a wrong byte there makes AutoCAD refuse the file
/// (one too many before 2007's security flags read as a password).
#[test]
fn the_plain_file_header_is_autocads() {
    use draftcraft_dxf::Version as Dv;
    let mut d = draftcraft_doc::Drawing::new_metric();
    d.add(
        &draftcraft_doc::Space::Model,
        draftcraft_doc::Common::default(),
        draftcraft_doc::EntityKind::Line(draftcraft_doc::Line { a: draftcraft_geom::Vec3::ZERO, b: draftcraft_geom::Vec3::new(10.0, 5.0, 0.0) }),
    )
    .expect("add");
    // The address fields: the preview's (0x0D), then from 2004 the summary information's (0x20)
    // and the application information's two (0x2C, 0x30). Before 2004 the header holds the
    // section locators instead, compared up to the first one.
    let paged: &[std::ops::Range<usize>] = &[0x0D..0x11, 0x20..0x24, 0x2C..0x34];
    for (version, autocad, compared) in [
        (Dv::R2000, "mi201-r2000.dwg", 0x15),
        (Dv::R2004, "mi201-r2004.dwg", 0x80),
        (Dv::R2007, "mi201-r2007.dwg", 0x80),
        (Dv::R2010, "mi201-r2010.dwg", 0x80),
        (Dv::R2013, "mi201-r2013.dwg", 0x80),
        (Dv::R2018, "mi201-r2018.dwg", 0x80),
    ] {
        let (ours, _) = crate::write_dwg_own(&d, version).unwrap_or_else(|e| panic!("{version:?}: {e}"));
        let theirs = mi201(autocad);
        for i in (0..compared).filter(|i| !paged.iter().any(|r| r.contains(i))) {
            assert_eq!(ours[i], theirs[i], "{version:?}: byte {i:#04x}\nours     {:02x?}\nAutoCAD's {:02x?}", &ours[..compared], &theirs[..compared]);
        }
    }
}

/// A drawing saved by the writer and read again by the reader, in every version it writes.
#[test]
fn a_drawing_reads_back() {
    use draftcraft_doc::{Drawing, EntityKind, Line};
    use draftcraft_geom::Vec3;
    let mut d = Drawing::new_metric();
    d.add(
        &draftcraft_doc::Space::Model,
        draftcraft_doc::Common::default(),
        EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 0.0), b: Vec3::new(10.0, 5.0, 0.0) }),
    )
    .expect("add");
    for version in [
        draftcraft_dxf::Version::R2000,
        draftcraft_dxf::Version::R2004,
        draftcraft_dxf::Version::R2007,
        draftcraft_dxf::Version::R2010,
        draftcraft_dxf::Version::R2013,
        draftcraft_dxf::Version::R2018,
    ] {
        let (bytes, unwritten) = crate::write_dwg_own(&d, version).unwrap_or_else(|e| panic!("{version:?}: {e}"));
        println!("{version:?}: {} bytes, unwritten {unwritten:?}", bytes.len());
        let (back, unread) = crate::dwg::read(&bytes).unwrap_or_else(|e| panic!("{version:?}: {e}"));
        println!("  unread {:?} notes {:?}", unread.by_type, back.read_notes);
        assert_eq!(back.model.len(), 1, "{version:?}");
        assert!(unread.is_empty(), "{version:?}: {}", unread.summary());
    }
}

/// The header variables of AutoCAD's DXF, as the writer would store them, against the values
/// AutoCAD's DWG of the same drawing holds. Prints the differences.
#[test]
#[ignore]
fn probe_header() {
    use draftcraft_dwg::file::HeaderValue;
    for (dxf, dwg, ver) in PAIRS {
        let tags = draftcraft_dxf::parse(&mi201(dxf)).expect("AutoCAD's DXF");
        let doc = Doc::parse(&tags);
        let cx = context(&doc, ver);
        let ours = header::values(&cx);
        let bytes = mi201(dwg);
        let file = DwgFile::open(&bytes).expect("AutoCAD's DWG");
        println!("== {dwg}");
        for (name, theirs) in file.header().expect("header").vars {
            let same = match (ours.get(name), &theirs) {
                (Some(HeaderValue::Real(a)), HeaderValue::Real(b)) => (a - b).abs() <= 1e-9 * b.abs().max(1.0),
                (Some(HeaderValue::Point2(a)), HeaderValue::Point2(b)) => a.iter().zip(b).all(|(x, y)| (x - y).abs() <= 1e-9 * y.abs().max(1.0)),
                (Some(HeaderValue::Point3(a)), HeaderValue::Point3(b)) => a.iter().zip(b).all(|(x, y)| (x - y).abs() <= 1e-9 * y.abs().max(1.0)),
                (Some(a), b) => a == b,
                (None, _) => false,
            };
            if !same {
                println!("  {name}: ours {:?} theirs {theirs:?}", ours.get(name));
            }
        }
    }
}

/// A region's SAT text comes back as written in every version that keeps it in the entity:
/// lines over 255 characters (continued in groups 3), carets, and more text than one 4096-byte
/// block holds; the end-of-data line is left out, as AutoCAD stores it.
#[test]
fn sat_text_survives_the_dwg_versions_that_hold_it() {
    let long: String = "kooo ".repeat(80);
    let mut body = String::from("1\nkooo n n o\n1\n");
    body.push_str(&long);
    body.push_str("\n1\na^ b\n");
    for i in 0..200 {
        body.push_str(&format!("1\n-{i} ;o:+>-,r+:,: n n o\n"));
    }
    body.push_str("1\nZ1;r09r^ \\VLr;>+>\n");
    let dxf = format!(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1015\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
         0\nREGION\n5\n2B\n100\nAcDbEntity\n8\n0\n100\nAcDbModelerGeometry\n70\n1\n{body}0\nENDSEC\n0\nEOF\n"
    );
    let d = crate::read_dxf(dxf.as_bytes()).expect("dxf");
    // The lines of text, a group 3 continuing the line before it.
    let sat = |d: &draftcraft_doc::Drawing| -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();
        let tags = d.model.iter().find_map(|e| match &e.kind {
            draftcraft_doc::EntityKind::Unknown(u) if u.dxf_type == "REGION" => Some(u.tags.clone()),
            _ => None,
        });
        for t in tags.unwrap_or_default().iter().filter(|t| matches!(t.code, 1 | 3)) {
            match (t.code, lines.last_mut()) {
                (3, Some(l)) => l.push_str(&t.value),
                _ => lines.push(t.value.clone()),
            }
        }
        lines
    };
    let mut want = sat(&d);
    assert!(want.iter().map(String::len).sum::<usize>() > 4096);
    want.pop();
    for v in [crate::Version::R2000, crate::Version::R2004, crate::Version::R2010] {
        let dwg = crate::write_with(&d, "x.dwg", &crate::SaveOptions { version: Some(v), binary: false }).expect("write");
        let back = crate::read(&dwg, "x.dwg").expect("read");
        assert_eq!(sat(&back), want, "{v:?}");
    }
}

/// The save history's dates are AutoCAD's: the universal time of the save, to the second.
#[test]
fn dates_read_as_autocads_save_history_gives_them() {
    let d = crate::read_dwg_with(&mi201("mi201-r2007.dwg"), crate::DwgReader::Direct).unwrap();
    let updated = d.header.get("TDUUPDATE").and_then(draftcraft_doc::HVal::as_f64).unwrap();
    assert_eq!(iso_date(updated), "2026-10-02T16:57:01");
    assert_eq!(iso_date(2_440_588.5), "1970-01-01T12:00:00");
    assert_eq!(iso_date(0.0), "");
    assert_eq!(iso_date(f64::NAN), "");
    assert_eq!(iso_date(1e300), "");
}

/// Every file carries a preview 256 pixels wide: a PNG from 2013, an 8-bit bitmap before.
#[test]
fn files_carry_a_preview() {
    let d = crate::read_dxf(&mi201("mi201-r2018.dxf")).unwrap();
    for version in
        [crate::Version::R2000, crate::Version::R2004, crate::Version::R2007, crate::Version::R2010, crate::Version::R2013, crate::Version::R2018]
    {
        let (bytes, _) = crate::write_dwg_own(&d, version).unwrap();
        let has = |pattern: &[u8]| bytes.windows(pattern.len()).any(|w| w == pattern);
        if version >= crate::Version::R2013 {
            assert!(has(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0x0D, b'I', b'H', b'D', b'R', 0, 0, 1, 0]), "{version:?}");
        } else {
            assert!(has(&[40, 0, 0, 0, 0, 1, 0, 0]), "{version:?}");
        }
    }
    let empty = draftcraft_doc::Drawing::default();
    assert!(preview(&empty).is_none());
}
