//! Self-contained SVG export.

use std::collections::HashMap;
use std::fmt::Write as _;

use super::geom::{self, Pt};
use super::{LineStyle, MapData, RenderOptions, legend_svg};
use crate::basemap::{BaseMap, GeoPoint};
use crate::grid::VisibilityGrid;
use crate::projection::{MapProjection, Projection};

/// `#rrggbb`.
pub(crate) fn rgb(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// Escapes text for XML content and attribute values.
pub(crate) fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' => {}
            c => out.push(c),
        }
    }
    out
}

/// Appends path data for `pieces` (normalised) scaled to `w` x `h`,
/// dropping points closer than a quarter pixel and collinear points.
fn path_data(pieces: &[Vec<Pt>], w: f64, h: f64, closed: bool, out: &mut String) {
    let mut px: Vec<(f32, f32)> = Vec::new();
    let mut simple: Vec<(f32, f32)> = Vec::new();
    for piece in pieces {
        px.clear();
        px.extend(piece.iter().map(|&(x, y)| ((x * w) as f32, (y * h) as f32)));
        geom::simplify(&px, 0.25, &mut simple);
        let pts = drop_collinear(&simple);
        if pts.len() < 2 || (closed && pts.len() < 3) {
            continue;
        }
        for (i, p) in pts.iter().enumerate() {
            let _ = write!(
                out,
                "{}{:.1} {:.1}",
                if i == 0 { 'M' } else { 'L' },
                p.0,
                p.1
            );
        }
        if closed {
            out.push('Z');
        }
    }
}

fn drop_collinear(pts: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut out: Vec<(f32, f32)> = Vec::with_capacity(pts.len());
    for (i, &p) in pts.iter().enumerate() {
        if i + 1 < pts.len()
            && let Some(&a) = out.last()
        {
            let b = pts[i + 1];
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len = dx.hypot(dy);
            if len > 0.0 && ((p.0 - a.0) * dy - (p.1 - a.1) * dx).abs() / len < 0.05 {
                // p lies on a-b, and between them.
                let t = ((p.0 - a.0) * dx + (p.1 - a.1) * dy) / (len * len);
                if (0.0..=1.0).contains(&t) {
                    continue;
                }
            }
        }
        out.push(p);
    }
    out
}

fn geo(ring: &[GeoPoint]) -> Vec<(f64, f64)> {
    ring.iter()
        .map(|p| (f64::from(p.lat_deg), f64::from(p.lon_deg)))
        .collect()
}

fn stroke_attrs(style: LineStyle) -> String {
    let c = style.color;
    format!(
        r#"fill="none" stroke="{}" stroke-opacity="{:.3}" stroke-width="{}" stroke-linejoin="round" stroke-linecap="round""#,
        rgb([c[0], c[1], c[2]]),
        f32::from(c[3]) / 255.0,
        style.width
    )
}

fn line_layer<'a>(
    out: &mut String,
    proj: &dyn Projection,
    lines: impl IntoIterator<Item = Vec<(f64, f64)>> + 'a,
    style: LineStyle,
    class: &str,
    (w, h): (f64, f64),
) {
    let mut pieces = Vec::new();
    for l in lines {
        geom::project_polyline(proj, l, &mut pieces);
    }
    let mut d = String::new();
    path_data(&pieces, w, h, false, &mut d);
    if !d.is_empty() {
        let _ = write!(
            out,
            r#"<path class="{class}" {} d="{d}"/>"#,
            stroke_attrs(style)
        );
    }
}

/// Zone rectangles in grid space: horizontal runs of equal cells, merged
/// vertically when consecutive rows repeat the same run. Returns
/// `(key, row0, row1, col0, col1)` (inclusive), key 256 for `None`.
fn zone_rects(grid: &VisibilityGrid) -> Vec<(u16, usize, usize, usize, usize)> {
    let mut done = Vec::new();
    let mut open: HashMap<(usize, usize, u16), usize> = HashMap::new();
    for r in 0..grid.rows {
        let mut next: HashMap<(usize, usize, u16), usize> = HashMap::new();
        let mut c = 0;
        while c < grid.cols {
            let key = grid.get(r, c).map_or(256, u16::from);
            let mut e = c;
            while e + 1 < grid.cols && grid.get(r, e + 1).map_or(256, u16::from) == key {
                e += 1;
            }
            let start = open.remove(&(c, e, key)).unwrap_or(r);
            next.insert((c, e, key), start);
            c = e + 1;
        }
        for ((c0, c1, key), r0) in open.drain() {
            done.push((key, r0, r - 1, c0, c1));
        }
        open = next;
    }
    for ((c0, c1, key), r0) in open {
        done.push((key, r0, grid.rows - 1, c0, c1));
    }
    done.sort_unstable();
    done
}

/// Renders a self-contained SVG document of the map.
///
/// Zones come from the grid cells (the continuous field, if any, is not
/// used), as merged rectangles reprojected into the map plane, so the
/// output is exact at grid resolution.
pub fn render_svg(data: &MapData<'_>, opts: &RenderOptions) -> String {
    let (wi, hi) = (opts.width.max(1), opts.height.max(1));
    let (w, h) = (f64::from(wi), f64::from(hi));
    let proj = &opts.projection;
    let base = BaseMap::embedded(opts.basemap);
    let mut out = String::with_capacity(1 << 16);
    let _ = write!(
        out,
        r#"<?xml version="1.0" encoding="UTF-8"?><svg xmlns="http://www.w3.org/2000/svg" width="{wi}" height="{hi}" viewBox="0 0 {wi} {hi}">"#
    );
    out.push_str("<title>Crescent visibility map</title>");
    let mut frame = String::new();
    path_data(&[proj.outline()], w, h, true, &mut frame);
    let _ = write!(
        out,
        r#"<defs><clipPath id="mc-frame"><path d="{frame}"/></clipPath></defs>"#
    );
    let bg = opts.background;
    if bg[3] > 0 {
        let _ = write!(
            out,
            r#"<rect width="{wi}" height="{hi}" fill="{}" fill-opacity="{:.3}"/>"#,
            rgb([bg[0], bg[1], bg[2]]),
            f32::from(bg[3]) / 255.0
        );
    }
    out.push_str(r#"<g clip-path="url(#mc-frame)">"#);
    let _ = write!(
        out,
        r#"<path class="sea" fill="{}" d="{frame}"/>"#,
        rgb(opts.sea_color)
    );

    // Land.
    let mut pieces = Vec::new();
    for poly in &base.land {
        for ring in &poly.rings {
            geom::project_ring(proj, &geo(ring), &mut pieces);
        }
    }
    let mut d = String::new();
    path_data(&pieces, w, h, true, &mut d);
    let _ = write!(
        out,
        r#"<path class="land" fill="{}" fill-rule="evenodd" d="{d}"/>"#,
        rgb(opts.land_color)
    );

    // Zones.
    let g = data.grid;
    if g.rows > 0 && g.cols > 0 {
        let dlat = (g.lat_max_deg - g.lat_min_deg) / g.rows as f64;
        let dlon = (g.lon_max_deg - g.lon_min_deg) / g.cols as f64;
        let crisp = if matches!(proj, MapProjection::Equirectangular(_)) {
            r#" shape-rendering="crispEdges""#
        } else {
            ""
        };
        let _ = write!(
            out,
            r#"<g class="zones" opacity="{:.3}"{crisp}>"#,
            opts.zone_opacity.clamp(0.0, 1.0)
        );
        let mut by_key: HashMap<u16, Vec<Vec<Pt>>> = HashMap::new();
        for (key, r0, r1, c0, c1) in zone_rects(g) {
            let color = if key == 256 {
                data.none_color
            } else {
                data.palette.get(key as usize).copied().or(data.none_color)
            };
            if color.is_none() {
                continue;
            }
            let top = g.lat_max_deg - r0 as f64 * dlat;
            let bot = g.lat_max_deg - (r1 + 1) as f64 * dlat;
            let west = g.lon_min_deg + c0 as f64 * dlon;
            let east = g.lon_min_deg + (c1 + 1) as f64 * dlon;
            let ring = [(top, west), (top, east), (bot, east), (bot, west)];
            // Undrawable zones (no palette entry) fall back to the None colour.
            let k = if key != 256 && (key as usize) >= data.palette.len() {
                256
            } else {
                key
            };
            geom::project_ring(proj, &ring, by_key.entry(k).or_default());
        }
        let mut keys: Vec<_> = by_key.keys().copied().collect();
        keys.sort_unstable();
        for k in keys {
            let color = if k == 256 {
                data.none_color
            } else {
                Some(data.palette[k as usize])
            };
            let Some(color) = color else { continue };
            let mut d = String::new();
            path_data(&by_key[&k], w, h, true, &mut d);
            if !d.is_empty() {
                let _ = write!(out, r#"<path fill="{}" d="{d}"/>"#, rgb(color));
            }
        }
        out.push_str("</g>");
    }

    if let Some(gr) = opts.graticule {
        let lines = geom::graticule_lines(gr.spacing_deg, proj.seam_lon_deg());
        line_layer(&mut out, proj, lines, gr.style, "graticule", (w, h));
    }
    if let Some(style) = opts.borders {
        line_layer(
            &mut out,
            proj,
            base.borders.iter().map(|l| geo(l)),
            style,
            "borders",
            (w, h),
        );
    }
    if let Some(style) = opts.coastlines {
        line_layer(
            &mut out,
            proj,
            base.coastlines.iter().map(|l| geo(l)),
            style,
            "coastlines",
            (w, h),
        );
    }
    out.push_str("</g>");
    if let Some(style) = opts.frame {
        let _ = write!(
            out,
            r#"<path class="frame" {} d="{frame}"/>"#,
            stroke_attrs(style)
        );
    }

    // Markers.
    if !opts.markers.is_empty() {
        let fs = 9 * opts.label_scale.max(1);
        let _ = write!(
            out,
            r#"<g class="markers" font-family="sans-serif" font-size="{fs}">"#
        );
        for m in &opts.markers {
            let Some((x, y)) = proj.forward(m.lat_deg, m.lon_deg) else {
                continue;
            };
            let (cx, cy) = (x * w, y * h);
            let _ = write!(
                out,
                r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="{}" fill="{}" stroke="#ffffff" stroke-width="1.2"/>"##,
                m.radius_px,
                rgb(m.color)
            );
            if let Some(label) = &m.label {
                let _ = write!(
                    out,
                    r##"<text x="{:.1}" y="{:.1}" fill="#141414" stroke="#ffffff" stroke-width="2.5" stroke-opacity="0.8" paint-order="stroke">{}</text>"##,
                    cx + f64::from(m.radius_px) + 4.0,
                    cy + f64::from(fs) * 0.35,
                    escape(label)
                );
            }
        }
        out.push_str("</g>");
    }
    if let Some(legend) = &opts.legend {
        out.push_str(&legend_svg(legend, wi, hi));
    }
    out.push_str("</svg>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection::{Equirectangular, Orthographic, Robinson};
    use crate::render::{Legend, Marker};

    #[test]
    fn zone_rects_merge_runs() {
        let mut g = VisibilityGrid::new(-90.0, 90.0, -180.0, 180.0, 3, 4);
        for r in 0..3 {
            for c in 0..4 {
                g.set(r, c, Some(if c < 2 { 0 } else { 1 }));
            }
        }
        g.set(2, 3, None);
        let rects = zone_rects(&g);
        assert_eq!(
            rects,
            vec![
                (0, 0, 2, 0, 1),
                (1, 0, 1, 2, 3),
                (1, 2, 2, 2, 2),
                (256, 2, 2, 3, 3)
            ]
        );
        let area: usize = rects
            .iter()
            .map(|&(_, r0, r1, c0, c1)| (r1 - r0 + 1) * (c1 - c0 + 1))
            .sum();
        assert_eq!(area, 12);
    }

    /// Minimal well-formedness check: balanced tags, quoted attributes and
    /// no stray `<`/`&`.
    fn assert_well_formed(svg: &str) {
        assert!(svg.starts_with("<?xml"));
        let mut stack: Vec<String> = Vec::new();
        let mut rest = svg;
        while let Some(i) = rest.find('<') {
            let text = &rest[..i];
            assert!(!text.contains('>'), "stray '>' in text");
            for (j, _) in text.match_indices('&') {
                let ent = &text[j..];
                assert!(
                    ["&amp;", "&lt;", "&gt;", "&quot;", "&apos;"]
                        .iter()
                        .any(|e| ent.starts_with(e))
                );
            }
            let end = rest[i..].find('>').expect("unterminated tag") + i;
            let tag = &rest[i + 1..end];
            assert!(!tag.contains('<'), "'<' inside tag {tag}");
            assert_eq!(
                tag.matches('"').count() % 2,
                0,
                "unbalanced quotes in {tag}"
            );
            if let Some(name) = tag.strip_prefix('/') {
                assert_eq!(stack.pop().as_deref(), Some(name), "mismatched </{name}>");
            } else if !tag.starts_with('?') && !tag.ends_with('/') {
                stack.push(tag.split_whitespace().next().unwrap().to_owned());
            }
            rest = &rest[end + 1..];
        }
        assert!(stack.is_empty(), "unclosed {stack:?}");
    }

    #[test]
    fn svg_is_well_formed_for_every_projection() {
        let mut g = VisibilityGrid::new(-90.0, 90.0, -180.0, 180.0, 18, 36);
        for r in 0..18 {
            for c in 0..36 {
                g.set(r, c, if r > 14 { None } else { Some((c / 12) as u8) });
            }
        }
        let palette = [[0, 160, 0], [250, 200, 0], [220, 0, 0]];
        let data = MapData {
            grid: &g,
            palette: &palette,
            none_color: Some([50, 50, 50]),
            field: None,
        };
        for proj in [
            MapProjection::from(Equirectangular::world(150.0)),
            Robinson::new(0.0).into(),
            Orthographic::new(30.0, 20.0).into(),
        ] {
            let opts = RenderOptions {
                projection: proj,
                basemap: crate::basemap::Scale::Ne110m,
                markers: vec![Marker::new(21.4, 39.8, "Makkah <&> \"x\"")],
                legend: Some(Legend::from_palette(
                    Some("Zones"),
                    &palette,
                    &["A", "B", "C & D"],
                )),
                ..RenderOptions::default()
            }
            .fit_height();
            let svg = render_svg(&data, &opts);
            assert_well_formed(&svg);
            assert!(svg.contains(r#"class="land""#) && svg.contains(r#"class="coastlines""#));
            assert!(
                svg.contains("#fac800"),
                "zone 1 straddles the prime meridian"
            );
            if proj.seam_lon_deg().is_some() {
                assert!(svg.contains("#00a000") && svg.contains("#323232"));
            }
            assert!(svg.contains("Makkah &lt;&amp;&gt; &quot;x&quot;"));
            assert!(svg.contains("C &amp; D"));
        }
    }
}
