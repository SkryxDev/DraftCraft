//! Decoded raster image pixels (IMAGE entities). The drawing keeps them for the session only:
//! they are loaded from the image files after a drawing opens and are never written to a file.

use std::collections::BTreeMap;
use std::sync::Arc;

/// The pixels of a raster image: 8-bit RGBA, not premultiplied, rows from the top.
#[derive(Clone, PartialEq)]
pub struct Raster {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    opaque: bool,
}

impl Raster {
    /// `None` unless both sides are at least one pixel and `rgba` holds exactly four bytes per pixel.
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Option<Raster> {
        let pixels = usize::try_from(width).ok()?.checked_mul(usize::try_from(height).ok()?)?;
        if pixels == 0 || rgba.len() != pixels.checked_mul(4)? {
            return None;
        }
        let opaque = rgba.as_chunks::<4>().0.iter().all(|[_, _, _, alpha]| *alpha == u8::MAX);
        Some(Raster { width, height, rgba, opaque })
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    /// Four bytes per pixel (red, green, blue, alpha), `width * height` pixels.
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
    /// No pixel is transparent.
    pub fn opaque(&self) -> bool {
        self.opaque
    }
    /// The pixel in column `x` of row `y` (from the top): red, green, blue, alpha.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let (x, y, width) = (usize::try_from(x).ok()?, usize::try_from(y).ok()?, usize::try_from(self.width).ok()?);
        let pixels = self.rgba.as_chunks::<4>().0;
        pixels.get(y.checked_mul(width)?.checked_add(x)?).copied()
    }
}

impl std::fmt::Debug for Raster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Raster({}x{})", self.width, self.height)
    }
}

/// The loaded pixels of a drawing's raster images, by image file path ([`crate::Image::path`]).
/// Session data: two drawings are equal whatever images they have loaded, and copying a drawing
/// (undo) shares the pixels.
#[derive(Clone, Default)]
pub struct Rasters(Arc<BTreeMap<String, Arc<Raster>>>);

impl Rasters {
    pub fn get(&self, path: &str) -> Option<&Arc<Raster>> {
        self.0.get(path)
    }
    pub fn insert(&mut self, path: &str, raster: Arc<Raster>) {
        Arc::make_mut(&mut self.0).insert(path.to_string(), raster);
    }
    pub fn remove(&mut self, path: &str) {
        if self.0.contains_key(path) {
            Arc::make_mut(&mut self.0).remove(path);
        }
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl PartialEq for Rasters {
    fn eq(&self, _: &Rasters) -> bool {
        true
    }
}

impl std::fmt::Debug for Rasters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Rasters({})", self.0.len())
    }
}

/// An object an importer (PDFIMPORT) read from another file format, before it is placed: its
/// geometry in the source's own units, and what it looked like there.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportedShape {
    pub kind: crate::EntityKind,
    /// Its colour in the source; `None` when it had none of its own (a pattern, a shading).
    pub color: Option<draftcraft_color::Rgb>,
    /// Its stroke width in the source's units (0 for fills and text).
    pub width: f64,
    pub class: ShapeClass,
}

/// What an [`ImportedShape`] was in the source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeClass {
    /// A stroked line or curve.
    Vector,
    /// A filled area.
    Fill,
    /// Text.
    Text,
}
