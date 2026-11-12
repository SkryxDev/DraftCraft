//! A DWG file opened to its parts: the version, the code page and the list of its objects, each
//! with its handle, its type and where its bytes are. This is the level below any drawing model:
//! a reader that builds its own drawing (DraftCraft's, in `draftcraft-io`) starts here and decodes
//! each object straight into that drawing.
//!
//! Reads R13 to AutoCAD 2018 and later, except AutoCAD 2007–2009 (AC1021), whose container is not
//! implemented yet.

use std::collections::HashMap;

use crate::read::header::Val;
use crate::read::model::{ClassDef, Color, Diagnostic};
use crate::read::{self, object};
use crate::records::Result;

/// One object of the file, before anything of its content is decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectRef {
    /// The object's handle, from the object map.
    pub handle: u64,
    /// Where the object starts in the object data.
    pub offset: usize,
    /// The DWG type number (0x13 for LINE; 500 and up for class-defined types).
    pub type_code: u16,
    /// The DXF name of the type: `LINE`, `LAYER`, or the class's name. `?` when the object's
    /// first fields can't be read.
    pub type_name: String,
    /// True for entities (objects that are drawn).
    pub is_entity: bool,
}

/// A colour as DWG stores it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorValue {
    ByLayer,
    ByBlock,
    /// An AutoCAD Color Index number, 1 to 255.
    Index(u8),
    /// A true colour.
    Rgb(u8, u8, u8),
    /// No colour.
    None,
}

/// The value of one header variable, as the file stores it.
#[derive(Clone, Debug, PartialEq)]
pub enum HeaderValue {
    Bool(bool),
    Int(i64),
    Real(f64),
    Point2([f64; 2]),
    Point3([f64; 3]),
    Text(String),
    /// A reference to another object of the file (the current layer, a table's control object).
    Handle(u64),
    Color(ColorValue),
}

/// The header variables of a file.
#[derive(Clone, Debug, PartialEq)]
pub struct HeaderRead {
    /// Every variable in file order, by its DWG name. A file of an older version has fewer.
    pub vars: Vec<(&'static str, HeaderValue)>,
    /// True when the handles the header names point at objects of the right types, which shows
    /// the variables were read at their right places.
    pub verified: bool,
    /// MEASUREMENT (0 imperial, 1 metric), which DWG keeps outside the header section.
    pub measurement: Option<u16>,
    /// Who last saved the file, from its summary section (AutoCAD 2004 and later).
    pub last_saved_by: Option<String>,
}

/// A DWG file opened to its sections.
pub struct DwgFile {
    version: read::DwgVersion,
    /// The maintenance release the file records.
    pub maintenance: u8,
    /// The code page of the file's 8-bit text.
    pub codepage: u16,
    classes: Vec<ClassDef>,
    header: Option<Vec<u8>>,
    others: Vec<(String, Vec<u8>)>,
    objects: Vec<u8>,
    entries: Vec<(u64, usize)>,
    /// What opening the file had to say (sections skipped, checksums that failed).
    pub notes: Vec<String>,
}

impl DwgFile {
    /// Open a DWG file's container and read its class list and object map.
    pub fn open(bytes: &[u8]) -> Result<DwgFile> {
        let (version, c) = read::open_container(bytes)?;
        let mut diagnostics: Vec<Diagnostic> = c.notes;
        let classes = match &c.classes {
            Some(data) => match read::classes::parse(data, version, c.maintenance, c.codepage) {
                Ok((list, _)) => list,
                Err(e) => {
                    diagnostics.push(Diagnostic { severity: read::model::Severity::Warning, message: format!("classes: {e}") });
                    Vec::new()
                }
            },
            None => Vec::new(),
        };
        let map = read::choose_object_map(&c.handles, version, &c.objects, &mut diagnostics)?;
        let len = c.objects.len();
        let entries = map
            .iter()
            .filter(|e| e.handle != 0)
            .filter_map(|e| usize::try_from(e.location).ok().filter(|&o| o < len).map(|o| (e.handle, o)))
            .collect();
        Ok(DwgFile {
            version,
            maintenance: c.maintenance,
            codepage: c.codepage,
            classes,
            header: c.header,
            others: c.others,
            objects: c.objects,
            entries,
            notes: diagnostics.into_iter().map(|d| d.message).collect(),
        })
    }

    /// The file's version code, e.g. `AC1032`.
    pub fn version_code(&self) -> &'static str {
        self.version.magic()
    }

    /// The DXF name of the file's code page (`ANSI_1252`), when it is one the reader knows.
    pub fn codepage_name(&self) -> Option<&'static str> {
        (self.codepage == 30).then_some("ANSI_1252")
    }

    /// True for the versions before AutoCAD 2000 (R13 and R14).
    pub fn before_2000(&self) -> bool {
        self.version < read::DwgVersion::R2000
    }

    /// How many objects the object map lists.
    pub fn object_count(&self) -> usize {
        self.entries.len()
    }

    /// The header variables. An error when the file has no header section or it can't be read.
    pub fn header(&self) -> Result<HeaderRead> {
        let data = self.header.as_ref().ok_or_else(|| crate::records::Error::format("no header section"))?;
        let by_handle: HashMap<u64, usize> = self.entries.iter().copied().collect();
        let type_of = |h: u64| by_handle.get(&h).and_then(|&o| object::peek(self.version, &self.objects, o).ok()).map(|(_, code)| code);
        let (vals, verified, _note) = read::header::parse_values(data, self.version, self.maintenance, self.codepage, &type_of)?;
        let vars = vals
            .into_iter()
            .map(|(name, v)| {
                let v = match v {
                    Val::Bool(b) => HeaderValue::Bool(b),
                    Val::Int(i) => HeaderValue::Int(i),
                    Val::Real(r) => HeaderValue::Real(r),
                    Val::P2(p) => HeaderValue::Point2([p.x, p.y]),
                    Val::P3(p) => HeaderValue::Point3([p.x, p.y, p.z]),
                    Val::Text(t) => HeaderValue::Text(t),
                    Val::Handle(h) => HeaderValue::Handle(h),
                    Val::Color(c) => HeaderValue::Color(match c {
                        Color::ByLayer => ColorValue::ByLayer,
                        Color::ByBlock => ColorValue::ByBlock,
                        Color::Index(i) => ColorValue::Index(i),
                        Color::Rgb(r, g, b) => ColorValue::Rgb(r, g, b),
                        Color::None => ColorValue::None,
                    }),
                };
                (name, v)
            })
            .collect();
        let measurement = self.others.iter().find(|(n, _)| n == "AcDb:Template").and_then(|(_, s)| read::template_measurement(s));
        let wide = self.version >= read::DwgVersion::R2007;
        let last_saved_by =
            self.others.iter().find(|(n, _)| n == "AcDb:SummaryInfo").and_then(|(_, s)| read::summary_info(s, wide)).map(|p| p.last_saved_by);
        Ok(HeaderRead { vars, verified, measurement, last_saved_by })
    }

    /// Every object of the file, in handle order as the object map lists them.
    pub fn objects(&self) -> impl Iterator<Item = ObjectRef> + '_ {
        self.entries.iter().map(|&(handle, offset)| match object::peek(self.version, &self.objects, offset) {
            Ok((_, type_code)) => {
                let (type_name, is_entity) = object::classify(type_code, &self.classes);
                ObjectRef { handle, offset, type_code, type_name, is_entity }
            }
            Err(_) => ObjectRef { handle, offset, type_code: 0, type_name: "?".into(), is_entity: false },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_a_dwg_is_an_error() {
        assert!(DwgFile::open(b"hello").is_err());
        assert!(DwgFile::open(b"").is_err());
        assert!(DwgFile::open(b"AC1032").is_err());
    }
}
