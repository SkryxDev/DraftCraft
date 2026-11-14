//! DraftCraft file formats: DXF/DWG read, SVG and SVGZ import, SVG/PNG/PDF export (plotting).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

/// What a DXF file holds that DraftCraft doesn't model, kept and written back (#528).
mod dxf_raw;

pub mod dwg;
pub mod dwg_read;
/// XCLIP boundaries as SPATIAL_FILTER objects.
mod dxf_clip;
mod dxf_ext;
mod dxf_image;
mod dxf_mleader;
mod dxf_read;
mod dxf_table;
mod dxf_underlay;
mod dxf_write;
mod dxf_xref;
mod legacy_names;
pub mod pdf;
pub mod pdf_read;
pub mod plotter;
/// Raster image files (PNG, JPEG, BMP, TIFF) decoded to pixels.
pub mod raster;
/// Open → save → reopen comparison and the real-file corpus harness (gaps FF1, FF5).
pub mod roundtrip;
pub mod svg;
mod svg_read;

/// Multilines and their styles (MLINE, MLINESTYLE).
mod dxf_mline;

use draftcraft_doc::{Drawing, Space};
use draftcraft_geom::Bounds2;
use std::path::{Path, PathBuf};

pub use draftcraft_doc::Format;
pub use draftcraft_dxf::Version;
pub use dxf_read::entity_from_groups;
pub use dxf_write::entity_groups;
pub use pdf::{PdfOptions, pdf, plot};

/// How a drawing is saved as DXF or DWG.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SaveOptions {
    /// The file version; `None` saves in the drawing's own version ([`save_version`]).
    pub version: Option<Version>,
    /// Binary instead of ASCII DXF (DXF only; DWG is always binary).
    pub binary: bool,
}

/// The version a drawing is saved in when none is asked for: the version of the file it was
/// read from (`$ACADVER`, the nearest one DraftCraft writes), and 2018 for a new drawing.
pub fn save_version(d: &Drawing) -> Version {
    d.header.get("ACADVER").and_then(draftcraft_doc::HVal::as_str).and_then(Version::for_acadver).unwrap_or_default()
}

#[derive(Debug, thiserror::Error)]
pub enum IoError {
    #[error("{0}")]
    Format(String),
    #[error("unsupported file type `{0}`")]
    Unsupported(String),
}

pub type Result<T> = std::result::Result<T, IoError>;

fn ext(name: impl AsRef<Path>) -> String {
    name.as_ref().extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

/// Which reader opens DWG files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DwgReader {
    /// DraftCraft's direct reader ([`dwg::read`]): each object decoded field by field into the
    /// drawing. The default. What it leaves out is listed in the drawing's read notes.
    Direct,
    /// The first native mapping ([`dwg_read`]), over BlackCAD's records: kept to compare.
    Native,
}

impl DwgReader {
    /// `DRAFTCRAFT_DWG_READER=native` to compare; anything else, or nothing, is the direct reader.
    /// (A browser has no environment: it always reads directly.)
    pub fn from_env() -> DwgReader {
        match std::env::var("DRAFTCRAFT_DWG_READER").unwrap_or_default().to_ascii_lowercase().as_str() {
            "native" => DwgReader::Native,
            _ => DwgReader::Direct,
        }
    }
}

/// Read a DWG file with the reader named.
pub fn read_dwg_with(bytes: &[u8], reader: DwgReader) -> Result<Drawing> {
    match reader {
        DwgReader::Direct => {
            let (mut d, unread) = dwg::read(bytes)?;
            d.read_notes.extend(unread.note());
            Ok(d)
        }
        DwgReader::Native => {
            let limit = draftcraft_dwg::Limits::DEFAULT.max_dwg_bytes;
            if bytes.len() > limit {
                return Err(IoError::Format(format!("DWG: the file is larger than the {} MB DraftCraft opens", limit >> 20)));
            }
            let (mut d, cov) = dwg_read::read(bytes)?;
            if !cov.is_whole() {
                d.read_notes.push(format!("DWG: left out of the drawing: {}", cov.summary()));
            }
            Ok(d)
        }
    }
}

/// Read a drawing; the format comes from the content (DXF ASCII/binary, DWG) or the name
/// (SVG and gzip-compressed SVGZ).
pub fn read(bytes: &[u8], name: impl AsRef<Path>) -> Result<Drawing> {
    if draftcraft_dwg::is_dwg(bytes) {
        return read_dwg_with(bytes, DwgReader::from_env());
    }
    let name = name.as_ref();
    match (Format::from_path(name), ext(name).as_str()) {
        (Some(Format::Svg | Format::Svgz), _) => svg_read::read(bytes),
        (Some(Format::Dxf), _) => dxf_read::read(bytes),
        (_, e) if bytes.len() > 4 => dxf_read::read(bytes).map_err(|_| IoError::Unsupported(e.to_string())),
        (_, e) => Err(IoError::Unsupported(e.to_string())),
    }
}

/// Write a drawing in the format chosen by the name's extension. Images (PNG, SVG, PDF) show
/// model space fitted to its extents. DXF and DWG are written in the drawing's version
/// ([`save_version`]), DXF as ASCII.
pub fn write(d: &Drawing, name: impl AsRef<Path>) -> Result<Vec<u8>> {
    write_framed(d, name, None)
}

/// Like [`write`], with the DXF/DWG version and binary DXF chosen by `opts`. Binary applies to
/// DXF only; asking for it with another format is an error.
pub fn write_with(d: &Drawing, name: impl AsRef<Path>, opts: &SaveOptions) -> Result<Vec<u8>> {
    let name = name.as_ref();
    let version = opts.version.unwrap_or_else(|| save_version(d));
    match Format::from_path(name) {
        Some(Format::Dxf) => {
            let tags = dxf_write::tags(d, version);
            // Before 2007 the text is in the code page the header declares.
            let code_page = dxf_write::code_page(d, version);
            Ok(match (opts.binary, version.utf8()) {
                (true, true) => draftcraft_dxf::write_binary(&tags, true),
                (true, false) => draftcraft_dxf::write_binary_in(&tags, &code_page),
                (false, true) => draftcraft_dxf::write_ascii_as(&tags, true).into_bytes(),
                (false, false) => draftcraft_dxf::write_ascii_in(&tags, &code_page),
            })
        }
        // Without a version asked for, at least 2004: R2000 DWG has no true colours,
        // transparency, table styles or gradients, so a drawing from an R2000 DXF doesn't lose
        // them by default.
        Some(Format::Dwg) if !opts.binary => write_dwg_own(d, opts.version.unwrap_or_else(|| version.max(Version::R2004))).map(|(dwg, _)| dwg),
        _ if opts.binary => Err(IoError::Format(format!("binary output is for DXF files, not `{}`", ext(name)))),
        _ => write_framed(d, name, None),
    }
}

/// [`write_with`], also returning what the file leaves out of the drawing: for a DWG, how many
/// records of each DXF type the DWG writer can't write (other programs' custom entities and
/// objects); for other formats, nothing.
pub fn write_noted(d: &Drawing, name: impl AsRef<Path>, opts: &SaveOptions) -> Result<(Vec<u8>, dwg::write::Unwritten)> {
    let name = name.as_ref();
    if matches!(Format::from_path(name), Some(Format::Dwg)) && !opts.binary {
        return write_dwg_own(d, opts.version.unwrap_or_else(|| save_version(d).max(Version::R2004)));
    }
    write_with(d, name, opts).map(|b| (b, dwg::write::Unwritten::new()))
}

/// The drawing as a DWG file of `version` by DraftCraft's own writer ([`dwg::write`]), with the
/// records the file leaves out, by type.
pub fn write_dwg_own(d: &Drawing, version: Version) -> Result<(Vec<u8>, dwg::write::Unwritten)> {
    let ver = dwg::write::version_of(version.code()).ok_or_else(|| IoError::Format(format!("DWG: no writer for {}", version.code())))?;
    let d = &*with_handles(d);
    let mut tags = dxf_write::dwg_records(d, version);
    dwg::write::add_variable_dictionary(&mut tags, d);
    let preview = dwg::write::preview(d);
    let info = dwg::write::FileInfo {
        app_name: "DraftCraft",
        app_version: env!("CARGO_PKG_VERSION"),
        preview_png: preview.as_ref().map(|p| p.png.as_slice()),
        preview_bmp: preview.as_ref().map(|p| p.bmp.as_slice()),
    };
    let (bytes, unwritten) = dwg::write::write(&tags, ver, &info)?;
    check_dwg(&bytes, d, unwritten.values().sum())?;
    Ok((bytes, unwritten))
}

/// `d` with a handle of its own for every entity that has none (handle 0, as a program that
/// builds a drawing directly may leave a block's entities): a DWG object can't be written without
/// one. The drawing itself when every entity has a handle.
fn with_handles(d: &Drawing) -> std::borrow::Cow<'_, Drawing> {
    let unnumbered = |s: &draftcraft_doc::EntityStore| s.iter().any(|e| e.handle.0 == 0);
    if !unnumbered(&d.model) && !d.layouts.iter().any(|l| unnumbered(&l.entities)) && !d.blocks.values().any(|b| unnumbered(&b.entities)) {
        return std::borrow::Cow::Borrowed(d);
    }
    let mut d = d.clone();
    fn renumbered(d: &mut Drawing, s: &draftcraft_doc::EntityStore) -> draftcraft_doc::EntityStore {
        let mut out = draftcraft_doc::EntityStore::new();
        for e in s.iter() {
            let mut e = draftcraft_doc::Entity::clone(e);
            if e.handle.0 == 0 {
                e.handle = d.new_handle();
            }
            out.push(e);
        }
        out
    }
    if unnumbered(&d.model) {
        let model = d.model.clone();
        d.model = renumbered(&mut d, &model);
    }
    for i in 0..d.layouts.len() {
        let Some(store) = d.layouts.get(i).filter(|l| unnumbered(&l.entities)).map(|l| l.entities.clone()) else { continue };
        let store = renumbered(&mut d, &store);
        if let Some(l) = d.layouts.get_mut(i) {
            l.entities = store;
        }
    }
    let names: Vec<String> = d.blocks.iter().filter(|(_, b)| unnumbered(&b.entities)).map(|(n, _)| n.clone()).collect();
    for name in names {
        let Some(store) = d.blocks.get(&name).map(|b| b.entities.clone()) else { continue };
        let store = renumbered(&mut d, &store);
        if let Some(b) = d.blocks.get_mut(&name) {
            std::sync::Arc::make_mut(b).entities = store;
        }
    }
    std::borrow::Cow::Owned(d)
}

/// A DWG file just written, read back before it is handed over: a file this program can't read
/// again, or that reads as an empty drawing when it was given entities (more than the `left_out`
/// the version can't hold), is never saved.
fn check_dwg(bytes: &[u8], d: &Drawing, left_out: usize) -> Result<()> {
    let back = read_dwg_with(bytes, DwgReader::Direct)
        .map_err(|e| IoError::Format(format!("DWG: the file written doesn't read back ({e}), so it was not saved")))?;
    if d.entity_count() > left_out && back.entity_count() == 0 {
        return Err(IoError::Format("DWG: the file written reads back without its entities, so it was not saved".into()));
    }
    Ok(())
}

/// Like [`write`], but image formats (PNG, SVG, PDF) show `window` (a model-space rectangle in
/// drawing units, centred and scaled to fit the image without distortion) instead of the
/// extents. Formats that are not images (DXF, DWG) ignore the window. The window is checked with
/// [`check_window`].
pub fn write_framed(d: &Drawing, name: impl AsRef<Path>, window: Option<Bounds2>) -> Result<Vec<u8>> {
    let name = name.as_ref();
    // A drawing template (.dwt) is DWG content.
    let format = Format::from_path(name);
    if format == Some(Format::Dwt) {
        let mut dwg = name.as_os_str().to_owned();
        dwg.push(".dwg");
        return write_framed(d, PathBuf::from(dwg), None);
    }
    if let Some(w) = &window
        && is_image(name)
    {
        check_window(w)?;
    }
    match format {
        Some(Format::Dxf | Format::Dwg) => write_with(d, name, &SaveOptions::default()),
        Some(Format::Svg) => Ok(svg::export_window(d, &Space::Model, window).into_bytes()),
        Some(Format::Png) => png_framed(d, &Space::Model, 2400, 1600, window),
        Some(Format::Pdf) => pdf::pdf(d, &Space::Model, &PdfOptions { compress: true, window, ..PdfOptions::default() }),
        _ => Err(IoError::Unsupported(ext(name))),
    }
}

/// Whether `name` is written as an image (PNG, SVG, PDF), i.e. whether framing applies to it.
pub fn is_image(name: impl AsRef<Path>) -> bool {
    Format::from_path(name).is_some_and(Format::is_image)
}

/// Largest coordinate magnitude accepted in an export window.
pub const MAX_WINDOW_COORD: f64 = 1.0e12;

/// Validate an export window: finite corners within ±[`MAX_WINDOW_COORD`], and a width and height
/// that are positive and not vanishingly small next to the coordinates (at least 1e-9 of the
/// largest coordinate magnitude, and at least 1e-9 drawing units).
pub fn check_window(w: &Bounds2) -> Result<()> {
    let coords = [w.min.x, w.min.y, w.max.x, w.max.y];
    if coords.iter().any(|c| !c.is_finite()) {
        return Err(IoError::Format("export window: coordinates must be finite numbers".into()));
    }
    let mag = coords.iter().fold(0.0_f64, |m, c| m.max(c.abs()));
    if mag > MAX_WINDOW_COORD {
        return Err(IoError::Format(format!("export window: coordinates must lie within ±{MAX_WINDOW_COORD:e}")));
    }
    let min_side = mag.max(1.0) * 1e-9;
    let (width, height) = (w.max.x - w.min.x, w.max.y - w.min.y);
    if !(width >= min_side && height >= min_side) {
        return Err(IoError::Format(format!("export window: width and height must be positive (got {width} x {height})")));
    }
    Ok(())
}

/// Render a space to PNG, fitted to its extents.
pub fn png(d: &Drawing, space: &Space, width: u32, height: u32) -> Result<Vec<u8>> {
    png_framed(d, space, width, height, None)
}

/// Render a space to PNG showing `window` (centred, fitted without margin), or fitted to its
/// extents (5% margin) when `window` is `None`. Objects are drawn with their transparency, as on
/// screen.
pub fn png_framed(d: &Drawing, space: &Space, width: u32, height: u32, window: Option<Bounds2>) -> Result<Vec<u8>> {
    let list = draftcraft_render::build(d, space, &draftcraft_render::Options { transparency: true, ..draftcraft_render::Options::default() });
    let view = png_view(&list.bounds, width, height, window);
    draftcraft_render::raster::render_png(&list, &view, &draftcraft_render::raster::RasterOptions::default())
        .ok_or_else(|| IoError::Format("render failed".into()))
}

/// The raster view a PNG export uses.
pub fn png_view(extents: &Bounds2, width: u32, height: u32, window: Option<Bounds2>) -> draftcraft_render::raster::View {
    match window {
        Some(w) => draftcraft_render::raster::View::fit(&w, width, height, 0.0),
        None => draftcraft_render::raster::View::fit(extents, width, height, 0.05),
    }
}

pub fn read_dxf(bytes: &[u8]) -> Result<Drawing> {
    dxf_read::read(bytes)
}
/// The drawing as R2000 ASCII DXF, whatever its own version ([`write_with`] chooses one).
pub fn write_dxf(d: &Drawing) -> String {
    dxf_write::write(d)
}

/// Regions, written as closed polylines.
mod dxf_region;

#[cfg(test)]
mod tests;
