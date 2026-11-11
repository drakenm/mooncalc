//! Zone legends, drawn into a raster or emitted as SVG.

use std::fmt::Write as _;

use super::RgbaImage;
use super::font::{self, GLYPH_H};
use super::raster::draw_text;
use super::svg::{escape, rgb};

/// One legend row: a colour swatch and its description.
#[derive(Clone, Debug, PartialEq)]
pub struct LegendEntry {
    pub color: [u8; 3],
    pub label: String,
}

/// Which corner of the map the legend sits in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LegendPosition {
    TopLeft,
    TopRight,
    #[default]
    BottomLeft,
    BottomRight,
}

/// A legend block: optional title, then one row per entry.
#[derive(Clone, Debug, PartialEq)]
pub struct Legend {
    pub title: Option<String>,
    pub entries: Vec<LegendEntry>,
    pub position: LegendPosition,
    /// Integer scale of the built-in 5x7 font (raster) and of all metrics.
    pub text_scale: u32,
    pub background: [u8; 4],
    pub text_color: [u8; 3],
    pub border_color: Option<[u8; 3]>,
}

impl Default for Legend {
    fn default() -> Self {
        Self {
            title: None,
            entries: Vec::new(),
            position: LegendPosition::BottomLeft,
            text_scale: 1,
            background: [255, 255, 255, 225],
            text_color: [20, 20, 20],
            border_color: Some([90, 90, 90]),
        }
    }
}

impl Legend {
    /// A legend pairing palette colours with labels (extra items of the
    /// longer list are ignored).
    pub fn from_palette(title: Option<&str>, palette: &[[u8; 3]], labels: &[&str]) -> Self {
        Self {
            title: title.map(str::to_owned),
            entries: palette
                .iter()
                .zip(labels)
                .map(|(&color, &l)| LegendEntry {
                    color,
                    label: l.to_owned(),
                })
                .collect(),
            ..Self::default()
        }
    }
}

/// Pixel layout of a legend, shared by the raster and SVG back ends.
struct Layout {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    pad: i32,
    row_h: i32,
    swatch: i32,
    gap: i32,
    title_h: i32,
}

const MARGIN: i32 = 8;

fn layout(legend: &Legend, canvas_w: u32, canvas_h: u32) -> Layout {
    let s = legend.text_scale.max(1) as i32;
    let pad = 5 * s;
    let row_h = 11 * s;
    let swatch = 9 * s;
    let gap = 5 * s;
    let title_h = if legend.title.is_some() {
        row_h + 2 * s
    } else {
        0
    };
    let tw = |t: &str| font::text_width(t, s as u32) as i32;
    let rows_w = legend
        .entries
        .iter()
        .map(|e| swatch + gap + tw(&e.label))
        .max()
        .unwrap_or(0);
    let title_w = legend.title.as_deref().map_or(0, tw);
    let w = 2 * pad + rows_w.max(title_w);
    let h = 2 * pad + title_h + legend.entries.len() as i32 * row_h
        - if legend.entries.is_empty() { 0 } else { 2 * s };
    let (cw, ch) = (canvas_w as i32, canvas_h as i32);
    let (x, y) = match legend.position {
        LegendPosition::TopLeft => (MARGIN, MARGIN),
        LegendPosition::TopRight => (cw - MARGIN - w, MARGIN),
        LegendPosition::BottomLeft => (MARGIN, ch - MARGIN - h),
        LegendPosition::BottomRight => (cw - MARGIN - w, ch - MARGIN - h),
    };
    Layout {
        x,
        y,
        w,
        h,
        pad,
        row_h,
        swatch,
        gap,
        title_h,
    }
}

/// Draws the legend into `img` at its corner.
pub fn draw_legend(img: &mut RgbaImage, legend: &Legend) {
    if legend.entries.is_empty() && legend.title.is_none() {
        return;
    }
    let s = legend.text_scale.max(1);
    let l = layout(legend, img.width, img.height);
    for y in l.y..l.y + l.h {
        for x in l.x..l.x + l.w {
            img.blend(x, y, legend.background, 1.0);
        }
    }
    if let Some(b) = legend.border_color {
        let c = [b[0], b[1], b[2], 255];
        for x in l.x..l.x + l.w {
            img.blend(x, l.y, c, 1.0);
            img.blend(x, l.y + l.h - 1, c, 1.0);
        }
        for y in l.y..l.y + l.h {
            img.blend(l.x, y, c, 1.0);
            img.blend(l.x + l.w - 1, y, c, 1.0);
        }
    }
    let tc = [
        legend.text_color[0],
        legend.text_color[1],
        legend.text_color[2],
        255,
    ];
    let text_dy = (l.swatch - (GLYPH_H * s) as i32) / 2;
    let mut y = l.y + l.pad;
    if let Some(title) = &legend.title {
        draw_text(img, l.x + l.pad, y + text_dy, title, s, tc, None);
        y += l.title_h;
    }
    for e in &legend.entries {
        let sw = [e.color[0], e.color[1], e.color[2], 255];
        for yy in y..y + l.swatch {
            for xx in l.x + l.pad..l.x + l.pad + l.swatch {
                img.blend(xx, yy, sw, 1.0);
            }
        }
        draw_text(
            img,
            l.x + l.pad + l.swatch + l.gap,
            y + text_dy,
            &e.label,
            s,
            tc,
            None,
        );
        y += l.row_h;
    }
}

/// The legend as an SVG `<g>` fragment for a canvas of the given size.
pub fn legend_svg(legend: &Legend, canvas_w: u32, canvas_h: u32) -> String {
    let mut out = String::new();
    if legend.entries.is_empty() && legend.title.is_none() {
        return out;
    }
    let s = legend.text_scale.max(1) as i32;
    let l = layout(legend, canvas_w, canvas_h);
    let bg = legend.background;
    let _ = write!(
        out,
        r#"<g class="legend" font-family="sans-serif" font-size="{}"><rect x="{}" y="{}" width="{}" height="{}" fill="{}" fill-opacity="{:.3}""#,
        9 * s,
        l.x,
        l.y,
        l.w,
        l.h,
        rgb([bg[0], bg[1], bg[2]]),
        f32::from(bg[3]) / 255.0
    );
    if let Some(b) = legend.border_color {
        let _ = write!(out, r#" stroke="{}" stroke-width="1""#, rgb(b));
    }
    out.push_str("/>");
    let tc = rgb(legend.text_color);
    let mut y = l.y + l.pad;
    // Baseline roughly at the bottom of a 7-pixel cap height.
    let baseline = |y: i32| y + l.swatch / 2 + 3 * s;
    if let Some(title) = &legend.title {
        let _ = write!(
            out,
            r#"<text x="{}" y="{}" fill="{tc}" font-weight="bold">{}</text>"#,
            l.x + l.pad,
            baseline(y),
            escape(title)
        );
        y += l.title_h;
    }
    for e in &legend.entries {
        let _ = write!(
            out,
            r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}"/><text x="{}" y="{}" fill="{tc}">{}</text>"#,
            l.x + l.pad,
            y,
            l.swatch,
            l.swatch,
            rgb(e.color),
            l.x + l.pad + l.swatch + l.gap,
            baseline(y),
            escape(&e.label)
        );
        y += l.row_h;
    }
    out.push_str("</g>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legend_draws_swatches_and_text() {
        let legend = Legend {
            position: LegendPosition::TopRight,
            ..Legend::from_palette(
                Some("Yallop"),
                &[[0, 200, 0], [200, 0, 0]],
                &["A: easily visible", "F: not visible"],
            )
        };
        let mut img = RgbaImage::new(300, 120, [0, 0, 0, 255]);
        draw_legend(&mut img, &legend);
        let l = layout(&legend, 300, 120);
        assert_eq!(l.x + l.w, 300 - MARGIN);
        // First swatch, below the title.
        let (sx, sy) = (
            (l.x + l.pad + 2) as u32,
            (l.y + l.pad + l.title_h + 2) as u32,
        );
        assert_eq!(img.pixel(sx, sy), [0, 200, 0, 255]);
        assert_eq!(img.pixel(sx, sy + l.row_h as u32), [200, 0, 0, 255]);
        // Some text pixels exist in the label area.
        let dark = (l.y..l.y + l.h).any(|y| img.pixel((l.x + l.w - 10) as u32, y as u32)[0] == 20);
        assert!(dark || legend.entries[0].label.len() < 5);
        let svg = legend_svg(&legend, 300, 120);
        assert!(svg.starts_with("<g") && svg.ends_with("</g>"));
        assert!(svg.contains("A: easily visible") && svg.contains("#00c800"));
    }
}
