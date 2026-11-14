//! DWG files written from the drawing: the mirror of the reader in [`super`].
//!
//! The drawing is taken as its records ([`rec::Doc`]: what the DXF writer writes for it, with
//! every handle, owner and generated block already in place), and each record is encoded as the
//! DWG object of the same meaning, field by field in the order the format stores it. So a
//! drawing saved as DWG holds what the same drawing saved as DXF holds, and whatever a command
//! adds to one is in the other.
//!
//! `draftcraft-dwg` lays the file out ([`draftcraft_dwg::compose`]); everything about what a
//! record means is decided here. A record the writer has no encoding for is counted in
//! [`Unwritten`], by type: nothing is left out silently.

mod annotation;
mod common;
mod dynblock;
mod entities;
mod header;
mod mleader;
mod objects;
mod rec;
mod solid;
mod table;
mod tables;
mod variables;

use std::collections::{BTreeMap, HashMap, HashSet};

use draftcraft_dwg::DwgVersion as V;
use draftcraft_dwg::compose::{self, Class, ObjectWriter, Parts, Properties, TextEncoding};
use draftcraft_dxf::Tag;

use crate::{IoError, Result};
use rec::{Doc, Rec};

/// The records a DWG file leaves out, counted by their DXF type.
pub type Unwritten = BTreeMap<String, usize>;

/// What the file records beside the drawing.
#[derive(Clone, Debug, Default)]
pub struct FileInfo<'a> {
    /// The program writing the file and its version.
    pub app_name: &'a str,
    pub app_version: &'a str,
    /// The picture file browsers show: a PNG, and the same as a Windows bitmap without its file
    /// header (for the versions before AutoCAD 2013).
    pub preview_png: Option<&'a [u8]>,
    pub preview_bmp: Option<&'a [u8]>,
    /// When the drawing was created and saved, as `YYYY-MM-DDTHH:MM:SS`.
    pub created_iso: String,
    pub updated_iso: String,
}

/// 8-bit text in the code page a file's number names, as the DXF writer stores strings before
/// AutoCAD 2007.
fn ansi(s: &str, code_page: u16) -> Vec<u8> {
    draftcraft_dxf::encode_code_page(s, draftcraft_dwg::file::code_page_name(code_page).unwrap_or("ANSI_1252"))
}

/// The number DWG stores for a code page name (`ANSI_1252` is 30).
fn code_page_number(name: &str) -> u16 {
    (1..=44u16).find(|n| draftcraft_dwg::file::code_page_name(*n).is_some_and(|c| c.eq_ignore_ascii_case(name.trim()))).unwrap_or(30)
}

/// The DWG version of a version code (`AC1032`); `None` for one this writer has no layout for.
pub fn version_of(code: &str) -> Option<V> {
    V::from_tag(code).filter(|v| *v >= V::R2000)
}

/// What the encoders need to know about the whole drawing.
pub(super) struct Cx<'a> {
    pub ver: V,
    /// Parts of records the file has no place for, counted by what they are.
    pub lost: std::cell::RefCell<Unwritten>,
    pub codepage: u16,
    pub doc: &'a Doc<'a>,
    /// Table records by upper-case name.
    pub layers: HashMap<String, u64>,
    pub linetypes: HashMap<String, u64>,
    pub styles: HashMap<String, u64>,
    pub dim_styles: HashMap<String, u64>,
    pub apps: HashMap<String, u64>,
    pub blocks: HashMap<String, u64>,
    pub ucss: HashMap<String, u64>,
    /// The type number of each class, by upper-case DXF name.
    pub classes: HashMap<String, u16>,
    /// The block records of model space and of the active paper space.
    pub model: u64,
    pub paper: u64,
    /// Each table's control object, by table name.
    pub controls: HashMap<&'a str, u64>,
    /// What each object owns, in order: a block record's entities, a polyline's vertices and
    /// its SEQEND, a block reference's attributes and its SEQEND.
    pub owned: HashMap<u64, Vec<u64>>,
    /// An entity's neighbours in its block (AutoCAD 2000 files).
    pub links: HashMap<u64, (u64, u64)>,
    /// Each block record's BLOCK and ENDBLK.
    pub block_ends: HashMap<u64, (u64, u64)>,
    /// The block references of each block record.
    pub inserts: HashMap<u64, Vec<u64>>,
    /// Entries of the named-object dictionary, by upper-case name.
    pub named: HashMap<String, u64>,
    pub root: u64,
    /// The layout of each block record.
    pub layouts: HashMap<u64, u64>,
    /// Every handle the drawing has an object for.
    pub handles: std::collections::HashSet<u64>,
    /// The name a block record is stored under, where it is not its own: DWG stores anonymous
    /// blocks unnumbered (`*D`, `*U`).
    pub block_names: HashMap<u64, String>,
    /// The type of each entity, by handle.
    pub kinds: HashMap<u64, &'a str>,
}

impl Cx<'_> {
    /// A writer for `r` as the DWG type `type_code`, with its extended data written.
    pub fn writer(&self, type_code: u16, r: &Rec<'_>) -> ObjectWriter {
        let mut w = ObjectWriter::new(self.ver, type_code, r.handle, self.codepage, TextEncoding { ansi });
        w.xdata(&common::xdata(self, r.xdata));
        w
    }
    pub fn layer(&self, name: &str) -> u64 {
        self.layers.get(&name.trim().to_uppercase()).or_else(|| self.layers.get("0")).copied().unwrap_or(0)
    }
    pub fn linetype(&self, name: &str) -> u64 {
        self.linetypes.get(&name.trim().to_uppercase()).copied().unwrap_or(0)
    }
    pub fn style(&self, name: &str) -> u64 {
        self.styles.get(&name.trim().to_uppercase()).or_else(|| self.styles.get("STANDARD")).copied().unwrap_or(0)
    }
    pub fn block(&self, name: &str) -> u64 {
        self.blocks.get(&name.trim().to_uppercase()).copied().unwrap_or(0)
    }
    /// A handle the record names, when the drawing has that object; 0 otherwise.
    pub fn known(&self, h: u64) -> u64 {
        if self.handles.contains(&h) { h } else { 0 }
    }
    /// The type of the record with the handle `h`, when it is an entity.
    pub fn doc_kind(&self, h: u64) -> Option<&str> {
        self.kinds.get(&h).copied()
    }
    /// The type number of the class `dxf_name`.
    /// Count a part of a record the file leaves out.
    pub fn lose(&self, what: &str) {
        if let Ok(mut lost) = self.lost.try_borrow_mut() {
            *lost.entry(what.to_string()).or_default() += 1;
        }
    }
    pub fn class(&self, dxf_name: &str) -> Option<u16> {
        self.classes.get(&dxf_name.to_ascii_uppercase()).copied()
    }
    /// The type number of a type the format has a number for (`fixed`), unless the file
    /// declares it as a class.
    pub fn type_code(&self, dxf_name: &str, fixed: u16) -> u16 {
        self.class(dxf_name).unwrap_or(fixed)
    }
}

fn index<'a>(doc: &Doc<'a>, table: &str) -> HashMap<String, u64> {
    doc.table(table).map(|t| t.records.iter().map(|r| (r.all().s(2).trim().to_uppercase(), r.handle)).collect()).unwrap_or_default()
}

/// One encoded object.
pub(super) struct Encoded {
    pub handle: u64,
    /// Its DXF type, which the comparison with AutoCAD's files tallies by.
    #[cfg_attr(not(test), allow(dead_code))]
    pub kind: String,
    pub bytes: Vec<u8>,
}

/// Every object of the drawing, encoded for `ver`, with what was left out.
pub(super) struct Objects<'a> {
    pub cx: Cx<'a>,
    pub objects: Vec<Encoded>,
    pub unwritten: Unwritten,
    /// The handles of the records left out.
    pub dropped: HashSet<u64>,
}

/// AutoCAD's class definitions of the types the writer encodes that have no type number of their
/// own: (DXF name, C++ name, application, proxy flags, entity).
const STANDARD_CLASSES: &[(&str, &str, &str, u16, bool)] = &[
    ("ACAD_EVALUATION_GRAPH", "AcDbEvalGraph", "ObjectDBX Classes", 0, false),
    ("ACAD_TABLE", "AcDbTable", "ObjectDBX Classes", 1025, true),
    ("ACDBBLOCKPARAMDEPENDENCYBODY", "AcDbBlockParameterDependencyBody", "ObjectDBX Classes", 0, false),
    ("ACDBDETAILVIEWSTYLE", "AcDbDetailViewStyle", "ObjectDBX Classes", 1025, false),
    ("ACDBDICTIONARYWDFLT", "AcDbDictionaryWithDefault", "ObjectDBX Classes", 0, false),
    ("ACDBPLACEHOLDER", "AcDbPlaceHolder", "ObjectDBX Classes", 0, false),
    ("ACDBSECTIONVIEWSTYLE", "AcDbSectionViewStyle", "ObjectDBX Classes", 1025, false),
    ("ACDB_BLOCKREPRESENTATION_DATA", "AcDbBlockRepresentationData", "ObjectDBX Classes", 0, false),
    ("ACDB_DYNAMICBLOCKPROXYNODE", "AcDbDynamicBlockProxyNode", "ObjectDBX Classes", 0, false),
    ("ACDB_DYNAMICBLOCKPURGEPREVENTER_VERSION", "AcDbDynamicBlockPurgePreventer", "ObjectDBX Classes", 0, false),
    ("BLOCKALIGNEDCONSTRAINTPARAMETER", "AcDbBlockAlignedConstraintParameter", "ObjectDBX Classes", 0, false),
    ("BLOCKALIGNMENTGRIP", "AcDbBlockAlignmentGrip", "ObjectDBX Classes", 0, false),
    ("BLOCKALIGNMENTPARAMETER", "AcDbBlockAlignmentParameter", "ObjectDBX Classes", 0, false),
    ("BLOCKANGULARCONSTRAINTPARAMETER", "AcDbBlockAngularConstraintParameter", "ObjectDBX Classes", 0, false),
    ("BLOCKARRAYACTION", "AcDbBlockArrayAction", "ObjectDBX Classes", 0, false),
    ("BLOCKBASEPOINTPARAMETER", "AcDbBlockBasepointParameter", "ObjectDBX Classes", 0, false),
    ("BLOCKDIAMETRICCONSTRAINTPARAMETER", "AcDbBlockDiametricConstraintParameter", "ObjectDBX Classes", 0, false),
    ("BLOCKFLIPACTION", "AcDbBlockFlipAction", "ObjectDBX Classes", 0, false),
    ("BLOCKFLIPGRIP", "AcDbBlockFlipGrip", "ObjectDBX Classes", 0, false),
    ("BLOCKFLIPPARAMETER", "AcDbBlockFlipParameter", "ObjectDBX Classes", 0, false),
    ("BLOCKGRIPLOCATIONCOMPONENT", "AcDbBlockGripExpr", "ObjectDBX Classes", 0, false),
    ("BLOCKHORIZONTALCONSTRAINTPARAMETER", "AcDbBlockHorizontalConstraintParameter", "ObjectDBX Classes", 0, false),
    ("BLOCKLINEARCONSTRAINTPARAMETER", "AcDbBlockLinearConstraintParameter", "ObjectDBX Classes", 0, false),
    ("BLOCKLINEARGRIP", "AcDbBlockLinearGrip", "ObjectDBX Classes", 0, false),
    ("BLOCKLINEARPARAMETER", "AcDbBlockLinearParameter", "ObjectDBX Classes", 0, false),
    ("BLOCKLOOKUPACTION", "AcDbBlockLookupAction", "ObjectDBX Classes", 0, false),
    ("BLOCKLOOKUPGRIP", "AcDbBlockLookupGrip", "ObjectDBX Classes", 0, false),
    ("BLOCKLOOKUPPARAMETER", "AcDbBlockLookupParameter", "ObjectDBX Classes", 0, false),
    ("BLOCKMOVEACTION", "AcDbBlockMoveAction", "ObjectDBX Classes", 0, false),
    ("BLOCKPARAMDEPENDENCYBODY", "AcDbBlockParameterDependencyBody", "ObjectDBX Classes", 0, false),
    ("BLOCKPOINTPARAMETER", "AcDbBlockPointParameter", "ObjectDBX Classes", 0, false),
    ("BLOCKPOLARGRIP", "AcDbBlockPolarGrip", "ObjectDBX Classes", 0, false),
    ("BLOCKPOLARPARAMETER", "AcDbBlockPolarParameter", "ObjectDBX Classes", 0, false),
    ("BLOCKPOLARSTRETCHACTION", "AcDbBlockPolarStretchAction", "ObjectDBX Classes", 0, false),
    ("BLOCKPROPERTIESTABLEGRIP", "AcDbBlockPropertiesTableGrip", "ObjectDBX Classes", 0, false),
    ("BLOCKRADIALCONSTRAINTPARAMETER", "AcDbBlockRadialConstraintParameter", "ObjectDBX Classes", 0, false),
    ("BLOCKROTATEACTION", "AcDbBlockRotationAction", "ObjectDBX Classes", 0, false),
    ("BLOCKROTATIONGRIP", "AcDbBlockRotationGrip", "ObjectDBX Classes", 0, false),
    ("BLOCKROTATIONPARAMETER", "AcDbBlockRotationParameter", "ObjectDBX Classes", 0, false),
    ("BLOCKSCALEACTION", "AcDbBlockScaleAction", "ObjectDBX Classes", 0, false),
    ("BLOCKSTRETCHACTION", "AcDbBlockStretchAction", "ObjectDBX Classes", 0, false),
    ("BLOCKUSERPARAMETER", "AcDbBlockUserParameter", "ObjectDBX Classes", 0, false),
    ("BLOCKVERTICALCONSTRAINTPARAMETER", "AcDbBlockVerticalConstraintParameter", "ObjectDBX Classes", 0, false),
    ("BLOCKVISIBILITYGRIP", "AcDbBlockVisibilityGrip", "ObjectDBX Classes", 0, false),
    ("BLOCKVISIBILITYPARAMETER", "AcDbBlockVisibilityParameter", "ObjectDBX Classes", 0, false),
    ("BLOCKXYGRIP", "AcDbBlockXYGrip", "ObjectDBX Classes", 0, false),
    ("BLOCKXYPARAMETER", "AcDbBlockXYParameter", "ObjectDBX Classes", 0, false),
    ("CELLSTYLEMAP", "AcDbCellStyleMap", "ObjectDBX Classes", 1152, false),
    ("DICTIONARYVAR", "AcDbDictionaryVar", "ObjectDBX Classes", 0, false),
    ("DIMASSOC", "AcDbDimAssoc", "AcDbDimAssoc", 0, false),
    ("HELIX", "AcDbHelix", "ObjectDBX Classes", 4095, true),
    ("IMAGE", "AcDbRasterImage", "ISM", 127, true),
    ("IMAGEDEF", "AcDbRasterImageDef", "ISM", 0, false),
    ("IMAGEDEF_REACTOR", "AcDbRasterImageDefReactor", "ISM", 1, false),
    ("LAYOUT", "AcDbLayout", "ObjectDBX Classes", 0, false),
    ("MATERIAL", "AcDbMaterial", "ObjectDBX Classes", 1153, false),
    ("MLEADERSTYLE", "AcDbMLeaderStyle", "ACDB_MLEADERSTYLE_CLASS", 4095, false),
    ("MULTILEADER", "AcDbMLeader", "ACDB_MLEADER_CLASS", 3071, true),
    ("PDFDEFINITION", "AcDbPdfDefinition", "ObjectDBX Classes", 1153, false),
    ("PDFUNDERLAY", "AcDbPdfReference", "ObjectDBX Classes", 4095, true),
    ("RASTERVARIABLES", "AcDbRasterVariables", "ISM", 0, false),
    ("SCALE", "AcDbScale", "ObjectDBX Classes", 1153, false),
    ("SORTENTSTABLE", "AcDbSortentsTable", "ObjectDBX Classes", 0, false),
    ("SPATIAL_FILTER", "AcDbSpatialFilter", "ObjectDBX Classes", 0, false),
    ("TABLESTYLE", "AcDbTableStyle", "ObjectDBX Classes", 4095, false),
    ("VISUALSTYLE", "AcDbVisualStyle", "ObjectDBX Classes", 4095, false),
    ("WIPEOUT", "AcDbWipeout", "WipeOut", 127, true),
    ("WIPEOUTVARIABLES", "AcDbWipeoutVariables", "WipeOut", 0, false),
];

/// The classes of the file: the drawing's, numbered from 500 in order. An AutoCAD 2000 file
/// also declares the placeholder and the layout as classes (from 2004 they are types of the
/// format), as AutoCAD's own 2000 files do.
fn classes(doc: &Doc<'_>, ver: V) -> Vec<Class> {
    let mut list: Vec<Class> = doc
        .classes
        .iter()
        .take(usize::from(u16::MAX) - 600)
        .map(|c| Class {
            number: 0,
            dxf_name: c.dxf_name.to_string(),
            cpp_name: c.cpp_name.to_string(),
            app_name: c.app_name.to_string(),
            proxy_flags: u16::try_from(c.proxy_flags).unwrap_or(0),
            instance_count: u32::try_from(c.instance_count).unwrap_or(0),
            was_zombie: c.was_zombie,
            is_entity: c.is_entity,
            versions: compose::class_versions(c.dxf_name),
        })
        .collect();
    // A type with no number of its own needs its class: the records' declare most, AutoCAD's
    // definition stands in for one they don't (an object the writer adds, a record kept from
    // another program's file whose class was not).
    let kinds: std::collections::BTreeSet<String> = doc
        .entities
        .iter()
        .chain(&doc.objects)
        .chain(doc.blocks.iter().flat_map(|b| &b.entities))
        .map(|r| r.kind.to_ascii_uppercase())
        .chain(if ver <= V::R2000 { vec!["ACDBPLACEHOLDER".to_string(), "LAYOUT".to_string()] } else { Vec::new() })
        .collect();
    for kind in &kinds {
        if list.iter().any(|c| c.dxf_name.eq_ignore_ascii_case(kind)) {
            continue;
        }
        // From 2004 the placeholder and the layout are types of the format.
        if ver >= V::R2004 && matches!(kind.as_str(), "ACDBPLACEHOLDER" | "LAYOUT") {
            continue;
        }
        if let Some(&(dxf_name, cpp_name, app_name, proxy_flags, is_entity)) = STANDARD_CLASSES.iter().find(|c| c.0 == kind) {
            list.push(Class {
                number: 0,
                dxf_name: dxf_name.to_string(),
                cpp_name: cpp_name.to_string(),
                app_name: app_name.to_string(),
                proxy_flags,
                instance_count: 0,
                was_zombie: false,
                is_entity,
                versions: compose::class_versions(dxf_name),
            });
        }
    }
    for (i, c) in list.iter_mut().enumerate() {
        c.number = 500 + u16::try_from(i).unwrap_or(0);
    }
    list
}

fn context<'a>(doc: &'a Doc<'a>, ver: V) -> Cx<'a> {
    let blocks = index(doc, "BLOCK_RECORD");
    let model = blocks.get("*MODEL_SPACE").copied().unwrap_or(0);
    let paper = blocks.get("*PAPER_SPACE").copied().unwrap_or(0);
    let controls = doc.tables.iter().map(|t| (t.name, t.head.handle)).collect();
    let codepage = doc.header.get("DWGCODEPAGE").and_then(|v| v.first()).map_or(30, |t| code_page_number(rec::text(t)));
    let mut cx = Cx {
        ver,
        lost: std::cell::RefCell::default(),
        codepage,
        doc,
        layers: index(doc, "LAYER"),
        linetypes: index(doc, "LTYPE"),
        styles: index(doc, "STYLE"),
        dim_styles: index(doc, "DIMSTYLE"),
        apps: index(doc, "APPID"),
        blocks,
        ucss: index(doc, "UCS"),
        classes: classes(doc, ver).into_iter().map(|c| (c.dxf_name.to_ascii_uppercase(), c.number)).collect(),
        model,
        paper,
        controls,
        owned: HashMap::new(),
        links: HashMap::new(),
        block_ends: HashMap::new(),
        inserts: HashMap::new(),
        named: HashMap::new(),
        root: 0,
        layouts: HashMap::new(),
        handles: std::collections::HashSet::new(),
        kinds: HashMap::new(),
        block_names: HashMap::new(),
    };
    common::structure(&mut cx);
    cx
}

/// Encode every record of `doc`.
pub(super) fn encode<'a>(doc: &'a Doc<'a>, ver: V) -> Objects<'a> {
    let cx = context(doc, ver);
    let mut out = Objects { cx, objects: Vec::new(), unwritten: Unwritten::new(), dropped: HashSet::new() };
    let put = |out: &mut Objects<'a>, r: &Rec<'_>, w: Option<ObjectWriter>| match w {
        Some(w) if r.handle != 0 => out.objects.push(Encoded { handle: r.handle, kind: r.kind.to_string(), bytes: w.finish() }),
        _ => {
            *out.unwritten.entry(r.kind.to_string()).or_default() += 1;
            if r.handle != 0 {
                out.dropped.insert(r.handle);
            }
        }
    };
    for t in &doc.tables {
        let w = tables::control(&out.cx, t);
        put(&mut out, &t.head, w);
        for r in &t.records {
            let w = tables::record(&out.cx, t, r);
            put(&mut out, r, w);
        }
    }
    for b in &doc.blocks {
        let w = entities::entity(&out.cx, &b.begin);
        put(&mut out, &b.begin, w);
        for r in &b.entities {
            let w = entities::entity(&out.cx, r);
            put(&mut out, r, w);
        }
        if let Some(end) = &b.end {
            let w = entities::entity(&out.cx, end);
            put(&mut out, end, w);
        }
    }
    for r in &doc.entities {
        let w = entities::entity(&out.cx, r);
        put(&mut out, r, w);
    }
    for r in &doc.objects {
        let w = objects::object(&out.cx, r);
        put(&mut out, r, w);
    }
    let lost = out.cx.lost.take();
    for (what, n) in lost {
        *out.unwritten.entry(what).or_default() += n;
    }
    out
}

/// The objects of `doc` (not the entities) the file can't hold, by handle and counted by type,
/// with the extension dictionaries that hold nothing else: the drawing is encoded without them,
/// so that no dictionary entry, reactor or reference names an object the file doesn't have.
/// AutoCAD leaves out the same: from 2010 a table style's cell style map, kept for DXF, and
/// the dictionary that holds it.
pub(super) fn left_out(doc: &Doc<'_>, ver: V) -> (HashSet<u64>, Unwritten) {
    let cx = context(doc, ver);
    let mut dropped = HashSet::new();
    let mut counted = Unwritten::new();
    for r in doc.objects.iter().filter(|r| r.handle != 0) {
        if objects::object(&cx, r).is_none() {
            dropped.insert(r.handle);
            *counted.entry(r.kind.to_string()).or_default() += 1;
        }
    }
    empty_extension_dictionaries(doc, &mut dropped);
    (dropped, counted)
}

/// Add to `dropped` each extension dictionary whose entries are all dropped (and the ones that
/// hold only those, and so on).
fn empty_extension_dictionaries(doc: &Doc<'_>, dropped: &mut HashSet<u64>) {
    if dropped.is_empty() {
        return;
    }
    let extension: HashSet<u64> = doc.every().map(|r| r.xdict).filter(|h| *h != 0).collect();
    let dictionaries: Vec<&Rec<'_>> =
        doc.objects.iter().filter(|r| matches!(r.kind, "DICTIONARY" | "ACDBDICTIONARYWDFLT") && extension.contains(&r.handle)).collect();
    // Each round drops at least one dictionary or ends.
    for _ in 0..=dictionaries.len() {
        let mut changed = false;
        for r in &dictionaries {
            if dropped.contains(&r.handle) {
                continue;
            }
            let entries = common::entries(r);
            if !entries.is_empty() && entries.iter().all(|(_, h)| dropped.contains(h)) {
                dropped.insert(r.handle);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

/// Encode `doc` for `ver` without the records the file can't hold: `pruned` holds `doc`
/// without the objects [`left_out`] finds, `again` the drawing without what the encoding of
/// that still leaves out (an entity), when there is any. Every record left out is counted.
pub(super) fn encode_pruned<'d>(doc: &'d Doc<'d>, ver: V, counted: Unwritten, again: &'d mut Option<Doc<'d>>) -> Objects<'d> {
    let first = encode(doc, ver);
    let mut more = first.dropped.clone();
    let mut out = if more.is_empty() {
        first
    } else {
        empty_extension_dictionaries(doc, &mut more);
        let next = again.insert(doc.without(&more));
        let mut out = encode(next, ver);
        merge(&mut out.unwritten, first.unwritten);
        out
    };
    merge(&mut out.unwritten, counted);
    out
}

/// Add the counts of `from` to `into`, each type at the larger of its two counts.
fn merge(into: &mut Unwritten, from: Unwritten) {
    for (k, n) in from {
        let v = into.entry(k).or_default();
        *v = (*v).max(n);
    }
}

/// The drawing whose records are `tags`, as a DWG file of version `ver`, with the records the
/// file leaves out.
/// Add the drawing variables dictionary AutoCAD writes in every DWG, with the drawing's values,
/// when the records have none (see `variables`).
pub fn add_variable_dictionary(tags: &mut Vec<Tag>, d: &draftcraft_doc::Drawing) {
    variables::add(tags, &variables::values(d));
}

pub fn write(tags: &[Tag], ver: V, info: &FileInfo<'_>) -> Result<(Vec<u8>, Unwritten)> {
    let parsed = Doc::parse(tags);
    let (dropped, counted) = left_out(&parsed, ver);
    let pruned = if dropped.is_empty() { None } else { Some(parsed.without(&dropped)) };
    let doc = pruned.as_ref().unwrap_or(&parsed);
    let mut again = None;
    let encoded = encode_pruned(doc, ver, counted, &mut again);
    let doc = &encoded.cx.doc;
    let cx = &encoded.cx;
    let objects: Vec<(u64, Vec<u8>)> = encoded.objects.into_iter().map(|o| (o.handle, o.bytes)).collect();
    // Two objects under one handle: a file AutoCAD would open with one of them lost. It happens
    // only when the drawing has used up its handles (the records then repeat the last one).
    let mut seen = std::collections::HashSet::with_capacity(objects.len());
    if let Some((h, _)) = objects.iter().find(|(h, _)| !seen.insert(*h)) {
        return Err(IoError::Format(format!(
            "DWG: the drawing has run out of object handles (two objects would share {h:X}); it can't be saved as DWG"
        )));
    }
    let top = objects.iter().map(|o| o.0).max().unwrap_or(0);
    let vars = header::values(cx);
    let seed = match vars.get("HANDSEED") {
        Some(draftcraft_dwg::file::HeaderValue::Handle(h)) => (*h).max(top.saturating_add(1)),
        _ => top.saturating_add(1),
    };
    let real = |name: &str| match vars.get(name) {
        Some(draftcraft_dwg::file::HeaderValue::Real(r)) => *r,
        _ => 0.0,
    };
    let properties = Properties { last_saved_by: header::text(doc, "LASTSAVEDBY").to_string(), ..Properties::default() };
    let fonts: Vec<(String, u64)> =
        doc.table("STYLE").map(|t| t.records.iter().map(|r| (r.all().s(3).to_string(), r.handle)).collect()).unwrap_or_default();
    let class_list = classes(doc, ver);
    let parts = Parts {
        version: ver,
        codepage: cx.codepage,
        text: TextEncoding { ansi },
        header: &|name| vars.get(name).cloned(),
        classes: &class_list,
        objects: &objects,
        handseed: seed,
        measurement: u16::try_from(header::int(doc, "MEASUREMENT")).unwrap_or(0),
        created: real("TDCREATE"),
        updated: real("TDUPDATE"),
        editing: real("TDINDWG"),
        created_iso: &info.created_iso,
        updated_iso: &info.updated_iso,
        properties: &properties,
        preview_png: info.preview_png,
        preview_bmp: info.preview_bmp,
        app_name: info.app_name,
        app_version: info.app_version,
        fonts: &fonts,
    };
    let bytes = compose::compose(&parts).map_err(|e| IoError::Format(format!("DWG: {e}")))?;
    Ok((bytes, encoded.unwritten))
}

#[cfg(test)]
mod tests;
