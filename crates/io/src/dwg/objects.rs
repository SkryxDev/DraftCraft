//! The objects of a DWG file that DraftCraft reads into the drawing's own model. So far: the
//! DIMASSOC objects that make a dimension follow the geometry it measures.

use std::collections::{HashMap, HashSet};

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

/// Most entries of one dictionary that are read.
const MAX_ENTRIES: u32 = 1 << 20;

/// DICTIONARY (§20.4.56): its entries, by name, each with the handle of the object it holds.
fn dictionary(r: &mut ObjectReader) -> DwgResult<Vec<(String, u64)>> {
    r.common_refs()?;
    let n = r.bl()?;
    // From AutoCAD 2007 on the names are in the string stream: an entry may take no data bits.
    let min_bits = if r.has_string_stream() { 0 } else { 2 };
    if n > MAX_ENTRIES {
        return Err(draftcraft_dwg::records::Error::format(format!("too many dictionary entries: {n}")));
    }
    let n = r.count(n, min_bits, "dictionary entry")?;
    if r.version() == draftcraft_dwg::DwgVersion::R14 {
        let _unknown = r.rc()?;
    } else if r.version() >= draftcraft_dwg::DwgVersion::R2000 {
        let _cloning = r.bs()?;
        let _hard_owner = r.rc()?;
    }
    let mut names = Vec::with_capacity(n);
    for _ in 0..n {
        names.push(r.text()?);
    }
    names.into_iter().map(|name| Ok((name, r.reference()?))).collect()
}

/// The entries of the dictionary named `key` in the dictionary at `root` (the named-object
/// dictionary): each object's handle, with its name. Empty when either can't be read.
pub(super) fn named_entries(file: &DwgFile, objects: &[ObjectRef], root: u64, key: &str) -> HashMap<u64, String> {
    let entries = |handle: u64| {
        objects
            .iter()
            .find(|o| o.handle == handle && o.type_name == "DICTIONARY")
            .and_then(|o| file.read_object(o).and_then(|mut r| dictionary(&mut r)).ok())
            .unwrap_or_default()
    };
    let Some((_, inner)) = entries(root).into_iter().find(|(name, _)| name.eq_ignore_ascii_case(key)) else {
        return HashMap::new();
    };
    entries(inner).into_iter().map(|(name, h)| (h, name)).collect()
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
