//! SHEETSET (Sheet Set Manager): an ordered list of sheets, each a layout of a drawing file,
//! kept in a sheet set file of DraftCraft's own (`.dcss`, JSON; AutoCAD's `.dst` format is not
//! published). Sheets can be added, renumbered, retitled, reordered and removed, and the whole
//! set published to PDF, one file per sheet.
//!
//! The sheet set file is input like any other file: its size, its number of sheets and every
//! field are checked. A drawing inside the sheet set file's folder is stored by its relative path,
//! so the project folder can move.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::cmd::blocks::insert_file::read_drawing;
use crate::cmd::file::io;
use crate::cmd::{CommandSpec, bad, str_param};
use crate::{Result, Session};

/// Most sheets in a set.
pub const MAX_SHEETS: usize = 1_000;
/// Largest sheet set file read.
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
/// Longest name, title, number or description kept.
const MAX_TEXT: usize = 1_000;
/// The `format` value of a sheet set file.
const FORMAT: &str = "draftcraft-sheetset";

/// A sheet set as saved.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetSet {
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub sheets: Vec<Sheet>,
}

/// One sheet: a layout of a drawing file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sheet {
    #[serde(default)]
    pub number: String,
    #[serde(default)]
    pub title: String,
    /// The drawing, relative to the sheet set file's folder when it is inside it.
    pub file: String,
    pub layout: String,
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("sheetset", "Sheet Set Manager", run_sheetset)
            .menu(&["Tools", "Palettes", "Sheet Set Manager"])
            .alias(&["ssm", "opensheetset", "newsheetset"])
            .params(
                "{operation: new|list|add|update|move|remove|publish, path: sheet set file (.dcss), name?, description? (new), file?: \
                 drawing (add; its layouts, or `layout`), layout?, number?, title? (add, update), index?: 0-based sheet (update, move, \
                 remove), to?: new index (move), output?: folder for publish (else base64 `data` per sheet), overwrite?: bool (new)} → \
                 {path, name, description, sheets: [{index, number, title, file, layout}]} (publish: {sheets: [{index, layout, path? | \
                 data}]})",
            )
            .noundo(),
    ]
}

fn run_sheetset(s: &mut Session, p: &Value) -> Result<Value> {
    let path = str_param(p, "path").filter(|x| !x.trim().is_empty()).ok_or_else(|| bad("sheetset", "`path` names the sheet set file (.dcss)"))?;
    let op = str_param(p, "operation").unwrap_or("list").to_ascii_lowercase();
    let text = |k: &str| str_param(p, k).map(|v| clip(v.trim()));
    let index = |set: &SheetSet| -> Result<usize> {
        let i = p.get("index").and_then(Value::as_u64).ok_or_else(|| bad("sheetset", "`index` is the sheet's position, from 0"))?;
        usize::try_from(i).ok().filter(|i| *i < set.sheets.len()).ok_or_else(|| bad("sheetset", format!("there is no sheet {i}")))
    };
    match op.as_str() {
        "new" => {
            if exists(path) && !p.get("overwrite").and_then(Value::as_bool).unwrap_or(false) {
                return Err(bad("sheetset", format!("{path} already exists (pass overwrite: true to replace it)")));
            }
            let name = text("name").filter(|n| !n.is_empty()).unwrap_or_else(|| stem(path));
            let set = SheetSet { format: FORMAT.into(), version: 1, name, description: text("description").unwrap_or_default(), sheets: Vec::new() };
            save(path, &set)?;
            Ok(describe(path, &set, format!("Created sheet set \"{}\".", set.name)))
        }
        "list" | "open" => {
            let set = load(path)?;
            let n = set.sheets.len();
            Ok(describe(path, &set, format!("Sheet set \"{}\": {n} sheet(s).", set.name)))
        }
        "add" => {
            let mut set = load(path)?;
            let file = text("file").filter(|f| !f.is_empty()).ok_or_else(|| bad("sheetset", "`file` names the drawing whose layout is the sheet"))?;
            let resolved = resolve(path, &file);
            let layouts: Vec<String> = match text("layout").filter(|l| !l.is_empty()) {
                Some(l) => vec![l],
                None => {
                    let d = read_drawing(&resolved).map_err(|e| bad("sheetset", e.to_string()))?;
                    let mut ls: Vec<_> = d.layouts.iter().collect();
                    ls.sort_by_key(|l| l.tab_order);
                    ls.iter().map(|l| l.name.clone()).collect()
                }
            };
            if layouts.is_empty() {
                return Err(bad("sheetset", format!("{file} has no layouts")));
            }
            if set.sheets.len().saturating_add(layouts.len()) > MAX_SHEETS {
                return Err(bad("sheetset", format!("a sheet set holds at most {MAX_SHEETS} sheets")));
            }
            let stored = relative(path, &resolved);
            let one = layouts.len() == 1;
            for layout in layouts {
                let number = if one { text("number") } else { None }.unwrap_or_else(|| (set.sheets.len() + 1).to_string());
                let title = if one { text("title") } else { None }.unwrap_or_else(|| format!("{} - {layout}", stem(&stored)));
                set.sheets.push(Sheet { number, title, file: stored.clone(), layout });
            }
            save(path, &set)?;
            Ok(describe(path, &set, format!("Sheet set \"{}\": {} sheet(s).", set.name, set.sheets.len())))
        }
        "update" => {
            let mut set = load(path)?;
            let i = index(&set)?;
            if let Some(sh) = set.sheets.get_mut(i) {
                if let Some(n) = text("number") {
                    sh.number = n;
                }
                if let Some(t) = text("title") {
                    sh.title = t;
                }
                if let Some(l) = text("layout").filter(|l| !l.is_empty()) {
                    sh.layout = l;
                }
            }
            save(path, &set)?;
            Ok(describe(path, &set, format!("Sheet {i} updated.")))
        }
        "move" => {
            let mut set = load(path)?;
            let i = index(&set)?;
            let to = p
                .get("to")
                .and_then(Value::as_u64)
                .and_then(|t| usize::try_from(t).ok())
                .ok_or_else(|| bad("sheetset", "`to` is the new position"))?;
            let to = to.min(set.sheets.len().saturating_sub(1));
            let sh = set.sheets.remove(i);
            set.sheets.insert(to, sh);
            save(path, &set)?;
            Ok(describe(path, &set, format!("Sheet moved to {to}.")))
        }
        "remove" => {
            let mut set = load(path)?;
            let i = index(&set)?;
            let gone = set.sheets.remove(i);
            save(path, &set)?;
            Ok(describe(path, &set, format!("Removed sheet {} {}.", gone.number, gone.title)))
        }
        "publish" => publish(s, path, text("output").filter(|o| !o.is_empty())),
        other => Err(bad("sheetset", format!("unknown operation \"{other}\" (new, list, add, update, move, remove, publish)"))),
    }
}

/// Plot every sheet to a PDF of its own: `<output>/<number> <title>.pdf`, or base64 data.
fn publish(_s: &mut Session, path: &str, output: Option<String>) -> Result<Value> {
    if io().and_then(|h| h.plot).is_none() {
        return Err(bad("sheetset", "plotting is not available in this build"));
    }
    let set = load(path)?;
    if set.sheets.is_empty() {
        return Err(bad("sheetset", "the sheet set has no sheets"));
    }
    let mut out = Vec::new();
    let mut cache: Option<(String, draftcraft_doc::Drawing)> = None;
    for (i, sh) in set.sheets.iter().enumerate() {
        let file = resolve(path, &sh.file);
        let d = match cache.take() {
            Some((f, d)) if f == file => d,
            _ => read_drawing(&file).map_err(|e| bad("sheetset", format!("sheet {i}: {e}")))?,
        };
        let mut sheet = Session::new();
        *sheet.doc_mut()? = d.clone();
        cache = Some((file, d));
        let mut q = json!({ "layout": sh.layout });
        if let Some(dir) = &output {
            let name = file_part(&format!("{} {}", sh.number, sh.title));
            q["path"] = json!(std::path::Path::new(dir).join(format!("{}.pdf", name.trim())).to_string_lossy());
        }
        let mut r = sheet.execute("plot", &q).map_err(|e| bad("sheetset", format!("sheet {i} ({}): {e}", sh.layout)))?;
        if let Some(o) = r.as_object_mut() {
            o.insert("index".into(), json!(i));
            o.insert("layout".into(), json!(sh.layout));
        }
        out.push(r);
    }
    let message = format!("Published {} sheet(s) of \"{}\".", out.len(), set.name);
    Ok(json!({ "sheets": out, "message": message }))
}

fn describe(path: &str, set: &SheetSet, message: String) -> Value {
    let sheets: Vec<Value> = set
        .sheets
        .iter()
        .enumerate()
        .map(|(i, sh)| json!({ "index": i, "number": sh.number, "title": sh.title, "file": sh.file, "layout": sh.layout }))
        .collect();
    json!({ "path": path, "name": set.name, "description": set.description, "sheets": sheets, "message": message })
}

fn clip(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(MAX_TEXT).collect()
}

/// A text made safe for a file name.
fn file_part(name: &str) -> String {
    name.chars().map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.') { c } else { '_' }).collect()
}

fn stem(path: &str) -> String {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    file.rsplit_once('.').map(|(s, _)| s).filter(|s| !s.is_empty()).unwrap_or(file).to_string()
}

/// A sheet's drawing path as stored, against the sheet set file's folder.
fn resolve(set_path: &str, file: &str) -> String {
    let f = std::path::Path::new(file);
    if f.is_absolute() {
        return file.to_string();
    }
    match std::path::Path::new(set_path).parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(f).to_string_lossy().into_owned(),
        _ => file.to_string(),
    }
}

/// A drawing inside the sheet set file's folder is stored by its relative path.
fn relative(set_path: &str, file: &str) -> String {
    let dir = std::path::Path::new(set_path).parent().filter(|d| !d.as_os_str().is_empty());
    match dir.and_then(|d| std::path::Path::new(file).strip_prefix(d).ok()) {
        Some(rel) if !rel.as_os_str().is_empty() => rel.to_string_lossy().into_owned(),
        _ => file.to_string(),
    }
}

/// Check a sheet set read from a file: its format, and its size.
fn checked(mut set: SheetSet) -> Result<SheetSet> {
    if set.format != FORMAT {
        return Err(bad("sheetset", "not a DraftCraft sheet set file"));
    }
    if set.sheets.len() > MAX_SHEETS {
        return Err(bad("sheetset", format!("a sheet set holds at most {MAX_SHEETS} sheets")));
    }
    set.name = clip(&set.name);
    set.description = clip(&set.description);
    for sh in &mut set.sheets {
        sh.number = clip(&sh.number);
        sh.title = clip(&sh.title);
        sh.file = clip(&sh.file);
        sh.layout = clip(&sh.layout);
    }
    Ok(set)
}

/// Parse a sheet set file's text.
pub fn parse(text: &str) -> Result<SheetSet> {
    let set: SheetSet = serde_json::from_str(text).map_err(|e| bad("sheetset", format!("not a sheet set file: {e}")))?;
    checked(set)
}

#[cfg(not(target_arch = "wasm32"))]
fn exists(path: &str) -> bool {
    std::path::Path::new(path).exists()
}

#[cfg(target_arch = "wasm32")]
fn exists(_path: &str) -> bool {
    false
}

#[cfg(not(target_arch = "wasm32"))]
fn load(path: &str) -> Result<SheetSet> {
    use std::io::Read;
    let f = std::fs::File::open(path).map_err(|e| bad("sheetset", format!("{path}: {e}")))?;
    let mut text = String::new();
    f.take(MAX_FILE_BYTES + 1).read_to_string(&mut text).map_err(|e| bad("sheetset", format!("{path}: {e}")))?;
    if text.len() as u64 > MAX_FILE_BYTES {
        return Err(bad("sheetset", format!("{path} is too large for a sheet set file")));
    }
    parse(&text)
}

#[cfg(target_arch = "wasm32")]
fn load(_path: &str) -> Result<SheetSet> {
    Err(bad("sheetset", "sheet sets are not available on the web"))
}

#[cfg(not(target_arch = "wasm32"))]
fn save(path: &str, set: &SheetSet) -> Result<()> {
    let text = serde_json::to_string_pretty(set).map_err(|e| bad("sheetset", e.to_string()))?;
    if let Some(dir) = std::path::Path::new(path).parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| bad("sheetset", format!("{path}: {e}")))?;
    }
    std::fs::write(path, text).map_err(|e| bad("sheetset", format!("{path}: {e}")))
}

#[cfg(target_arch = "wasm32")]
fn save(_path: &str, _set: &SheetSet) -> Result<()> {
    Err(bad("sheetset", "sheet sets are not available on the web"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("dc-sheetset-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_sheet_set_is_built_and_edited() {
        let d = dir();
        let path = d.join("Project.dcss").to_string_lossy().into_owned();
        let drawing = d.join("plans").join("A-101.dwg").to_string_lossy().into_owned();
        let mut s = Session::new();
        let r = s.execute("sheetset", &json!({"operation": "new", "path": path})).unwrap();
        assert_eq!(r["name"], "Project");
        assert!(s.execute("sheetset", &json!({"operation": "new", "path": path})).is_err(), "no silent overwrite");
        let r = s
            .execute(
                "sheetset",
                &json!({"operation": "add", "path": path, "file": drawing, "layout": "Plan", "number": "A-101", "title": "Ground floor"}),
            )
            .unwrap();
        assert_eq!(r["sheets"][0]["file"], format!("plans{}A-101.dwg", std::path::MAIN_SEPARATOR), "stored relative to the set");
        s.execute("sheetset", &json!({"operation": "add", "path": path, "file": drawing, "layout": "Roof"})).unwrap();
        s.execute("sheetset", &json!({"operation": "move", "path": path, "index": 1, "to": 0})).unwrap();
        let r = s.execute("sheetset", &json!({"operation": "update", "path": path, "index": 0, "title": "Roof plan"})).unwrap();
        assert_eq!(r["sheets"][0]["title"], "Roof plan");
        assert_eq!(r["sheets"][1]["number"], "A-101");
        let r = s.execute("sheetset", &json!({"operation": "remove", "path": path, "index": 0})).unwrap();
        assert_eq!(r["sheets"].as_array().unwrap().len(), 1);
        assert!(s.execute("sheetset", &json!({"operation": "remove", "path": path, "index": 9})).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn hostile_sheet_set_files_are_refused() {
        assert!(parse("[]").is_err());
        assert!(parse(r#"{"format":"other","sheets":[]}"#).is_err());
        let many: Vec<Value> = (0..MAX_SHEETS + 1).map(|_| json!({"file": "a.dwg", "layout": "L"})).collect();
        assert!(parse(&json!({"format": FORMAT, "sheets": many}).to_string()).is_err());
        let long = "x".repeat(10 * MAX_TEXT);
        let set = parse(&json!({"format": FORMAT, "name": long, "sheets": [{"file": "a\u{7}.dwg", "layout": "L"}]}).to_string()).unwrap();
        assert_eq!(set.name.chars().count(), MAX_TEXT);
        assert_eq!(set.sheets[0].file, "a.dwg");
    }
}
