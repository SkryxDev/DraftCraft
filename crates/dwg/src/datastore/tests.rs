use super::*;

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// A stand-in modeler description of `n` bytes.
fn sab(n: usize, seed: u8) -> Vec<u8> {
    let mut v = b"ASM BinaryFile4".to_vec();
    v.extend((0..n.saturating_sub(15)).map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed)));
    v.truncate(n);
    v
}

#[test]
fn a_store_reads_back_as_written() {
    let a = sab(838, 1);
    let b = sab(7780, 2);
    let store = solids(&[(0x2EB, &a), (0x2E2, &b)]);
    let bytes = write(&store);
    assert_eq!(&bytes[..4], b"jard");
    assert_eq!(bytes.len() % 0x80, 0);
    assert_eq!(u32_at(&bytes, 0x34) as usize, bytes.len(), "the header records the size");
    let back = read(&bytes).unwrap();
    assert_eq!(back, store);
    let got: Vec<(u64, usize)> = back.records_of(ASM_SCHEMA).map(|r| (r.handle, r.data.len())).collect();
    assert_eq!(got, vec![(0x2EB, 838), (0x2E2, 7780)]);
}

/// The layout AutoCAD gives a new store: the file header, the segment index first, every
/// segment signed and named, its data padded with 0x70 to a multiple of 0x80 bytes.
#[test]
fn the_layout_is_autocads() {
    let a = sab(100, 3);
    let bytes = write(&solids(&[(0x2E2, &a)]));
    // jard, header size, 8, version 2, 0, revision 1, segment index at 0x80, 0, 7 segments,
    // schema index 4, data index 2, search 6, no previous save.
    let header: Vec<u32> = (0..13).map(|i| u32_at(&bytes, 4 + 4 * i)).collect();
    assert_eq!(&header[..12], &[0x80, 8, 2, 0, 1, 0x80, 0, 7, 4, 2, 6, 0]);
    let names: Vec<&[u8]> = (0..7)
        .skip(1)
        .map(|i| {
            let at = u32_at(&bytes, 0x80 + 0x30 + 12 * i) as usize;
            assert_eq!(&bytes[at..at + 2], &[0xAC, 0xD5]);
            assert_eq!(u32_at(&bytes, at + 8) as usize, i, "the segment's own index");
            assert_eq!(&bytes[at + 0x28..at + 0x30], &[0x55; 8]);
            &bytes[at + 2..at + 8]
        })
        .collect();
    assert_eq!(names, [&b"segidx"[..], b"datidx", b"_data_", b"schidx", b"schdat", b"search"]);
    // The data segment: one record header (size 20, 1, handle, offset 0), its data on the next
    // 16-byte boundary (0x50), padded with 0x62 up to there.
    let data = u32_at(&bytes, 0x80 + 0x30 + 12 * 3) as usize;
    assert_eq!(u32_at(&bytes, data + 0x24), 5, "the data starts at 0x50");
    assert_eq!(u32_at(&bytes, data + 0x30), 20);
    assert_eq!(u32_at(&bytes, data + 0x34), 1);
    assert_eq!(u32_at(&bytes, data + 0x38), 0x2E2);
    assert_eq!(&bytes[data + 0x44..data + 0x50], &[0x62; 12]);
    assert_eq!(u32_at(&bytes, data + 0x50), 100);
    assert_eq!(&bytes[data + 0x54..data + 0x54 + 15], b"ASM BinaryFile4");
}

/// A record larger than 0x40000 bytes is stored in blob pages of at most 0xFFFB0 bytes.
#[test]
fn a_large_record_goes_in_blob_pages() {
    let big = sab(2_971_153, 4);
    let small = sab(692, 5);
    let bytes = write(&solids(&[(0x928, &big), (0x2E6, &small)]));
    let back = read(&bytes).unwrap();
    assert_eq!(back.records.len(), 2);
    assert_eq!(back.records[0].data, big);
    assert_eq!(back.records[1].data, small);
    let count = u32_at(&bytes, 0x20) as usize;
    let blobs = (1..count).filter(|&i| {
        let at = u32_at(&bytes, 0x80 + 0x30 + 12 * i) as usize;
        &bytes[at + 2..at + 8] == b"blob01"
    });
    assert_eq!(blobs.count(), 3, "2,971,153 bytes in pages of 1,048,496");
}

/// Many records fill more than one data segment.
#[test]
fn many_records_fill_several_data_segments() {
    let datas: Vec<Vec<u8>> = (0..12).map(|i| sab(200_000, i as u8)).collect();
    let records: Vec<(u64, &[u8])> = datas.iter().enumerate().map(|(i, d)| (0x100 + i as u64, d.as_slice())).collect();
    let store = solids(&records);
    let back = read(&write(&store)).unwrap();
    assert_eq!(back, store);
}

/// A damaged section is an error, never a panic.
#[test]
fn hostile_bytes_do_not_panic() {
    let a = sab(500, 6);
    let good = write(&solids(&[(0x2E2, &a)]));
    assert!(read(&[]).is_err());
    assert!(read(b"jard").is_err());
    for cut in [8, 0x40, 0x80, 0x100, good.len() / 2, good.len() - 1] {
        let _ = read(&good[..cut]);
    }
    for at in (0..good.len()).step_by(7) {
        let mut b = good.clone();
        b[at] ^= 0xFF;
        let _ = read(&b);
        b[at] = 0xFF;
        let _ = read(&b);
    }
}

/// AutoCAD's own sections, read and written again: `DWG_DS_FILES` names sections written by
/// `probe_dump_section` (`:` between them). The records come back; a section AutoCAD laid out
/// fresh (revision 1, as an empty drawing's) is written byte for byte as AutoCAD wrote it.
#[test]
#[ignore]
fn probe_autocads_sections() {
    for f in std::env::var("DWG_DS_FILES").unwrap_or_default().split(':').filter(|f| !f.is_empty()) {
        let bytes = std::fs::read(f).unwrap();
        let store = read(&bytes).unwrap();
        println!("{f}: {} schemas {:?}", store.schemas.len(), store.schemas.iter().map(|s| &s.name).collect::<Vec<_>>());
        for r in &store.records {
            println!(
                "  record {:X} schema {} {} bytes {:?}",
                r.handle,
                r.schema,
                r.data.len(),
                String::from_utf8_lossy(&r.data[..r.data.len().min(15)])
            );
        }
        let again = write(&store);
        if u32_at(&bytes, 0x14) == 1 {
            let first = again.iter().zip(&bytes).position(|(a, b)| a != b);
            println!("  fresh layout: same bytes {} (first difference {first:?})", again == bytes);
        }
        assert_eq!(read(&again).unwrap(), store);
    }
}
