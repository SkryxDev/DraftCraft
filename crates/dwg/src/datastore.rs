//! The data store section of AutoCAD 2013 and later files (`AcDb:AcDsPrototype_1b`, spec
//! chapter 24): where 3D solids, regions and bodies keep their modeler description (binary
//! ACIS), one record per entity, and where AutoCAD 2027 keeps the drawing's thumbnail.
//!
//! A byte stream, not a bit stream: a file header, then file segments (each a header, its data
//! and padding to a multiple of 0x80 bytes). The segment index lists where each segment is; the
//! data index lists each record (its data segment, where its header is, its schema); a data
//! segment holds the records' headers (handle and where the data is) and their data, a record
//! larger than 0x40000 bytes in pages of blob segments; the schema index and schema data name
//! what each record is; the search segment lists each schema's records by handle (AutoCAD
//! ignores a record that is not listed there).
//!
//! [`read`] takes a section apart; [`write`] lays one out the way AutoCAD lays out a new file
//! (revision 1, each segment once, in index order), and [`solids`] is the store of a drawing's
//! modeler records: the schema AutoCAD gives them and its four standard schemas.

use std::collections::BTreeMap;

/// A record's data, by the entity it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub handle: u64,
    /// The record's schema: an index into [`Store::schemas`].
    pub schema: u32,
    pub data: Vec<u8>,
}

/// One property of a schema: a name and a typed value list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Property {
    pub flags: u32,
    pub name: String,
    /// The value type (0 to 15), `None` when flag 2 says it has none.
    pub kind: Option<u32>,
    /// The size of a value of the custom type (14).
    pub custom_size: u32,
    /// The number that follows flag 1 or flag 8.
    pub extra: u32,
    pub values: Vec<Vec<u8>>,
}

/// A schema: what a kind of record holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schema {
    pub name: String,
    pub indexes: Vec<u64>,
    pub properties: Vec<Property>,
}

/// A data store: its schemas, the unknown properties (size and flags, with the id the schema
/// index gives each) and its records.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Store {
    pub schemas: Vec<Schema>,
    pub unknown: Vec<(u32, u32, u32)>,
    pub records: Vec<Record>,
}

impl Store {
    /// The records of the schema named `name`.
    pub fn records_of<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Record> + 'a {
        let index = self.schemas.iter().position(|s| s.name == name).and_then(|i| u32::try_from(i).ok());
        self.records.iter().filter(move |r| Some(r.schema) == index)
    }
}

/// The schema of a solid's, region's or body's modeler description.
pub const ASM_SCHEMA: &str = "AcDb3DSolid_ASM_Data";

/// Why a data store could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "data store: {}", self.0)
    }
}

impl std::error::Error for Error {}

type Result<T> = std::result::Result<T, Error>;

fn err<T>(what: &str) -> Result<T> {
    Err(Error(what.to_string()))
}

const SIGNATURE: &[u8; 4] = b"jard";
const SEGMENT_SIGNATURE: u16 = 0xD5AC;
const SEGMENT_HEADER: usize = 0x30;
/// The size of a record header in a data segment.
const RECORD_HEADER: usize = 20;
/// The first four bytes of a record whose data is in blob pages.
const BLOB_REFERENCE: u32 = 0xBB10_6BB1;
/// The largest record kept in a data segment; a larger one goes in blob pages.
const MAX_INLINE: usize = 0x40000;
/// The most data one blob page holds.
const BLOB_PAGE: usize = 0xF_FFB0;
/// A data segment is closed once its records pass this many bytes.
const DATA_SEGMENT: usize = 0x10_0000;
/// The size of a value of each type (spec 24.2.2.6.1.1).
const TYPE_SIZES: [u32; 16] = [0, 0, 2, 1, 2, 4, 8, 1, 2, 4, 8, 4, 8, 0, 0, 0];
/// Most segments, records, schemas, properties, names and values read (a damaged file's counts
/// can be anything).
const MAX_COUNT: usize = 1 << 20;
/// Most bytes of record data read in all.
const MAX_DATA: usize = 1 << 30;

/// A little-endian reader over a section, every read checked.
struct Bytes<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Bytes<'a> {
    fn new(b: &'a [u8], at: usize) -> Bytes<'a> {
        Bytes { b, at }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(n).ok_or_else(|| Error("offset overflow".into()))?;
        let s = self.b.get(self.at..end).ok_or_else(|| Error(format!("reads past the end at {:#x}", self.at)))?;
        self.at = end;
        Ok(s)
    }
    fn u16(&mut self) -> Result<u16> {
        let s = self.take(2)?;
        Ok(u16::from_le_bytes([s.first().copied().unwrap_or(0), s.get(1).copied().unwrap_or(0)]))
    }
    fn u32(&mut self) -> Result<u32> {
        let mut a = [0u8; 4];
        a.copy_from_slice(self.take(4)?);
        Ok(u32::from_le_bytes(a))
    }
    fn u64(&mut self) -> Result<u64> {
        let mut a = [0u8; 8];
        a.copy_from_slice(self.take(8)?);
        Ok(u64::from_le_bytes(a))
    }
    fn count(&mut self) -> Result<usize> {
        let n = usize::try_from(self.u32()?).map_err(|_| Error("count overflow".into()))?;
        if n > MAX_COUNT { err("implausible count") } else { Ok(n) }
    }
    /// A zero-terminated name.
    fn name(&mut self) -> Result<String> {
        let rest = self.b.get(self.at..).unwrap_or(&[]);
        let n = rest.iter().position(|&c| c == 0).ok_or_else(|| Error("a name runs past the end".into()))?;
        let s = String::from_utf8_lossy(self.take(n)?).into_owned();
        self.at += 1;
        Ok(s)
    }
}

/// A segment's header fields: index, unknown, size, unknown, revision, unknown, system data
/// offset (in 16 bytes), object data offset (in 16 bytes).
struct SegmentHeader {
    system_offset: usize,
    object_offset: usize,
}

fn segment_header(b: &[u8], at: usize) -> Result<SegmentHeader> {
    let mut r = Bytes::new(b, at);
    if r.u16()? != SEGMENT_SIGNATURE {
        return err("a segment without its signature");
    }
    let _name = r.take(6)?;
    let _index = r.u32()?;
    let _unknown = r.u32()?;
    let _size = r.u32()?;
    let _unknown2 = r.u32()?;
    let _revision = r.u32()?;
    let _unknown3 = r.u32()?;
    let system_offset = (r.u32()? as usize).checked_mul(16).ok_or_else(|| Error("offset overflow".into()))?;
    let object_offset = (r.u32()? as usize).checked_mul(16).ok_or_else(|| Error("offset overflow".into()))?;
    Ok(SegmentHeader { system_offset, object_offset })
}

/// Read a data store section.
pub fn read(b: &[u8]) -> Result<Store> {
    let mut h = Bytes::new(b, 0);
    if h.take(4)? != SIGNATURE {
        return err("not a data store");
    }
    let _header_size = h.u32()?;
    let _unknown = h.u32()?;
    let _version = h.u32()?;
    let _unknown2 = h.u32()?;
    let _revision = h.u32()?;
    let segment_index_at = h.u32()? as usize;
    let _unknown3 = h.u32()?;
    let segment_count = h.count()?;
    let schema_index = h.u32()? as usize;
    let data_index = h.u32()? as usize;
    let _search = h.u32()?;

    // Where each segment starts.
    let mut s = Bytes::new(b, segment_index_at.checked_add(SEGMENT_HEADER).ok_or_else(|| Error("offset overflow".into()))?);
    let mut segments = Vec::with_capacity(segment_count.min(4096));
    for _ in 0..segment_count {
        let at = usize::try_from(s.u64()?).map_err(|_| Error("offset overflow".into()))?;
        let size = s.u32()? as usize;
        segments.push((at, size));
    }
    let segment = |i: usize| -> Result<usize> {
        match segments.get(i) {
            Some(&(at, size)) if size > 0 => Ok(at),
            _ => err("a reference to a missing segment"),
        }
    };

    // The schemas and the names of their properties.
    let index_at = segment(schema_index)?;
    let index_header = segment_header(b, index_at)?;
    let mut x = Bytes::new(b, index_at + SEGMENT_HEADER);
    let schema_count = x.count()?;
    let _unknown = x.u32()?;
    let mut schema_refs = Vec::with_capacity(schema_count.min(4096));
    for _ in 0..schema_count {
        let index = x.u32()?;
        let seg = x.u32()? as usize;
        let local = x.u32()? as usize;
        schema_refs.push((index, seg, local));
    }
    let _marker = x.u64()?;
    let unknown_count = x.count()?;
    let _unknown2 = x.u32()?;
    let mut unknown_refs = Vec::with_capacity(unknown_count.min(4096));
    for _ in 0..unknown_count {
        let seg = x.u32()? as usize;
        let local = x.u32()? as usize;
        let id = x.u32()?;
        unknown_refs.push((seg, local, id));
    }
    let mut n = Bytes::new(b, index_at + index_header.system_offset);
    let name_count = n.count()?;
    let mut schema_names = Vec::with_capacity(name_count.min(4096));
    for _ in 0..name_count {
        schema_names.push(n.name()?);
    }

    // Each schema data segment's property names.
    let mut property_names: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    let mut names_of = |seg: usize| -> Result<Vec<String>> {
        if let Some(v) = property_names.get(&seg) {
            return Ok(v.clone());
        }
        let at = segment(seg)?;
        let hd = segment_header(b, at)?;
        let mut n = Bytes::new(b, at + hd.system_offset);
        let count = n.count()?;
        let mut v = Vec::with_capacity(count.min(4096));
        for _ in 0..count {
            v.push(n.name()?);
        }
        property_names.insert(seg, v.clone());
        Ok(v)
    };
    let mut unknown = Vec::with_capacity(unknown_refs.len());
    for &(seg, local, id) in &unknown_refs {
        let mut u = Bytes::new(b, segment(seg)? + SEGMENT_HEADER + local);
        unknown.push((id, u.u32()?, u.u32()?));
    }
    let mut schemas: Vec<(u32, Schema)> = Vec::with_capacity(schema_refs.len());
    for (k, &(index, seg, local)) in schema_refs.iter().enumerate() {
        let names = names_of(seg)?;
        let mut r = Bytes::new(b, segment(seg)? + SEGMENT_HEADER + local);
        let mut schema = schema(&mut r, &names)?;
        schema.name = schema_names.get(k).cloned().unwrap_or_default();
        schemas.push((index, schema));
    }
    schemas.sort_by_key(|(i, _)| *i);

    // The records.
    let mut d = Bytes::new(b, segment(data_index)? + SEGMENT_HEADER);
    let record_count = d.count()?;
    let _unknown = d.u32()?;
    let mut records = Vec::with_capacity(record_count.min(4096));
    let mut total = 0usize;
    for _ in 0..record_count {
        let seg = d.u32()? as usize;
        let local = d.u32()? as usize;
        let schema = d.u32()?;
        if seg == 0 {
            continue;
        }
        let at = segment(seg)?;
        let hd = segment_header(b, at)?;
        let mut rh = Bytes::new(b, at + SEGMENT_HEADER + local);
        let _entry_size = rh.u32()?;
        let _unknown = rh.u32()?;
        let handle = rh.u64()?;
        let offset = rh.u32()? as usize;
        let start = at.checked_add(hd.object_offset).and_then(|v| v.checked_add(offset)).ok_or_else(|| Error("offset overflow".into()))?;
        let mut rd = Bytes::new(b, start);
        let size = rd.u32()?;
        let data = if size == BLOB_REFERENCE { blob(b, &mut rd, &segment)? } else { rd.take(size as usize)?.to_vec() };
        total = total.saturating_add(data.len());
        if total > MAX_DATA {
            return err("implausibly much data");
        }
        records.push(Record { handle, schema, data });
    }
    Ok(Store { schemas: schemas.into_iter().map(|(_, s)| s).collect(), unknown, records })
}

/// A record's data from its blob pages.
fn blob(b: &[u8], r: &mut Bytes<'_>, segment: &dyn Fn(usize) -> Result<usize>) -> Result<Vec<u8>> {
    let total = usize::try_from(r.u64()?).map_err(|_| Error("size overflow".into()))?;
    if total > MAX_DATA {
        return err("an implausibly large record");
    }
    let pages = r.count()?;
    let _record_size = r.u32()?;
    let _page_size = r.u32()?;
    let _last_page = r.u32()?;
    let _unknown = r.u32()?;
    let _unknown2 = r.u32()?;
    let mut out = Vec::with_capacity(total);
    for _ in 0..pages {
        let seg = r.u32()? as usize;
        let _size = r.u32()?;
        let mut p = Bytes::new(b, segment(seg)? + SEGMENT_HEADER);
        let _total = p.u64()?;
        let _start = p.u64()?;
        let _index = p.u32()?;
        let _count = p.u32()?;
        let n = usize::try_from(p.u64()?).map_err(|_| Error("size overflow".into()))?;
        if out.len().saturating_add(n) > total {
            return err("blob pages hold more than the record");
        }
        out.extend_from_slice(p.take(n)?);
    }
    Ok(out)
}

fn schema(r: &mut Bytes<'_>, names: &[String]) -> Result<Schema> {
    let n = r.u16()?;
    let mut indexes = Vec::with_capacity(usize::from(n));
    for _ in 0..n {
        indexes.push(r.u64()?);
    }
    let n = r.u16()?;
    let mut properties = Vec::with_capacity(usize::from(n));
    for _ in 0..n {
        let flags = r.u32()?;
        let name_index = r.u32()?;
        let name = names.get(name_index as usize).cloned().unwrap_or_default();
        let (mut kind, mut custom_size, mut size) = (None, 0, 0);
        if flags & 2 == 0 {
            let k = r.u32()?;
            if k == 0xE {
                custom_size = r.u32()?;
                size = custom_size;
            } else {
                size = TYPE_SIZES.get(k as usize).copied().unwrap_or(0);
            }
            kind = Some(k);
        }
        let extra = if flags == 1 || flags == 8 { r.u32()? } else { 0 };
        let count = r.u16()?;
        let mut values = Vec::with_capacity(usize::from(count));
        if size > 0 {
            for _ in 0..count {
                values.push(r.take(size as usize)?.to_vec());
            }
        }
        properties.push(Property { flags, name, kind, custom_size, extra, values });
    }
    Ok(Schema { name: String::new(), indexes, properties })
}

/// The store of a drawing's modeler records, as AutoCAD (and the Open Design Alliance) lay out
/// a new one: the modeler schema first, then the four standard schemas.
pub fn solids(records: &[(u64, &[u8])]) -> Store {
    let flag = |name: &str| Schema {
        name: format!("{name}Schema"),
        indexes: Vec::new(),
        properties: vec![Property { flags: 0, name: name.to_string(), kind: Some(1), custom_size: 0, extra: 0, values: Vec::new() }],
    };
    let mut indexable = flag("AcDs:Indexable");
    indexable.name = "AcDbDs::IndexedPropertySchema".into();
    let schemas = vec![
        Schema {
            name: ASM_SCHEMA.into(),
            indexes: vec![0, 1],
            properties: vec![
                Property {
                    flags: 0,
                    name: "AcDbDs::ID".into(),
                    kind: Some(10),
                    custom_size: 0,
                    extra: 0,
                    values: vec![2u64.to_le_bytes().to_vec(), 3u64.to_le_bytes().to_vec()],
                },
                Property { flags: 0, name: "ASM_Data".into(), kind: Some(15), custom_size: 0, extra: 0, values: Vec::new() },
            ],
        },
        flag("AcDbDs::TreatedAsObjectData"),
        flag("AcDbDs::Legacy"),
        indexable,
        Schema {
            name: "AcDbDs::HandleAttributeSchema".into(),
            indexes: Vec::new(),
            properties: vec![Property {
                flags: 8,
                name: "AcDbDs::HandleAttribute".into(),
                kind: Some(7),
                custom_size: 0,
                extra: 1,
                values: vec![vec![0]],
            }],
        },
    ];
    Store {
        schemas,
        unknown: vec![(1, 8, 1), (2, 8, 1), (3, 8, 1), (4, 8, 0)],
        records: records.iter().map(|(h, d)| Record { handle: *h, schema: 0, data: d.to_vec() }).collect(),
    }
}

/// One segment being laid out: its name, its data and the two offsets its header records.
struct Seg {
    name: &'static [u8; 6],
    unknown: u32,
    data: Vec<u8>,
    /// Where the data stops and the 0x70 padding starts, before the segment is rounded up.
    system_offset: u32,
    object_offset: u32,
}

fn pad_to(v: &mut Vec<u8>, multiple: usize, byte: u8) {
    while !v.len().is_multiple_of(multiple) {
        v.push(byte);
    }
}

/// Pad a segment's data so what follows starts on a 16-byte boundary of the segment.
fn pad_to_boundary(v: &mut Vec<u8>, byte: u8) {
    while !(SEGMENT_HEADER + v.len()).is_multiple_of(16) {
        v.push(byte);
    }
}

fn put32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn put64(v: &mut Vec<u8>, x: u64) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn len32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Lay out a data store section.
pub fn write(store: &Store) -> Vec<u8> {
    // Segment numbers: 0 unused, 1 the segment index, 2 the data index, then the data segments,
    // the schema index, the schema data, the search segment, and the blob pages last.
    let groups = data_groups(store);
    let first_data = 3usize;
    let schema_index = first_data + groups.len();
    let schema_data = schema_index + 1;
    let search = schema_data + 1;
    let data = data_segments(store, &groups, first_data, search + 1);
    let (sdata, names_offset, schema_at, unknown_at) = schema_data_segment(store);
    let (sindex, schema_names_offset) = schema_index_segment(store, &schema_at, &unknown_at, len32(schema_data));

    // The segments in order, then the segment index that lists them.
    let mut ordered: Vec<Seg> = vec![Seg { name: b"datidx", unknown: 0, data: data.index, system_offset: 0, object_offset: 0 }];
    ordered.extend(data.segments);
    ordered.push(Seg { name: b"schidx", unknown: 0, data: sindex, system_offset: schema_names_offset, object_offset: 0 });
    ordered.push(Seg { name: b"schdat", unknown: 0, data: sdata, system_offset: names_offset, object_offset: 0 });
    ordered.push(Seg { name: b"search", unknown: 0, data: search_segment(store), system_offset: 0, object_offset: 0 });
    ordered.extend(data.blobs);
    lay_out(&ordered, schema_index, search)
}

/// The records split into data segments of at most [`DATA_SEGMENT`] bytes: (record number,
/// record) lists.
fn data_groups(store: &Store) -> Vec<Vec<(usize, &Record)>> {
    let mut groups: Vec<Vec<(usize, &Record)>> = vec![Vec::new()];
    let mut size = 0usize;
    for (k, r) in store.records.iter().enumerate() {
        let stored = if r.data.len() > MAX_INLINE { 52 } else { r.data.len() + 4 };
        if size > 0 && size + stored > DATA_SEGMENT {
            groups.push(Vec::new());
            size = 0;
        }
        size += stored;
        if let Some(last) = groups.last_mut() {
            last.push((k, r));
        }
    }
    groups
}

/// The data segments, the data index that finds each record, and the blob pages of the large
/// records.
struct DataSegments {
    segments: Vec<Seg>,
    index: Vec<u8>,
    blobs: Vec<Seg>,
}

fn data_segments(store: &Store, groups: &[Vec<(usize, &Record)>], first_data: usize, first_blob: usize) -> DataSegments {
    let mut segments: Vec<Seg> = Vec::new();
    let mut index_entries: Vec<(u32, u32, u32)> = vec![(0, 0, 0); store.records.len()];
    let mut blobs: Vec<Seg> = Vec::new();
    for (n, list) in groups.iter().enumerate() {
        let seg_no = len32(first_data + n);
        let mut heads = Vec::new();
        let mut body = Vec::new();
        for (j, (k, r)) in list.iter().enumerate() {
            if let Some(e) = index_entries.get_mut(*k) {
                *e = (seg_no, len32(j * RECORD_HEADER), r.schema);
            }
            put32(&mut heads, len32(RECORD_HEADER));
            put32(&mut heads, 1);
            put64(&mut heads, r.handle);
            put32(&mut heads, len32(body.len()));
            if r.data.len() <= MAX_INLINE {
                put32(&mut body, len32(r.data.len()));
                body.extend_from_slice(&r.data);
            } else {
                blob_reference(&mut body, &r.data, first_blob + blobs.len(), &mut blobs);
            }
        }
        // The data starts on a 16-byte boundary of the segment, after the headers.
        let mut data = heads;
        pad_to_boundary(&mut data, 0x62);
        let object_offset = len32((SEGMENT_HEADER + data.len()) / 16);
        data.extend_from_slice(&body);
        segments.push(Seg { name: b"_data_", unknown: 0, data, system_offset: 0, object_offset });
    }
    let mut index = Vec::new();
    put32(&mut index, len32(index_entries.len()));
    put32(&mut index, 0);
    for (s, l, sc) in &index_entries {
        put32(&mut index, *s);
        put32(&mut index, *l);
        put32(&mut index, *sc);
    }
    DataSegments { segments, index, blobs }
}

/// A large record: a reference to its blob pages in `body`, the pages (segments numbered from
/// `first`) added to `blobs`.
fn blob_reference(body: &mut Vec<u8>, data: &[u8], first: usize, blobs: &mut Vec<Seg>) {
    let pages: Vec<&[u8]> = data.chunks(BLOB_PAGE).collect();
    put32(body, BLOB_REFERENCE);
    put64(body, data.len() as u64);
    put32(body, len32(pages.len()));
    put32(body, len32(32 + 8 * pages.len()));
    put32(body, len32(BLOB_PAGE.min(data.len())));
    put32(body, len32(pages.last().map_or(0, |p| p.len())));
    put32(body, 0);
    put32(body, 0);
    let mut start = 0u64;
    for (i, p) in pages.iter().enumerate() {
        put32(body, len32(first + i));
        put32(body, len32(p.len()));
        let mut page = Vec::with_capacity(p.len() + 32);
        put64(&mut page, data.len() as u64);
        put64(&mut page, start);
        put32(&mut page, len32(i));
        put32(&mut page, len32(pages.len()));
        put64(&mut page, p.len() as u64);
        page.extend_from_slice(p);
        start += p.len() as u64;
        blobs.push(Seg { name: b"blob01", unknown: 1, data: page, system_offset: 0, object_offset: 0 });
    }
}

/// The schema data: the unknown properties, the schemas, then the property names. With the
/// names' place (in 16 bytes), each schema's place, and each unknown property's (place, id).
fn schema_data_segment(store: &Store) -> (Vec<u8>, u32, Vec<u32>, Vec<(u32, u32)>) {
    let mut names: Vec<String> = Vec::new();
    let mut name_index = |n: &str| -> u32 {
        match names.iter().position(|x| x == n) {
            Some(i) => len32(i),
            None => {
                names.push(n.to_string());
                len32(names.len() - 1)
            }
        }
    };
    let mut sdata = Vec::new();
    let mut unknown_at = Vec::new();
    for (id, size, flags) in &store.unknown {
        unknown_at.push((len32(sdata.len()), *id));
        put32(&mut sdata, *size);
        put32(&mut sdata, *flags);
    }
    let mut schema_at = Vec::new();
    for s in &store.schemas {
        schema_at.push(len32(sdata.len()));
        sdata.extend_from_slice(&u16::try_from(s.indexes.len()).unwrap_or(0).to_le_bytes());
        for i in &s.indexes {
            put64(&mut sdata, *i);
        }
        sdata.extend_from_slice(&u16::try_from(s.properties.len()).unwrap_or(0).to_le_bytes());
        for p in &s.properties {
            put32(&mut sdata, p.flags);
            put32(&mut sdata, name_index(&p.name));
            if let Some(k) = p.kind {
                put32(&mut sdata, k);
                if k == 0xE {
                    put32(&mut sdata, p.custom_size);
                }
            }
            if p.flags == 1 || p.flags == 8 {
                put32(&mut sdata, p.extra);
            }
            sdata.extend_from_slice(&u16::try_from(p.values.len()).unwrap_or(0).to_le_bytes());
            for v in &p.values {
                sdata.extend_from_slice(v);
            }
        }
    }
    pad_to_boundary(&mut sdata, 0x73);
    let names_offset = len32((SEGMENT_HEADER + sdata.len()) / 16);
    put32(&mut sdata, len32(names.len()));
    for n in &names {
        sdata.extend_from_slice(n.as_bytes());
        sdata.push(0);
    }
    (sdata, names_offset, schema_at, unknown_at)
}

/// The schema index: each schema's place, the unknown properties' places, then the schema names
/// (their place, in 16 bytes, returned with it).
fn schema_index_segment(store: &Store, schema_at: &[u32], unknown_at: &[(u32, u32)], schema_data_no: u32) -> (Vec<u8>, u32) {
    let mut sindex = Vec::new();
    put32(&mut sindex, len32(store.schemas.len()));
    put32(&mut sindex, 0);
    for (i, at) in schema_at.iter().enumerate() {
        put32(&mut sindex, len32(i));
        put32(&mut sindex, schema_data_no);
        put32(&mut sindex, *at);
    }
    put64(&mut sindex, 0x000A_F10C);
    put32(&mut sindex, len32(unknown_at.len()));
    put32(&mut sindex, 0);
    for (at, id) in unknown_at {
        put32(&mut sindex, schema_data_no);
        put32(&mut sindex, *at);
        put32(&mut sindex, *id);
    }
    pad_to_boundary(&mut sindex, 0x73);
    let schema_names_offset = len32((SEGMENT_HEADER + sindex.len()) / 16);
    put32(&mut sindex, len32(store.schemas.len()));
    for s in &store.schemas {
        sindex.extend_from_slice(s.name.as_bytes());
        sindex.push(0);
    }
    (sindex, schema_names_offset)
}

/// The search segment: for each schema with records, the numbers of its records' data index
/// entries in order, then the records by handle with that number, as AutoCAD writes it.
fn search_segment(store: &Store) -> Vec<u8> {
    let mut found = Vec::new();
    let with_records: Vec<(usize, Vec<(u64, usize)>)> = store
        .schemas
        .iter()
        .enumerate()
        .filter_map(|(i, _)| {
            let mut list: Vec<(u64, usize)> =
                store.records.iter().enumerate().filter(|(_, r)| r.schema as usize == i).map(|(k, r)| (r.handle, k)).collect();
            list.sort();
            (!list.is_empty()).then_some((i, list))
        })
        .collect();
    put32(&mut found, len32(with_records.len()));
    for (i, list) in &with_records {
        put32(&mut found, len32(*i));
        put64(&mut found, list.len() as u64);
        let mut numbers: Vec<usize> = list.iter().map(|(_, k)| *k).collect();
        numbers.sort_unstable();
        for k in numbers {
            put64(&mut found, k as u64);
        }
        put32(&mut found, 1);
        put32(&mut found, 0);
        put32(&mut found, len32(list.len()));
        for (h, k) in list {
            put64(&mut found, *h);
            put64(&mut found, 1);
            put64(&mut found, *k as u64);
        }
    }
    found
}

/// The file header, the segment index and the segments in order (numbered from 2).
fn lay_out(ordered: &[Seg], schema_index: usize, search: usize) -> Vec<u8> {
    let count = ordered.len() + 2;
    let index_size = round(SEGMENT_HEADER + round(12 * count, 16), 0x80);
    let mut places: Vec<(u64, u32)> = vec![(0, 0), (0x80, len32(index_size))];
    let mut at = 0x80 + index_size;
    for s in ordered {
        let size = round(SEGMENT_HEADER + s.data.len(), 0x80);
        places.push((at as u64, len32(size)));
        at += size;
    }
    let total = at;

    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(SIGNATURE);
    for v in [0x80, 8, 2, 0, 1, 0x80, 0, len32(count), len32(schema_index), 2, len32(search), 0, len32(total)] {
        put32(&mut out, v);
    }
    out.resize(0x80, 0);
    let mut index_data = Vec::new();
    for (a, s) in &places {
        put64(&mut index_data, *a);
        put32(&mut index_data, *s);
    }
    pad_to(&mut index_data, 16, 0);
    segment(&mut out, &Seg { name: b"segidx", unknown: 0, data: index_data, system_offset: 0, object_offset: 0 }, 1);
    for (i, s) in ordered.iter().enumerate() {
        segment(&mut out, s, len32(i + 2));
    }
    out
}

fn round(n: usize, to: usize) -> usize {
    n.div_ceil(to) * to
}

/// One segment: header, data, 0x70 padding to a multiple of 0x80 bytes.
fn segment(out: &mut Vec<u8>, s: &Seg, index: u32) {
    let start = out.len();
    out.extend_from_slice(&SEGMENT_SIGNATURE.to_le_bytes());
    out.extend_from_slice(s.name);
    let size = round(SEGMENT_HEADER + s.data.len(), 0x80);
    for v in [index, s.unknown, len32(size), 0, 1, 0, s.system_offset, s.object_offset] {
        put32(out, v);
    }
    out.extend_from_slice(&[0x55; 8]);
    out.extend_from_slice(&s.data);
    while out.len() - start < size {
        out.push(0x70);
    }
}

#[cfg(test)]
mod tests;
