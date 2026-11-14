//! The whole file from its parts, laid out as its version stores it.

use crate::bits::BitWriter;
use crate::crc::{crc16, file_header_crc_xor};
use crate::file::HeaderValue;
use crate::read::section::FILE_HEADER_END;
use crate::read::version::DwgVersion as V;
use crate::records::{Error, Result};
use crate::write::container::{self, FileInfo, MAX_PAGE, Section};
use crate::write::sections as shared;

use super::object::TextEncoding;
use super::sections::{Class, classes_section, header_section, object_map};

/// The drawing's properties (DWGPROPS), which a file keeps apart from its objects.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Properties {
    pub title: String,
    pub subject: String,
    pub author: String,
    pub keywords: String,
    pub comments: String,
    pub last_saved_by: String,
    pub revision: String,
    pub hyperlink_base: String,
    pub custom: Vec<(String, String)>,
}

/// Everything a DWG file holds, ready to be laid out.
pub struct Parts<'a> {
    pub version: V,
    /// The code page number of the file's 8-bit text (30 is Windows-1252).
    pub codepage: u16,
    pub text: TextEncoding,
    /// A header variable's value by its DWG name; `None` for AutoCAD's default.
    pub header: &'a dyn Fn(&str) -> Option<HeaderValue>,
    pub classes: &'a [Class],
    /// Every object, finished ([`super::ObjectWriter::finish`]), with its handle, in the order
    /// they are stored.
    pub objects: &'a [(u64, Vec<u8>)],
    /// The handle the next new object gets: above every handle in `objects`.
    pub handseed: u64,
    /// 0 imperial, 1 metric.
    pub measurement: u16,
    /// When the drawing was created and last saved (Julian dates, universal time), and the time
    /// spent editing it (days).
    pub created: f64,
    pub updated: f64,
    pub editing: f64,
    /// The same two moments as `YYYY-MM-DDTHH:MM:SS`, for the save history.
    pub created_iso: &'a str,
    pub updated_iso: &'a str,
    pub properties: &'a Properties,
    /// The picture file browsers show, as a PNG (stored from AutoCAD 2013) and as a Windows
    /// bitmap without its file header (stored before).
    pub preview_png: Option<&'a [u8]>,
    pub preview_bmp: Option<&'a [u8]>,
    /// The program writing the file, recorded as its writer. The file never claims to come from
    /// another program.
    pub app_name: &'a str,
    pub app_version: &'a str,
    /// The font file of each text style, with the style's handle.
    pub fonts: &'a [(String, u64)],
}

/// The maintenance release each version's file records, as AutoCAD writes it today.
fn maintenance(ver: V) -> u8 {
    match ver {
        V::R2000 => 15,
        V::R2004 => 104,
        V::R2007 => 50,
        V::R2010 => 226,
        V::R2013 => 125,
        _ => 0,
    }
}

fn round_up(n: usize, to: usize) -> usize {
    n.max(1).div_ceil(to) * to
}

fn td(out: &mut Vec<u8>, v: f64) {
    let v = if v.is_finite() && v > 0.0 { v } else { 0.0 };
    let day = v.floor();
    let ms = ((v - day) * 86_400_000.0).round();
    out.extend_from_slice(&(day as u32).to_le_bytes());
    out.extend_from_slice(&(ms as u32).to_le_bytes());
}

/// A string of the small sections: its length counting the 0 that ends it, then UTF-16 (from
/// AutoCAD 2007) or the file's 8-bit text.
fn summary_text(out: &mut Vec<u8>, s: &str, p: &Parts<'_>) {
    if p.version >= V::R2007 {
        let units: Vec<u16> = s.encode_utf16().filter(|&u| u != 0).take(0x7FFE).chain([0]).collect();
        out.extend_from_slice(&u16::try_from(units.len()).unwrap_or(u16::MAX).to_le_bytes());
        for u in units {
            out.extend_from_slice(&u.to_le_bytes());
        }
    } else {
        let mut bytes: Vec<u8> = (p.text.ansi)(s, p.codepage).into_iter().filter(|&b| b != 0).take(0x7FFE).collect();
        bytes.push(0);
        out.extend_from_slice(&u16::try_from(bytes.len()).unwrap_or(u16::MAX).to_le_bytes());
        out.extend_from_slice(&bytes);
    }
}

/// The drawing's properties, its editing time and dates, its custom properties.
fn summary_info(p: &Parts<'_>) -> Vec<u8> {
    let q = p.properties;
    let mut o = Vec::new();
    for s in [&q.title, &q.subject, &q.author, &q.keywords, &q.comments, &q.last_saved_by, &q.revision, &q.hyperlink_base] {
        summary_text(&mut o, s, p);
    }
    td(&mut o, p.editing);
    td(&mut o, p.created);
    td(&mut o, p.updated);
    let custom = q.custom.get(..q.custom.len().min(1024)).unwrap_or(&[]);
    o.extend_from_slice(&u16::try_from(custom.len()).unwrap_or(0).to_le_bytes());
    for (k, v) in custom {
        summary_text(&mut o, k, p);
        summary_text(&mut o, v, p);
    }
    o.extend_from_slice(&[0; 8]);
    o
}

/// The auxiliary header: the version and maintenance release three times, the save count, the
/// creation and update times, the handle seed. As AutoCAD writes it for each version.
fn aux_header(p: &Parts<'_>) -> Vec<u8> {
    let ver: u16 = 33;
    let saves: u32 = 1;
    let maint = |o: &mut Vec<u8>| {
        if p.version >= V::R2018 {
            o.extend_from_slice(&shared::ACAD_MAINTENANCE.to_le_bytes());
        } else {
            o.extend_from_slice(&u16::try_from(shared::ACAD_MAINTENANCE).unwrap_or(0).to_le_bytes());
        }
    };
    let mut o = vec![0xFF, 0x77, 0x01];
    o.extend_from_slice(&ver.to_le_bytes());
    maint(&mut o);
    o.extend_from_slice(&saves.to_le_bytes());
    o.extend_from_slice(&(-1i32).to_le_bytes());
    o.extend_from_slice(&1u16.to_le_bytes());
    o.extend_from_slice(&0u16.to_le_bytes());
    o.extend_from_slice(&0u32.to_le_bytes());
    for _ in 0..2 {
        o.extend_from_slice(&ver.to_le_bytes());
        maint(&mut o);
    }
    o.extend_from_slice(&[0; 8]);
    o.extend_from_slice(&0u16.to_le_bytes());
    o.extend_from_slice(&1u16.to_le_bytes());
    o.extend_from_slice(&[0; 12]);
    o.extend_from_slice(&[0x00, 0x01, 0x00, 0x00, 0x01, 0x00]);
    o.extend_from_slice(&1u16.to_le_bytes());
    td(&mut o, p.created);
    td(&mut o, p.updated);
    let seed = u32::try_from(p.handseed).ok().filter(|s| *s < 0x7FFF_FFFF).unwrap_or(u32::MAX);
    o.extend_from_slice(&seed.to_le_bytes());
    o.extend_from_slice(&0u32.to_le_bytes());
    o.extend_from_slice(&0u16.to_le_bytes());
    o.extend_from_slice(&1u16.to_le_bytes());
    o.extend_from_slice(&[0; 12]);
    o.extend_from_slice(&saves.to_le_bytes());
    o.extend_from_slice(&[0; 16]);
    o
}

/// The template section: the description's length (none) and MEASUREMENT.
fn template(p: &Parts<'_>) -> Vec<u8> {
    let mut o = if p.version >= V::R2007 { vec![1, 0, 0, 0] } else { vec![0, 0] };
    o.extend_from_slice(&p.measurement.min(1).to_le_bytes());
    o
}

/// The free-space section: the object count, the update time, and the four value pairs every
/// file of AutoCAD's has.
fn obj_free_space(p: &Parts<'_>) -> Vec<u8> {
    let count = p.objects.len() as u64;
    let mut o = Vec::with_capacity(89);
    if p.version >= V::R2010 {
        o.extend_from_slice(&0u64.to_le_bytes());
        o.extend_from_slice(&count.to_le_bytes());
        td(&mut o, p.updated);
        o.push(4);
        for v in [0x32u64, 0x64, 0x200, 0xFFFF_FFFF] {
            o.extend_from_slice(&v.to_le_bytes());
            o.extend_from_slice(&0u64.to_le_bytes());
        }
    } else {
        o.extend_from_slice(&0u32.to_le_bytes());
        o.extend_from_slice(&u32::try_from(count).unwrap_or(u32::MAX).to_le_bytes());
        td(&mut o, p.updated);
        o.extend_from_slice(&0u32.to_le_bytes());
        o.push(4);
        for v in [0x32u32, 0x64, 0x200, 0xFFFF_FFFF] {
            o.extend_from_slice(&v.to_le_bytes());
            o.extend_from_slice(&0u32.to_le_bytes());
        }
    }
    o
}

fn xml_text(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn json_text(s: &str) -> String {
    let mut o = String::new();
    for c in s.chars() {
        match c {
            '"' => o += "\\\"",
            '\\' => o += "\\\\",
            c if u32::from(c) < 0x20 => o += &format!("\\u{:04x}", u32::from(c)),
            c => o.push(c),
        }
    }
    o
}

/// The list of files the drawing refers to (each text style's font), as JSON after its length.
fn xref_manifest(p: &Parts<'_>) -> Vec<u8> {
    let entries: Vec<String> = p
        .fonts
        .iter()
        .filter(|(f, _)| !f.is_empty())
        .map(|(font, handle)| {
            format!(
                "        {{\n            \"path\": \"{}\",\n            \"handle\": \"{handle:x}\",\n            \"class\": \"AcDbTextStyleTableRecord\"\n        }}",
                json_text(font)
            )
        })
        .collect();
    let list = if entries.is_empty() { "[]".to_string() } else { format!("[\n{}\n    ]", entries.join(",\n")) };
    let json = format!("{{\n    \"xrefs\": {list},\n    \"version\": \"1.0\",\n    \"wasFullSave\": \"true\"\n}}\n");
    let mut o = (json.len() as u64).to_le_bytes().to_vec();
    o.extend_from_slice(json.as_bytes());
    o
}

/// The save history: the same list as the application information, with a property set (when
/// the drawing was created and saved, by whom, with which program).
fn app_info_history(p: &Parts<'_>) -> Vec<u8> {
    let (name, version) = (xml_text(p.app_name), xml_text(p.app_version));
    let mut props = String::from("<prop_set fmt_id=\"{f29f85e0-4ff9-1068-ab91-08002b27b3d9}\">");
    props += &format!("<prop id=\"12\"><datetime>{}</datetime></prop>", xml_text(p.created_iso));
    props += &format!("<prop id=\"10\"><datetime>{}</datetime></prop>", xml_text(p.updated_iso));
    props += "<prop id=\"257\"><string></string></prop>";
    props += &format!("<prop id=\"8\"><string>{}</string></prop>", xml_text(&p.properties.last_saved_by));
    props += &format!("<prop id=\"258\"><string>{name}</string></prop>");
    props += &format!("<prop id=\"259\"><string>{version}</string></prop>");
    props += "</prop_set>";
    shared::app_info_history_with(&props, p.app_name, p.app_version)
}

const PREVIEW_START: [u8; 16] = [0x1F, 0x25, 0x6D, 0x07, 0xD4, 0x36, 0x28, 0x28, 0x9D, 0x57, 0xCA, 0x3F, 0x9D, 0x44, 0x10, 0x2B];
const PREVIEW_END: [u8; 16] = [0xE0, 0xDA, 0x92, 0xF8, 0x2B, 0xC9, 0xD7, 0xD7, 0x62, 0xA8, 0x35, 0xC0, 0x62, 0xBB, 0xEF, 0xD4];

/// The preview section: a header entry of 80 zero bytes and the picture (a PNG from AutoCAD
/// 2013, a bitmap before; none when the file has no picture of that kind). The entries give
/// where their data is in the file, so the caller passes where the section's first byte will be.
fn preview(p: &Parts<'_>, section_address: u64) -> Vec<u8> {
    let picture: Option<(u8, &[u8])> = if p.version >= V::R2013 { p.preview_png.map(|d| (6, d)) } else { p.preview_bmp.map(|d| (2, d)) };
    let header = [0u8; 80];
    let entries = 1 + usize::from(picture.is_some());
    let head = 16 + 4 + 1 + entries * 9;
    let header_at = section_address + head as u64;
    let picture_at = header_at + header.len() as u64;
    let picture_len = picture.map_or(0, |(_, d)| d.len());
    let mut o = PREVIEW_START.to_vec();
    let size = 1 + entries * 9 + header.len() + picture_len;
    o.extend_from_slice(&u32::try_from(size).unwrap_or(u32::MAX).to_le_bytes());
    o.push(u8::try_from(entries).unwrap_or(1));
    o.push(1);
    o.extend_from_slice(&u32::try_from(header_at).unwrap_or(0).to_le_bytes());
    o.extend_from_slice(&80u32.to_le_bytes());
    if let Some((code, data)) = picture {
        o.push(code);
        o.extend_from_slice(&u32::try_from(picture_at).unwrap_or(0).to_le_bytes());
        o.extend_from_slice(&u32::try_from(data.len()).unwrap_or(0).to_le_bytes());
    }
    o.extend_from_slice(&header);
    if let Some((_, data)) = picture {
        o.extend_from_slice(data);
    }
    o.extend_from_slice(&PREVIEW_END);
    o
}

/// The header section with the handle seed in it.
fn header(p: &Parts<'_>) -> Vec<u8> {
    let seed = p.handseed;
    header_section(p.version, p.codepage, p.text, &|name| if name == "HANDSEED" { Some(HeaderValue::Handle(seed)) } else { (p.header)(name) })
}

/// Lay the file out. An error for a version this writer has no layout for, or for parts that
/// can't make a file (no objects, an object beyond what the format addresses).
pub fn compose(p: &Parts<'_>) -> Result<Vec<u8>> {
    if p.objects.is_empty() {
        return Err(Error::format("the drawing has no objects to write"));
    }
    match p.version {
        V::R2000 => flat(p),
        V::R2004 | V::R2010 | V::R2013 | V::R2018 => paged(p),
        V::R2007 => r2007(p),
        v => Err(Error::unsupported(format!("writing {} files", v.magic()))),
    }
}

/// The objects one after another from `base`, and the object map of where each starts.
fn object_stream(p: &Parts<'_>, base: usize) -> (Vec<u8>, Vec<u8>) {
    let mut data = 0x0DCAu32.to_le_bytes().to_vec();
    let mut map: Vec<(u64, u64)> = Vec::with_capacity(p.objects.len());
    for (h, b) in p.objects {
        map.push((*h, (base + data.len()) as u64));
        data.extend_from_slice(b);
    }
    // The map is in handle order; of two objects with one handle the first is kept.
    map.sort_by_key(|e| e.0);
    map.dedup_by_key(|e| e.0);
    (data, object_map(&map))
}

/// The section map order AutoCAD uses (ids descending from the first).
const MAP_ORDER: [&str; 13] = [
    "AcDb:XrefManifest",
    "AcDb:AppInfoHistory",
    "AcDb:AppInfo",
    "AcDb:Preview",
    "AcDb:SummaryInfo",
    "AcDb:RevHistory",
    "AcDb:AcDbObjects",
    "AcDb:ObjFreeSpace",
    "AcDb:Template",
    "AcDb:Handles",
    "AcDb:Classes",
    "AcDb:AuxHeader",
    "AcDb:Header",
];

/// AutoCAD 2004, 2010, 2013 and 2018: named sections in compressed pages.
fn paged(p: &Parts<'_>) -> Result<Vec<u8>> {
    let ver = p.version;
    let (objects, handles) = object_stream(p, 0);
    let summary = summary_info(p);
    let summary_page = round_up(summary.len(), 0x80);
    let preview_len = preview(p, 0).len();
    let preview_page = round_up(preview_len, 0x400);
    let preview_address = 0x100 + container::uncompressed_page_extent(summary_page) + 0x20;
    let preview = preview(p, preview_address as u64);
    let app_info = shared::app_info(p.app_name, p.app_version);
    let app_info_page = round_up(app_info.len(), 0x80);
    let history = app_info_history(p);
    let history_page = round_up(history.len(), 0x80);
    let compressed = |name, data| Section { name, data, compressed: true, page_size: MAX_PAGE };
    let stream = vec![
        Section { name: "AcDb:SummaryInfo", data: summary, compressed: false, page_size: summary_page },
        Section { name: "AcDb:Preview", data: preview, compressed: false, page_size: preview_page },
        Section { name: "AcDb:AppInfo", data: app_info, compressed: false, page_size: app_info_page },
        Section { name: "AcDb:AppInfoHistory", data: history, compressed: false, page_size: history_page },
        compressed("AcDb:XrefManifest", xref_manifest(p)),
        compressed("AcDb:RevHistory", shared::rev_history()),
        compressed("AcDb:AcDbObjects", objects),
        compressed("AcDb:ObjFreeSpace", obj_free_space(p)),
        compressed("AcDb:Template", template(p)),
        compressed("AcDb:Handles", handles),
        compressed("AcDb:Classes", classes_section(ver, p.codepage, p.text, p.classes)),
        compressed("AcDb:AuxHeader", aux_header(p)),
        compressed("AcDb:Header", header(p)),
    ];
    let mut code = [0u8; 6];
    code.copy_from_slice(ver.magic().as_bytes().get(..6).unwrap_or(b"AC1032"));
    let info = FileInfo {
        version: code,
        maintenance: maintenance(ver),
        app_version: 33,
        app_maintenance: if ver >= V::R2018 { 0 } else { 0xFF },
        codepage: p.codepage,
        acad_maintenance: if ver >= V::R2018 { shared::ACAD_MAINTENANCE } else { 0 },
    };
    container::pack(&info, &stream, &MAP_ORDER)
}

/// AutoCAD 2007: named sections in pages of Reed-Solomon codewords.
fn r2007(p: &Parts<'_>) -> Result<Vec<u8>> {
    let (objects, handles) = object_stream(p, 0);
    let sections = vec![
        ("AcDb:SummaryInfo", summary_info(p)),
        ("AcDb:AppInfo", shared::app_info(p.app_name, p.app_version)),
        ("AcDb:AppInfoHistory", app_info_history(p)),
        ("AcDb:XrefManifest", xref_manifest(p)),
        ("AcDb:RevHistory", shared::rev_history()),
        ("AcDb:AcDbObjects", objects),
        ("AcDb:ObjFreeSpace", obj_free_space(p)),
        ("AcDb:Template", template(p)),
        ("AcDb:Handles", handles),
        ("AcDb:Classes", classes_section(p.version, p.codepage, p.text, p.classes)),
        ("AcDb:AuxHeader", aux_header(p)),
        ("AcDb:Header", header(p)),
    ];
    // The random numbers come from the save's time: two saves of one drawing differ only there,
    // as AutoCAD's do.
    let seed = p.updated.to_bits() & 0x00FF_FFFF_FFFF_FFFF;
    let preview = |at: u64| preview(p, at);
    let input = super::r2007::Input { codepage: p.codepage, maintenance: maintenance(p.version), seed, sections, preview: &preview, map_order: &MAP_ORDER };
    super::r2007::compose(&input)
}

const SECOND_HEADER_START: [u8; 16] = [0xD4, 0x7B, 0x21, 0xCE, 0x28, 0x93, 0x9F, 0xBF, 0x53, 0x24, 0x40, 0x09, 0x12, 0x3C, 0xAA, 0x01];
const SECOND_HEADER_END: [u8; 16] = [0x2B, 0x84, 0xDE, 0x31, 0xD7, 0x6C, 0x60, 0x40, 0xAC, 0xDB, 0xBF, 0xF6, 0xED, 0xC3, 0x55, 0xFE];

/// The second header of an AutoCAD 2000 file: where it is, the version, the section list again
/// and the handles a recovery starts from.
pub(super) fn second_header(location: u32, maintenance: u8, codepage: u16, records: &[(u8, u32, u32)], handles: &[u64]) -> Vec<u8> {
    let mut w = BitWriter::new();
    w.bl(location);
    w.raw_bytes(b"AC1015");
    w.raw_bytes(&[0; 5]);
    w.rc(maintenance);
    w.rc(1);
    w.bs(0xFF21);
    w.rs(codepage);
    w.bs(u16::try_from(records.len()).unwrap_or(0));
    for &(number, address, size) in records {
        w.rc(number);
        w.bl(address);
        w.bl(size);
    }
    w.bs(u16::try_from(handles.len()).unwrap_or(0));
    for (i, &h) in handles.iter().enumerate() {
        let bytes: Vec<u8> = h.to_be_bytes().into_iter().skip_while(|b| *b == 0).collect();
        w.rc(u8::try_from(bytes.len()).unwrap_or(8));
        w.rc(u8::try_from(i).unwrap_or(u8::MAX));
        w.raw_bytes(&bytes);
    }
    let data = w.into_bytes();
    let mut sized = u32::try_from(4 + data.len() + 2 + 8).unwrap_or(u32::MAX).to_le_bytes().to_vec();
    sized.extend_from_slice(&data);
    let crc = crc16(0xC0C1, &sized);
    let mut out = SECOND_HEADER_START.to_vec();
    out.extend_from_slice(&sized);
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&[0; 8]);
    out.extend_from_slice(&SECOND_HEADER_END);
    out
}

/// AutoCAD 2000: the sections one after another, listed at the start of the file.
fn flat(p: &Parts<'_>) -> Result<Vec<u8>> {
    const RECORDS: usize = 6;
    let head = 0x19 + RECORDS * 9 + 2 + 16;
    let aux = aux_header(p);
    let preview_at = head + aux.len();
    let preview = preview(p, preview_at as u64);
    let header = header(p);
    let classes = classes_section(p.version, p.codepage, p.text, p.classes);
    let header_at = preview_at + preview.len();
    let classes_at = header_at + header.len();
    let objects_at = classes_at + classes.len();
    let (objects, map) = object_stream(p, objects_at);
    let map_at = objects_at + objects.len();
    let second_at = map_at + map.len();
    let at = |n: usize| u32::try_from(n).map_err(|_| Error::format("the drawing is too large for an AutoCAD 2000 file"));
    let mut records = vec![
        (0u8, at(header_at)?, at(header.len())?),
        (1, at(classes_at)?, at(classes.len())?),
        (2, at(map_at)?, at(map.len())?),
        (3, 0, 0),
        (4, 0, 0),
        (5, at(head)?, at(aux.len())?),
    ];
    let var = |name: &str| match (p.header)(name) {
        Some(HeaderValue::Handle(h)) => h,
        _ => 0,
    };
    let handles = [
        p.handseed,
        var("BLOCK_CONTROL"),
        var("LAYER_CONTROL"),
        var("STYLE_CONTROL"),
        var("LTYPE_CONTROL"),
        var("VIEW_CONTROL"),
        var("UCS_CONTROL"),
        var("VPORT_CONTROL"),
        var("APPID_CONTROL"),
        var("DIMSTYLE_CONTROL"),
        var("VX_CONTROL"),
        var("DICTIONARY_NAMED_OBJECTS"),
        var("DICTIONARY_ACAD_MLINESTYLE"),
        var("DICTIONARY_ACAD_GROUP"),
    ];
    let m = maintenance(p.version);
    let second = second_header(at(second_at)?, m, p.codepage, &records, &handles);
    let template_at = second_at + second.len();
    let template = template(p);
    if let Some(r) = records.get_mut(4) {
        *r = (4, at(template_at)?, at(template.len())?);
    }

    let mut out = Vec::with_capacity(template_at + template.len());
    out.extend_from_slice(b"AC1015");
    out.extend_from_slice(&[0; 5]);
    out.push(m);
    out.push(1);
    out.extend_from_slice(&at(preview_at)?.to_le_bytes());
    out.extend_from_slice(&[33, 0xFF]);
    out.extend_from_slice(&p.codepage.to_le_bytes());
    out.extend_from_slice(&(RECORDS as u32).to_le_bytes());
    for (number, address, size) in &records {
        out.push(*number);
        out.extend_from_slice(&address.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
    }
    let crc = crc16(0, &out) ^ file_header_crc_xor(RECORDS as u32);
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&FILE_HEADER_END);
    for part in [&aux, &preview, &header, &classes, &objects, &map, &second, &template] {
        out.extend_from_slice(part);
    }
    Ok(out)
}
