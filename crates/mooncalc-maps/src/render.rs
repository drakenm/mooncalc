//! Raster and SVG rendering of visibility maps.
//!
//! Everything here is pure and in memory: [`render_raster`] returns an
//! [`RgbaImage`], [`encode_png`] turns it into PNG bytes and
//! [`render_svg`] returns a self-contained SVG document as a `String`.
//! Callers write files. No threads and no file system access are used, so
//! the module also builds for `wasm32-unknown-unknown`.
//!
//! # Layers
//!
//! From bottom to top: background (outside the projection), sea and land
//! (from the embedded Natural Earth [`BaseMap`]), the zone layer blended at
//! [`RenderOptions::zone_opacity`], graticule, country borders, coastlines,
//! the frame outline, markers and the legend.
//!
//! # Smooth zone edges
//!
//! The raster renderer samples the zone layer once per output pixel through
//! the projection's inverse, so every projection is handled the same way.
//! Three sampling modes trade fidelity against what the caller supplies:
//!
//! * [`ZoneSmoothing::Nearest`]: the cell under the pixel. Exact but
//!   blocky; shows the grid's true resolution.
//! * [`ZoneSmoothing::Smooth`] (default): a bilinear vote. Each zone
//!   present among the four surrounding cell centres gets the bilinear
//!   weight of its cells and the heaviest zone wins, which turns staircase
//!   boundaries into curves at grid resolution. With anti-aliasing, the
//!   winning margin is converted to a sub-pixel distance and blended with
//!   the runner-up. Needs only the zone indices, but boundaries are an
//!   approximation (they pass midway between differing cells).
//! * A continuous [`ZoneField`] (for example Yallop's *q* per cell) plus
//!   descending thresholds: the field is interpolated bilinearly and
//!   classified per pixel, so zone boundaries are the true contour lines of
//!   the interpolated field (equivalent to marching squares at grid
//!   resolution), and the field gradient gives an exact anti-aliasing
//!   distance. This is the most faithful option, at the cost of the grid
//!   engine storing one `f32` per cell. Cells that are `None` in the grid
//!   still mask the field.
//!
//! The SVG back end emits zones from the grid cells (horizontal runs merged
//! vertically into rectangles in lat/lon, reprojected), which keeps files
//! compact and exact at grid resolution.

mod font;
mod geom;
mod legend;
mod raster;
mod svg;
mod zones;

pub use legend::{Legend, LegendEntry, LegendPosition, draw_legend, legend_svg};
pub use raster::render_raster;
pub use svg::render_svg;

use std::fmt;

use crate::basemap::Scale;
use crate::grid::VisibilityGrid;
use crate::projection::{MapProjection, Projection};

/// An 8-bit RGBA image, row-major, not premultiplied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` bytes.
    pub pixels: Vec<u8>,
}

impl RgbaImage {
    /// An image filled with one colour.
    pub fn new(width: u32, height: u32, fill: [u8; 4]) -> Self {
        let pixels = fill.repeat(width as usize * height as usize);
        Self {
            width,
            height,
            pixels,
        }
    }

    /// The pixel at (`x`, `y`). Panics if out of bounds.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        assert!(x < self.width && y < self.height, "pixel out of bounds");
        let i = (y as usize * self.width as usize + x as usize) * 4;
        [
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ]
    }

    /// Blends `color` with coverage `alpha` (0..1) over the pixel at
    /// (`x`, `y`) with the "over" operator. Out-of-bounds pixels are ignored.
    #[inline]
    pub fn blend(&mut self, x: i32, y: i32, color: [u8; 4], alpha: f32) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let a = alpha.clamp(0.0, 1.0) * f32::from(color[3]) / 255.0;
        if a <= 0.0 {
            return;
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        let p = &mut self.pixels[i..i + 4];
        let da = f32::from(p[3]) / 255.0;
        let oa = a + da * (1.0 - a);
        if oa <= 0.0 {
            return;
        }
        for c in 0..3 {
            let v = (f32::from(color[c]) * a + f32::from(p[c]) * da * (1.0 - a)) / oa;
            p[c] = (v + 0.5) as u8;
        }
        p[3] = (oa * 255.0 + 0.5) as u8;
    }
}

/// Error from [`encode_png`].
#[derive(Debug)]
pub struct EncodeError(String);

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PNG encoding failed: {}", self.0)
    }
}

impl std::error::Error for EncodeError {}

/// Encodes an image as an 8-bit RGBA PNG, in memory.
pub fn encode_png(img: &RgbaImage) -> Result<Vec<u8>, EncodeError> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, img.width, img.height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut w = enc.write_header().map_err(|e| EncodeError(e.to_string()))?;
        w.write_image_data(&img.pixels)
            .map_err(|e| EncodeError(e.to_string()))?;
        w.finish().map_err(|e| EncodeError(e.to_string()))?;
    }
    Ok(out)
}

/// A continuous per-cell value field whose contours define the zones (see
/// the module docs).
#[derive(Clone, Copy, Debug)]
pub struct ZoneField<'a> {
    /// One value per grid cell, same layout as [`VisibilityGrid::cells`].
    /// Non-finite values are treated like `None` cells.
    pub values: &'a [f32],
    /// Zone boundaries in **descending** order: a value `>= thresholds[0]`
    /// is zone 0, `>= thresholds[1]` zone 1, ..., anything lower is zone
    /// `thresholds.len()`. For Yallop's q: `[0.216, -0.014, -0.160, -0.232,
    /// -0.293]` gives zones A..F as 0..5.
    pub thresholds: &'a [f32],
}

impl ZoneField<'_> {
    /// The zone of a value.
    pub fn classify(&self, value: f32) -> u8 {
        self.thresholds.iter().take_while(|&&t| value < t).count() as u8
    }
}

/// The data to draw: the grid, how to colour it, and optionally a
/// continuous field for smooth contours.
#[derive(Clone, Copy, Debug)]
pub struct MapData<'a> {
    pub grid: &'a VisibilityGrid,
    /// Colour per zone index. Zones without a palette entry are drawn as
    /// `None` cells.
    pub palette: &'a [[u8; 3]],
    /// Colour for `None` cells; `None` leaves them transparent so the base
    /// map shows through.
    pub none_color: Option<[u8; 3]>,
    /// Optional continuous field (raster only). Ignored if its length does
    /// not match the grid.
    pub field: Option<ZoneField<'a>>,
}

/// How the raster renderer samples zones from grid cells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ZoneSmoothing {
    /// The cell under each pixel.
    Nearest,
    /// Bilinear vote between the four nearest cell centres.
    #[default]
    Smooth,
}

/// Stroke colour (RGBA) and width in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineStyle {
    pub color: [u8; 4],
    pub width: f32,
}

impl LineStyle {
    pub const fn new(color: [u8; 4], width: f32) -> Self {
        Self { color, width }
    }
}

/// Latitude/longitude grid lines.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Graticule {
    /// Spacing between lines, degrees.
    pub spacing_deg: f64,
    pub style: LineStyle,
}

/// A labelled point.
#[derive(Clone, Debug, PartialEq)]
pub struct Marker {
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub label: Option<String>,
    pub color: [u8; 3],
    pub radius_px: f32,
}

impl Marker {
    pub fn new(lat_deg: f64, lon_deg: f64, label: impl Into<String>) -> Self {
        Self {
            lat_deg,
            lon_deg,
            label: Some(label.into()),
            color: [220, 30, 30],
            radius_px: 3.5,
        }
    }
}

/// Everything about how a map is drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderOptions {
    pub projection: MapProjection,
    /// Output size in pixels. The projection is stretched to fill it; use
    /// [`RenderOptions::fit_height`] for an undistorted map.
    pub width: u32,
    pub height: u32,
    /// Which embedded base map to draw.
    pub basemap: Scale,
    /// Colour outside the projection (e.g. around a globe).
    pub background: [u8; 4],
    pub sea_color: [u8; 3],
    pub land_color: [u8; 3],
    /// Opacity of the zone layer over land and sea, 0..1.
    pub zone_opacity: f32,
    pub smoothing: ZoneSmoothing,
    /// Anti-aliased lines, coasts and zone edges.
    pub antialias: bool,
    pub coastlines: Option<LineStyle>,
    pub borders: Option<LineStyle>,
    pub graticule: Option<Graticule>,
    /// Outline of the map frame (rectangle, Robinson outline or globe limb).
    pub frame: Option<LineStyle>,
    pub markers: Vec<Marker>,
    /// Integer scale of the built-in 5x7 font for marker labels.
    pub label_scale: u32,
    pub legend: Option<Legend>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            projection: MapProjection::default(),
            width: 1440,
            height: 720,
            basemap: Scale::Ne50m,
            background: [0, 0, 0, 0],
            sea_color: [196, 220, 240],
            land_color: [236, 232, 220],
            zone_opacity: 0.65,
            smoothing: ZoneSmoothing::Smooth,
            antialias: true,
            coastlines: Some(LineStyle::new([60, 60, 70, 255], 0.8)),
            borders: Some(LineStyle::new([110, 110, 120, 200], 0.5)),
            graticule: Some(Graticule {
                spacing_deg: 30.0,
                style: LineStyle::new([90, 90, 110, 90], 0.6),
            }),
            frame: Some(LineStyle::new([60, 60, 70, 255], 1.0)),
            markers: Vec::new(),
            label_scale: 1,
            legend: None,
        }
    }
}

impl RenderOptions {
    /// Sets `height` from `width` and the projection's aspect ratio.
    pub fn fit_height(mut self) -> Self {
        self.height = ((self.width as f64 / self.projection.aspect_ratio()).round() as u32).max(1);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_round_trips() {
        let mut img = RgbaImage::new(3, 2, [10, 20, 30, 255]);
        img.blend(1, 1, [255, 0, 0, 255], 1.0);
        let bytes = encode_png(&img).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        let dec = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = dec.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!((info.width, info.height), (3, 2));
        assert_eq!(&buf[..info.buffer_size()], &img.pixels[..]);
    }

    #[test]
    fn blend_is_over_operator() {
        let mut img = RgbaImage::new(1, 1, [0, 0, 0, 255]);
        img.blend(0, 0, [255, 255, 255, 255], 0.5);
        assert_eq!(img.pixel(0, 0), [128, 128, 128, 255]);
        let mut img = RgbaImage::new(1, 1, [0, 0, 0, 0]);
        img.blend(0, 0, [200, 100, 0, 255], 0.5);
        assert_eq!(img.pixel(0, 0), [200, 100, 0, 128]);
    }

    #[test]
    fn field_classifies_yallop_q() {
        let f = ZoneField {
            values: &[],
            thresholds: &[0.216, -0.014, -0.160, -0.232, -0.293],
        };
        assert_eq!(f.classify(0.5), 0);
        assert_eq!(f.classify(0.216), 0);
        assert_eq!(f.classify(0.0), 1);
        assert_eq!(f.classify(-0.2), 3);
        assert_eq!(f.classify(-1.0), 5);
    }
}
