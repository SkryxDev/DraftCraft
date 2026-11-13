//! A DWG file composed from its parts: the mirror of [`crate::file`]. A writer that knows the
//! drawing (DraftCraft's, in `draftcraft-io`) encodes each object with [`ObjectWriter`] and hands
//! the objects, the header variables and the classes over; this module lays the file out as the
//! version asked for stores it.

mod file;
mod object;
mod r2007;
mod sections;

pub use file::{Parts, Properties, compose};
pub use object::{EntityStart, ObjectWriter, Ref, TextEncoding, frame, method_word};
pub use sections::{Class, class_versions, classes_section, header_section, object_map};

#[cfg(test)]
mod tests;
