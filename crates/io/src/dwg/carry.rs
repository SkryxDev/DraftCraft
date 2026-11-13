//! The file's other objects: dictionaries, extension records, scales, plot style names, style
//! objects. What the drawing models of them it takes by the DXF reader's rules (DraftCraft's own
//! records, the scale list, the layers' plot styles, table styles); what it does not model it
//! keeps as the DXF reader keeps a DXF file's (`dxf_raw`): the objects a named-object dictionary
//! entry DraftCraft doesn't write itself leads to, as their DXF groups in
//! [`draftcraft_doc::Foreign`], written back on save.

use std::collections::{HashMap, HashSet};

use draftcraft_doc::foreign::{Foreign, ForeignObject};
use draftcraft_doc::{Drawing, Group, Handle, RawTag};
use draftcraft_dwg::file::{CommonRefs, DwgFile, Groups, NonGraphic, ObjectReader, ObjectRef, XrValue};
use draftcraft_dwg::records::Result as DwgResult;

use super::Unread;
use super::objects::Named;
use super::tables::Tables;

/// Most owners followed from an object up to the named object dictionary.
const MAX_DEPTH: usize = 64;

/// Most members of one group that are read.
const MAX_MEMBERS: u32 = 1_000_000;

/// The structure the drawing rebuilds on save, as it does a DXF file's, when nothing of it is
/// kept: dictionaries, their placeholders and scales, DraftCraft's records, dictionary variables
/// and the raster variables of its images.
const REBUILT: &[&str] = &["DICTIONARY", "ACDBDICTIONARYWDFLT", "ACDBPLACEHOLDER", "SCALE", "XRECORD", "DICTIONARYVAR", "RASTERVARIABLES"];

/// One object of the file this pass decodes: its type, what the codec makes of it (`None` for a
/// type it doesn't decode) and the references every object starts with.
struct Obj {
    type_name: String,
    value: Option<NonGraphic>,
    refs: CommonRefs,
}

/// A number as a DXF group's text, as the DXF reader keeps it.
fn real(v: f64) -> String {
    draftcraft_dxf::Value::Real(v).as_str()
}

fn tag(code: i32, value: impl Into<String>) -> RawTag {
    RawTag { code, value: value.into() }
}

fn hex(h: u64) -> String {
    format!("{h:X}")
}

/// An extension record's or style object's items as DXF groups: a point as its x, y and z.
fn item_tags(items: &[(u16, XrValue)], out: &mut Vec<RawTag>) {
    for (code, v) in items {
        let code = i32::from(*code);
        match v {
            XrValue::Str(s) => out.push(tag(code, s.clone())),
            XrValue::Point(p) => {
                out.push(tag(code, real(p.x)));
                out.push(tag(code + 10, real(p.y)));
                out.push(tag(code + 20, real(p.z)));
            }
            XrValue::Real(x) => out.push(tag(code, real(*x))),
            XrValue::Int8(i) => out.push(tag(code, i.to_string())),
            XrValue::Int16(i) => out.push(tag(code, i.to_string())),
            XrValue::Int32(i) => out.push(tag(code, i.to_string())),
            XrValue::Int64(i) => out.push(tag(code, i.to_string())),
            XrValue::Bool(b) => out.push(tag(code, if *b { "1" } else { "0" })),
            XrValue::Bytes(b) => out.push(tag(code, b.iter().map(|x| format!("{x:02X}")).collect::<String>())),
            XrValue::Handle(h) => out.push(tag(code, hex(h.0))),
        }
    }
}

/// The DXF groups of an object after its type, as AutoCAD writes them and the DXF reader keeps
/// them: its handle, reactors, extension dictionary and owner, then its own groups. `None` for a
/// value that has no DXF form here.
fn object_tags(handle: u64, o: &Obj) -> Option<Vec<RawTag>> {
    let mut t = vec![tag(5, hex(handle))];
    if !o.refs.reactors.is_empty() {
        t.push(tag(102, "{ACAD_REACTORS"));
        t.extend(o.refs.reactors.iter().map(|r| tag(330, hex(*r))));
        t.push(tag(102, "}"));
    }
    if o.refs.xdictionary != 0 {
        t.push(tag(102, "{ACAD_XDICTIONARY"));
        t.push(tag(360, hex(o.refs.xdictionary)));
        t.push(tag(102, "}"));
    }
    t.push(tag(330, hex(o.refs.owner)));
    match o.value.as_ref()? {
        NonGraphic::Dictionary(dict) => {
            t.push(tag(100, "AcDbDictionary"));
            // The hard-owner flag only when set, as AutoCAD's DXF has it.
            if dict.hard_owner != 0 {
                t.push(tag(280, dict.hard_owner.to_string()));
            }
            t.push(tag(281, dict.cloning.to_string()));
            let code = if dict.hard_owner != 0 { 360 } else { 350 };
            for (key, h) in &dict.entries {
                t.push(tag(3, key.clone()));
                t.push(tag(code, hex(h.0)));
            }
            if let Some(default) = dict.default {
                t.push(tag(100, "AcDbDictionaryWithDefault"));
                t.push(tag(340, hex(default.0)));
            }
        }
        NonGraphic::XRecord(x) => {
            t.push(tag(100, "AcDbXrecord"));
            t.push(tag(280, x.cloning.to_string()));
            item_tags(&x.items, &mut t);
        }
        NonGraphic::Placeholder => {}
        NonGraphic::DictionaryVar { schema, value } => {
            t.push(tag(100, "DictionaryVariables"));
            t.push(tag(280, schema.to_string()));
            t.push(tag(1, value.clone()));
        }
        NonGraphic::Scale(s) => {
            t.push(tag(100, "AcDbScale"));
            t.push(tag(70, s.flag.to_string()));
            t.push(tag(300, s.name.clone()));
            t.push(tag(140, real(s.paper_units)));
            t.push(tag(141, real(s.drawing_units)));
            t.push(tag(290, if s.unit_scale { "1" } else { "0" }));
        }
        NonGraphic::Groups(g) => item_tags(&g.items, &mut t),
        // Modelled by the drawing: never kept as groups.
        NonGraphic::Layout(_) | NonGraphic::MLineStyle(_) => return None,
    }
    Some(t)
}

/// GROUP (§20.4.?): description, unnamed and selectable flags, members; as its DXF groups.
fn group(r: &mut ObjectReader) -> DwgResult<(Groups, CommonRefs)> {
    let refs = r.common_refs()?;
    let description = r.text()?;
    let unnamed = r.bs()?;
    let selectable = r.bs()?;
    let n = r.bl()?;
    if n > MAX_MEMBERS {
        return Err(draftcraft_dwg::records::Error::format(format!("too many group members: {n}")));
    }
    let mut items = vec![
        (100, XrValue::Str("AcDbGroup".into())),
        (300, XrValue::Str(description)),
        (70, XrValue::Int16(i16::try_from(unnamed).unwrap_or(0))),
        (71, XrValue::Int16(i16::try_from(selectable).unwrap_or(1))),
    ];
    for _ in 0..n {
        items.push((340, XrValue::Handle(draftcraft_dwg::file::Handle(r.reference()?))));
    }
    Ok((Groups { kind: "GROUP".into(), items }, refs))
}

/// WIPEOUTVARIABLES: whether wipeout frames show (DXF 70), as its DXF groups.
fn wipeout_variables(r: &mut ObjectReader) -> DwgResult<(Groups, CommonRefs)> {
    let refs = r.common_refs()?;
    let frame = r.bs()?;
    let items = vec![(100, XrValue::Str("AcDbWipeoutVariables".into())), (70, XrValue::Int16(i16::try_from(frame).unwrap_or(1)))];
    Ok((Groups { kind: "WIPEOUTVARIABLES".into(), items }, refs))
}

/// The 16-bit item `code` of groups.
fn group_int(g: &Groups, code: u16) -> Option<i64> {
    g.items.iter().find(|(c, _)| *c == code).and_then(|(_, v)| match v {
        XrValue::Int8(i) => Some(i64::from(*i)),
        XrValue::Int16(i) => Some(i64::from(*i)),
        XrValue::Int32(i) => Some(i64::from(*i)),
        _ => None,
    })
}

/// A DXF CLASS record's groups, for a kept object of a class.
fn class_tags(c: &draftcraft_dwg::file::ClassInfo) -> Vec<RawTag> {
    vec![
        tag(1, c.dxf_name.clone()),
        tag(2, c.cpp_name.clone()),
        tag(3, c.app_name.clone()),
        tag(90, c.proxy_flags.to_string()),
        tag(91, c.instance_count.to_string()),
        tag(280, if c.was_zombie { "1" } else { "0" }),
        tag(281, if c.is_entity { "1" } else { "0" }),
    ]
}

/// Whether the object `handle` is kept as the DXF reader keeps a DXF file's objects: its owners
/// lead to an entry of the named object dictionary (`root`) DraftCraft doesn't write itself.
fn retained(handle: u64, objs: &HashMap<u64, Obj>, root: u64, root_keys: &HashMap<u64, String>, memo: &mut HashMap<u64, bool>) -> bool {
    let mut path = Vec::new();
    let mut at = handle;
    let keep = loop {
        if let Some(&known) = memo.get(&at) {
            break known;
        }
        if path.contains(&at) || path.len() >= MAX_DEPTH {
            break false;
        }
        path.push(at);
        let Some(o) = objs.get(&at) else { break false };
        if o.refs.owner == root {
            break root_keys.get(&at).is_some_and(|key| !crate::dxf_raw::modelled_key(key));
        }
        if objs.contains_key(&o.refs.owner) {
            at = o.refs.owner;
            continue;
        }
        break false;
    };
    for p in path {
        memo.insert(p, keep);
    }
    keep
}

/// Read the objects the earlier passes left (`done`): the drawing's part of them, and what it
/// keeps of the rest. Returns the handles accounted for.
#[allow(clippy::too_many_arguments)]
pub(super) fn read(
    file: &DwgFile,
    objects: &[ObjectRef],
    tables: &Tables,
    named: &Named,
    root: u64,
    done: &HashSet<u64>,
    d: &mut Drawing,
    unread: &mut Unread,
) -> HashSet<u64> {
    let text_styles: Vec<(String, u64)> = tables.styles.iter().map(|(h, n)| (n.clone(), *h)).collect();
    let mut objs: HashMap<u64, Obj> = HashMap::new();
    let mut order: Vec<u64> = Vec::new();
    let mut read = HashSet::new();
    let mut style_tags: Vec<(u64, Vec<draftcraft_dxf::Tag>)> = Vec::new();
    for o in objects.iter().filter(|o| !o.is_entity && !done.contains(&o.handle)) {
        let Ok(mut r) = file.read_object(o) else { continue };
        // Table styles by the DXF reader's rule, from the DXF groups AutoCAD writes of them.
        if o.type_name == "TABLESTYLE" {
            match super::table::table_style(&mut r, &tables.styles) {
                Ok(tags) => style_tags.push((o.handle, tags)),
                Err(_) => unread.count("TABLESTYLE (not decoded)"),
            }
            read.insert(o.handle);
            continue;
        }
        let own = match o.type_name.as_str() {
            "GROUP" => Some(group(&mut r)),
            "WIPEOUTVARIABLES" => Some(wipeout_variables(&mut r)),
            _ => None,
        };
        let decoded = match own {
            Some(res) => Some(res.map(|(g, refs)| (NonGraphic::Groups(g), refs))),
            None => r.non_graphic(&text_styles),
        };
        let (value, refs) = match decoded {
            Some(Ok((value, refs))) => (Some(value), refs),
            Some(Err(_)) => {
                unread.count(&format!("{} (not decoded)", o.type_name));
                read.insert(o.handle);
                continue;
            }
            // A type the codec doesn't decode: its owner says whether the drawing would keep it.
            None => match r.common_refs() {
                Ok(refs) => (None, refs),
                Err(_) => continue,
            },
        };
        objs.insert(o.handle, Obj { type_name: o.type_name.clone(), value, refs });
        order.push(o.handle);
    }
    // Dictionary entries by the handle of the object they name.
    let mut keys: HashMap<u64, String> = HashMap::new();
    let mut root_entries: Vec<(String, u64)> = Vec::new();
    for &h in &order {
        if let Some(NonGraphic::Dictionary(dict)) = objs.get(&h).and_then(|o| o.value.as_ref()) {
            for (key, e) in &dict.entries {
                keys.entry(e.0).or_insert_with(|| key.clone());
                if h == root {
                    root_entries.push((key.clone(), e.0));
                }
            }
        }
    }
    let root_keys: HashMap<u64, String> = root_entries.iter().map(|(k, h)| (*h, k.clone())).collect();
    let entry = |key: &str| root_entries.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, h)| *h);
    let dictionary = |h: u64| match objs.get(&h).and_then(|o| o.value.as_ref()) {
        Some(NonGraphic::Dictionary(dict)) => Some(dict),
        _ => None,
    };

    // The annotation scale list, in its dictionary's order.
    let scale_handles: Vec<u64> = match entry("ACAD_SCALELIST").and_then(dictionary) {
        Some(dict) => dict.entries.iter().map(|(_, h)| h.0).collect(),
        None => order.iter().copied().filter(|h| objs.get(h).is_some_and(|o| o.type_name == "SCALE")).collect(),
    };
    let scales: Vec<draftcraft_doc::AnnoScale> = scale_handles
        .iter()
        .filter_map(|h| match objs.get(h).and_then(|o| o.value.as_ref()) {
            Some(NonGraphic::Scale(s)) => crate::dxf_ext::scale_from(&s.name, s.paper_units, s.drawing_units),
            _ => None,
        })
        .fold(Vec::new(), |mut list, s| {
            if list.len() < draftcraft_doc::MAX_SCALES && !list.iter().any(|x: &draftcraft_doc::AnnoScale| x.name.eq_ignore_ascii_case(&s.name)) {
                list.push(s);
            }
            list
        });
    crate::dxf_read::apply_scales(d, &scales);

    // DraftCraft's own records, under their keys in the named object dictionary.
    let text = |key: &str| -> Option<String> {
        let h = entry(key)?;
        let Some(NonGraphic::XRecord(x)) = objs.get(&h).and_then(|o| o.value.as_ref()) else { return None };
        let mut out = String::new();
        for (code, v) in &x.items {
            if let (1 | 3, XrValue::Str(s)) = (code, v) {
                if out.len() > crate::dxf_ext::MAX_PAYLOAD {
                    break;
                }
                out.push_str(s);
            }
        }
        Some(out)
    };
    crate::dxf_read::apply_own_records(d, &text);
    // A file DraftCraft wrote names its own next handle (#495), as the DXF reader takes it.
    if let Some(seed) = text(crate::dxf_ext::HANDSEED_KEY).and_then(|t| Handle::parse_hex(t.trim())) {
        d.bump_handseed(Handle(seed.0.saturating_sub(1)));
    }

    // The layers' plot styles, named by the plot style dictionary's placeholders.
    let plot_styles: HashMap<u64, String> = entry(crate::dxf_ext::PLOT_STYLE_NAMES_KEY)
        .and_then(dictionary)
        .map(|dict| dict.entries.iter().map(|(name, h)| (h.0, name.clone())).collect())
        .unwrap_or_default();
    for (layer, h) in &tables.layer_plot_styles {
        match (plot_styles.get(h), d.layers.iter_mut().find(|l| &l.name == layer)) {
            (Some(name), Some(l)) => l.plot_style.clone_from(name),
            _ => unread.count("LAYER plot style"),
        }
    }

    // Table styles, named by the ACAD_TABLESTYLE dictionary, by the DXF reader's rule: a name the
    // drawing has is replaced.
    let mut modelled: HashSet<u64> = HashSet::new();
    for (h, tags) in &style_tags {
        let Some(name) = named.table_styles.get(h).filter(|n| !n.is_empty()) else {
            unread.count("TABLESTYLE of no dictionary entry");
            continue;
        };
        let st = crate::dxf_table::read_table_style(name, tags);
        match d.table_styles.iter_mut().find(|s| s.name.eq_ignore_ascii_case(name)) {
            Some(x) => *x = st,
            None => d.table_styles.push(st),
        }
    }

    // Groups, named by their dictionary (ACAD_GROUP), by the DXF reader's rule.
    let mut groups = Vec::new();
    for &h in &order {
        let Some(o) = objs.get(&h) else { continue };
        let (Some(NonGraphic::Groups(g)), "GROUP") = (&o.value, o.type_name.as_str()) else { continue };
        let Some(name) = keys.get(&h).filter(|n| !n.is_empty()) else { continue };
        let description = g.items.iter().find_map(|(c, v)| if let (300, XrValue::Str(s)) = (c, v) { Some(s.clone()) } else { None });
        let members = g.items.iter().filter_map(|(c, v)| if let (340, XrValue::Handle(m)) = (c, v) { Some(Handle(m.0)) } else { None }).collect();
        groups.push(Group {
            name: name.clone(),
            description: description.unwrap_or_default(),
            selectable: group_int(g, 71).unwrap_or(1) != 0,
            members,
        });
        modelled.insert(h);
    }
    crate::dxf_read::put_groups(d, groups);
    // The wipeout frame setting (WIPEOUTFRAME), which the DXF reader takes from the object too.
    for o in objs.values().filter(|o| o.type_name == "WIPEOUTVARIABLES") {
        if let Some(NonGraphic::Groups(g)) = &o.value
            && let Some(v) = group_int(g, 70).filter(|v| (0..=2).contains(v))
        {
            d.header.set_i64("WIPEOUTFRAME", v);
        }
    }

    // What the drawing doesn't model, kept as the DXF reader keeps it; the rest of the
    // structure (the dictionaries of what it models, their scales and placeholders, DraftCraft's
    // records) it rebuilds on save, as it does a DXF file's. Anything else is counted.
    let mut memo = HashMap::new();
    let mut foreign = Foreign::default();
    let mut classes: HashSet<String> = HashSet::new();
    for &h in &order {
        let Some(o) = objs.get(&h) else { continue };
        if modelled.contains(&h) {
            read.insert(h);
            continue;
        }
        if !retained(h, &objs, root, &root_keys, &mut memo) {
            if REBUILT.contains(&o.type_name.as_str()) {
                read.insert(h);
            }
            continue;
        }
        let Some(tags) = object_tags(h, o) else {
            unread.count(&o.type_name);
            read.insert(h);
            continue;
        };
        if o.refs.owner == root
            && let Some(key) = root_keys.get(&h)
        {
            foreign.dictionary.push((key.clone(), Handle(h)));
        }
        if let Some(c) = file.class_of(&o.type_name)
            && classes.insert(c.dxf_name.to_ascii_uppercase())
        {
            foreign.classes.push(class_tags(&c));
        }
        foreign.objects.push(ForeignObject { handle: Handle(h), kind: o.type_name.clone(), tags });
        read.insert(h);
    }
    if !foreign.objects.is_empty() {
        let f = std::sync::Arc::make_mut(&mut d.foreign);
        f.objects.extend(foreign.objects);
        f.dictionary.extend(foreign.dictionary);
        f.classes.extend(foreign.classes);
    }
    read
}
