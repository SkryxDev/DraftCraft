//! The pieces of the container against AutoCAD's own 2007 file of the MI-201 drawing.

use super::codes::{self, CODEWORD, Code};
use super::random::{Random, rotated};
use crate::read::r2007::{HEADER_AT, HEADER_FIELDS, PAGES_AT, decompress, deinterleave, u64_at};

fn autocad() -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../io/tests/data/mi201/mi201-r2007.dwg");
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The file header's block (three codewords' data) and its fields.
fn header(file: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let block = deinterleave(&file[HEADER_AT..HEADER_AT + 3 * CODEWORD], 3, 239, 3 * 239);
    let stored = i32::from_le_bytes(block[24..28].try_into().unwrap());
    assert!(stored > 0);
    let fields = decompress(&block[32..32 + stored as usize], HEADER_FIELDS).unwrap();
    (block, fields)
}

fn field(fields: &[u8], i: usize) -> u64 {
    u64_at(fields, i * 8).unwrap()
}

/// The data bytes stored in a page of `blocks` codewords at `at`.
fn stored(file: &[u8], at: usize, blocks: usize, code: Code, len: usize) -> Vec<u8> {
    deinterleave(&file[at..at + blocks * CODEWORD], blocks, code.data(), len)
}

/// Each codeword of a page, data and check bytes.
fn codewords(file: &[u8], at: usize, blocks: usize) -> Vec<Vec<u8>> {
    (0..blocks).map(|i| (0..CODEWORD).map(|j| file[at + i + j * blocks]).collect()).collect()
}

#[test]
fn the_header_crc_is_autocads() {
    let (block, fields) = header(&autocad());
    let mut zeroed = fields.clone();
    zeroed[0x108..].fill(0);
    assert_eq!(codes::normal(0, &zeroed), field(&fields, 33));
    // The block's own checks: the compressed fields' CRC, and the checking sequence's.
    let stored = i32::from_le_bytes(block[24..28].try_into().unwrap()) as usize;
    assert_eq!(codes::normal(0, &block[32..32 + stored]), u64_at(&block, 16).unwrap());
    let v1 = u64_at(&block, 8).unwrap();
    let v2 = v1.rotate_left((v1 & 0x1F) as u32);
    let seq = [v1.to_le_bytes(), v2.to_le_bytes()].concat();
    assert_eq!(codes::normal_seed1(0, &seq), u64_at(&block, 0).unwrap());
}

#[test]
fn the_map_crcs_are_autocads() {
    let file = autocad();
    let (_, f) = header(&file);
    // The page map: 4 codewords at the start of the pages.
    let at = PAGES_AT as usize + field(&f, 7) as usize;
    let comp = stored(&file, at, 4, Code::System, field(&f, 10) as usize);
    let raw = decompress(&comp, field(&f, 11) as usize).unwrap();
    assert_eq!(codes::mirrored(0, &raw), field(&f, 16), "page map");
    assert_eq!(codes::mirrored(0, &comp), field(&f, 2), "page map, compressed");
    // The section map, page 18 at 0xF120: 15 codewords in a page of 0x1200.
    let comp = stored(&file, 0xF120, 15, Code::System, field(&f, 22) as usize);
    let raw = decompress(&comp, field(&f, 25) as usize).unwrap();
    assert_eq!(codes::mirrored(0, &raw), field(&f, 21), "section map");
    assert_eq!(codes::mirrored(0, &comp), field(&f, 26), "section map, compressed");
}

#[test]
fn page_checks_are_autocads() {
    let file = autocad();
    // AcDb:Template, stored as it is (6 bytes) in one codeword at 0xE520.
    let raw = stored(&file, 0xE520, 1, Code::Page, 6);
    assert_eq!(codes::page_checksum(&raw), 0x4873_36B2);
    assert_eq!(codes::mirrored(0, &raw), 0xEEAB_8DEC_FB07_4960);
    // The first page of AcDb:AcDbObjects: 0x2A8B bytes for 0xF480, 44 codewords at 0xAA40.
    let comp = stored(&file, 0xAA40, 44, Code::Page, 0x2A8B);
    assert_eq!(codes::mirrored(0, &comp), 0xB7F3_5BCB_CC06_E535);
    let raw = decompress(&comp, 0xF480).unwrap();
    assert_eq!(codes::page_checksum(&raw), 0x7ED7_929E);
}

#[test]
fn the_codewords_are_autocads() {
    let file = autocad();
    for (what, at, blocks, code) in
        [("page map", 0x480, 4, Code::System), ("objects", 0xAA40, 44, Code::Page), ("section map", 0xF120, 15, Code::System)]
    {
        for (i, cw) in codewords(&file, at, blocks).iter().enumerate() {
            let k = code.data();
            let check = code.parities(&cw[..k]);
            assert_eq!(check[0], cw[k..], "{what}: codeword {i}");
        }
    }
}

/// The encoded seeds, the check data and the fill come from the file's random seed.
#[test]
fn the_random_numbers_are_autocads() {
    let file = autocad();
    let (_, f) = header(&file);
    let mut r = Random::new(field(&f, 32));
    r.skip(37);
    assert_eq!(r.encode(0), field(&f, 28), "section map CRC seed");
    assert_eq!(r.encode(0), field(&f, 4), "page map CRC seed");
    let page = &file[HEADER_AT..HEADER_AT + 0x400];
    let check = &page[0x3D8..];
    let (r1, r2) = (r.next(), r.next());
    assert_eq!((r1, r2), (u64_at(check, 16).unwrap(), u64_at(check, 24).unwrap()), "check data randoms");
    assert_eq!(r.encode(0), u64_at(check, 32).unwrap(), "check data seed");
    assert_eq!(r.encode(0), field(&f, 31), "CRC seed");
    let words = |w: [u64; 8]| w.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
    let mut b = [0u64; 8];
    b[0] = rotated(r1, r2);
    b[1] = rotated(b[0], b[0]);
    b[2] = rotated(r2, b[1]);
    b[3] = rotated(b[2], b[2]);
    b[4] = rotated(r1, b[3]);
    b[5] = rotated(b[4], b[4]);
    b[6] = rotated(b[5], b[5]);
    b[7] = rotated(b[6], b[6]);
    let normal = codes::check_normal(r2, &words(b));
    assert_eq!(normal, u64_at(check, 0).unwrap(), "check data normal CRC");
    let mut m = [0u64; 8];
    m[0] = rotated(r1, r2);
    m[1] = rotated(normal, m[0]);
    m[2] = rotated(r2, m[1]);
    m[3] = rotated(normal, m[2]);
    m[4] = rotated(r1, m[3]);
    m[5] = rotated(normal, m[4]);
    m[6] = rotated(r2, m[5]);
    m[7] = rotated(m[6], m[6]);
    assert_eq!(codes::check_mirrored(r1, &words(m)), u64_at(check, 8).unwrap(), "check data mirrored CRC");
    // The checking sequence of the header's block is the next number.
    let (block, _) = header(&file);
    assert_eq!(r.next(), u64_at(&block, 8).unwrap(), "checking sequence");
    // The room after the header's codewords is filled from the first.
    assert_eq!(&page[3 * CODEWORD..3 * CODEWORD + 16], &r.fill(16)[..]);
}

/// The pages of the small sections, stored as they are with their check bytes, are AutoCAD's to
/// the byte: the data, the check bytes of its blocks of 251 (the last made up with fill), fill.
#[test]
fn plain_pages_are_autocads() {
    let file = autocad();
    let (_, f) = header(&file);
    let r = Random::new(field(&f, 32));
    // (file address, page size, data length, the most the section holds, checksum, CRC)
    for (at, size, len, max, checksum, crc) in [
        (0xC80usize, 0xA0usize, 0x4Eusize, 0x80usize, 0x1068_5DCAu32, 0xD85D_CB99_B55F_B828u64),
        (0x9F80, 0x320, 0x2CE, 0x300, 0xA802_D401, 0xCF0D_466D_EF04_BC00),
        (0xA2A0, 0x5A0, 0x522, 0x580, 0x7EF2_8B2D, 0xC4CF_56B6_8180_2170),
        (0xD20, 0x9260, 0x8DAF, 0x9000, 0x3C0F_4539, 0x7EF8_DF31_166C_0CAC),
    ] {
        let data = &file[at..at + len];
        // AutoCAD pads the data to 8 bytes with what its buffer held; the check bytes cover it.
        let padded = &file[at..at + len.div_ceil(8) * 8];
        assert_eq!(super::plain_page(padded, max, &r), &file[at..at + size], "page at {at:#x}");
        assert_eq!(codes::page_checksum(data), checksum, "page at {at:#x}");
        assert_eq!(codes::mirrored(0, data), crc, "page at {at:#x}");
    }
}

/// The file header's check data comes from the random numbers alone, and its block holds what
/// the reader takes.
#[test]
fn the_header_page_is_autocads() {
    let file = autocad();
    let (block, f) = header(&file);
    let mut r = Random::new(field(&f, 32));
    let draws = super::Draws::new(&mut r);
    assert_eq!(draws.sequence, u64_at(&block, 8).unwrap());
    let page = super::header_page(&f, &draws, &r);
    assert_eq!(page.len(), 0x400);
    assert_eq!(&page[super::CHECK_AT..], &file[HEADER_AT + super::CHECK_AT..HEADER_AT + 0x400], "check data");
    assert_eq!(&page[3 * CODEWORD..super::CHECK_AT], &file[HEADER_AT + 3 * CODEWORD..HEADER_AT + super::CHECK_AT], "fill");
    let mut ours = vec![0u8; HEADER_AT];
    ours.extend_from_slice(&page);
    let (ours, fields) = header(&ours);
    assert_eq!(fields, f);
    assert_eq!(&ours[..16], &block[..16], "checking sequence");
}

/// A map page holds its data as many times as AutoCAD's does, in a page of the same size.
#[test]
fn map_pages_are_sized_as_autocads() {
    let file = autocad();
    let (_, f) = header(&file);
    let r = Random::new(field(&f, 32));
    for (len, size) in [(0x130usize, 0x400usize), (0x856, 0x1200)] {
        assert_eq!(super::system_size(len), size);
    }
    // AutoCAD's own compressed page map, laid out again.
    let at = PAGES_AT as usize + field(&f, 7) as usize;
    let comp = stored(&file, at, 4, Code::System, field(&f, 10) as usize);
    let raw = decompress(&comp, field(&f, 11) as usize).unwrap();
    let page = super::system_page(&raw, &r);
    assert_eq!(page.page.len(), 0x400);
    assert!(page.stored.len() <= 0x69, "{:#x}", page.stored.len());
    assert_eq!(page.repeat, 4 * 239 / (page.stored.len().div_ceil(8) * 8));
    assert_eq!(decompress(&stored(&page.page, 0, 4, Code::System, page.stored.len()), raw.len()).unwrap(), raw);
}

#[test]
#[ignore = "scratch"]
fn scratch_ours() {
    let Ok(path) = std::env::var("DWG_FILE") else { return };
    let file = std::fs::read(path).unwrap();
    let (_, f) = header(&file);
    let at = PAGES_AT as usize + field(&f, 7) as usize;
    let blocks = (field(&f, 10) as usize).div_ceil(8) * 8 * field(&f, 3) as usize;
    let comp = stored(&file, at, blocks.div_ceil(239), Code::System, field(&f, 10) as usize);
    let pm = decompress(&comp, field(&f, 11) as usize).unwrap();
    let mut off = 0u64;
    for c in pm.chunks(16) {
        let (s, id) = (u64_at(c, 0).unwrap(), u64_at(c, 8).unwrap() as i64);
        println!("page id {id} at {:#x} size {s:#x}", off + PAGES_AT);
        off += s;
    }
    let c = crate::read::r2007::read(&file, crate::read::version::DwgVersion::R2007).unwrap();
    for s in &c.sections {
        println!("{} {:#x}", s.name, s.size);
    }
}
