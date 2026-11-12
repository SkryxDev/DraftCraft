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
mod tables;

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
    let objects: Vec<_> = file.objects().collect();
    let refs = match header::read(&file, &mut d) {
        Ok(refs) => Some(refs),
        Err(_) => {
            unread.count("header");
            None
        }
    };
    let tables = tables::read(&file, &objects, &mut d, &mut unread);
    // The header variables that name a table record, now that the tables are read. The others
    // (a dimension style, a multiline style, a block, a UCS) wait for their categories.
    for (name, handle) in refs.iter().flat_map(|r| &r.names) {
        let table = match *name {
            "CLAYER" => &tables.layers,
            "CELTYPE" | "DIMLTYPE" | "DIMLTEX1" | "DIMLTEX2" => &tables.linetypes,
            "TEXTSTYLE" | "DIMTXSTY" => &tables.styles,
            _ => {
                unread.count(&format!("header ${name}"));
                continue;
            }
        };
        match table.get(handle) {
            Some(record) => d.header.set_str(name, record),
            // No reference: the variable is empty, as AutoCAD's DXF has it.
            None if *handle == 0 => d.header.set_str(name, ""),
            None => unread.count(&format!("header ${name}")),
        }
    }
    for o in &objects {
        if !tables.read.contains(&o.handle) {
            unread.count(&o.type_name);
        }
    }
    Ok((d, unread))
}
