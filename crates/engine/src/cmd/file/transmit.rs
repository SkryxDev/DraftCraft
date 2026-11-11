//! ETRANSMIT: package the drawing with the files it needs (raster images, PDF underlays,
//! external references, and font files kept beside the drawing) into a folder or a .zip file,
//! with a transmittal report. Every file goes into one folder and the packaged drawing names its
//! references by file name, so the package opens anywhere.
//!
//! The referenced paths come from the drawing, which is input: they are resolved against the
//! drawing's folder, counted and size-capped, and a file that can't be read is reported missing
//! instead of failing the package.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use draftcraft_doc::{Drawing, EntityKind};
use serde_json::{Value, json};

use super::io;
use crate::cmd::{CommandSpec, bad, str_param};
use crate::{Result, Session};

/// Most referenced files packaged.
const MAX_FILES: usize = 10_000;
/// Largest referenced file packaged, and largest package.
const MAX_FILE_BYTES: u64 = 1 << 30;
const MAX_TOTAL_BYTES: u64 = 4 << 30;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("etransmit", "eTransmit...", run_etransmit)
            .menu(&["File", "eTransmit..."])
            .params(
                "{path: folder, or NAME.zip, to write the package to, report?: bool (default true: a transmittal report)} → {path, \
                 drawing, files: [{name, source}], missing: [paths], bytes}",
            )
            .noundo(),
    ]
}

/// A file the package carries: its name in the package and the bytes.
struct Packed {
    name: String,
    source: String,
    bytes: Vec<u8>,
}

/// A free name in the package for `wanted`: `plan (2).png` when `plan.png` is taken.
fn unique(taken: &mut BTreeSet<String>, wanted: &str) -> String {
    let (stem, ext) = match wanted.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (wanted.to_string(), String::new()),
    };
    let mut name = wanted.to_string();
    let mut i = 2;
    while taken.contains(&name.to_lowercase()) && i < 100_000 {
        name = format!("{stem} ({i}){ext}");
        i += 1;
    }
    taken.insert(name.to_lowercase());
    name
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Read a referenced file: as given, else against the drawing's folder, else by its name in the
/// drawing's folder.
fn read_reference(dir: Option<&std::path::Path>, path: &str) -> Option<(String, Vec<u8>)> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut candidates = vec![std::path::PathBuf::from(path)];
        if let Some(dir) = dir {
            candidates.push(dir.join(path));
            candidates.push(dir.join(file_name(path)));
        }
        for c in candidates {
            if let Ok(meta) = std::fs::metadata(&c)
                && meta.is_file()
                && meta.len() <= MAX_FILE_BYTES
                && let Ok(bytes) = std::fs::read(&c)
            {
                return Some((c.to_string_lossy().into_owned(), bytes));
            }
        }
        None
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (dir, path);
        None
    }
}

/// The drawing's references: images and underlays by path, xrefs by path, and fonts by name.
fn references(d: &Drawing) -> (BTreeSet<String>, BTreeSet<String>, BTreeSet<String>) {
    let mut images = BTreeSet::new();
    let stores = std::iter::once(&d.model)
        .chain(d.layouts.iter().map(|l| &l.entities))
        .chain(d.blocks.values().filter(|b| b.xref_path.is_none()).map(|b| &b.entities));
    for store in stores {
        for e in store.iter() {
            if let EntityKind::Image(i) = &e.kind
                && !i.path.trim().is_empty()
            {
                images.insert(i.path.clone());
            }
            if images.len() > MAX_FILES {
                break;
            }
        }
    }
    let xrefs = d.blocks.values().filter_map(|b| b.xref_path.clone()).filter(|p| !p.trim().is_empty()).take(MAX_FILES).collect();
    let fonts = d
        .text_styles
        .iter()
        .flat_map(|t| [t.font.clone(), t.big_font.clone()])
        .filter(|f| {
            let l = f.to_ascii_lowercase();
            l.ends_with(".shx") || l.ends_with(".ttf") || l.ends_with(".otf")
        })
        .take(MAX_FILES)
        .collect();
    (images, xrefs, fonts)
}

fn run_etransmit(s: &mut Session, p: &Value) -> Result<Value> {
    let out = str_param(p, "path")
        .filter(|x| !x.trim().is_empty())
        .ok_or_else(|| bad("etransmit", "`path` is the folder or .zip file to write"))?
        .trim()
        .to_string();
    let hooks = io().ok_or_else(|| bad("etransmit", "file formats are not available in this build"))?;
    let state = s.state()?;
    let src_path = state.path.clone();
    let dir = src_path.as_deref().and_then(|p| std::path::Path::new(p).parent()).map(std::path::Path::to_path_buf);
    let mut d: Drawing = (**state.drawing()).clone();
    let drawing_name = match src_path.as_deref().map(file_name) {
        Some(n) if n.to_ascii_lowercase().ends_with(".dwg") || n.to_ascii_lowercase().ends_with(".dxf") => n.to_string(),
        Some(n) => format!("{}.dxf", n.rsplit_once('.').map(|(s, _)| s).unwrap_or(n)),
        None => "Drawing.dxf".into(),
    };
    let (images, xrefs, fonts) = references(&d);
    let mut taken: BTreeSet<String> = [drawing_name.to_lowercase()].into();
    let mut packed: Vec<Packed> = Vec::new();
    let mut missing = Vec::new();
    let mut renames: BTreeMap<String, String> = BTreeMap::new();
    let mut total: u64 = 0;
    for path in images.iter().chain(&xrefs).chain(&fonts) {
        if renames.contains_key(path) {
            continue;
        }
        match read_reference(dir.as_deref(), path) {
            Some((source, bytes)) => {
                total = total.saturating_add(bytes.len() as u64);
                if total > MAX_TOTAL_BYTES {
                    return Err(bad("etransmit", "the package would be larger than 4 GB"));
                }
                let name = unique(&mut taken, file_name(path));
                renames.insert(path.clone(), name.clone());
                packed.push(Packed { name, source, bytes });
            }
            None => missing.push(path.clone()),
        }
    }
    // The packaged drawing names its images and xrefs by their file names in the package.
    let rename_store = |store: &mut draftcraft_doc::EntityStore| {
        let hs: Vec<_> =
            store.iter().filter(|e| matches!(&e.kind, EntityKind::Image(i) if renames.contains_key(&i.path))).map(|e| e.handle).collect();
        for h in hs {
            store.modify(h, |e| {
                if let EntityKind::Image(i) = &mut e.kind
                    && let Some(n) = renames.get(&i.path)
                {
                    i.path = n.clone();
                }
            });
        }
    };
    rename_store(&mut d.model);
    for l in &mut d.layouts {
        rename_store(&mut l.entities);
    }
    for b in d.blocks.values_mut() {
        if let Some(n) = b.xref_path.as_ref().and_then(|x| renames.get(x)).cloned() {
            Arc::make_mut(b).xref_path = Some(n);
        } else if b.xref_path.is_none() && b.entities.iter().any(|e| matches!(&e.kind, EntityKind::Image(i) if renames.contains_key(&i.path))) {
            rename_store(&mut Arc::make_mut(b).entities);
        }
    }
    let drawing = (hooks.write)(&d, &drawing_name).map_err(|e| bad("etransmit", e))?;
    let mut files = vec![Packed { name: drawing_name.clone(), source: src_path.clone().unwrap_or_default(), bytes: drawing }];
    files.append(&mut packed);
    if p.get("report").and_then(Value::as_bool).unwrap_or(true) {
        let stem = drawing_name.rsplit_once('.').map(|(s, _)| s).unwrap_or(&drawing_name).to_string();
        let mut text = format!("Transmittal report for {drawing_name}\nMade by DraftCraft eTransmit.\n\nFiles in this package:\n");
        for f in &files {
            text.push_str(&format!("  {}{}\n", f.name, if f.source.is_empty() { String::new() } else { format!("  (from {})", f.source) }));
        }
        if !missing.is_empty() {
            text.push_str("\nReferenced files that were not found (not included):\n");
            for m in &missing {
                text.push_str(&format!("  {m}\n"));
            }
        }
        let name = unique(&mut taken, &format!("{stem} - Transmittal.txt"));
        files.push(Packed { name, source: String::new(), bytes: text.into_bytes() });
    }
    let bytes: u64 = files.iter().map(|f| f.bytes.len() as u64).sum();
    write_package(&out, &files)?;
    let list: Vec<Value> = files.iter().map(|f| json!({ "name": f.name, "source": f.source })).collect();
    let message = if missing.is_empty() {
        format!("Packaged {} file(s) to {out}.", files.len())
    } else {
        format!("Packaged {} file(s) to {out}; {} referenced file(s) not found: {}.", files.len(), missing.len(), missing.join(", "))
    };
    Ok(json!({ "path": out, "drawing": drawing_name, "files": list, "missing": missing, "bytes": bytes, "message": message }))
}

#[cfg(not(target_arch = "wasm32"))]
fn write_package(out: &str, files: &[Packed]) -> Result<()> {
    use std::io::Write;
    let path = std::path::Path::new(out);
    if out.to_ascii_lowercase().ends_with(".zip") {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).large_file(true);
            for f in files {
                zip.start_file(f.name.as_str(), opts).map_err(|e| bad("etransmit", e.to_string()))?;
                zip.write_all(&f.bytes).map_err(|e| bad("etransmit", e.to_string()))?;
            }
            zip.finish().map_err(|e| bad("etransmit", e.to_string()))?;
        }
        crate::atomic_write::write_atomic_making_folders(path, buf.get_ref()).map_err(|e| bad("etransmit", format!("{out}: {e}")))
    } else {
        std::fs::create_dir_all(path).map_err(|e| bad("etransmit", format!("{out}: {e}")))?;
        for f in files {
            crate::atomic_write::write_atomic(&path.join(&f.name), &f.bytes).map_err(|e| bad("etransmit", format!("{}: {e}", f.name)))?;
        }
        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
fn write_package(_out: &str, _files: &[Packed]) -> Result<()> {
    Err(bad("etransmit", "eTransmit is not available on the web"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_names_never_collide() {
        let mut taken = BTreeSet::new();
        assert_eq!(unique(&mut taken, "plan.png"), "plan.png");
        assert_eq!(unique(&mut taken, "PLAN.png"), "PLAN (2).png");
        assert_eq!(unique(&mut taken, "plan.png"), "plan (3).png");
        assert_eq!(unique(&mut taken, "README"), "README");
        assert_eq!(unique(&mut taken, "readme"), "readme (2)");
    }
}
