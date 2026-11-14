//! The fields of one object as three bit streams (data, text, references), for the encoders that
//! write an object field by field from a layout ([`crate::read::styles`]); [`super::ObjectWriter`]
//! takes the streams over.

use crate::bits::BitWriter;
use crate::read::model::Handle;
use crate::read::version::DwgVersion;

/// The three streams an R2007+ object is written as.
pub(crate) struct ObjW {
    pub ver: DwgVersion,
    pub dat: BitWriter,
    strs: BitWriter,
    has_strings: bool,
    pub hdl: BitWriter,
}

/// Which handle-reference codes a field takes (spec §2.13): soft
/// hard ownership 3, soft pointer 4, hard pointer 5.
#[derive(Clone, Copy)]
pub(crate) enum FieldRef {
    HardOwner = 3,
    SoftPtr = 4,
    HardPtr = 5,
}

impl ObjW {
    pub fn new(ver: DwgVersion) -> Self {
        ObjW { ver, dat: BitWriter::new(), strs: BitWriter::new(), has_strings: false, hdl: BitWriter::new() }
    }

    /// The object's three streams as written so far: fields, text (and whether it has any),
    /// references.
    pub(crate) fn into_streams(self) -> (BitWriter, BitWriter, bool, BitWriter) {
        (self.dat, self.strs, self.has_strings, self.hdl)
    }

    /// TV: from R2007 in the string stream, as UTF-16.
    pub fn tv(&mut self, s: &str) {
        if self.ver.has_string_stream() {
            self.strs.text16(s);
            self.has_strings = true;
        } else {
            let bytes: Vec<u8> = s.chars().map(|c| if (c as u32) < 0x100 { c as u8 } else { b'?' }).collect();
            self.dat.text8(&bytes);
        }
    }

    /// An absolute reference.
    pub fn h(&mut self, kind: FieldRef, target: Handle) {
        self.hdl.handle(kind as u8, target.0);
    }
}
