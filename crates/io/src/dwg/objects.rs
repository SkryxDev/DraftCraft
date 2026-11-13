//! The objects of a DWG file that DraftCraft reads into the drawing's own model. So far: the
//! DIMASSOC objects that make a dimension follow the geometry it measures.

use std::collections::HashSet;

use draftcraft_doc::Drawing;
use draftcraft_dwg::file::{DwgFile, ObjectReader, ObjectRef};
use draftcraft_dwg::records::Result as DwgResult;
use draftcraft_dxf::Tag;

use super::Unread;

/// Most references one DIMASSOC point names.
const MAX_REFS: u32 = 64;

/// DIMASSOC (from AutoCAD's own files: the specification does not describe it), as the DXF groups
/// it is in a DXF file: the dimension it belongs to, which of its points are associated, and for
/// each point the object snap and the objects it names.
fn dimassoc(r: &mut ObjectReader) -> DwgResult<Vec<Tag>> {
    r.common_refs()?;
    let flags = r.bl()?;
    let trans = r.b()?;
    let rotated = r.rc()?;
    let dimension = r.reference()?;
    let mut tags = vec![
        Tag::s(100, "AcDbDimAssoc"),
        Tag::s(330, format!("{dimension:X}")),
        Tag::i(90, i64::from(flags)),
        Tag::i(70, i64::from(trans)),
        Tag::i(71, i64::from(rotated)),
    ];
    for bit in 0..4 {
        if flags & (1 << bit) == 0 {
            continue;
        }
        let class = r.text()?;
        let osnap = r.rc()?;
        let n = r.bl()?;
        if n > MAX_REFS {
            return Err(draftcraft_dwg::records::Error::format("DIMASSOC: object count"));
        }
        let mut mains = Vec::new();
        for _ in 0..n {
            mains.push(r.reference()?);
        }
        let subentity = r.bs()?;
        let marker = r.bl()?;
        // Intersection and external reference data are not in AutoCAD's files of this layout.
        if r.bl()? != 0 {
            return Err(draftcraft_dwg::records::Error::format("DIMASSOC: intersection or xref data"));
        }
        let param = r.bd()?;
        let [x, y, z] = r.bd3()?;
        let last = r.b()?;
        tags.push(Tag::s(1, class));
        tags.push(Tag::i(72, i64::from(osnap)));
        tags.extend(mains.into_iter().map(|m| Tag::s(331, format!("{m:X}"))));
        tags.push(Tag::i(73, i64::from(subentity)));
        tags.push(Tag::i(91, i64::from(marker)));
        tags.push(Tag::f(40, param));
        tags.extend([Tag::f(10, x), Tag::f(20, y), Tag::f(30, z)]);
        tags.push(Tag::i(75, i64::from(last)));
    }
    Ok(tags)
}

/// Read the objects into `d`. Returns the handles of the objects read.
pub(super) fn read(file: &DwgFile, objects: &[ObjectRef], d: &mut Drawing, unread: &mut Unread) -> HashSet<u64> {
    let mut read = HashSet::new();
    for o in objects.iter().filter(|o| o.type_name == "DIMASSOC") {
        match file.read_object(o).and_then(|mut r| dimassoc(&mut r)) {
            Ok(tags) => {
                crate::dxf_read::apply_dimassoc(d, &tags);
                read.insert(o.handle);
            }
            Err(_) => unread.count("DIMASSOC (not decoded)"),
        }
    }
    read
}
