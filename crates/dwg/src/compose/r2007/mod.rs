//! AutoCAD 2007 (AC1021): the container of Reed-Solomon codewords (spec chapter 5), laid out as
//! AutoCAD lays its own files out.
//!
//! After 0x80 bytes of metadata come the file header (its fields compressed, in three codewords,
//! with check data at the end of its 0x400 bytes), then the pages from 0x480: the page map and a
//! copy of it, the data pages in the order AutoCAD writes the sections, the section map and its
//! copy, and last a copy of the file header. The page map and the section map are compressed and
//! stored in codewords of 239 data bytes as many times over as their page has room for; data
//! pages are compressed and stored in codewords of 251 data bytes, except the four small sections
//! (properties, preview, application information and history), which are stored as they are
//! with their check bytes after them, where AutoCAD and other programs read them without
//! decoding. Room a page leaves over holds the file's fill bytes.

mod codes;
mod compress;
mod random;

use codes::{CODEWORD, Code, PAGE_DATA, SYSTEM_DATA};
use compress::compress;
use random::{Random, rotated};

use crate::read::r2007::{HEADER_FIELDS, PAGES_AT};
use crate::records::{Error, Result};

/// What the file holds, as the shared writer builds it.
pub(super) struct Input<'a> {
    pub codepage: u16,
    pub maintenance: u8,
    /// The seed of the file's random numbers (its fill bytes, the hidden CRC seeds).
    pub seed: u64,
    /// Every section but the preview, by name.
    pub sections: Vec<(&'static str, Vec<u8>)>,
    /// The preview section, given the file address its page starts at (its entries say where
    /// their data is in the file).
    pub preview: &'a dyn Fn(u64) -> Vec<u8>,
    /// The order of the section map.
    pub map_order: &'a [&'static str],
}

/// A section's hash, the most one of its pages holds as AutoCAD records it, and whether its
/// pages are stored as they are (encoding 1) rather than compressed in codewords (encoding 4).
/// In the order AutoCAD writes their pages.
const SECTIONS: [(&str, u64, usize, bool); 13] = [
    ("AcDb:SummaryInfo", 0x717A_060F, 0x80, true),
    ("AcDb:Preview", 0x40AA_0473, 0x400, true),
    ("AcDb:AppInfo", 0x3FA0_043E, 0x80, true),
    ("AcDb:AppInfoHistory", 0x96DE_0737, 0x80, true),
    ("AcDb:XrefManifest", 0x7AE4_0662, 0xF800, false),
    ("AcDb:RevHistory", 0x60A2_05B3, 0x1000, false),
    ("AcDb:AcDbObjects", 0x674C_05A9, 0xF800, false),
    ("AcDb:ObjFreeSpace", 0x77E2_061F, 0xF800, false),
    ("AcDb:Template", 0x4A14_04CE, 0x400, false),
    ("AcDb:Handles", 0x3F6E_0450, 0xF800, false),
    ("AcDb:Classes", 0x3F54_045F, 0xF800, false),
    ("AcDb:AuxHeader", 0x54F0_050A, 0x800, false),
    ("AcDb:Header", 0x32B8_03D9, 0x800, false),
];

/// The most a compressed page holds before the section goes on in another (AutoCAD's first page
/// of objects).
const PAGE_CHUNK: usize = 0xF480;
/// The size of the file header and of its copy at the end.
const HEADER_PAGE: usize = 0x400;
/// Where the file header's check data is in it.
const CHECK_AT: usize = 0x3D8;
/// The first page id of the data pages: AutoCAD leaves 1 and 2 unused.
const FIRST_ID: u64 = 3;

/// `n` rounded up to a multiple of `to`.
fn align(n: usize, to: usize) -> usize {
    n.div_ceil(to) * to
}

/// `out` filled up to `to` bytes with the file's fill bytes, from the first.
fn fill(r: &Random, out: &mut Vec<u8>, to: usize) {
    let n = to.saturating_sub(out.len());
    out.extend(r.fill(n));
}

/// `raw` compressed, or as it is when compressing doesn't make it shorter.
fn packed(raw: &[u8]) -> Vec<u8> {
    compress(raw).filter(|c| c.len() < raw.len()).unwrap_or_else(|| raw.to_vec())
}

/// A data page: `raw` compressed, in codewords of 251 data bytes. The page, and the bytes it
/// stores (what its CRC is of).
fn data_page(raw: &[u8], r: &Random) -> (Vec<u8>, Vec<u8>) {
    let stored = packed(raw);
    let a = align(stored.len(), 8);
    let blocks = a.div_ceil(PAGE_DATA).max(1);
    let mut data = stored.clone();
    data.resize(a, 0);
    fill(r, &mut data, blocks * PAGE_DATA);
    let mut page = Code::Page.interleaved(&data);
    let size = align(page.len(), 32);
    fill(r, &mut page, size);
    (page, stored)
}

/// A page of the small sections: `raw` as it is, padded to 8 bytes, the check bytes of each of
/// its blocks of 251 (the last one made up with fill), in a page sized for `max` bytes.
fn plain_page(raw: &[u8], max: usize, r: &Random) -> Vec<u8> {
    let a = align(raw.len(), 8);
    let blocks = a.div_ceil(PAGE_DATA);
    let mut page = raw.to_vec();
    page.resize(a, 0);
    let mut blocks_data = page.clone();
    fill(r, &mut blocks_data, blocks * PAGE_DATA);
    for check in Code::Page.parities(&blocks_data) {
        page.extend_from_slice(&check);
    }
    let max = max.max(a);
    fill(r, &mut page, align(max + max.div_ceil(PAGE_DATA) * 4, 32));
    page
}

/// The page size of a map that decompresses to `len` bytes.
fn system_size(len: usize) -> usize {
    let s = align(len, 8) * 2;
    align(s.div_ceil(SYSTEM_DATA) * CODEWORD, 32).max(0x400)
}

/// A page map or a section map: compressed, repeated as many times as the page has room for, in
/// codewords of 239 data bytes.
struct SystemPage {
    page: Vec<u8>,
    stored: Vec<u8>,
    repeat: usize,
}

fn system_page(raw: &[u8], r: &Random) -> SystemPage {
    let stored = packed(raw);
    let a = align(stored.len(), 8).max(8);
    let size = system_size(raw.len());
    let repeat = (size / CODEWORD * SYSTEM_DATA / a).max(1);
    let mut data = Vec::with_capacity(repeat * a);
    for i in 1..=repeat {
        data.extend_from_slice(&stored);
        data.resize(i * a, 0);
    }
    let blocks = data.len().div_ceil(SYSTEM_DATA);
    fill(r, &mut data, blocks * SYSTEM_DATA);
    let mut page = Code::System.interleaved(&data);
    fill(r, &mut page, size);
    SystemPage { page, stored, repeat }
}

/// The random numbers the file header holds, drawn in AutoCAD's order.
struct Draws {
    section_map_seed: u64,
    page_map_seed: u64,
    r1: u64,
    r2: u64,
    check_seed: u64,
    crc_seed: u64,
    sequence: u64,
}

impl Draws {
    fn new(r: &mut Random) -> Draws {
        // AutoCAD draws 37 words for its own ends before the file header's.
        r.skip(37);
        let section_map_seed = r.encode(0);
        let page_map_seed = r.encode(0);
        let (r1, r2) = (r.next(), r.next());
        let check_seed = r.encode(0);
        let crc_seed = r.encode(0);
        let sequence = r.next();
        Draws { section_map_seed, page_map_seed, r1, r2, check_seed, crc_seed, sequence }
    }
}

fn words(w: &[u64]) -> Vec<u8> {
    w.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// The file header's 0x400 bytes: its fields compressed behind their checks, repeated over three
/// codewords, the fill, and the check data.
fn header_page(fields: &[u8], d: &Draws, r: &Random) -> Vec<u8> {
    let stored = packed(fields);
    let length = i32::try_from(stored.len()).unwrap_or(i32::MAX);
    let length = if stored.len() < fields.len() { length } else { -length };
    let v2 = rotated(d.sequence, d.sequence);
    let mut unit = codes::normal_seed1(0, &words(&[d.sequence, v2])).to_le_bytes().to_vec();
    unit.extend_from_slice(&d.sequence.to_le_bytes());
    unit.extend_from_slice(&codes::normal(0, &stored).to_le_bytes());
    unit.extend_from_slice(&length.to_le_bytes());
    unit.extend_from_slice(&0u32.to_le_bytes());
    unit.extend_from_slice(&stored);
    unit.resize(align(unit.len(), 8), 0);
    let block: Vec<u8> = unit.iter().copied().cycle().take(3 * SYSTEM_DATA).collect();
    let mut page = Code::System.interleaved(&block);
    fill(r, &mut page, CHECK_AT);
    page.truncate(CHECK_AT);
    // The check data: two CRCs of words made from two random numbers, the numbers, and a seed.
    let (r1, r2) = (d.r1, d.r2);
    let mut b = [0u64; 8];
    b[0] = rotated(r1, r2);
    b[1] = rotated(b[0], b[0]);
    b[2] = rotated(r2, b[1]);
    b[3] = rotated(b[2], b[2]);
    b[4] = rotated(r1, b[3]);
    b[5] = rotated(b[4], b[4]);
    b[6] = rotated(b[5], b[5]);
    b[7] = rotated(b[6], b[6]);
    let normal = codes::check_normal(r2, &words(&b));
    let mut m = [0u64; 8];
    m[0] = rotated(r1, r2);
    m[1] = rotated(normal, m[0]);
    m[2] = rotated(r2, m[1]);
    m[3] = rotated(normal, m[2]);
    m[4] = rotated(r1, m[3]);
    m[5] = rotated(normal, m[4]);
    m[6] = rotated(r2, m[5]);
    m[7] = rotated(m[6], m[6]);
    let mirrored = codes::check_mirrored(r1, &words(&m));
    page.extend_from_slice(&words(&[normal, mirrored, r1, r2, d.check_seed]));
    page
}

/// A data page laid out: where it is among the pages, and what the section map says of it.
struct Page {
    id: u64,
    bytes: Vec<u8>,
    /// Where its data starts in its section.
    start: u64,
    uncompressed: u64,
    compressed: u64,
    checksum: u64,
    crc: u64,
}

/// A section laid out.
struct Laid {
    name: &'static str,
    size: u64,
    max: u64,
    hash: u64,
    plain: bool,
    pages: Vec<Page>,
}

fn u64s(out: &mut Vec<u8>, values: &[u64]) {
    for v in values {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

/// The section map: each section in the map's order, with its pages, and an empty one last.
fn section_map(laid: &[Laid], order: &[&str]) -> Vec<u8> {
    let mut out = Vec::new();
    for name in order {
        let Some(s) = laid.iter().find(|s| s.name == *name) else { continue };
        let units: Vec<u16> = s.name.encode_utf16().chain([0]).collect();
        let encoding = if s.plain { 1 } else { 4 };
        u64s(&mut out, &[s.size, s.max, 0, s.hash, (units.len() * 2) as u64, 0, encoding, s.pages.len() as u64]);
        for u in units {
            out.extend_from_slice(&u.to_le_bytes());
        }
        for p in &s.pages {
            u64s(&mut out, &[p.start, s.max, p.id, p.uncompressed, p.compressed, p.checksum, p.crc]);
        }
    }
    u64s(&mut out, &[0, 0xF800, 0, 0, 0, 0, 4, 0]);
    out
}

fn address(n: usize) -> Result<u32> {
    u32::try_from(n).map_err(|_| Error::format("the drawing is too large for an AutoCAD 2007 file"))
}

/// Lay the file out.
pub(super) fn compose(input: &Input<'_>) -> Result<Vec<u8>> {
    let mut random = Random::new(input.seed);
    let draws = Draws::new(&mut random);
    let r = &random;

    // The sections' contents and how many pages each takes, so that the page map's size (and so
    // where the data pages start) is known before the preview, which records its own address.
    let preview_len = (input.preview)(0).len();
    let content = |name: &str| input.sections.iter().find(|(n, _)| *n == name).map_or(&[][..], |(_, d)| d.as_slice());
    let chunk = |max: usize| max.min(PAGE_CHUNK);
    let count: usize = SECTIONS
        .iter()
        .map(|&(name, _, max, plain)| {
            let len = if name == "AcDb:Preview" { preview_len } else { content(name).len() };
            if plain { 1 } else { len.div_ceil(chunk(max)) }
        })
        .sum();
    let page_count = count + 4;
    let page_map_size = system_size(16 * page_count);

    let mut laid: Vec<Laid> = Vec::with_capacity(SECTIONS.len());
    let mut id = FIRST_ID;
    let mut offset = 2 * page_map_size;
    // The file addresses of the small sections, for the metadata.
    let mut addresses = std::collections::HashMap::new();
    for &(name, hash, max, plain) in &SECTIONS {
        let mut pages = Vec::new();
        let (size, max) = if plain {
            let at = PAGES_AT as usize + offset;
            let data = if name == "AcDb:Preview" { (input.preview)(at as u64) } else { content(name).to_vec() };
            let max = align(data.len(), max).max(max);
            addresses.insert(name, at);
            let bytes = plain_page(&data, max, r);
            let n = data.len() as u64;
            pages.push(Page {
                id,
                start: 0,
                uncompressed: n,
                compressed: n,
                checksum: u64::from(codes::page_checksum(&data)),
                crc: codes::mirrored(0, &data),
                bytes,
            });
            offset += pages.last().map_or(0, |p| p.bytes.len());
            id += 1;
            (n, max)
        } else {
            let data = content(name);
            for (i, raw) in data.chunks(chunk(max)).enumerate() {
                let (bytes, stored) = data_page(raw, r);
                offset += bytes.len();
                pages.push(Page {
                    id,
                    start: (i * chunk(max)) as u64,
                    uncompressed: raw.len() as u64,
                    compressed: stored.len() as u64,
                    checksum: u64::from(codes::page_checksum(raw)),
                    crc: codes::mirrored(0, &stored),
                    bytes,
                });
                id += 1;
            }
            (data.len() as u64, max)
        };
        laid.push(Laid { name, size, max: max as u64, hash, plain, pages });
    }
    let data_pages: Vec<&Page> = laid.iter().flat_map(|s| &s.pages).collect();
    if data_pages.len() != count {
        return Err(Error::format("the AutoCAD 2007 page count changed while laying the file out"));
    }

    let section_map_id = id;
    let section_map = section_map(&laid, input.map_order);
    let sm = system_page(&section_map, r);
    let page_map_id = section_map_id + 4;
    let mut page_map = Vec::with_capacity(16 * page_count);
    u64s(&mut page_map, &[page_map_size as u64, page_map_id, page_map_size as u64, page_map_id + 1]);
    for p in &data_pages {
        u64s(&mut page_map, &[p.bytes.len() as u64, p.id]);
    }
    u64s(&mut page_map, &[sm.page.len() as u64, section_map_id, sm.page.len() as u64, section_map_id + 1]);
    let pm = system_page(&page_map, r);
    if pm.page.len() != page_map_size {
        return Err(Error::format("the AutoCAD 2007 page map's size changed while laying the file out"));
    }

    let copy_at = offset + 2 * sm.page.len();
    let file_size = PAGES_AT as usize + copy_at + HEADER_PAGE;
    let mut f = [0u64; HEADER_FIELDS / 8];
    let fields: [(usize, u64); 33] = [
        (0, 0x70),
        (1, file_size as u64),
        (2, codes::mirrored(0, &pm.stored)),
        (3, pm.repeat as u64),
        (4, draws.page_map_seed),
        (5, page_map_size as u64),
        (6, page_map_id + 1),
        (7, 0),
        (8, page_map_id),
        (9, copy_at as u64),
        (10, pm.stored.len() as u64),
        (11, page_map.len() as u64),
        (12, page_count as u64),
        (13, page_map_id + 1),
        (14, 0x20),
        (15, 0x40),
        (16, codes::mirrored(0, &page_map)),
        (17, 0xF800),
        (18, 4),
        (19, 1),
        (20, SECTIONS.len() as u64 + 1),
        (21, codes::mirrored(0, &section_map)),
        (22, sm.stored.len() as u64),
        (23, section_map_id + 1),
        (24, section_map_id),
        (25, section_map.len() as u64),
        (26, codes::mirrored(0, &sm.stored)),
        (27, sm.repeat as u64),
        (28, draws.section_map_seed),
        (29, 0x60100),
        (30, 0),
        (31, draws.crc_seed),
        (32, input.seed),
    ];
    for (i, v) in fields {
        if let Some(slot) = f.get_mut(i) {
            *slot = v;
        }
    }
    let crc = codes::normal(0, &words(&f));
    if let Some(slot) = f.last_mut() {
        *slot = crc;
    }
    let header = header_page(&words(&f), &draws, r);

    let mut out = Vec::with_capacity(file_size);
    out.extend_from_slice(b"AC1021");
    out.extend_from_slice(&[0; 5]);
    out.push(input.maintenance);
    out.push(3);
    let at = |name: &str| address(addresses.get(name).copied().unwrap_or(0));
    out.extend_from_slice(&at("AcDb:Preview")?.to_le_bytes());
    out.extend_from_slice(&[0x21, 0xFF]);
    out.extend_from_slice(&input.codepage.to_le_bytes());
    out.extend_from_slice(&[0, 0, 0x21, 0xFF]);
    out.resize(0x20, 0);
    for v in [at("AcDb:SummaryInfo")?, 0, 0x80, at("AcDb:AppInfo")?, at("AcDb:AppInfoHistory")?, 0x202, 0x1DD] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.resize(0x80, 0);
    out.extend_from_slice(&header);
    out.extend_from_slice(&pm.page);
    out.extend_from_slice(&pm.page);
    for p in &data_pages {
        out.extend_from_slice(&p.bytes);
    }
    out.extend_from_slice(&sm.page);
    out.extend_from_slice(&sm.page);
    out.extend_from_slice(&header);
    if out.len() != file_size {
        return Err(Error::format("the AutoCAD 2007 file's size changed while laying it out"));
    }
    address(file_size)?;
    Ok(out)
}

#[cfg(test)]
mod tests;
