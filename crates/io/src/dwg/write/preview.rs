//! The picture a DWG file carries for file browsers and AutoCAD's open dialog: model space as
//! AutoCAD draws its own, 256 pixels wide on the dark canvas, as a PNG (stored from AutoCAD
//! 2013) and as an 8-bit Windows bitmap without its file header (stored before).

use draftcraft_doc::{Drawing, Space};
use draftcraft_render::raster::{RasterOptions, View, render};

/// The width AutoCAD's previews have; the height follows the drawing, up to the same.
const WIDTH: u32 = 256;

pub struct Preview {
    pub png: Vec<u8>,
    pub bmp: Vec<u8>,
}

/// The preview of `d`; `None` when model space is empty or can't be drawn.
pub fn preview(d: &Drawing) -> Option<Preview> {
    let list = draftcraft_render::build(d, &Space::Model, &draftcraft_render::Options::default());
    let b = list.bounds;
    if b.is_empty() || !(b.width().is_finite() && b.height().is_finite()) {
        return None;
    }
    let ratio = if b.width() > 0.0 { b.height() / b.width() } else { 1.0 };
    let height = (f64::from(WIDTH) * ratio).round().clamp(16.0, f64::from(WIDTH)) as u32;
    let view = View::fit(&b, WIDTH, height, 0.05);
    // Without smoothing the picture has the drawing's own colours only, which a palette holds.
    let pixmap = render(&list, &view, &RasterOptions { antialias: false, ..RasterOptions::default() })?;
    let png = pixmap.encode_png().ok()?;
    let rgb: Vec<[u8; 3]> = pixmap.data().as_chunks::<4>().0.iter().map(|p| [p[0], p[1], p[2]]).collect();
    let bmp = bitmap(&rgb, pixmap.width(), pixmap.height())?;
    Some(Preview { png, bmp })
}

/// An 8-bit bitmap (its information header, the palette, the rows from the bottom up, each
/// padded to four bytes) of `rgb`, `width` × `height` pixels. The palette holds the picture's
/// colours; a picture of more than 256 is reduced to 3 bits of red and green and 2 of blue.
fn bitmap(rgb: &[[u8; 3]], width: u32, height: u32) -> Option<Vec<u8>> {
    let (w, h) = (usize::try_from(width).ok()?, usize::try_from(height).ok()?);
    if rgb.len() != w.checked_mul(h)? {
        return None;
    }
    let mut palette: Vec<[u8; 3]> = Vec::with_capacity(256);
    let mut index = std::collections::HashMap::with_capacity(256);
    let mut exact = true;
    for p in rgb {
        if !index.contains_key(p) {
            if palette.len() == 256 {
                exact = false;
                break;
            }
            index.insert(*p, palette.len() as u8);
            palette.push(*p);
        }
    }
    let reduced = |p: &[u8; 3]| (p[0] & 0xE0) | ((p[1] >> 3) & 0x1C) | (p[2] >> 6);
    if !exact {
        let scale = |v: u8, top: u16| (u16::from(v) * 255 / top) as u8;
        palette = (0..=255u8).map(|i| [scale(i >> 5, 7), scale((i >> 2) & 7, 7), scale(i & 3, 3)]).collect();
    }
    let row = w.div_ceil(4) * 4;
    let pixels = row.checked_mul(h)?;
    let mut out = Vec::with_capacity(40 + 1024 + pixels);
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&i32::try_from(width).ok()?.to_le_bytes());
    out.extend_from_slice(&i32::try_from(height).ok()?.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&8u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&u32::try_from(pixels).ok()?.to_le_bytes());
    out.extend_from_slice(&[0; 8]);
    out.extend_from_slice(&256u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    for i in 0..256 {
        let [r, g, b] = palette.get(i).copied().unwrap_or([0; 3]);
        out.extend_from_slice(&[b, g, r, 0]);
    }
    for y in (0..h).rev() {
        let line = rgb.get(y * w..(y + 1) * w)?;
        out.extend(line.iter().map(|p| if exact { index.get(p).copied().unwrap_or(0) } else { reduced(p) }));
        out.resize(out.len() + row - w, 0);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::bitmap;

    #[test]
    fn a_bitmap_holds_its_colours_bottom_row_first() {
        let red = [255, 0, 0];
        let blue = [0, 0, 255];
        // Three pixels wide, two high: the top row red, the bottom blue.
        let b = bitmap(&[red, red, red, blue, blue, blue], 3, 2).unwrap();
        assert_eq!(u32::from_le_bytes(b[4..8].try_into().unwrap()), 3);
        assert_eq!(u16::from_le_bytes(b[14..16].try_into().unwrap()), 8);
        assert_eq!(b.len(), 40 + 1024 + 2 * 4);
        let palette = &b[40..40 + 1024];
        assert_eq!(&palette[..8], &[0, 0, 255, 0, 255, 0, 0, 0]);
        // Rows from the bottom: blue (index 1) first, padded to four bytes.
        assert_eq!(&b[40 + 1024..], &[1, 1, 1, 0, 0, 0, 0, 0]);
        assert!(bitmap(&[red], 2, 2).is_none());
    }

    #[test]
    fn more_than_256_colours_fall_back_to_a_fixed_palette() {
        let rgb: Vec<[u8; 3]> = (0..300u32).map(|i| [(i % 256) as u8, (i / 256) as u8 * 100, 7]).collect();
        let b = bitmap(&rgb, 300, 1).unwrap();
        assert_eq!(b.len(), 40 + 1024 + 300);
        // White in the fixed palette is white.
        assert_eq!(&b[40 + 255 * 4..40 + 256 * 4], &[255, 255, 255, 0]);
    }
}
