//! A DWG file opened to its parts: the version, the code page and the list of its objects, each
//! with its handle, its type and where its bytes are. This is the level below any drawing model:
//! a reader that builds its own drawing (DraftCraft's, in `draftcraft-io`) starts here and decodes
//! each object straight into that drawing.
//!
//! Reads R13 to AutoCAD 2018 and later, except AutoCAD 2007–2009 (AC1021), whose container is not
//! implemented yet.

use crate::read::model::{ClassDef, Diagnostic};
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

/// A DWG file opened to its sections.
pub struct DwgFile {
    version: read::DwgVersion,
    /// The maintenance release the file records.
    pub maintenance: u8,
    /// The code page of the file's 8-bit text.
    pub codepage: u16,
    classes: Vec<ClassDef>,
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
            objects: c.objects,
            entries,
            notes: diagnostics.into_iter().map(|d| d.message).collect(),
        })
    }

    /// The file's version code, e.g. `AC1032`.
    pub fn version_code(&self) -> &'static str {
        self.version.magic()
    }

    /// How many objects the object map lists.
    pub fn object_count(&self) -> usize {
        self.entries.len()
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
