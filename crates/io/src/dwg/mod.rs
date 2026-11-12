//! DWG files read straight into the drawing: each object of the file is decoded into
//! `draftcraft-doc`'s own types, with no other drawing model and no DXF in between.
//!
//! `draftcraft-dwg` opens the file to its parts ([`draftcraft_dwg::file::DwgFile`]: version,
//! sections, the object list); everything about what an object means is decided here, where the
//! drawing is known.
//!
//! The reader is built category by category. Whatever it does not put in the drawing yet is counted
//! in [`Unread`], by type, so nothing is left out silently: a drawing is the whole file only when
//! [`Unread::is_empty`].

mod header;

use std::collections::BTreeMap;

use draftcraft_doc::Drawing;
use draftcraft_dwg::file::DwgFile;

use crate::{IoError, Result};

/// The objects of a file that the reader did not put in the drawing, counted by type name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Unread {
    pub by_type: BTreeMap<String, usize>,
}

impl Unread {
    /// True when the drawing holds everything the file does.
    pub fn is_empty(&self) -> bool {
        self.by_type.is_empty()
    }
    /// How many objects were not read.
    pub fn total(&self) -> usize {
        self.by_type.values().sum()
    }
    /// One line for messages: `LINE × 12, LAYER × 3`.
    pub fn summary(&self) -> String {
        self.by_type.iter().map(|(k, n)| format!("{k} × {n}")).collect::<Vec<_>>().join(", ")
    }
    fn count(&mut self, type_name: &str) {
        *self.by_type.entry(type_name.to_string()).or_default() += 1;
    }
}

/// Read a DWG file into a drawing, with what the reader does not read yet.
pub fn read(bytes: &[u8]) -> Result<(Drawing, Unread)> {
    let limit = draftcraft_dwg::Limits::DEFAULT.max_dwg_bytes;
    if bytes.len() > limit {
        return Err(IoError::Format(format!("DWG: the file is larger than the {} MB DraftCraft opens", limit >> 20)));
    }
    let file = DwgFile::open(bytes).map_err(|e| IoError::Format(format!("DWG: {e}")))?;
    let mut d = Drawing::new_imperial();
    d.header.set_str("ACADVER", file.version_code());
    // A file without the variable (before AutoCAD 2000) has no units.
    d.header.set_i64("INSUNITS", 0);
    let mut unread = Unread::default();
    match header::read(&file, &mut d) {
        // The variables that name other objects wait for the tables.
        Ok(refs) => refs.names.keys().for_each(|name| unread.count(&format!("header ${name}"))),
        Err(_) => unread.count("header"),
    }
    for o in file.objects() {
        // No category is read yet: every object is accounted for as unread.
        unread.count(&o.type_name);
    }
    Ok((d, unread))
}
