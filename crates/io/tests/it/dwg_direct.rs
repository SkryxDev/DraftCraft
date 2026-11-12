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
        // Nothing is read yet, so every object must be counted as unread: none dropped silently.
        assert_eq!(unread.total(), file.object_count(), "{name}: {}", unread.summary());
        // Every object's type is known by name.
        assert!(!unread.by_type.contains_key("?"), "{name}: {}", unread.summary());
        for t in ["LINE", "CIRCLE", "ARC", "TEXT", "DIMENSION_LINEAR", "LAYER", "BLOCK_HEADER", "DICTIONARY"] {
            assert!(unread.by_type.contains_key(t), "{name}: no {t} among {}", unread.summary());
        }
    }
}

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
