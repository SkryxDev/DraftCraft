//! The drawing variables dictionary (`AcDbVariableDictionary`) AutoCAD writes in the named
//! object dictionary of every DWG. DraftCraft's DXF has none, as AutoCAD's own DXF often has
//! none either; but some programs read a DWG without it as empty (#1813: Vectric Aspire 10.5 and
//! an AutoCAD 2004 file), so a DWG gets AutoCAD's dictionary when the drawing has none. A
//! dictionary the drawing has (read from another program's file) keeps its own entries.

use draftcraft_doc::{Drawing, HVal};
use draftcraft_dxf::Tag;

use super::rec::{hex, text};

/// AutoCAD's key for the dictionary in the named object dictionary. Keys are matched without
/// regard to case, as AutoCAD matches them.
const KEY: &str = "AcDbVariableDictionary";

/// The variables the dictionary holds, by name, with the drawing's values: the current
/// multileader and table styles and annotation scale, DIMASSOC and HIDETEXT (a value the
/// variable doesn't accept, which a hostile file can hold, is AutoCAD's default), and the two
/// layer notification settings, which have no header variable.
pub(crate) fn values(d: &Drawing) -> Vec<(&'static str, String)> {
    let name = |var: &str, default: &str| {
        d.header.get(var).and_then(HVal::as_str).map(str::trim).filter(|s| !s.is_empty()).unwrap_or(default).to_string()
    };
    let int = |var: &str, valid: std::ops::RangeInclusive<i64>, default: i64| {
        d.header.get(var).and_then(HVal::as_i64).filter(|v| valid.contains(v)).unwrap_or(default).to_string()
    };
    vec![
        ("CANNOSCALE", name("CANNOSCALE", "1:1")),
        ("CMLEADERSTYLE", name("CMLEADERSTYLE", "Standard")),
        ("CTABLESTYLE", name("CTABLESTYLE", "Standard")),
        ("DIMASSOC", int("DIMASSOC", 0..=2, 2)),
        ("HIDETEXT", int("HIDETEXT", 0..=1, 1)),
        ("LAYEREVAL", "0".to_string()),
        ("LAYERNOTIFY", "0".to_string()),
    ]
}

/// Add the dictionary and its variables to the records, unless the named object dictionary
/// already has one. Records that are not laid out as DraftCraft's DXF writer lays them out (no
/// OBJECTS section, no dictionary first in it) are left as they are.
pub(crate) fn add(tags: &mut Vec<Tag>, variables: &[(&str, String)]) {
    let Some(objects) = tags.windows(2).position(|w| w.first().is_some_and(|t| t.code == 0 && text(t) == "SECTION") && w.get(1).is_some_and(|t| t.code == 2 && text(t) == "OBJECTS")) else {
        return;
    };
    let root_start = objects + 2;
    if !tags.get(root_start).is_some_and(|t| t.code == 0 && text(t) == "DICTIONARY") {
        return;
    }
    let root_end = tags.iter().skip(root_start + 1).position(|t| t.code == 0).map_or(tags.len(), |n| root_start + 1 + n);
    let root = tags.get(root_start..root_end).unwrap_or(&[]);
    if root.iter().any(|t| t.code == 3 && text(t).trim().eq_ignore_ascii_case(KEY)) {
        return;
    }
    let Some(root_handle) = root.iter().find(|t| t.code == 5).map(|t| text(t).trim().to_string()) else {
        return;
    };
    let Some(end) = tags.iter().skip(root_end).position(|t| t.code == 0 && text(t) == "ENDSEC").map(|n| root_end + n) else {
        return;
    };
    // New handles above every handle the records use.
    let top = tags.iter().filter(|t| t.code == 5 || t.code == 105).map(|t| hex(text(t))).max().unwrap_or(0);
    let mut next = top;
    let mut handle = || {
        next = next.saturating_add(1);
        format!("{next:X}")
    };
    let dictionary = handle();
    let entries: Vec<(&str, &str, String)> = variables.iter().map(|(name, value)| (*name, value.as_str(), handle())).collect();
    let mut added = vec![
        Tag::s(0, "DICTIONARY"),
        Tag::s(5, dictionary.clone()),
        Tag::s(102, "{ACAD_REACTORS"),
        Tag::s(330, root_handle.clone()),
        Tag::s(102, "}"),
        Tag::s(330, root_handle),
        Tag::s(100, "AcDbDictionary"),
        Tag::i(281, 1),
    ];
    for (name, _, h) in &entries {
        added.push(Tag::s(3, *name));
        added.push(Tag::s(350, h.clone()));
    }
    for (_, value, h) in &entries {
        added.extend([
            Tag::s(0, "DICTIONARYVAR"),
            Tag::s(5, h.clone()),
            Tag::s(102, "{ACAD_REACTORS"),
            Tag::s(330, dictionary.clone()),
            Tag::s(102, "}"),
            Tag::s(330, dictionary.clone()),
            Tag::s(100, "DictionaryVariables"),
            Tag::i(280, 0),
            Tag::s(1, *value),
        ]);
    }
    tags.splice(end..end, added);
    // The entry in the named object dictionary, where AutoCAD's order (by character code) puts it.
    let entries_at: Vec<usize> = (root_start..root_end).filter(|&i| tags.get(i).is_some_and(|t| t.code == 3)).collect();
    let at = entries_at.iter().copied().find(|&i| tags.get(i).is_some_and(|t| text(t) > KEY)).unwrap_or(root_end);
    tags.splice(at..at, [Tag::s(3, KEY), Tag::s(350, dictionary)]);
}
