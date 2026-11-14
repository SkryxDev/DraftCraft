//! The sections around the objects, in the form each version stores them: the header variables,
//! the classes and the object map.

use crate::bits::BitWriter;
use crate::crc::crc16;
use crate::file::{ColorValue, HeaderValue};
use crate::read::header::{self, VarKind};
use crate::read::section::{CLASSES_END, CLASSES_START, HEADER_END, HEADER_START};
use crate::read::version::DwgVersion as V;

use super::object::{TextEncoding, method_word};

/// A class of the file's objects or entities: what a DXF CLASS record says of it, and the number
/// its objects carry as their type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Class {
    /// 500 and up, in the order of the list.
    pub number: u16,
    pub dxf_name: String,
    pub cpp_name: String,
    pub app_name: String,
    pub proxy_flags: u16,
    pub instance_count: u32,
    pub was_zombie: bool,
    pub is_entity: bool,
    /// The DWG version and maintenance release the class came with ([`class_versions`]).
    pub versions: (u32, u32),
}

/// Fields, text and references kept apart, joined as a section of AutoCAD 2007 and later stores
/// them; one stream for the versions before.
struct Streams {
    ver: V,
    codepage: u16,
    text: TextEncoding,
    dat: BitWriter,
    strs: BitWriter,
    hdl: BitWriter,
}

impl Streams {
    fn new(ver: V, codepage: u16, text: TextEncoding) -> Self {
        Streams { ver, codepage, text, dat: BitWriter::new(), strs: BitWriter::new(), hdl: BitWriter::new() }
    }
    fn tv(&mut self, s: &str) {
        if self.ver.has_string_stream() {
            self.strs.text16(s);
        } else {
            self.dat.text8(&super::object::terminated((self.text.ansi)(s, self.codepage)));
        }
    }
    fn h(&mut self, code: u8, value: u64) {
        if self.ver.has_string_stream() {
            self.hdl.handle(code, value);
        } else {
            self.dat.handle(code, value);
        }
    }
    /// The section's body. From AutoCAD 2007: where the fields and text end (in bits), the
    /// fields, the text with its size and the bit that says it is there, then the references.
    fn body(self, pad: bool) -> Vec<u8> {
        if !self.ver.has_string_stream() {
            let mut w = self.dat;
            while !w.position().is_multiple_of(8) {
                w.b(pad);
            }
            return w.into_bytes();
        }
        let mut w = BitWriter::new();
        w.rl(0);
        w.append(&self.dat);
        let n = self.strs.position();
        w.append(&self.strs);
        if n >= 0x8000 {
            w.rs(u16::try_from(n >> 15).unwrap_or(u16::MAX));
            w.rs(u16::try_from(n & 0x7FFF).unwrap_or(0) | 0x8000);
        } else {
            w.rs(u16::try_from(n).unwrap_or(0));
        }
        w.b(true);
        let end = u32::try_from(w.position()).unwrap_or(u32::MAX);
        w.append(&self.hdl);
        while !w.position().is_multiple_of(8) {
            w.b(pad);
        }
        let mut out = w.into_bytes();
        if let Some(head) = out.get_mut(..4) {
            head.copy_from_slice(&end.to_le_bytes());
        }
        out
    }
}

/// A body between its sentinels: its size (twice as wide from AutoCAD 2010), the body, the
/// checksum.
fn wrap(start: &[u8; 16], end: &[u8; 16], body: &[u8], ver: V) -> Vec<u8> {
    let mut out = start.to_vec();
    let from = out.len();
    out.extend_from_slice(&u32::try_from(body.len()).unwrap_or(u32::MAX).to_le_bytes());
    if ver >= V::R2010 {
        out.extend_from_slice(&0u32.to_le_bytes());
    }
    out.extend_from_slice(body);
    let crc = crc16(0xC0C1, out.get(from..).unwrap_or(&[]));
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(end);
    out
}

fn julian(v: f64) -> (u32, u32) {
    let v = if v.is_finite() && v > 0.0 { v } else { 0.0 };
    let day = v.floor();
    let ms = ((v - day) * 86_400_000.0).round();
    (day as u32, ms as u32)
}

fn parse_floats(t: &str) -> Vec<f64> {
    t.trim_matches(|c| c == '(' || c == ')').split(',').filter_map(|x| x.trim().parse().ok()).collect()
}

/// A variable's value when the drawing has none: AutoCAD's own default.
fn default_value(name: &str, kind: VarKind) -> HeaderValue {
    let dt = super::small::default_text(name);
    match kind {
        VarKind::B => HeaderValue::Bool(match dt {
            "" => matches!(
                name,
                "DIMASO"
                    | "DIMSHO"
                    | "REGENMODE"
                    | "FILLMODE"
                    | "PSLTSCALE"
                    | "USRTIMER"
                    | "WORLDVIEW"
                    | "TILEMODE"
                    | "VISRETAIN"
                    | "DIMTIH"
                    | "DIMTOH"
            ),
            t => t == "true" || t == "1",
        }),
        VarKind::Bs | VarKind::Bl | VarKind::Bll | VarKind::Rc => HeaderValue::Int(dt.parse().unwrap_or(0)),
        VarKind::Bd | VarKind::Time => HeaderValue::Real(dt.parse().unwrap_or(0.0)),
        VarKind::Rd2 => {
            let v = parse_floats(dt);
            HeaderValue::Point2([v.first().copied().unwrap_or(0.0), v.get(1).copied().unwrap_or(0.0)])
        }
        VarKind::Bd3 => {
            let v = parse_floats(dt);
            HeaderValue::Point3([v.first().copied().unwrap_or(0.0), v.get(1).copied().unwrap_or(0.0), v.get(2).copied().unwrap_or(0.0)])
        }
        VarKind::Tv => HeaderValue::Text(dt.trim_matches('"').to_string()),
        VarKind::Cmc => HeaderValue::Color(match name {
            "CECOLOR" => ColorValue::ByLayer,
            "INTERFERECOLOR" => ColorValue::Index(1),
            _ => ColorValue::ByBlock,
        }),
        VarKind::H | VarKind::Seed => HeaderValue::Handle(0),
    }
}

/// The header section: every variable the version stores, in the format's order, by its DWG name
/// ([`crate::file::HeaderRead::vars`] lists them). `value` gives a variable's value, or `None`
/// for AutoCAD's default; a value of the wrong kind is taken as the default too.
pub fn header_section(ver: V, codepage: u16, text: TextEncoding, value: &dyn Fn(&str) -> Option<HeaderValue>) -> Vec<u8> {
    let mut s = Streams::new(ver, codepage, text);
    let mut cepsntype = 0i64;
    for (name, kind, applies) in header::vars() {
        if !applies(ver, cepsntype) {
            continue;
        }
        let given = value(name);
        let default = || default_value(name, kind);
        let int = |v: &Option<HeaderValue>| match v {
            Some(HeaderValue::Int(i)) => *i,
            Some(HeaderValue::Bool(b)) => i64::from(*b),
            _ => match default() {
                HeaderValue::Int(i) => i,
                _ => 0,
            },
        };
        let real = |v: &Option<HeaderValue>| match v {
            Some(HeaderValue::Real(r)) if r.is_finite() => *r,
            Some(HeaderValue::Int(i)) => *i as f64,
            _ => match default() {
                HeaderValue::Real(r) => r,
                _ => 0.0,
            },
        };
        let finite = |a: &[f64]| a.iter().all(|c| c.is_finite());
        match kind {
            VarKind::B => s.dat.b(match &given {
                Some(HeaderValue::Bool(b)) => *b,
                Some(HeaderValue::Int(i)) => *i != 0,
                // The bit after the variables: set in AutoCAD's 2004 files.
                _ if name == "UNKNOWN_B_TAIL" => true,
                _ => matches!(default(), HeaderValue::Bool(true)),
            }),
            VarKind::Bs => {
                let v = int(&given);
                if name == "CEPSNTYPE" {
                    cepsntype = v;
                }
                s.dat.bs(v as i16 as u16);
            }
            VarKind::Bl => s.dat.bl(int(&given) as i32 as u32),
            VarKind::Bll => s.dat.bll(int(&given) as u64),
            VarKind::Rc => s.dat.rc(int(&given) as u8),
            VarKind::Bd => s.dat.bd(real(&given)),
            VarKind::Rd2 => s.dat.rd2(match (&given, default()) {
                (Some(HeaderValue::Point2(p)), _) if finite(p) => *p,
                (Some(HeaderValue::Point3([x, y, _])), _) if finite(&[*x, *y]) => [*x, *y],
                (_, HeaderValue::Point2(p)) => p,
                _ => [0.0; 2],
            }),
            VarKind::Bd3 => s.dat.bd3(match (&given, default()) {
                (Some(HeaderValue::Point3(p)), _) if finite(p) => *p,
                (Some(HeaderValue::Point2([x, y])), _) if finite(&[*x, *y]) => [*x, *y, 0.0],
                (_, HeaderValue::Point3(p)) => p,
                _ => [0.0; 3],
            }),
            VarKind::Tv => match (&given, default()) {
                (Some(HeaderValue::Text(t)), _) => s.tv(t),
                (_, HeaderValue::Text(t)) => s.tv(&t),
                _ => s.tv(""),
            },
            VarKind::Cmc => {
                let c = match (&given, default()) {
                    (Some(HeaderValue::Color(c)), _) => *c,
                    (Some(HeaderValue::Int(i)), _) => match *i {
                        0 => ColorValue::ByBlock,
                        256 => ColorValue::ByLayer,
                        i => u8::try_from(i).map_or(ColorValue::ByLayer, ColorValue::Index),
                    },
                    (_, HeaderValue::Color(c)) => c,
                    _ => ColorValue::ByLayer,
                };
                if ver < V::R2004 {
                    s.dat.bs(match c {
                        ColorValue::ByBlock => 0,
                        ColorValue::Index(i) => u16::from(i),
                        _ => 256,
                    });
                } else {
                    s.dat.bs(0);
                    s.dat.bl(method_word(c));
                    s.dat.rc(0);
                }
            }
            VarKind::H => {
                let h = match &given {
                    Some(HeaderValue::Handle(h)) => *h,
                    _ => 0,
                };
                // The tables' control objects and the named-object dictionary are owned; the
                // rest are pointers.
                let code = if name.ends_with("_CONTROL") || name == "DICTIONARY_NAMED_OBJECTS" { 3 } else { 5 };
                s.h(code, h);
            }
            VarKind::Seed => {
                let h = match &given {
                    Some(HeaderValue::Handle(h)) => *h,
                    _ => 0,
                };
                s.dat.handle(0, h);
            }
            VarKind::Time => {
                let (day, ms) = julian(real(&given));
                s.dat.bl(day);
                s.dat.bl(ms);
            }
        }
    }
    let body = s.body(false);
    wrap(&HEADER_START, &HEADER_END, &body, ver)
}

/// The DWG and maintenance version a class says it came with: the values AutoCAD writes for the
/// classes of its own drawings, 0 for another program's.
pub fn class_versions(dxf_name: &str) -> (u32, u32) {
    match dxf_name {
        "ACDBDICTIONARYWDFLT" => (22, 42),
        "MATERIAL" => (23, 14),
        "VISUALSTYLE" => (26, 0),
        "TABLESTYLE" => (25, 20),
        "SCALE" => (27, 1),
        "MLEADERSTYLE" => (33, 427),
        "ACDBSECTIONVIEWSTYLE" => (27, 50),
        "ACDBDETAILVIEWSTYLE" => (33, 427),
        "DIMASSOC" => (23, 12),
        "DICTIONARYVAR" => (20, 0),
        "ARC_DIMENSION" | "LARGE_RADIAL_DIMENSION" => (25, 20),
        _ => (0, 0),
    }
}

/// The classes section.
pub fn classes_section(ver: V, codepage: u16, text: TextEncoding, classes: &[Class]) -> Vec<u8> {
    let mut s = Streams::new(ver, codepage, text);
    if ver >= V::R2004 {
        s.dat.bs(classes.iter().map(|c| c.number).max().unwrap_or(499));
        s.dat.rc(0);
        s.dat.rc(0);
        s.dat.b(true);
    }
    for c in classes {
        s.dat.bs(c.number);
        s.dat.bs(c.proxy_flags);
        s.tv(&c.app_name);
        s.tv(&c.cpp_name);
        s.tv(&c.dxf_name);
        s.dat.b(c.was_zombie);
        s.dat.bs(if c.is_entity { 0x1F2 } else { 0x1F3 });
        if ver >= V::R2004 {
            let (dwg, maint) = c.versions;
            s.dat.bl(c.instance_count);
            s.dat.bl(dwg);
            s.dat.bl(maint);
            s.dat.bl(0);
            s.dat.bl(0);
        }
    }
    let body = s.body(true);
    let mut out = wrap(&CLASSES_START, &CLASSES_END, &body, ver);
    if ver >= V::R2004 {
        out.extend_from_slice(&[0u8; 8]);
    }
    out
}

/// The object map: each object's handle and where it starts, in handle order, as differences
/// from the entry before, in blocks of at most 2032 bytes, each with its size and checksum
/// (most significant byte first), ended by an empty block. `entries` must be sorted by handle.
pub fn object_map(entries: &[(u64, u64)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < entries.len() {
        let mut block: Vec<u8> = vec![0, 0];
        let (mut last_h, mut last_o) = (0u64, 0i64);
        while let Some(&(h, o)) = entries.get(i) {
            let o = i64::try_from(o).unwrap_or(i64::MAX);
            let mut w = BitWriter::new();
            w.mc_unsigned(h.wrapping_sub(last_h));
            w.mc_signed(o.wrapping_sub(last_o));
            let bytes = w.into_bytes();
            if block.len() + bytes.len() > 2032 {
                break;
            }
            block.extend_from_slice(&bytes);
            last_h = h;
            last_o = o;
            i += 1;
        }
        close_block(&mut out, block);
    }
    close_block(&mut out, vec![0, 0]);
    out
}

fn close_block(out: &mut Vec<u8>, mut block: Vec<u8>) {
    let n = u16::try_from(block.len()).unwrap_or(u16::MAX);
    if let Some(head) = block.get_mut(..2) {
        head.copy_from_slice(&n.to_be_bytes());
    }
    let crc = crc16(0xC0C1, &block);
    out.extend_from_slice(&block);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// How a header variable holds its value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeaderKind {
    Bool,
    Int,
    Real,
    Point2,
    Point3,
    Text,
    Color,
    /// A reference to another object.
    Handle,
    /// A date or a duration: a Julian day number and its fraction.
    Time,
}

/// Every header variable DWG stores, by its DWG name, with how it holds its value: what
/// [`header_section`] asks for. A file of an older version stores fewer.
pub fn header_variables() -> impl Iterator<Item = (&'static str, HeaderKind)> {
    header::vars().map(|(name, kind, _)| {
        let kind = match kind {
            VarKind::B => HeaderKind::Bool,
            VarKind::Bs | VarKind::Bl | VarKind::Bll | VarKind::Rc => HeaderKind::Int,
            VarKind::Bd => HeaderKind::Real,
            VarKind::Rd2 => HeaderKind::Point2,
            VarKind::Bd3 => HeaderKind::Point3,
            VarKind::Tv => HeaderKind::Text,
            VarKind::Cmc => HeaderKind::Color,
            VarKind::H | VarKind::Seed => HeaderKind::Handle,
            VarKind::Time => HeaderKind::Time,
        };
        (name, kind)
    })
}
