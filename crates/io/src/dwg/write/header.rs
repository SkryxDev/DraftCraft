//! The header variables, as DWG holds them: the mirror of `super::super::header`.
//!
//! The records keep each variable under its DXF name with its DXF meaning, so the few that DWG
//! stores differently are converted here: the eight packed into one word, the three DWG names
//! differently, and those that name a table record or an object, which DWG refers to.

use std::collections::HashMap;

use draftcraft_dwg::compose::{HeaderKind, header_variables};
use draftcraft_dwg::file::{ColorValue, HeaderValue};
use draftcraft_dxf::Tag;

use super::Cx;
use super::common::lineweight;
use super::rec::{Doc, Groups, hex, text as text_of};

/// A text variable of the header; empty when the drawing has none.
pub(super) fn text<'a>(doc: &Doc<'a>, name: &str) -> &'a str {
    doc.header.get(name).and_then(|v| v.first()).map_or("", text_of)
}

/// An integer variable of the header; 0 when the drawing has none.
pub(super) fn int(doc: &Doc<'_>, name: &str) -> i64 {
    doc.header.get(name).and_then(|v| v.first()).map_or(0, Tag::i64)
}

/// The DXF name of a variable DWG names differently.
fn dxf_name(name: &'static str) -> &'static str {
    match name {
        "MENUNAME" => "MENU",
        "OBSCUREDCOLOR" => "OBSCOLOR",
        "OBSCUREDLTYPE" => "OBSLTYPE",
        "UNKNOWN_B_R2007" => "REALWORLDSCALE",
        "UNKNOWN_BD_R2007_2" => "SHADOWPLANELOCATION",
        // From AutoCAD 2000 the two dates are stored in universal time.
        "TDCREATE" => "TDUCREATE",
        "TDUPDATE" => "TDUUPDATE",
        n => n,
    }
}

/// The object a handle variable refers to; `None` for a variable the drawing gives no object.
fn reference(cx: &Cx<'_>, name: &str) -> Option<u64> {
    let doc = cx.doc;
    let named = |key: &str| cx.named.get(key).copied().unwrap_or(0);
    let by = |table: &HashMap<String, u64>, var: &str| {
        let n = text(doc, var).trim().to_uppercase();
        if n.is_empty() { 0 } else { table.get(&n).copied().unwrap_or(0) }
    };
    let control = |table: &str| cx.controls.iter().find(|(k, _)| k.eq_ignore_ascii_case(table)).map_or(0, |(_, h)| *h);
    Some(match name {
        "BLOCK_CONTROL" => control("BLOCK_RECORD"),
        "LAYER_CONTROL" => control("LAYER"),
        "STYLE_CONTROL" => control("STYLE"),
        "LTYPE_CONTROL" => control("LTYPE"),
        "VIEW_CONTROL" => control("VIEW"),
        "UCS_CONTROL" => control("UCS"),
        "VPORT_CONTROL" => control("VPORT"),
        "APPID_CONTROL" => control("APPID"),
        "DIMSTYLE_CONTROL" => control("DIMSTYLE"),
        "DICTIONARY_NAMED_OBJECTS" => cx.root,
        "DICTIONARY_ACAD_GROUP" => named("ACAD_GROUP"),
        "DICTIONARY_ACAD_MLINESTYLE" => named("ACAD_MLINESTYLE"),
        "DICTIONARY_LAYOUTS" => named("ACAD_LAYOUT"),
        "DICTIONARY_PLOTSETTINGS" => named("ACAD_PLOTSETTINGS"),
        "DICTIONARY_PLOTSTYLES" => named("ACAD_PLOTSTYLENAME"),
        "DICTIONARY_MATERIALS" => named("ACAD_MATERIAL"),
        "DICTIONARY_COLORS" => named("ACAD_COLOR"),
        "DICTIONARY_VISUALSTYLE" => named("ACAD_VISUALSTYLE"),
        "BLOCK_RECORD_MODEL_SPACE" => cx.model,
        "BLOCK_RECORD_PAPER_SPACE" => cx.paper,
        "LTYPE_BYLAYER" => cx.linetype("BYLAYER"),
        "LTYPE_BYBLOCK" => cx.linetype("BYBLOCK"),
        "LTYPE_CONTINUOUS" => cx.linetype("CONTINUOUS"),
        "CLAYER" => cx.layer(text(doc, "CLAYER")),
        "TEXTSTYLE" => cx.style(text(doc, "TEXTSTYLE")),
        "DIMTXSTY" => cx.style(text(doc, "DIMTXSTY")),
        "CELTYPE" => match text(doc, "CELTYPE") {
            "" => cx.linetype("BYLAYER"),
            n => cx.linetype(n),
        },
        "DIMSTYLE" => by(&cx.dim_styles, "DIMSTYLE"),
        "DIMLTYPE" | "DIMLTEX1" | "DIMLTEX2" => by(&cx.linetypes, name),
        "DIMBLK" | "DIMBLK1" | "DIMBLK2" | "DIMLDRBLK" => by(&cx.blocks, name),
        "UCSNAME" | "PUCSNAME" | "UCSBASE" | "PUCSBASE" | "UCSORTHOREF" | "PUCSORTHOREF" => by(&cx.ucss, name),
        // The current multiline style, by its key in the multiline style dictionary.
        "CMLSTYLE" => {
            let key = text(doc, "CMLSTYLE");
            let dict = doc.objects.iter().find(|r| r.handle == named("ACAD_MLINESTYLE") && r.handle != 0);
            let list = dict.map(super::common::entries).unwrap_or_default();
            list.iter().find(|(n, _)| n.eq_ignore_ascii_case(key)).or_else(|| list.first()).map_or(0, |(_, h)| *h)
        }
        "CMATERIAL" | "INTERFEREOBJVS" | "INTERFEREVPVS" | "DRAGVS" => cx.known(hex(text(doc, name))),
        _ => return None,
    })
}

/// Every header variable the drawing gives a value, by its DWG name.
pub(super) fn values(cx: &Cx<'_>) -> HashMap<&'static str, HeaderValue> {
    let doc = cx.doc;
    let mut out = HashMap::new();
    for (name, kind) in header_variables() {
        let groups = doc.header.get(dxf_name(name)).copied();
        let first = groups.and_then(<[Tag]>::first);
        let g = Groups(groups.unwrap_or(&[]));
        let value = match (name, kind) {
            ("HANDSEED", _) => first.map(|t| HeaderValue::Handle(hex(text_of(t)))),
            (_, HeaderKind::Handle) => reference(cx, name).map(HeaderValue::Handle),
            // One word holds eight variables.
            ("FLAGS", _) => {
                let var = |n: &str, default: i64| doc.header.get(n).and_then(|v| v.first()).map_or(default, Tag::i64);
                let mut f = i64::from(lineweight(var("CELWEIGHT", -1))) & 0x1F;
                f |= (var("ENDCAPS", 0) & 3) << 5;
                f |= (var("JOINSTYLE", 0) & 3) << 7;
                f |= i64::from(var("LWDISPLAY", 0) == 0) << 9;
                f |= i64::from(var("XEDIT", 1) == 0) << 10;
                f |= i64::from(var("EXTNAMES", 1) != 0) << 11;
                f |= i64::from(var("PSTYLEMODE", 1) != 0) << 13;
                f |= i64::from(var("OLESTARTUP", 0) != 0) << 14;
                Some(HeaderValue::Int(f))
            }
            // The decimal separator is a character; DWG stores its code.
            ("DIMDSEP", _) => first.map(|t| match &t.value {
                draftcraft_dxf::Value::Str(s) => HeaderValue::Int(s.chars().next().map_or(46, |c| i64::from(u32::from(c)))),
                _ => HeaderValue::Int(t.i64()),
            }),
            // DXF writes these two angles in degrees.
            ("ANGBASE" | "SNAPANG", _) => first.map(|t| HeaderValue::Real(t.f64().to_radians())),
            (_, HeaderKind::Bool) => first.map(|t| HeaderValue::Bool(t.i64() != 0)),
            (_, HeaderKind::Int) => first.map(|t| HeaderValue::Int(t.i64())),
            (_, HeaderKind::Real | HeaderKind::Time) => first.map(|t| HeaderValue::Real(t.f64())),
            (_, HeaderKind::Point2) => first.map(|_| HeaderValue::Point2(g.p2(10))),
            (_, HeaderKind::Point3) => first.map(|_| HeaderValue::Point3(g.p3(10))),
            (_, HeaderKind::Text) => first.map(|t| HeaderValue::Text(text_of(t).to_string())),
            (_, HeaderKind::Color) => first.map(|t| {
                HeaderValue::Color(match t.i64() {
                    0 => ColorValue::ByBlock,
                    256 => ColorValue::ByLayer,
                    257 => ColorValue::None,
                    i => u8::try_from(i).map_or(ColorValue::ByLayer, ColorValue::Index),
                })
            }),
        };
        if let Some(v) = value {
            out.insert(name, v);
        }
    }
    out
}
