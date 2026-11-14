//! DraftCraft's DWG codec.
//!
//! - [`read`]: DWG files from R13 to AutoCAD 2018 and later into the [`records`] of
//!   their objects, which `draftcraft-io` maps straight into the drawing.
//! - [`compose`]: DWG files of every version AutoCAD saves (2000 to 2018) from the
//!   drawing's objects.
//! - [`file`]: a DWG file opened to its parts (version, sections, the object list), for a reader
//!   that builds the drawing itself.
//! - [`bits`], [`crc`], [`lz77`]: the format's primitives, shared by both.
//!
//! The reader, writer and records come from BlackCAD (<https://github.com/withshalom/blackcad>,
//! MIT OR Apache-2.0), written from the Open Design Alliance's public DWG specification.
//! This crate depends on no other workspace crate.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod bits;
pub mod compose;
pub mod crc;
pub mod datastore;
pub mod file;
pub mod lz77;
pub mod read;
pub mod records;

pub use read::{DwgVersion, read_bytes};

/// DWG version codes we can write.
pub const VERSIONS: &[(&str, &str)] =
    &[("AC1015", "2000"), ("AC1018", "2004"), ("AC1021", "2007"), ("AC1024", "2010"), ("AC1027", "2013"), ("AC1032", "2018")];

/// True when the bytes look like a DWG file (`AC10xx` magic).
pub fn is_dwg(bytes: &[u8]) -> bool {
    bytes.len() > 6 && bytes.starts_with(b"AC10") && bytes.get(4..6).is_some_and(|v| v.iter().all(u8::is_ascii_digit))
}

/// The DWG version code of a file (e.g. "AC1032").
pub fn version(bytes: &[u8]) -> Option<String> {
    is_dwg(bytes).then(|| String::from_utf8_lossy(bytes.get(0..6).unwrap_or_default()).to_string())
}

/// Size limits for reading a DWG file. They exist because a damaged file can make a reader
/// build a drawing thousands of times larger than the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Largest DWG file read at all (checked before reading starts).
    pub max_dwg_bytes: usize,
}

impl Limits {
    /// 512 MB of DWG.
    pub const DEFAULT: Limits = Limits { max_dwg_bytes: 512 << 20 };
}
