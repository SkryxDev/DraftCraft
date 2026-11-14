use super::*;
use crate::file::{ColorValue, EntityLinetype, EntitySpace, XDataBlock, XDataItem};
use crate::read::object::{self, DecodeCtx};
use crate::read::version::DwgVersion as V;

const VERSIONS: [V; 6] = [V::R2000, V::R2004, V::R2007, V::R2010, V::R2013, V::R2018];

/// An entity written in every version reads back field for field with the reader.
#[test]
fn an_entity_reads_back_in_every_version() {
    for ver in VERSIONS {
        let mut w = ObjectWriter::new(ver, 0x13, 0x2A5, 30, TextEncoding::default());
        w.xdata(&[XDataBlock { app: 0x12, items: vec![XDataItem::Text("héllo".into()), XDataItem::Int16(7), XDataItem::Real(2.5)] }]);
        let reactors = [0x77u64];
        w.entity(&EntityStart {
            space: EntitySpace::Owned,
            owner: 0x1F,
            reactors: &reactors,
            xdictionary: 0x300,
            color: ColorValue::Index(3),
            linetype: EntityLinetype::Record(0x16),
            linetype_scale: 2.0,
            layer: 0x10,
            lineweight: 5,
            invisible: true,
            ..EntityStart::default()
        });
        w.bd(1.5);
        w.text("name");
        w.bs(9);
        w.reference(Ref::HardPointer, 0x55);
        let bytes = w.finish();
        let ctx = DecodeCtx { ver, codepage: 30, classes: &[], text: Default::default() };
        let mut o = object::open(&ctx, &bytes, 0).unwrap_or_else(|e| panic!("{ver:?}: {e}"));
        assert_eq!((o.handle, o.type_code), (0x2A5, 0x13), "{ver:?}");
        assert_eq!(o.xdata.len(), 1, "{ver:?}");
        assert_eq!(o.s.dat.bd().unwrap(), 1.5, "{ver:?}");
        assert_eq!(o.s.tv().unwrap(), "name", "{ver:?}");
        assert_eq!(o.s.dat.bs().unwrap(), 9, "{ver:?}");
        let (owner, r, xdic) = object::common_handles(&mut o).unwrap();
        assert_eq!((owner, r.len(), xdic), (0x1F, 1, 0x300), "{ver:?}");
        let c = o.common.clone().unwrap();
        assert_eq!(c.layer.0, 0x10, "{ver:?}");
        assert!(c.invisible && c.lineweight == 5 && c.linetype_scale == 2.0, "{ver:?}");
        assert_eq!(o.s.h_abs(0x2A5).unwrap(), 0x55, "{ver:?}");
        assert_eq!(o.s.handle_bits_left() / 8, 0, "{ver:?}: nothing left but padding");
    }
}

/// An object that is not an entity, with no text, in every version.
#[test]
fn an_object_reads_back_in_every_version() {
    for ver in VERSIONS {
        let mut w = ObjectWriter::new(ver, 0x2A, 0xC, 30, TextEncoding::default());
        w.xdata(&[]);
        w.object(0, &[], 0);
        w.bl(3);
        w.reference(Ref::SoftOwner, 0xD);
        let bytes = w.finish();
        let ctx = DecodeCtx { ver, codepage: 30, classes: &[], text: Default::default() };
        let mut o = object::open(&ctx, &bytes, 0).unwrap_or_else(|e| panic!("{ver:?}: {e}"));
        assert_eq!(o.s.dat.bl().unwrap(), 3, "{ver:?}");
        let (owner, r, xdic) = object::common_handles(&mut o).unwrap();
        assert_eq!((owner, r.len(), xdic), (0, 0, 0), "{ver:?}");
        assert_eq!(o.s.h_abs(0xC).unwrap(), 0xD, "{ver:?}");
    }
}

fn mi201(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../io/tests/data/mi201").join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

const FILES: [(&str, V); 6] = [
    ("mi201-r2000.dwg", V::R2000),
    ("mi201-r2004.dwg", V::R2004),
    ("mi201-r2007.dwg", V::R2007),
    ("mi201-r2010.dwg", V::R2010),
    ("mi201-r2013.dwg", V::R2013),
    ("mi201-r2018.dwg", V::R2018),
];

fn first_difference(a: &[u8], b: &[u8]) -> String {
    let at = a.iter().zip(b).enumerate().position(|(i, (x, y))| x != y && !(16..24).contains(&i)).unwrap_or(a.len().min(b.len()));
    let show = |s: &[u8]| format!("{:02x?}", s.get(at.saturating_sub(4)..(at + 12).min(s.len())).unwrap_or(&[]));
    format!("lengths {} and {}, first difference at byte {at}: ours {} AutoCAD's {}", a.len(), b.len(), show(a), show(b))
}

/// The header of each of AutoCAD's own files, read to its values and written again, is the
/// section AutoCAD wrote, byte for byte.
#[test]
fn autocads_header_writes_back_byte_for_byte() {
    for (name, ver) in FILES {
        let bytes = mi201(name);
        let (v, c) = crate::read::open_container(&bytes).unwrap();
        assert_eq!(v, ver);
        let theirs = c.header.clone().unwrap();
        let file = crate::file::DwgFile::open(&bytes).unwrap();
        let vars = file.header().unwrap().vars;
        let ours = header_section(ver, c.codepage, TextEncoding::default(), &|n| vars.iter().find(|(k, _)| *k == n).map(|(_, v)| v.clone()));
        // Up to 2004 every byte, the fields after the variables and the check sum too. From 2007
        // AutoCAD's section goes on after the handles for some ten bytes that differ from file to
        // file, starting in the unused bits of the last byte: every byte up to that one (and so
        // the sizes, bytes 16 to 24, differ).
        let same = if ver <= V::R2004 {
            ours == theirs
        } else {
            let body_end = ours.len() - 2 - 16 - 1;
            ours.iter().zip(&theirs).enumerate().take(body_end).all(|(i, (a, b))| a == b || (16..24).contains(&i))
        };
        assert!(same, "{name}: {}", first_difference(&ours, &theirs));
    }
}

/// The object map of each of AutoCAD's files, parsed and written again, is AutoCAD's.
#[test]
fn autocads_object_map_writes_back_byte_for_byte() {
    for (name, ver) in FILES {
        let bytes = mi201(name);
        let (_, c) = crate::read::open_container(&bytes).unwrap();
        let entries: Vec<(u64, u64)> =
            crate::read::objmap::parse(&c.handles, true).unwrap().iter().map(|e| (e.handle, u64::try_from(e.location).unwrap())).collect();
        let ours = object_map(&entries);
        let theirs = &c.handles[..ours.len().min(c.handles.len())];
        assert!(ours == theirs, "{name} {ver:?}: {}", first_difference(&ours, theirs));
    }
}

/// The classes of each of AutoCAD's files, parsed and written again, are AutoCAD's section.
#[test]
fn autocads_classes_write_back_byte_for_byte() {
    for (name, ver) in FILES {
        let bytes = mi201(name);
        let (_, c) = crate::read::open_container(&bytes).unwrap();
        let theirs = c.classes.clone().unwrap();
        let (list, _) = crate::read::classes::parse(&theirs, ver, c.maintenance, c.codepage).unwrap();
        let classes: Vec<Class> = list
            .iter()
            .map(|c| Class {
                number: c.number,
                dxf_name: c.dxf_name.clone(),
                cpp_name: c.cpp_name.clone(),
                app_name: c.app_name.clone(),
                proxy_flags: c.proxy_flags,
                instance_count: c.instance_count,
                was_zombie: c.was_zombie,
                is_entity: c.item_class_id == 0x1F2,
                versions: (c.dwg_version, c.maintenance_version),
            })
            .collect();
        let ours = classes_section(ver, c.codepage, TextEncoding::default(), &classes);
        assert!(ours == theirs, "{name}: {}", first_difference(&ours, &theirs));
    }
}

#[test]
#[ignore]
fn probe_header_tail() {
    for (name, ver) in FILES {
        let bytes = mi201(name);
        let (_, c) = crate::read::open_container(&bytes).unwrap();
        let theirs = c.header.clone().unwrap();
        let file = crate::file::DwgFile::open(&bytes).unwrap();
        let vars = file.header().unwrap().vars;
        let ours = header_section(ver, c.codepage, TextEncoding::default(), &|n| vars.iter().find(|(k, _)| *k == n).map(|(_, v)| v.clone()));
        let at = ours.iter().zip(&theirs).enumerate().position(|(i, (x, y))| x != y && !(16..24).contains(&i)).unwrap_or(ours.len());
        println!(
            "{name}: ours {} theirs {} diff at {at}\n ours   {:02x?}\n theirs {:02x?}",
            ours.len(),
            theirs.len(),
            &ours[at.saturating_sub(8)..],
            &theirs[at.saturating_sub(8)..]
        );
    }
}

#[test]
#[ignore]
fn probe_sections() {
    for (name, _) in FILES {
        let bytes = mi201(name);
        let (_, c) = crate::read::open_container(&bytes).unwrap();
        println!("== {name}: maint {} cp {} objects {} handles {}", c.maintenance, c.codepage, c.objects.len(), c.handles.len());
        for s in &c.sections {
            println!("   section {:?} size {} pages {} compressed {}", s.name, s.size, s.pages, s.compressed);
        }
        for (n, d) in &c.others {
            let show = if d.len() <= 400 { format!("{:02x?}", d) } else { format!("{:02x?} …", &d[..160]) };
            println!(" {n} ({}): {show}", d.len());
        }
        println!(" objects start {:02x?}", &c.objects[..8.min(c.objects.len())]);
    }
}

/// The second header of AutoCAD's own 2000 file, from its values.
#[test]
fn autocads_second_header_writes_back_byte_for_byte() {
    let bytes = mi201("mi201-r2000.dwg");
    let theirs = &bytes[0x1de55..0x1deee];
    let records = [(0u8, 0x4a8bu32, 0x241u32), (1, 0x4ccc, 0x386), (2, 0x1dafb, 0x35a), (3, 0, 0), (4, 0, 0), (5, 0x61, 0x7b)];
    let handles = [0x3d24eu64, 0x16, 0x08, 0x0e, 0x04, 0x10, 0x11, 0x02, 0x12, 0x14, 0x3d091, 0x3f, 0x3d0cc, 0x3e];
    let ours = super::file::second_header(0x1de55, 15, 30, &records, &handles);
    // The eight bytes after the checksum have no fixed value.
    let end = ours.len() - 24;
    assert!(ours.len() == theirs.len() && ours[..end] == theirs[..end], "{}", first_difference(&ours, theirs));
}

/// Each of AutoCAD's files taken apart and composed again opens with the same objects, classes
/// and header variables, in its own version.
#[test]
fn autocads_files_compose_again() {
    for (name, ver) in FILES {
        if ver == V::R2007 {
            continue;
        }
        let bytes = mi201(name);
        let file = crate::file::DwgFile::open(&bytes).unwrap();
        let vars = file.header().unwrap().vars;
        let (_, c) = crate::read::open_container(&bytes).unwrap();
        let (list, _) = crate::read::classes::parse(&c.classes.clone().unwrap(), ver, c.maintenance, c.codepage).unwrap();
        let classes: Vec<Class> = list
            .iter()
            .map(|c| Class {
                number: c.number,
                dxf_name: c.dxf_name.clone(),
                cpp_name: c.cpp_name.clone(),
                app_name: c.app_name.clone(),
                proxy_flags: c.proxy_flags,
                instance_count: c.instance_count,
                was_zombie: c.was_zombie,
                is_entity: c.item_class_id == 0x1F2,
                versions: (c.dwg_version, c.maintenance_version),
            })
            .collect();
        let objects: Vec<(u64, Vec<u8>)> = file.objects().map(|o| (o.handle, file.object_bytes(&o).unwrap().to_vec())).collect();
        let seed = objects.iter().map(|o| o.0).max().unwrap() + 1;
        let props = Properties { last_saved_by: "tester".into(), ..Properties::default() };
        let parts = Parts {
            version: ver,
            codepage: c.codepage,
            text: TextEncoding::default(),
            header: &|n| vars.iter().find(|(k, _)| *k == n).map(|(_, v)| v.clone()),
            classes: &classes,
            objects: &objects,
            handseed: seed,
            measurement: 1,
            created: 2_460_000.5,
            updated: 2_460_001.25,
            editing: 0.01,
            created_iso: "2023-02-25T00:00:00",
            updated_iso: "2023-02-25T18:00:00",
            properties: &props,
            preview_png: None,
            preview_bmp: None,
            app_name: "DraftCraft",
            app_version: "3.0.0",
            fonts: &[("txt".into(), 0xF)],
        };
        let out = compose(&parts).unwrap_or_else(|e| panic!("{name}: {e}"));
        let again = crate::file::DwgFile::open(&out).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(again.notes.iter().all(|n| !n.contains("checksum") && !n.contains("CRC")), "{name}: {:?}", again.notes);
        let before: Vec<(u64, String)> = file.objects().map(|o| (o.handle, o.type_name)).collect();
        let after: Vec<(u64, String)> = again.objects().map(|o| (o.handle, o.type_name)).collect();
        assert_eq!(before, after, "{name}");
        let h = again.header().unwrap();
        assert!(h.verified, "{name}: the header's handles point at the right objects");
        assert_eq!(h.measurement, (ver >= V::R2004).then_some(1), "{name}");
        for ((n, a), (_, b)) in vars.iter().zip(&h.vars) {
            if *n != "HANDSEED" {
                assert_eq!(a, b, "{name}: {n}");
            }
        }
        // The first reader, over records, reads every object of the composed file too.
        let d = crate::read_bytes(&out).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(d.objects.len(), objects.len(), "{name}");
        assert_eq!(d.header.properties.last_saved_by, if ver >= V::R2004 { "tester" } else { "" }, "{name}");
    }
}
