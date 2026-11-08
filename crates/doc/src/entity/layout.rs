use draftcraft_geom::Mat3;

use super::common::Handle;
use super::{Color, Deserialize, Serialize, Vec2, Vec3, standard, yes};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Viewport {
    pub center: Vec3,
    pub width: f64,
    pub height: f64,
    pub view_center: Vec2,
    pub view_height: f64,
    #[serde(default)]
    pub id: u32,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub frozen_layers: Vec<String>,
    /// Per-viewport layer colour overrides (VP Color in the Layer Properties Manager).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layer_colors: Vec<(String, Color)>,
    /// The paper-space entity whose outline clips the viewport (a closed polyline, circle,
    /// ellipse or spline; DXF group 340). `None`: the rectangle `center`/`width`/`height`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip: Option<Handle>,
    /// Display on (MVIEW ON/OFF). An off viewport shows its border but no model space.
    #[serde(default = "yes")]
    pub on: bool,
    /// The view twist (DVIEW TWist inside the viewport; DXF group 51): radians counterclockwise,
    /// the model view seen through the viewport is turned on the sheet by it. 0 is untwisted.
    #[serde(default, skip_serializing_if = "is_zero_twist")]
    pub twist: f64,
}
fn is_zero_twist(v: &f64) -> bool {
    *v == 0.0
}
impl Viewport {
    /// The view twist, usable: 0 when it isn't finite.
    pub fn view_twist(&self) -> f64 {
        if self.twist.is_finite() { self.twist } else { 0.0 }
    }
    /// Paper units per model unit.
    pub fn scale(&self) -> f64 {
        self.height / self.view_height
    }
    /// Where model point `m` is on the sheet: `center + scale · R(twist)(m − view_center)`.
    pub fn model_to_paper(&self, m: Vec2) -> Vec2 {
        self.center.xy() + (m - self.view_center).rotate(self.view_twist()) * self.scale()
    }
    /// The model point at sheet point `p` (the inverse of [`Viewport::model_to_paper`]).
    pub fn paper_to_model(&self, p: Vec2) -> Vec2 {
        self.view_center + ((p - self.center.xy()) / self.scale()).rotate(-self.view_twist())
    }
    /// [`Viewport::model_to_paper`] as a matrix.
    pub fn model_to_paper_matrix(&self) -> Mat3 {
        let s = self.scale();
        Mat3::translate(self.center.xy())
            .then_before(Mat3::scale(s, s))
            .then_before(Mat3::rotate(self.view_twist()))
            .then_before(Mat3::translate(-self.view_center))
    }
}

impl Default for Viewport {
    fn default() -> Self {
        Viewport {
            center: Vec3::ZERO,
            width: 1.0,
            height: 1.0,
            view_center: Vec2::ZERO,
            view_height: 1.0,
            id: 0,
            locked: false,
            frozen_layers: Vec::new(),
            layer_colors: Vec::new(),
            clip: None,
            on: true,
            twist: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Image {
    pub insert: Vec3,
    /// One pixel's width (U) and height (V) vectors in drawing units.
    pub u: Vec3,
    pub v: Vec3,
    /// Image size in pixels.
    pub size: Vec2,
    /// The image file (its IMAGEDEF object's path).
    pub path: String,
    /// The image definition's name (its key in the drawing's image dictionary).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Clip boundary in pixel coordinates (y down, counted from pixel centres: the centre of the
    /// top-left pixel is the origin, so the whole image is -0.5 to size - 0.5): two opposite
    /// corners of a rectangle, or a polygon's vertices. Empty: the whole image.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clip: Vec<Vec2>,
    /// The clip boundary is applied.
    #[serde(default)]
    pub clipping: bool,
    /// Display flags: 1 show the image, 2 show it when not aligned with the screen, 4 use the
    /// clip boundary, 8 transparency on.
    #[serde(default = "image_display")]
    pub display: u16,
    /// Brightness, contrast and fade, 0..=100.
    #[serde(default = "image_fifty")]
    pub brightness: u8,
    #[serde(default = "image_fifty")]
    pub contrast: u8,
    #[serde(default)]
    pub fade: u8,
    /// The definition's default size of one pixel and its resolution unit (0 none, 2 cm, 5 inch).
    #[serde(default = "image_pixel")]
    pub pixel_size: Vec2,
    #[serde(default)]
    pub resolution_units: u8,
    /// Set when this is an underlay (a page of a PDF file, PDFUNDERLAY) instead of a raster image.
    /// An underlay's "pixels" are the page's PDF points (1/72 inch): `size` is the page in points
    /// and `u`/`v` one point in drawing units, so the page draws at any resolution it is rendered at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underlay: Option<Underlay>,
}

/// The file type of an underlay.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UnderlayKind {
    #[default]
    Pdf,
}

impl UnderlayKind {
    /// The DXF type of the reference entity and of its definition object.
    pub fn dxf_names(self) -> (&'static str, &'static str) {
        match self {
            UnderlayKind::Pdf => ("PDFUNDERLAY", "PDFDEFINITION"),
        }
    }
}

/// What makes an [`Image`] an underlay: the file type and the page shown.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Underlay {
    pub kind: UnderlayKind,
    /// The page: its number, counted from 1, as text (the definition's group 2).
    pub page: String,
    /// Shown in one colour (the reference's flag 4).
    #[serde(default)]
    pub monochrome: bool,
    /// Colours adjusted for the background (the reference's flag 8).
    #[serde(default)]
    pub adjust_for_background: bool,
    /// The clip boundary as the file has it (inches from the insertion point, y up, before scale
    /// and rotation), written back as read. [`Image::clip`] shows it once the page size is known.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clip: Vec<Vec2>,
}

impl Underlay {
    /// The contrast and fade (percent) a new underlay starts with.
    pub const DEFAULT_CONTRAST: u8 = 20;
    pub const DEFAULT_FADE: u8 = 25;

    /// The page index counted from 0, when the page names a number.
    pub fn page_index(&self) -> Option<usize> {
        self.page.trim().parse::<usize>().ok().and_then(|n| n.checked_sub(1))
    }
}

/// PDF points per inch: an underlay's page is measured in points, its reference scale in drawing
/// units per inch.
pub const POINTS_PER_INCH: f64 = 72.0;

impl Image {
    /// An underlay's clip boundary ([`Underlay::clip`]) as image pixel coordinates (points from
    /// the top-left pixel centre, y down), once the page size is known; empty otherwise.
    pub fn underlay_display_clip(&self) -> Vec<Vec2> {
        let Some(u) = &self.underlay else { return Vec::new() };
        if u.clip.len() < 2 || self.size.y <= 0.0 {
            return Vec::new();
        }
        u.clip.iter().map(|q| Vec2::new(q.x * POINTS_PER_INCH - 0.5, self.size.y - q.y * POINTS_PER_INCH - 0.5)).collect()
    }

    /// The key of this image's pixels in [`crate::Drawing::rasters`]: the file, and for an
    /// underlay the page too.
    pub fn raster_key(&self) -> String {
        match &self.underlay {
            Some(u) => format!("{}#page={}", self.path, u.page.trim()),
            None => self.path.clone(),
        }
    }
}

fn image_display() -> u16 {
    7
}

fn image_fifty() -> u8 {
    50
}

fn image_pixel() -> Vec2 {
    Vec2::new(1.0, 1.0)
}

impl Default for Image {
    fn default() -> Self {
        Image {
            insert: Vec3::ZERO,
            u: Vec3::new(1.0, 0.0, 0.0),
            v: Vec3::new(0.0, 1.0, 0.0),
            size: Vec2::new(1.0, 1.0),
            path: String::new(),
            name: String::new(),
            clip: Vec::new(),
            clipping: false,
            display: image_display(),
            brightness: image_fifty(),
            contrast: image_fifty(),
            fade: 0,
            pixel_size: image_pixel(),
            resolution_units: 0,
            underlay: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableCell {
    pub text: String,
    #[serde(default)]
    pub merged: Option<(u32, u32)>,
    /// The cell's own cell style; empty: its row's.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub style: String,
    /// A formula (`=Sum(A1:B3)`); `text` holds its value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formula: Option<String>,
    /// The cell's own text height (DXF 140 in the cell record), before the table's scale;
    /// `None`: its cell style's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_height: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    pub insert: Vec3,
    pub col_widths: Vec<f64>,
    pub row_heights: Vec<f64>,
    pub cells: Vec<Vec<TableCell>>,
    #[serde(default = "standard")]
    pub style: String,
    #[serde(default = "default_text_h")]
    pub text_height: f64,
    /// First row is a title (merged across, larger text).
    #[serde(default)]
    pub title: bool,
    /// The row after the title holds column headers.
    #[serde(default)]
    pub header: bool,
    /// The cell style of each row; missing or empty: by position (Title, Header, Data).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub row_styles: Vec<String>,
    /// Table breaking (TABLEBREAK): the table drawn in several parts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub breaks: Option<crate::TableBreak>,
    /// The CSV file the cells come from (DATALINK).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_link: Option<crate::DataLink>,
}

fn default_text_h() -> f64 {
    0.18
}
