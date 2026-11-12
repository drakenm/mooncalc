//! The software rasteriser.

use super::geom::{self, Pt};
use super::zones::ZoneSampler;
use super::{LineStyle, MapData, Marker, RenderOptions, RgbaImage, draw_legend, font};
use crate::basemap::{BaseMap, GeoPoint};
use crate::projection::Projection;

/// Renders a map into a new RGBA image of `opts.width` x `opts.height`.
pub fn render_raster(data: &MapData<'_>, opts: &RenderOptions) -> RgbaImage {
    let (w, h) = (opts.width.max(1), opts.height.max(1));
    let proj = &opts.projection;
    let base = BaseMap::embedded(opts.basemap);
    let mut img = RgbaImage::new(w, h, opts.background);
    fill_surface(&mut img, data, opts, base);

    let mut lines = LineRaster::new(w, h, opts.antialias);
    if let Some(g) = opts.graticule {
        let geo = geom::graticule_lines(g.spacing_deg, proj.seam_lon_deg());
        lines.stroke_geo(proj, geo.iter().map(|l| l.iter().copied()), g.style.width);
        lines.composite(&mut img, g.style.color);
    }
    for (layer, style) in [
        (&base.borders, opts.borders),
        (&base.coastlines, opts.coastlines),
    ] {
        if let Some(style) = style {
            let iter = layer.iter().map(|l| {
                l.iter()
                    .map(|p: &GeoPoint| (f64::from(p.lat_deg), f64::from(p.lon_deg)))
            });
            lines.stroke_geo(proj, iter, style.width);
            lines.composite(&mut img, style.color);
        }
    }
    if let Some(style) = opts.frame {
        stroke_outline(&mut lines, proj.outline(), style);
        lines.composite(&mut img, style.color);
    }
    for m in &opts.markers {
        draw_marker(&mut img, proj, m, opts.label_scale, opts.antialias);
    }
    if let Some(legend) = &opts.legend {
        draw_legend(&mut img, legend);
    }
    img
}

fn stroke_outline(lines: &mut LineRaster, mut outline: Vec<Pt>, style: LineStyle) {
    if let Some(&first) = outline.first() {
        outline.push(first);
    }
    let (w, h) = (lines.w as f64, lines.h as f64);
    // Keep the stroke inside the image at the frame edges.
    let inset = f64::from(style.width) * 0.5;
    let px: Vec<(f32, f32)> = outline
        .iter()
        .map(|&(x, y)| {
            (
                (x * w).clamp(inset, w - inset) as f32,
                (y * h).clamp(inset, h - inset) as f32,
            )
        })
        .collect();
    lines.stroke_px(&px, style.width);
}

/// Land/sea and zones, one sample per pixel through the inverse projection.
fn fill_surface(img: &mut RgbaImage, data: &MapData<'_>, opts: &RenderOptions, base: &BaseMap) {
    let (w, h) = (img.width, img.height);
    let proj = &opts.projection;
    let mask = base.land_mask();
    let sampler = ZoneSampler::new(
        data,
        proj,
        (w, h),
        opts.zone_opacity,
        opts.smoothing,
        opts.antialias,
    );
    let sea = opts.sea_color.map(f32::from);
    let land = opts.land_color.map(f32::from);
    let (fw, fh) = (f64::from(w), f64::from(h));
    let xs: Vec<f64> = (0..w).map(|px| (f64::from(px) + 0.5) / fw).collect();
    let mut geo = vec![None; w as usize];
    for (py, row) in img.pixels.chunks_exact_mut(w as usize * 4).enumerate() {
        let y = (py as f64 + 0.5) / fh;
        proj.inverse_row(y, &xs, &mut geo);
        for (out, g) in row.as_chunks_mut::<4>().0.iter_mut().zip(&geo) {
            let Some((lat, lon)) = *g else {
                continue;
            };
            let mut f = mask.land_fraction(lat, lon);
            if !opts.antialias {
                f = if f >= 0.5 { 1.0 } else { 0.0 };
            }
            let z = sampler.sample(lat, lon);
            let inv = 1.0 - z[3];
            for c in 0..3 {
                let b = sea[c] + (land[c] - sea[c]) * f;
                out[c] = (z[c] + b * inv + 0.5) as u8;
            }
            out[3] = 255;
        }
    }
}

/// Accumulates line coverage (max-combined, so joints do not darken) and
/// composites it in one colour.
struct LineRaster {
    w: usize,
    h: usize,
    aa: bool,
    cov: Vec<u8>,
    /// Dirty rows (inclusive range) to composite and clear.
    dirty: Option<(usize, usize)>,
    pieces: Vec<Vec<Pt>>,
    px: Vec<(f32, f32)>,
    simple: Vec<(f32, f32)>,
}

impl LineRaster {
    fn new(w: u32, h: u32, aa: bool) -> Self {
        Self {
            w: w as usize,
            h: h as usize,
            aa,
            cov: vec![0; w as usize * h as usize],
            dirty: None,
            pieces: Vec::new(),
            px: Vec::new(),
            simple: Vec::new(),
        }
    }

    fn stroke_geo<I, L>(&mut self, proj: &dyn Projection, lines: I, width: f32)
    where
        I: IntoIterator<Item = L>,
        L: IntoIterator<Item = (f64, f64)>,
    {
        let (w, h) = (self.w as f64, self.h as f64);
        for line in lines {
            self.pieces.clear();
            geom::project_polyline(proj, line, &mut self.pieces);
            let pieces = std::mem::take(&mut self.pieces);
            for piece in &pieces {
                let mut px = std::mem::take(&mut self.px);
                px.clear();
                px.extend(piece.iter().map(|&(x, y)| ((x * w) as f32, (y * h) as f32)));
                let mut simple = std::mem::take(&mut self.simple);
                geom::simplify(&px, 0.5, &mut simple);
                self.stroke_px(&simple, width);
                self.px = px;
                self.simple = simple;
            }
            self.pieces = pieces;
        }
    }

    fn stroke_px(&mut self, pts: &[(f32, f32)], width: f32) {
        for seg in pts.windows(2) {
            self.segment(seg[0], seg[1], width);
        }
    }

    fn segment(&mut self, a: (f32, f32), b: (f32, f32), width: f32) {
        let hw = (width * 0.5).max(0.5);
        let reach = hw + 1.0;
        let x0 = (a.0.min(b.0) - reach).floor().max(0.0) as i64;
        let x1 = (a.0.max(b.0) + reach).ceil().min(self.w as f32 - 1.0) as i64;
        let y0 = (a.1.min(b.1) - reach).floor().max(0.0) as i64;
        let y1 = (a.1.max(b.1) + reach).ceil().min(self.h as f32 - 1.0) as i64;
        if x0 > x1 || y0 > y1 {
            return;
        }
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len2 = dx * dx + dy * dy;
        // AA lines: coverage = clamp(w/2 + 0.5 - d); thin lines keep their
        // nominal intensity by scaling with the width.
        let thin = width.min(1.0);
        for y in y0..=y1 {
            let py = y as f32 + 0.5;
            let row = y as usize * self.w;
            for x in x0..=x1 {
                let px = x as f32 + 0.5;
                let t = if len2 > 0.0 {
                    (((px - a.0) * dx + (py - a.1) * dy) / len2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let (ex, ey) = (a.0 + t * dx - px, a.1 + t * dy - py);
                let d = (ex * ex + ey * ey).sqrt();
                let c = if self.aa {
                    ((width * 0.5).max(0.5) + 0.5 - d).clamp(0.0, 1.0) * thin
                } else if d <= hw {
                    1.0
                } else {
                    0.0
                };
                if c > 0.0 {
                    let v = (c * 255.0 + 0.5) as u8;
                    let cell = &mut self.cov[row + x as usize];
                    if v > *cell {
                        *cell = v;
                    }
                }
            }
        }
        let (r0, r1) = (y0 as usize, y1 as usize);
        self.dirty = Some(match self.dirty {
            None => (r0, r1),
            Some((a, b)) => (a.min(r0), b.max(r1)),
        });
    }

    /// Blends the accumulated coverage in `color` and clears it.
    fn composite(&mut self, img: &mut RgbaImage, color: [u8; 4]) {
        let Some((r0, r1)) = self.dirty.take() else {
            return;
        };
        for y in r0..=r1 {
            let row = &mut self.cov[y * self.w..(y + 1) * self.w];
            for (x, c) in row.iter_mut().enumerate() {
                if *c > 0 {
                    img.blend(x as i32, y as i32, color, f32::from(*c) / 255.0);
                    *c = 0;
                }
            }
        }
    }
}

/// Draws `text` with its top-left corner at (`x`, `y`), optionally with a
/// one-pixel halo for legibility over busy backgrounds.
pub(crate) fn draw_text(
    img: &mut RgbaImage,
    x: i32,
    y: i32,
    text: &str,
    scale: u32,
    color: [u8; 4],
    halo: Option<[u8; 4]>,
) {
    if let Some(halo) = halo {
        let mut pts = Vec::new();
        font::for_each_pixel(text, x, y, scale, |px, py| pts.push((px, py)));
        let mut mask = std::collections::HashSet::new();
        for &(px, py) in &pts {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    mask.insert((px + dx, py + dy));
                }
            }
        }
        let mut halo_px: Vec<_> = mask.into_iter().collect();
        halo_px.sort_unstable();
        for (px, py) in halo_px {
            img.blend(px, py, halo, 1.0);
        }
    }
    font::for_each_pixel(text, x, y, scale, |px, py| img.blend(px, py, color, 1.0));
}

fn draw_marker(img: &mut RgbaImage, proj: &dyn Projection, m: &Marker, label_scale: u32, aa: bool) {
    let Some((x, y)) = proj.forward(m.lat_deg, m.lon_deg) else {
        return;
    };
    let (cx, cy) = (
        (x * f64::from(img.width)) as f32,
        (y * f64::from(img.height)) as f32,
    );
    if cx < -50.0 || cy < -50.0 || cx > img.width as f32 + 50.0 || cy > img.height as f32 + 50.0 {
        return;
    }
    let r = m.radius_px.max(1.0);
    let outer = r + 1.2;
    let fill = [m.color[0], m.color[1], m.color[2], 255];
    let (x0, x1) = (
        (cx - outer - 1.0).floor() as i32,
        (cx + outer + 1.0).ceil() as i32,
    );
    let (y0, y1) = (
        (cy - outer - 1.0).floor() as i32,
        (cy + outer + 1.0).ceil() as i32,
    );
    let cov = |rad: f32, d: f32| {
        if aa {
            (rad + 0.5 - d).clamp(0.0, 1.0)
        } else if d <= rad {
            1.0
        } else {
            0.0
        }
    };
    for py in y0..=y1 {
        for px in x0..=x1 {
            let d = (px as f32 + 0.5 - cx).hypot(py as f32 + 0.5 - cy);
            img.blend(px, py, [255, 255, 255, 255], cov(outer, d));
            img.blend(px, py, fill, cov(r, d));
        }
    }
    if let Some(label) = &m.label {
        let s = label_scale.max(1);
        let ty = cy as i32 - (font::GLYPH_H * s) as i32 / 2;
        draw_text(
            img,
            (cx + outer + 3.0) as i32,
            ty,
            label,
            s,
            [20, 20, 20, 255],
            Some([255, 255, 255, 200]),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::VisibilityGrid;
    use crate::projection::{Equirectangular, MapProjection, Orthographic};
    use crate::render::{Legend, LegendEntry, LegendPosition, ZoneField, ZoneSmoothing};

    const PALETTE: [[u8; 3]; 3] = [[0, 160, 0], [250, 200, 0], [220, 0, 0]];

    /// Zone 0 west of 0°, zone 1 from 0° to 90° E, zone 2 east of that,
    /// `None` south of 60° S.
    fn grid() -> VisibilityGrid {
        let mut g = VisibilityGrid::new(-90.0, 90.0, -180.0, 180.0, 18, 36);
        for r in 0..g.rows {
            for c in 0..g.cols {
                let (lat, lon) = g.cell_center(r, c);
                let z = if lat < -60.0 {
                    None
                } else if lon < 0.0 {
                    Some(0)
                } else if lon < 90.0 {
                    Some(1)
                } else {
                    Some(2)
                };
                g.set(r, c, z);
            }
        }
        g
    }

    fn bare(proj: MapProjection, w: u32, h: u32) -> RenderOptions {
        RenderOptions {
            projection: proj,
            width: w,
            height: h,
            zone_opacity: 1.0,
            coastlines: None,
            borders: None,
            graticule: None,
            frame: None,
            ..RenderOptions::default()
        }
    }

    fn px(img: &RgbaImage, lat: f64, lon: f64, proj: &dyn Projection) -> [u8; 4] {
        let (x, y) = proj.forward(lat, lon).unwrap();
        img.pixel(
            (x * f64::from(img.width)) as u32,
            (y * f64::from(img.height)) as u32,
        )
    }

    #[test]
    fn zones_land_on_the_right_pixels() {
        let g = grid();
        let data = MapData {
            grid: &g,
            palette: &PALETTE,
            none_color: Some([40, 40, 40]),
            field: None,
        };
        for smoothing in [ZoneSmoothing::Nearest, ZoneSmoothing::Smooth] {
            let opts = RenderOptions {
                smoothing,
                ..bare(Equirectangular::world(0.0).into(), 360, 180)
            };
            let img = render_raster(&data, &opts);
            let p = &opts.projection;
            assert_eq!(px(&img, 20.0, -100.0, p), [0, 160, 0, 255]);
            assert_eq!(px(&img, 20.0, 45.0, p), [250, 200, 0, 255]);
            assert_eq!(px(&img, 20.0, 135.0, p), [220, 0, 0, 255]);
            assert_eq!(px(&img, -80.0, 0.0, p), [40, 40, 40, 255]);
            // Across the antimeridian the grid wraps: zone 2 meets zone 0.
            assert_eq!(px(&img, 0.0, 179.0, p), [220, 0, 0, 255]);
        }
    }

    #[test]
    fn base_map_shows_through_transparent_cells() {
        let g = VisibilityGrid::new(-90.0, 90.0, -180.0, 180.0, 4, 8);
        let data = MapData {
            grid: &g,
            palette: &PALETTE,
            none_color: None,
            field: None,
        };
        let opts = bare(Equirectangular::world(0.0).into(), 720, 360);
        let img = render_raster(&data, &opts);
        let p = &opts.projection;
        let l = opts.land_color;
        let s = opts.sea_color;
        assert_eq!(px(&img, 23.0, 10.0, p), [l[0], l[1], l[2], 255], "Sahara");
        assert_eq!(px(&img, 0.0, -150.0, p), [s[0], s[1], s[2], 255], "Pacific");
    }

    #[test]
    fn opacity_blends_zone_over_base() {
        let mut g = VisibilityGrid::new(-90.0, 90.0, -180.0, 180.0, 2, 2);
        g.cells.fill(Some(0));
        let data = MapData {
            grid: &g,
            palette: &[[255, 0, 0]],
            none_color: None,
            field: None,
        };
        let opts = RenderOptions {
            zone_opacity: 0.5,
            ..bare(Equirectangular::world(0.0).into(), 360, 180)
        };
        let img = render_raster(&data, &opts);
        let s = opts.sea_color;
        let got = px(&img, 0.0, -150.0, &opts.projection);
        let expect = [
            ((255 + u32::from(s[0])) as f32 / 2.0).round() as u8,
            (s[1] as f32 / 2.0).round() as u8,
        ];
        assert!(
            (i32::from(got[0]) - i32::from(expect[0])).abs() <= 1,
            "{got:?}"
        );
        assert!(
            (i32::from(got[1]) - i32::from(expect[1])).abs() <= 1,
            "{got:?}"
        );
    }

    #[test]
    fn field_contours_follow_the_interpolated_value() {
        // q increases linearly eastwards; threshold 0 sits at lon 0.
        let mut g = VisibilityGrid::new(-90.0, 90.0, -180.0, 180.0, 10, 20);
        let mut q = vec![0f32; 200];
        for r in 0..10 {
            for c in 0..20 {
                g.set(r, c, Some(0));
                q[r * 20 + c] = g.cell_center(r, c).1 as f32 / 100.0;
            }
        }
        let field = ZoneField {
            values: &q,
            thresholds: &[0.0],
        };
        let data = MapData {
            grid: &g,
            palette: &[[0, 200, 0], [200, 0, 0]],
            none_color: None,
            field: Some(field),
        };
        let mut opts = bare(Equirectangular::from_grid(&g).into(), 720, 360);
        opts.antialias = false;
        let img = render_raster(&data, &opts);
        // Pixel just east of lon 0 is zone 0 (q >= 0), just west zone 1,
        // even though both lie inside the same 18°-wide cell column pair.
        assert_eq!(img.pixel(361, 180), [0, 200, 0, 255]);
        assert_eq!(img.pixel(358, 180), [200, 0, 0, 255]);
        assert_eq!(img.pixel(370, 180), [0, 200, 0, 255]);
    }

    #[test]
    fn field_cells_with_a_zone_but_no_finite_value_keep_their_zone() {
        // The engine gives "moonset after sunset" and "minimum lag" cells
        // where the Moon does not set zone 0 with value lag = +∞. They must
        // be drawn in zone 0, not as "no crescent".
        let mut g = VisibilityGrid::new(-90.0, 90.0, -180.0, 180.0, 10, 20);
        let mut lag = vec![f32::NAN; 200];
        for r in 0..10 {
            for c in 0..20 {
                if r < 3 {
                    // Polar band: Moon always up.
                    g.set(r, c, Some(0));
                    lag[r * 20 + c] = f32::INFINITY;
                } else if c < 10 {
                    g.set(r, c, Some(0));
                    lag[r * 20 + c] = 30.0;
                }
            }
        }
        let field = ZoneField {
            values: &lag,
            thresholds: &[0.0],
        };
        let data = MapData {
            grid: &g,
            palette: &[[0, 200, 0], [200, 0, 0]],
            none_color: Some([40, 40, 40]),
            field: Some(field),
        };
        for antialias in [false, true] {
            let mut opts = bare(Equirectangular::from_grid(&g).into(), 720, 360);
            opts.antialias = antialias;
            let img = render_raster(&data, &opts);
            let p = &opts.projection;
            // Inside the Moon-always-up band, and where it meets finite cells.
            assert_eq!(px(&img, 80.0, 90.0, p), [0, 200, 0, 255], "{antialias}");
            assert_eq!(px(&img, 63.0, -90.0, p), [0, 200, 0, 255], "{antialias}");
            // Finite cells and None cells are unchanged.
            assert_eq!(px(&img, 0.0, -90.0, p), [0, 200, 0, 255], "{antialias}");
            assert_eq!(px(&img, 0.0, 90.0, p), [40, 40, 40, 255], "{antialias}");
        }
    }

    #[test]
    fn degenerate_inputs_do_not_panic() {
        let empty = VisibilityGrid::new(-90.0, 90.0, -180.0, 180.0, 0, 0);
        let one = VisibilityGrid::new(10.0, 20.0, 30.0, 40.0, 1, 1);
        for g in [&empty, &one] {
            let data = MapData {
                grid: g,
                palette: &[],
                none_color: Some([1, 1, 1]),
                field: None,
            };
            let opts = RenderOptions {
                width: 16,
                height: 1,
                basemap: crate::basemap::Scale::Ne110m,
                ..Default::default()
            };
            let img = render_raster(&data, &opts);
            assert_eq!(img.pixels.len(), 16 * 4);
            assert!(crate::render::render_svg(&data, &opts).ends_with(
                "</svg>
"
            ));
        }
        // Mismatched field length is ignored rather than indexed.
        let data = MapData {
            grid: &one,
            palette: &PALETTE,
            none_color: None,
            field: Some(ZoneField {
                values: &[1.0, 2.0],
                thresholds: &[0.0],
            }),
        };
        render_raster(&data, &bare(Equirectangular::world(0.0).into(), 8, 4));
    }

    #[test]
    fn globe_leaves_background_outside_disc() {
        let g = grid();
        let data = MapData {
            grid: &g,
            palette: &PALETTE,
            none_color: None,
            field: None,
        };
        let opts = RenderOptions {
            background: [1, 2, 3, 0],
            ..bare(Orthographic::new(0.0, 45.0).into(), 200, 200)
        };
        let img = render_raster(&data, &opts);
        assert_eq!(img.pixel(0, 0), [1, 2, 3, 0]);
        assert_eq!(img.pixel(100, 100), [250, 200, 0, 255]);
    }

    #[test]
    fn lines_markers_and_legend_draw() {
        let g = VisibilityGrid::new(-90.0, 90.0, -180.0, 180.0, 1, 1);
        let data = MapData {
            grid: &g,
            palette: &PALETTE,
            none_color: None,
            field: None,
        };
        let mut opts = bare(Equirectangular::world(0.0).into(), 720, 360);
        opts.graticule = Some(super::super::Graticule {
            spacing_deg: 30.0,
            style: LineStyle::new([0, 0, 255, 255], 1.0),
        });
        opts.markers.push(Marker::new(0.0, -150.0, "X"));
        opts.legend = Some(Legend {
            title: None,
            entries: vec![LegendEntry {
                color: [9, 99, 199],
                label: "A".into(),
            }],
            position: LegendPosition::BottomLeft,
            ..Legend::default()
        });
        let img = render_raster(&data, &opts);
        // Equator graticule line at y = 180 between the pixel rows 179/180.
        let p = img.pixel(100, 180);
        assert!(p[2] > 200 && p[0] < 150, "{p:?}");
        // Marker centre (x = 60, y = 180) is red.
        assert_eq!(img.pixel(60, 178), [220, 30, 30, 255]);
        // Legend swatch colour appears in the bottom-left corner.
        let found = (300..360).any(|y| (0..60).any(|x| img.pixel(x, y)[..3] == [9, 99, 199]));
        assert!(found);
    }
}
