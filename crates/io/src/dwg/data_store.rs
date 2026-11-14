//! The data store section of a DWG file from AutoCAD 2013 on (`AcDb:AcDsPrototype_1b`): the ACIS
//! geometry of 3D solids, regions and bodies, which those entities no longer carry themselves.
//! DraftCraft keeps it as the DXF reader does (`Foreign::data_store`): the groups of a DXF
//! ACDSDATA section, laid out as AutoCAD writes one.

use draftcraft_doc::RawTag;
use draftcraft_dwg::datastore::{self, ASM_SCHEMA};

/// Bytes of modeler data on one group 310, as AutoCAD's DXF has them.
const BYTES_PER_LINE: usize = 127;

/// The groups of an ACDSDATA section holding the store's modeler records, or none when it has
/// none. Records of other schemas (a thumbnail) aren't kept: DraftCraft writes them nowhere.
pub(crate) fn groups(store: &datastore::Store) -> Vec<RawTag> {
    let records: Vec<&datastore::Record> = store.records_of(ASM_SCHEMA).filter(|r| r.handle != 0 && !r.data.is_empty()).collect();
    if records.is_empty() {
        return Vec::new();
    }
    let tag = |code: i32, value: &str| RawTag { code, value: value.to_string() };
    let mut t = vec![tag(70, "2"), tag(71, "4")];
    // The modeler schema (index 0): its two properties, and the four properties every schema
    // has, as sub-records.
    t.extend([
        tag(0, "ACDSSCHEMA"),
        tag(90, "0"),
        tag(1, ASM_SCHEMA),
        tag(2, "AcDbDs::ID"),
        tag(280, "10"),
        tag(91, "8"),
        tag(2, "ASM_Data"),
        tag(280, "15"),
        tag(91, "0"),
    ]);
    for (id, name) in [(1, "AcDbDs::TreatedAsObjectData"), (2, "AcDbDs::Legacy")] {
        t.extend([tag(101, "ACDSRECORD"), tag(95, "0"), tag(90, &id.to_string()), tag(2, name), tag(280, "1"), tag(291, "1")]);
    }
    t.extend([tag(101, "ACDSRECORD"), tag(1, "AcDbDs::ID"), tag(90, "3"), tag(2, "AcDs:Indexable"), tag(280, "1"), tag(291, "1")]);
    t.extend([tag(101, "ACDSRECORD"), tag(1, "AcDbDs::ID"), tag(90, "4"), tag(2, "AcDbDs::HandleAttribute"), tag(280, "7"), tag(282, "1")]);
    // The schemas of those four properties.
    for (i, (schema, property)) in [
        ("AcDbDs::TreatedAsObjectDataSchema", "AcDbDs::TreatedAsObjectData"),
        ("AcDbDs::LegacySchema", "AcDbDs::Legacy"),
        ("AcDbDs::IndexedPropertySchema", "AcDs:Indexable"),
    ]
    .iter()
    .enumerate()
    {
        t.extend([tag(0, "ACDSSCHEMA"), tag(90, &(i + 1).to_string()), tag(1, schema), tag(2, property), tag(280, "1"), tag(91, "0")]);
    }
    t.extend([
        tag(0, "ACDSSCHEMA"),
        tag(90, "4"),
        tag(1, "AcDbDs::HandleAttributeSchema"),
        tag(2, "AcDbDs::HandleAttribute"),
        tag(280, "7"),
        tag(91, "1"),
        tag(284, "1"),
    ]);
    for r in records {
        t.extend([
            tag(0, "ACDSRECORD"),
            tag(90, "0"),
            tag(2, "AcDbDs::ID"),
            tag(280, "10"),
            tag(320, &format!("{:X}", r.handle)),
            tag(2, "ASM_Data"),
            tag(280, "15"),
            tag(94, &r.data.len().to_string()),
        ]);
        t.extend(r.data.chunks(BYTES_PER_LINE).map(|c| RawTag { code: 310, value: hex(c) }));
    }
    t
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02X}");
    }
    s
}
