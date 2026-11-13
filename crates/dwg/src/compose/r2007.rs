//! AutoCAD 2007 (AC1021): the container of Reed-Solomon codewords.

use super::file::Parts;
use crate::records::{Error, Result};

pub(super) fn compose(_p: &Parts<'_>) -> Result<Vec<u8>> {
    Err(Error::unsupported("writing AutoCAD 2007 files"))
}
