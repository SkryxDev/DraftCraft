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
    let (dropped, counted) = left_out(&doc, ver);
    let pruned = if dropped.is_empty() { None } else { Some(doc.without(&dropped)) };
    let mut again = None;
    let ours = encode_pruned(pruned.as_ref().unwrap_or(&doc), ver, counted, &mut again);
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
    d.add(
        &draftcraft_doc::Space::Model,
        draftcraft_doc::Common::default(),
        EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 0.0), b: Vec3::new(10.0, 5.0, 0.0) }),
    )
    .expect("add");
    for version in [
        draftcraft_dxf::Version::R2000,
        draftcraft_dxf::Version::R2004,
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

#[test]
#[ignore]
fn scratch_dump() {
    use draftcraft_doc::{Drawing, EntityKind, Line};
    use draftcraft_geom::Vec3;
    let mut d = Drawing::new_metric();
    d.add(
        &draftcraft_doc::Space::Model,
        draftcraft_doc::Common::default(),
        EntityKind::Line(Line { a: Vec3::new(0.0, 0.0, 0.0), b: Vec3::new(10.0, 5.0, 0.0) }),
    )
    .expect("add");
    d.table_styles.push(draftcraft_doc::TableStyle {
        name: "Sched".into(),
        text_height: 2.5,
        margin: 1.0,
        title: false,
        header: true,
        ..Default::default()
    });
    if std::env::var("H").is_ok() {
        let base = u64::MAX - 0x2000;
        d.model = Default::default();
        for index in [5u64, 1, 9] {
            d.model.push(draftcraft_doc::Entity::new(
                draftcraft_doc::Handle(base + index * 17),
                EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(index as f64, 2.0, 3.0) }),
            ));
        }
        d.handseed = base + 0x1000;
        let tags = crate::dxf_write::dwg_records(&d, draftcraft_dxf::Version::R2000);
        for (i, t) in tags.iter().enumerate() {
            if t.code == 5 {
                println!("handle {:?} after {:?}", t.value, tags.get(i.saturating_sub(1)).map(|t| &t.value));
            }
        }
    }
    let ver = match std::env::var("V").as_deref() {
        Ok("2000") => draftcraft_dxf::Version::R2000,
        Ok("2018") => draftcraft_dxf::Version::R2018,
        Ok("2010") => draftcraft_dxf::Version::R2010,
        Ok("2013") => draftcraft_dxf::Version::R2013,
        _ => draftcraft_dxf::Version::R2004,
    };
    let (bytes, unw) = crate::write_dwg_own(&d, ver).expect("write");
    println!("unwritten {unw:?}");
    std::fs::write(std::env::var("OUT").unwrap_or("/dev/null".into()), &bytes).expect("out");
    let back = crate::read(&bytes, "x.dwg").expect("read");
    println!("styles back: {:?} notes {:?}", back.table_styles.iter().map(|s| &s.name).collect::<Vec<_>>(), back.read_notes);
    let file = DwgFile::open(&bytes).expect("open");
    let _ = file.header();
    for o in file.objects() {
        println!("{:X} {} {} bytes", o.handle, o.type_name, file.object_bytes(&o).map_or(0, |b| b.len()));
    }
    match acadrust::DwgReader::from_stream(std::io::Cursor::new(&bytes)).read() {
        Ok(_) => println!("acadrust OK"),
        Err(e) => println!("acadrust: {e:?}"),
    }
    match acadrust::DwgReader::from_stream_with_options(std::io::Cursor::new(&bytes), acadrust::io::dwg::DwgReadOptions::failsafe()).read() {
        Ok(doc) => {
            for n in &doc.notifications {
                println!("note: {n:?}");
            }
        }
        Err(e) => println!("failsafe: {e:?}"),
    }
}

#[test]
#[ignore]
fn scratch_fields() {
    let bytes = mi201(&std::env::var("F").expect("F"));
    let kind = std::env::var("K").expect("K");
    let fields: Vec<String> = std::env::var("S").unwrap_or_default().split(',').filter(|s| !s.is_empty()).map(str::to_string).collect();
    let max: usize = std::env::var("N").ok().and_then(|n| n.parse().ok()).unwrap_or(2);
    let file = DwgFile::open(&bytes).expect("open");
    for o in file.objects().filter(|o| o.type_name == kind).take(max) {
        let mut r = file.read_object(&o).expect("object");
        let _ = r.common_refs();
        let mut out = Vec::new();
        for f in &fields {
            let v = match f.as_str() {
                "T" => r.text().map(|t| format!("{t:?}")),
                "BL" => r.bl().map(|v| format!("{}", v as i32)),
                "BS" => r.bs().map(|v| format!("{}", v as i16)),
                "BD" => r.bd().map(|v| format!("{v}")),
                "B" => r.b().map(|v| format!("{}", u8::from(v))),
                "RC" => r.rc().map(|v| format!("{v}")),
                "CMC" => r.color().map(|v| format!("{v:?}")),
                "H" => r.reference().map(|v| format!("h{v:X}")),
                _ => Ok("?".to_string()),
            };
            match v {
                Ok(v) => out.push(format!("{f}={v}")),
                Err(e) => {
                    out.push(format!("{f}=ERR {e}"));
                    break;
                }
            }
        }
        println!("{:X}: {} | left {} bits", o.handle, out.join(" "), r.remaining_bits());
    }
}

#[test]
#[ignore]
fn scratch_types() {
    let dir = std::env::var("DIR").expect("DIR");
    let kinds: Vec<String> = std::env::var("KINDS").unwrap_or_default().split(',').map(str::to_string).collect();
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .expect("dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "dwg"))
        .collect();
    names.sort();
    for p in names {
        let bytes = std::fs::read(&p).expect("read");
        let Ok(file) = DwgFile::open(&bytes) else { continue };
        let mut tally: std::collections::BTreeMap<String, usize> = Default::default();
        for o in file.objects() {
            if kinds.contains(&o.type_name) {
                *tally.entry(o.type_name.clone()).or_default() += 1;
                if std::env::var("DUMP").is_ok() {
                    println!("  {:X} {} {}", o.handle, o.type_name, hex_of(file.object_bytes(&o).unwrap_or_default()));
                }
            }
        }
        println!("{} {}: {tally:?}", p.file_name().map(|n| n.to_string_lossy()).unwrap_or_default(), file.version_code());
    }
}

#[test]
#[ignore]
fn scratch_unwritten() {
    let dir = std::env::var("DIR").expect("DIR");
    let mut names: Vec<_> = std::fs::read_dir(&dir).expect("dir").filter_map(|e| e.ok()).map(|e| e.path()).collect();
    names.sort();
    for p in names {
        let Ok(bytes) = std::fs::read(&p) else { continue };
        let d = match crate::read(&bytes, &p) {
            Ok(d) => d,
            Err(e) => {
                println!("{}: read {e}", p.display());
                continue;
            }
        };
        for ver in [draftcraft_dxf::Version::R2000, draftcraft_dxf::Version::R2010, draftcraft_dxf::Version::R2018] {
            match crate::write_dwg_own(&d, ver) {
                Ok((_, unw)) => println!("{} {:?}: {unw:?}", p.file_name().map(|n| n.to_string_lossy()).unwrap_or_default(), ver),
                Err(e) => println!("{} {ver:?}: {e}", p.display()),
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
